use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, MSG, PeekMessageW, PM_REMOVE, TranslateMessage};
use runtime_binding_gen::{RuntimeExtensionMetadata, RuntimeExtensionRegistry};

use crate::{throw_js_error, Runtime, ASYNC_PUMP_HOOK, proxy_manifests};
use crate::type_description::build_runtime_type_descriptor;

// ── Private helpers ───────────────────────────────────────────────────────────

pub(crate) fn value_to_string(scope: &mut v8::PinScope<'_, '_>, value: v8::Local<v8::Value>) -> Option<String> {
    let value = value.to_string(scope)?;
    Some(value.to_rust_string_lossy(scope))
}

fn value_to_json_string(scope: &mut v8::PinScope<'_, '_>, value: v8::Local<v8::Value>) -> Option<String> {
    let json = v8::json::stringify(scope, value)?;
    Some(json.to_rust_string_lossy(scope))
}

fn try_get_async_status(
    scope: &mut v8::PinScope<'_, '_>,
    value: v8::Local<v8::Value>,
) -> Result<i32, String> {
    if !value.is_object() {
        return Err("Expected a wrapped WinRT async object".to_string());
    }
    let object = value
        .to_object(scope)
        .ok_or_else(|| "Expected a wrapped WinRT async object".to_string())?;
    let status_key = v8::String::new(scope, "Status")
        .ok_or_else(|| "Unable to allocate V8 string for async status lookup".to_string())?;
    let status = object
        .get(scope, status_key.into())
        .ok_or_else(|| "Async object does not expose Status".to_string())?;

    if let Ok(value) = v8::Local::<v8::Int32>::try_from(status) { return Ok(value.value()); }
    if let Ok(value) = v8::Local::<v8::Uint32>::try_from(status) { return Ok(value.value() as i32); }
    if let Ok(value) = v8::Local::<v8::Number>::try_from(status) { return Ok(value.value() as i32); }
    if let Some(value) = status.integer_value(scope) { return Ok(value as i32); }
    if let Some(value) = status.number_value(scope) {
        if value.is_finite() { return Ok(value as i32); }
    }
    if let Some(value) = status.to_string(scope) {
        let s = value.to_rust_string_lossy(scope).to_ascii_lowercase();
        return match s.as_str() {
            "started"             => Ok(0),
            "completed"           => Ok(1),
            "canceled" | "cancelled" => Ok(2),
            "error"               => Ok(3),
            _                     => Err(format!("Async Status is not a recognized value: {s}")),
        };
    }
    Err("Async Status is not a numeric value".to_string())
}

fn normalize_js_path(path: &str) -> PathBuf {
    if let Some(raw) = path.strip_prefix("file:///") {
        return PathBuf::from(raw.replace('/', "\\"));
    }
    if let Some(raw) = path.strip_prefix("file://") {
        return PathBuf::from(raw.replace('/', "\\"));
    }
    PathBuf::from(path)
}

fn try_resolve_with_known_extensions(candidate: PathBuf) -> PathBuf {
    if candidate.exists() { return candidate; }
    if candidate.extension().is_none() {
        for ext in ["js", "mjs", "cjs"] {
            let with_ext = candidate.with_extension(ext);
            if with_ext.exists() { return with_ext; }
        }
    }
    if candidate.is_dir() {
        for index_file in ["index.js", "index.mjs", "index.cjs"] {
            let with_index = candidate.join(index_file);
            if with_index.exists() { return with_index; }
        }
    }
    candidate
}

fn default_auto_capture_path() -> PathBuf {
    if let Ok(explicit) = std::env::var("NSWINRT_AUTO_METADATA_PATH") {
        return PathBuf::from(explicit);
    }
    if let Ok(out_dir) = std::env::var("SBG_OUTPUT_DIR") {
        return PathBuf::from(out_dir).join("sbg_metadata.json");
    }
    PathBuf::from("sbg_output").join("sbg_metadata.json")
}

fn polled_event_to_v8<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    event: crate::worker_threads::PolledWorkerEvent,
) -> Option<v8::Local<'s, v8::Value>> {
    match event {
        crate::worker_threads::PolledWorkerEvent::Message(bytes) => {
            Runtime::deserialize_value(scope, &bytes)
        }
        crate::worker_threads::PolledWorkerEvent::Error(error) => {
            let obj = v8::Object::new(scope);
            if let Some(key) = v8::String::new(scope, "__workerError") {
                if let Some(val) = v8::String::new(scope, error.as_str()) {
                    obj.set(scope, key.into(), val.into());
                }
            }
            Some(obj.into())
        }
        crate::worker_threads::PolledWorkerEvent::Exited => {
            let obj = v8::Object::new(scope);
            if let Some(key) = v8::String::new(scope, "__workerExit") {
                obj.set(scope, key.into(), v8::Boolean::new(scope, true).into());
            }
            Some(obj.into())
        }
    }
}

// ── Global function handlers ──────────────────────────────────────────────────

pub(crate) fn handle_host_wait_for_async(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 1 {
        throw_js_error(scope, "__nsHostWaitForAsync expects a WinRT async object");
        return;
    }

    let op_value = args.get(0);
    let timeout_ms = if args.length() >= 2 {
        let t = args.get(1);
        if let Some(v) = t.integer_value(scope) {
            if v >= 0 { v as u64 } else { 0 }
        } else if let Some(v) = t.number_value(scope) {
            if v.is_finite() && v >= 0.0 { v as u64 } else { 0 }
        } else {
            0
        }
    } else {
        0
    };

    match try_get_async_status(scope, op_value) {
        Ok(0) => {}
        Ok(_) => { retval.set(op_value); return; }
        Err(msg) => { throw_js_error(scope, msg.as_str()); return; }
    }

    let deadline = if timeout_ms == 0 { None } else { Some(Instant::now() + Duration::from_millis(timeout_ms)) };
    let mut message = MSG::default();
    loop {
        if let Some(deadline) = deadline {
            if Instant::now() >= deadline {
                throw_js_error(scope, format!("Timed out waiting for WinRT async operation after {timeout_ms}ms").as_str());
                return;
            }
        }
        match try_get_async_status(scope, op_value) {
            Ok(0) => {
                while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.into() {
                    unsafe { let _ = TranslateMessage(&message); DispatchMessageW(&message); }
                }
                ASYNC_PUMP_HOOK.with(|hook| {
                    if let Ok(mut guard) = hook.try_borrow_mut() {
                        if let Some(f) = guard.as_mut() { f(); }
                    }
                });
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(_) => { retval.set(op_value); return; }
            Err(msg) => { throw_js_error(scope, msg.as_str()); return; }
        }
    }
}

pub(crate) fn handle_enqueue_microtask(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 1 {
        throw_js_error(scope, "__nsEnqueueMicrotask(callback) expects 1 argument");
        return;
    }
    let callback = match v8::Local::<v8::Function>::try_from(args.get(0)) {
        Ok(cb) => cb,
        Err(_) => {
            throw_js_error(scope, "__nsEnqueueMicrotask(callback) expects callback to be a function");
            return;
        }
    };
    scope.enqueue_microtask(callback);
    retval.set_undefined();
}

fn try_extract_pointer_from_value(
    scope: &mut v8::PinScope<'_, '_>,
    value: v8::Local<v8::Value>,
) -> Option<*mut std::ffi::c_void> {
    if value.is_null_or_undefined() { return Some(std::ptr::null_mut()); }
    if let Ok(external) = v8::Local::<v8::External>::try_from(value) { return Some(external.value()); }
    if !value.is_object() { return None; }
    let object = value.to_object(scope)?;
    if let Some(handle_key) = v8::String::new(scope, "handle") {
        if let Some(handle) = object.get(scope, handle_key.into()) {
            if let Ok(external) = v8::Local::<v8::External>::try_from(handle) { return Some(external.value()); }
            if handle.is_null_or_undefined() { return Some(std::ptr::null_mut()); }
        }
    }
    None
}

pub(crate) fn handle_pointer_key(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 1 {
        throw_js_error(scope, "__nsPointerKey expects a pointer-like value");
        return;
    }
    let pointer = match try_extract_pointer_from_value(scope, args.get(0)) {
        Some(p) => p,
        None => { throw_js_error(scope, "Unable to extract native pointer from value"); return; }
    };
    let key = format!("0x{:x}", pointer as usize);
    if let Some(value) = v8::String::new(scope, key.as_str()) {
        retval.set(value.into());
    } else {
        retval.set_undefined();
    }
}

pub(crate) fn handle_buffer_to_pointer(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 1 {
        throw_js_error(scope, "__nsBufferToPointer expects an ArrayBuffer or ArrayBufferView");
        return;
    }
    let value = args.get(0);
    let pointer = if let Ok(ab) = v8::Local::<v8::ArrayBuffer>::try_from(value) {
        ab.data().map_or(std::ptr::null_mut(), |d| d.as_ptr())
    } else if let Ok(view) = v8::Local::<v8::ArrayBufferView>::try_from(value) {
        let byte_offset = view.byte_offset();
        let Some(buf) = view.buffer(scope) else {
            throw_js_error(scope, "ArrayBufferView does not expose a backing buffer");
            return;
        };
        buf.data().map_or(std::ptr::null_mut(), |d| unsafe { d.as_ptr().add(byte_offset) })
    } else if value.is_null_or_undefined() {
        std::ptr::null_mut()
    } else {
        throw_js_error(scope, "__nsBufferToPointer expects an ArrayBuffer or ArrayBufferView");
        return;
    };

    if pointer.is_null() {
        retval.set_null();
    } else {
        retval.set(v8::External::new(scope, pointer).into());
    }
}

pub(crate) fn handle_proxy_write_text_file(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 2 {
        throw_js_error(scope, "__nsProxyWriteTextFile(path, content) expects 2 arguments");
        return;
    }
    let Some(path) = value_to_string(scope, args.get(0)) else {
        throw_js_error(scope, "Unable to convert path argument to string");
        return;
    };
    let Some(content) = value_to_string(scope, args.get(1)) else {
        throw_js_error(scope, "Unable to convert content argument to string");
        return;
    };
    let path_buf = PathBuf::from(&path);
    if let Some(parent) = path_buf.parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(err) = fs::create_dir_all(parent) {
                throw_js_error(scope, format!("Failed to create directory: {err}").as_str());
                return;
            }
        }
    }
    if let Err(err) = fs::write(&path_buf, content) {
        throw_js_error(scope, format!("Failed to write file: {err}").as_str());
        return;
    }
    retval.set_bool(true);
}

pub(crate) fn handle_proxy_compile_project(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 1 {
        throw_js_error(scope, "__nsProxyCompileProject(csprojPath[, configuration]) expects at least 1 argument");
        return;
    }
    let Some(project_path) = value_to_string(scope, args.get(0)) else {
        throw_js_error(scope, "Unable to convert csprojPath argument to string");
        return;
    };
    let configuration = if args.length() >= 2 {
        value_to_string(scope, args.get(1)).unwrap_or_else(|| "Debug".to_string())
    } else {
        "Debug".to_string()
    };
    let output = match Command::new("dotnet")
        .args(["build", &project_path, "-c", &configuration, "-v", "minimal"])
        .output()
    {
        Ok(o) => o,
        Err(err) => { throw_js_error(scope, format!("Failed to execute dotnet build: {err}").as_str()); return; }
    };

    let result = v8::Object::new(scope);
    let success = output.status.success();
    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    macro_rules! set_prop {
        ($key:literal, $val:expr) => {
            if let Some(k) = v8::String::new(scope, $key) {
                result.set(scope, k.into(), $val);
            }
        };
    }
    set_prop!("success", v8::Boolean::new(scope, success).into());
    set_prop!("exitCode", v8::Integer::new(scope, exit_code).into());
    if let Some(v) = v8::String::new(scope, &stdout) { set_prop!("stdout", v.into()); }
    if let Some(v) = v8::String::new(scope, &stderr) { set_prop!("stderr", v.into()); }
    retval.set(result.into());
}

pub(crate) fn handle_proxy_register_manifest(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 1 {
        throw_js_error(scope, "__nsProxyRegisterManifest(manifestJson) expects 1 argument");
        return;
    }
    let Some(manifest) = value_to_string(scope, args.get(0)) else {
        throw_js_error(scope, "Unable to convert manifest argument to string");
        return;
    };
    let mut manifests = proxy_manifests().lock();
    manifests.push(manifest);
    let index = manifests.len() as i32 - 1;
    retval.set(v8::Integer::new(scope, index).into());
}

pub(crate) fn handle_proxy_auto_capture(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 1 {
        throw_js_error(scope, "__nsProxyAutoCapture(metadataJson) expects 1 argument");
        return;
    }
    let Some(metadata_json) = value_to_string(scope, args.get(0)) else {
        throw_js_error(scope, "Unable to convert metadata argument to string");
        return;
    };
    let path_buf = default_auto_capture_path();
    if let Some(parent) = path_buf.parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(err) = fs::create_dir_all(parent) {
                throw_js_error(scope, format!("Failed to create metadata directory: {err}").as_str());
                return;
            }
        }
    }
    let normalized = match serde_json::from_str::<Vec<RuntimeExtensionMetadata>>(metadata_json.as_str()) {
        Ok(extensions) => {
            let mut registry = RuntimeExtensionRegistry::new();
            for ext in extensions.iter().cloned() { registry.register(ext); }
            match serde_json::to_string_pretty(&extensions) {
                Ok(json) => json,
                Err(err) => { throw_js_error(scope, format!("Failed to normalize captured metadata: {err}").as_str()); return; }
            }
        }
        Err(_) => metadata_json,
    };
    if let Err(err) = fs::write(&path_buf, normalized) {
        throw_js_error(scope, format!("Failed to write captured metadata: {err}").as_str());
        return;
    }
    if let Some(path) = path_buf.to_str().and_then(|p| v8::String::new(scope, p)) {
        retval.set(path.into());
    } else {
        retval.set_bool(true);
    }
}

pub(crate) fn handle_read_text_file(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 1 {
        throw_js_error(scope, "__nsReadTextFile(path) expects 1 argument");
        return;
    }
    let Some(path) = value_to_string(scope, args.get(0)) else {
        throw_js_error(scope, "Unable to convert path argument to string");
        return;
    };
    match fs::read_to_string(Path::new(path.as_str())) {
        Ok(content) => {
            if let Some(value) = v8::String::new(scope, content.as_str()) {
                retval.set(value.into());
            } else {
                retval.set_null();
            }
        }
        Err(err) => throw_js_error(scope, format!("Failed to read module file: {err}").as_str()),
    }
}

pub(crate) fn handle_livesync_copy_file(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 2 {
        throw_js_error(scope, "__nsLiveSyncCopyFile(sourcePath, destPath) expects 2 arguments");
        return;
    }
    let Some(source_path) = value_to_string(scope, args.get(0)) else {
        throw_js_error(scope, "Unable to convert sourcePath argument to string");
        return;
    };
    let Some(dest_path) = value_to_string(scope, args.get(1)) else {
        throw_js_error(scope, "Unable to convert destPath argument to string");
        return;
    };
    if let Err(err) = crate::livesync::copy_file(source_path.as_str(), dest_path.as_str()) {
        throw_js_error(scope, err.as_str());
        return;
    }
    retval.set_bool(true);
}

pub(crate) fn handle_resolve_module_path(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 1 {
        throw_js_error(scope, "__nsResolveModulePath(specifier[, parentPath, appRoot]) expects at least 1 argument");
        return;
    }
    let Some(specifier) = value_to_string(scope, args.get(0)) else {
        throw_js_error(scope, "Unable to convert module specifier to string");
        return;
    };
    let parent_path = if args.length() >= 2 { value_to_string(scope, args.get(1)) } else { None };
    let app_root = if args.length() >= 3 { value_to_string(scope, args.get(2)).unwrap_or_default() } else { String::new() };

    let mut candidate = if specifier.starts_with("./") || specifier.starts_with("../") {
        let parent = parent_path
            .map(|v| normalize_js_path(v.as_str()))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let base = if parent.is_file() { parent.parent().map(Path::to_path_buf).unwrap_or(parent) } else { parent };
        base.join(&specifier)
    } else {
        let direct = normalize_js_path(specifier.as_str());
        if direct.is_absolute() {
            direct
        } else {
            let app_base = if app_root.is_empty() {
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
            } else {
                let lower = PathBuf::from(&app_root).join("app");
                if lower.exists() {
                    lower
                } else {
                    PathBuf::from(&app_root).join("App")
                }
            };
            app_base.join(direct)
        }
    };

    candidate = try_resolve_with_known_extensions(candidate);
    let resolved = candidate.canonicalize().unwrap_or(candidate);
    if let Some(value) = resolved.to_str().and_then(|p| v8::String::new(scope, p)) {
        retval.set(value.into());
    } else {
        retval.set_null();
    }
}

pub(crate) fn handle_proxy_list_manifests(
    scope: &mut v8::PinScope<'_, '_>,
    _args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let manifests = proxy_manifests().lock();
    let array = v8::Array::new(scope, manifests.len() as i32);
    for (i, entry) in manifests.iter().enumerate() {
        if let Some(value) = v8::String::new(scope, entry.as_str()) {
            array.set_index(scope, i as u32, value.into());
        }
    }
    retval.set(array.into());
}

pub(crate) fn handle_describe_winrt_type(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 1 {
        throw_js_error(scope, "__nsDescribeWinRTType(typeName) expects 1 argument");
        return;
    }
    let Some(type_name) = value_to_string(scope, args.get(0)) else {
        throw_js_error(scope, "Unable to convert typeName argument to string");
        return;
    };
    let Some(descriptor) = build_runtime_type_descriptor(type_name.as_str()) else {
        retval.set_null();
        return;
    };
    match serde_json::to_string(&descriptor) {
        Ok(json) => {
            if let Some(value) = v8::String::new(scope, json.as_str()) {
                retval.set(value.into());
            } else {
                retval.set_null();
            }
        }
        Err(error) => throw_js_error(scope, format!("Failed to serialize WinRT descriptor: {error}").as_str()),
    }
}

pub(crate) fn handle_worker_create_threaded(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 3 {
        throw_js_error(scope, "__nsWorkerCreateThreaded(source, filename, appRoot) expects 3 arguments");
        return;
    }
    let Some(source) = value_to_string(scope, args.get(0)) else {
        throw_js_error(scope, "Unable to convert worker source to string"); return;
    };
    let Some(filename) = value_to_string(scope, args.get(1)) else {
        throw_js_error(scope, "Unable to convert worker filename to string"); return;
    };
    let Some(app_root) = value_to_string(scope, args.get(2)) else {
        throw_js_error(scope, "Unable to convert appRoot to string"); return;
    };
    match crate::worker_threads::create_worker(app_root, source, filename) {
        Ok(worker_id) => retval.set_double(worker_id as f64),
        Err(err) => throw_js_error(scope, err.as_str()),
    }
}

pub(crate) fn handle_worker_post_message(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    _retval: v8::ReturnValue,
) {
    if args.length() < 2 {
        throw_js_error(scope, "__nsWorkerPostMessage(workerId, value) expects 2 arguments");
        return;
    }
    let worker_id = args.get(0).number_value(scope).unwrap_or(-1.0);
    if worker_id < 0.0 { throw_js_error(scope, "Invalid worker id"); return; }
    let value = args.get(1);
    let Some(bytes) = Runtime::serialize_value(scope, value) else {
        throw_js_error(scope, "DataCloneError: value could not be cloned.");
        return;
    };
    if let Err(err) = crate::worker_threads::post_message(worker_id as u64, bytes) {
        throw_js_error(scope, err.as_str());
    }
}

pub(crate) fn handle_worker_poll_messages(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 1 {
        throw_js_error(scope, "__nsWorkerPollMessages(workerId) expects 1 argument");
        return;
    }
    let worker_id = args.get(0).number_value(scope).unwrap_or(-1.0);
    if worker_id < 0.0 { throw_js_error(scope, "Invalid worker id"); return; }
    let events = match crate::worker_threads::poll_events(worker_id as u64) {
        Ok(e) => e,
        Err(err) => { throw_js_error(scope, err.as_str()); return; }
    };
    let array = v8::Array::new(scope, events.len() as i32);
    for (index, event) in events.into_iter().enumerate() {
        if let Some(value) = polled_event_to_v8(scope, event) {
            array.set_index(scope, index as u32, value);
        }
    }
    retval.set(array.into());
}

pub(crate) fn handle_worker_terminate(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    _retval: v8::ReturnValue,
) {
    if args.length() < 1 {
        throw_js_error(scope, "__nsWorkerTerminate(workerId) expects 1 argument");
        return;
    }
    let worker_id = args.get(0).number_value(scope).unwrap_or(-1.0);
    if worker_id < 0.0 { throw_js_error(scope, "Invalid worker id"); return; }
    if let Err(err) = crate::worker_threads::terminate_worker(worker_id as u64) {
        throw_js_error(scope, err.as_str());
    }
}

pub(crate) fn handle_worker_poll_messages_blocking(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 2 {
        throw_js_error(scope, "__nsWorkerPollMessagesBlocking(workerId, timeoutMs) expects 2 arguments");
        return;
    }
    let worker_id = args.get(0).number_value(scope).unwrap_or(-1.0);
    if worker_id < 0.0 { throw_js_error(scope, "Invalid worker id"); return; }
    let timeout_ms = args.get(1).number_value(scope).unwrap_or(0.0);
    let timeout_ms = if timeout_ms.is_sign_negative() { 0_u64 } else { timeout_ms as u64 };
    let events = match crate::worker_threads::poll_events_blocking(worker_id as u64, timeout_ms) {
        Ok(e) => e,
        Err(err) => { throw_js_error(scope, err.as_str()); return; }
    };
    let array = v8::Array::new(scope, events.len() as i32);
    for (index, event) in events.into_iter().enumerate() {
        if let Some(value) = polled_event_to_v8(scope, event) {
            array.set_index(scope, index as u32, value);
        }
    }
    retval.set(array.into());
}

// ── Runtime bootstrap JavaScript ──────────────────────────────────────────────

/// Installed into every context. Provides NSWinRT, module loader, async helpers,
/// proxy extension infrastructure, and the `Function.prototype.extend` NativeScript API.
const HELPER_SOURCE: &str = r#"
        (function () {
            if (typeof globalThis.queueMicrotask !== 'function') {
                globalThis.queueMicrotask = function (callback) {
                    if (typeof callback !== 'function') {
                        throw new TypeError('queueMicrotask callback must be a function');
                    }

                    if (typeof globalThis.__nsEnqueueMicrotask === 'function') {
                        globalThis.__nsEnqueueMicrotask(callback);
                        return;
                    }

                    Promise.resolve().then(callback).catch(function (err) {
                        if (typeof globalThis.setTimeout === 'function') {
                            globalThis.setTimeout(function () {
                                throw err;
                            }, 0);
                        } else {
                            throw err;
                        }
                    });
                };
            }

            var defaultTimeoutMs = 0;
            var statusEnum =
                (globalThis.Windows &&
                    globalThis.Windows.Foundation &&
                    globalThis.Windows.Foundation.AsyncStatus) ||
                { Started: 0, Completed: 1, Canceled: 2, Error: 3 };

            function normalizeTimeoutMs(options) {
                if (typeof options === 'number' && Number.isFinite(options) && options >= 0) {
                    return Math.floor(options);
                }

                if (options && typeof options === 'object') {
                    if (typeof options.timeoutMs === 'number' && Number.isFinite(options.timeoutMs) && options.timeoutMs >= 0) {
                        return Math.floor(options.timeoutMs);
                    }
                }

                return defaultTimeoutMs;
            }

            function normalizeStatus(status) {
                if (status == null) {
                    return Number.NaN;
                }
                if (typeof status === 'number') {
                    return status;
                }
                if (typeof status === 'string') {
                    var lower = status.toLowerCase();
                    if (lower === 'started') return 0;
                    if (lower === 'completed') return 1;
                    if (lower === 'canceled' || lower === 'cancelled') return 2;
                    if (lower === 'error') return 3;
                }
                if (typeof status.valueOf === 'function') {
                    var value = status.valueOf();
                    if (typeof value === 'number') {
                        return value;
                    }
                    if (typeof value === 'string') {
                        return normalizeStatus(value);
                    }
                }
                var coerced = Number(status);
                return Number.isNaN(coerced) ? Number.NaN : coerced;
            }

            function setDefaultTimeoutMs(timeoutMs) {
                if (typeof timeoutMs !== 'number' || !Number.isFinite(timeoutMs) || timeoutMs < 0) {
                    throw new Error('NSWinRT.setDefaultTimeoutMs(timeoutMs) expects a finite number >= 0');
                }
                defaultTimeoutMs = Math.floor(timeoutMs);
                return defaultTimeoutMs;
            }

            function wait(op, options) {
                if (typeof globalThis.__nsHostWaitForAsync === 'function') {
                    globalThis.__nsHostWaitForAsync(op, normalizeTimeoutMs(options));
                }
                return op;
            }

            function getStatus(op) {
                return normalizeStatus(op && op.Status);
            }

            function getResults(op) {
                if (op && typeof op.GetResults === 'function') {
                    return op.GetResults();
                }
                return undefined;
            }

            function toPromise(op, options) {
                if (op == null) {
                    return Promise.resolve(op);
                }

                if (typeof op.then === 'function') {
                    return op;
                }

                return new Promise(function (resolve, reject) {
                    function settleFromStatus(overrideStatus) {
                        try {
                            var status = normalizeStatus(
                                overrideStatus !== undefined ? overrideStatus : (op && op.Status)
                            );

                            if (status === statusEnum.Completed || status === 1) {
                                resolve(getResults(op));
                                return;
                            }
                            if (status === statusEnum.Canceled || status === 2) {
                                reject(new Error('WinRT async operation was canceled'));
                                return;
                            }
                            if (status === statusEnum.Error || status === 3) {
                                reject((op && op.ErrorCode) || new Error('WinRT async operation failed'));
                                return;
                            }

                            reject(
                                new Error('WinRT async operation is still pending and no completion callback was attached')
                            );
                        } catch (err) {
                            reject(err);
                        }
                    }

                    try {
                        if (op && 'Completed' in op) {
                            op.Completed = function (asyncInfo, asyncStatus) {
                                settleFromStatus(asyncStatus);
                            };
                            return;
                        }
                    } catch (_) {
                        // Fall through and attempt status-based settlement.
                    }

                    try {
                        wait(op, options);
                        settleFromStatus();
                    } catch (err) {
                        reject(err);
                    }
                });
            }

            function onCompleted(op, callback, options) {
                if (typeof callback !== 'function') {
                    throw new Error('NSWinRT.onCompleted(op, callback[, options]) expects callback to be a function');
                }

                try {
                    if (op && 'Completed' in op) {
                        op.Completed = function (asyncInfo, asyncStatus) {
                            callback(asyncInfo || op, normalizeStatus(asyncStatus));
                        };
                        return op;
                    }
                } catch (_) {
                    // Fall through to polling fallback.
                }

                Promise.resolve().then(function () {
                    wait(op, options);
                    callback(op, getStatus(op));
                });

                return op;
            }

            globalThis.__nsWinRTToPromise = toPromise;
            globalThis.NSWinRT = globalThis.NSWinRT || {};
            globalThis.NSWinRT.toPromise = toPromise;
            globalThis.NSWinRT.wait = wait;
            globalThis.NSWinRT.getStatus = getStatus;
            globalThis.NSWinRT.getResults = getResults;
            globalThis.NSWinRT.onCompleted = onCompleted;
            globalThis.NSWinRT.setDefaultTimeoutMs = setDefaultTimeoutMs;

            function Pointer(handle) {
                this.handle = handle == null ? null : handle;
            }

            Pointer.prototype.isNull = function () {
                return this.handle == null;
            };

            Pointer.prototype.unwrap = function () {
                return this.handle;
            };

            Pointer.prototype.toString = function () {
                return this.isNull() ? '[Pointer null]' : '[Pointer external]';
            };

            function asPointer(value) {
                return value instanceof Pointer ? value : new Pointer(value);
            }

            function handleOf(value) {
                return value instanceof Pointer ? value.handle : value;
            }

            function asBufferSource(value) {
                if (value == null) {
                    return value;
                }

                if (value instanceof ArrayBuffer || ArrayBuffer.isView(value)) {
                    return value;
                }

                throw new Error('NSWinRT.interop.asBufferSource(value) expects ArrayBuffer or ArrayBufferView');
            }

            function asUint8View(value) {
                var source = asBufferSource(value);
                if (source == null) {
                    return new Uint8Array(0);
                }

                if (source instanceof ArrayBuffer) {
                    return new Uint8Array(source);
                }

                return new Uint8Array(source.buffer, source.byteOffset, source.byteLength);
            }

            function asDataView(value) {
                var source = asBufferSource(value);
                if (source == null) {
                    return new DataView(new ArrayBuffer(0));
                }

                if (source instanceof ArrayBuffer) {
                    return new DataView(source);
                }

                return new DataView(source.buffer, source.byteOffset, source.byteLength);
            }

            // WinRT DateTime stores 100ns ticks since 1601-01-01T00:00:00Z.
            var winRtUnixEpochOffsetTicks = 116444736000000000n;

            function toWinRTDateTimeTicks(input) {
                var ms;
                if (input instanceof Date) {
                    ms = input.getTime();
                } else if (typeof input === 'number') {
                    ms = input;
                } else {
                    throw new Error('NSWinRT.interop.toWinRTDateTimeTicks expects Date or millisecond timestamp');
                }

                if (!Number.isFinite(ms)) {
                    throw new Error('Invalid Date/time value for WinRT conversion');
                }

                return BigInt(Math.trunc(ms)) * 10000n + winRtUnixEpochOffsetTicks;
            }

            function fromWinRTDateTimeTicks(value) {
                if (value == null) {
                    return new Date(Number.NaN);
                }

                var ticks = typeof value === 'bigint' ? value : BigInt(Math.trunc(Number(value)));
                var unixTicks = ticks - winRtUnixEpochOffsetTicks;
                var ms = Number(unixTicks / 10000n);
                return new Date(ms);
            }

            var pointerBufferRegistry = new Map();

            function pointerKey(value) {
                if (typeof globalThis.__nsPointerKey !== 'function') {
                    return null;
                }
                return globalThis.__nsPointerKey(value);
            }

            function pointerFromBuffer(value) {
                var source = asBufferSource(value);
                if (source == null || typeof globalThis.__nsBufferToPointer !== 'function') {
                    return null;
                }
                return globalThis.__nsBufferToPointer(source);
            }

            function trackBufferSource(value) {
                var source = asBufferSource(value);
                if (source == null) {
                    return null;
                }

                var pointer = pointerFromBuffer(source);
                var key = pointerKey(pointer);
                if (key != null) {
                    pointerBufferRegistry.set(String(key), source);
                }
                return pointer;
            }

            function resolveTrackedBuffer(pointerLike) {
                var key = pointerKey(pointerLike);
                if (key == null) {
                    return undefined;
                }
                return pointerBufferRegistry.get(String(key));
            }

            globalThis.NSWinRT.interop = {
                Pointer: Pointer,
                pointer: asPointer,
                isPointer: function (value) {
                    return value instanceof Pointer;
                },
                handleOf: handleOf,
                asBufferSource: asBufferSource,
                asUint8View: asUint8View,
                asDataView: asDataView,
                toWinRTDateTimeTicks: toWinRTDateTimeTicks,
                fromWinRTDateTimeTicks: fromWinRTDateTimeTicks,
                pointerKey: pointerKey,
                pointerFromBuffer: pointerFromBuffer,
                trackBufferSource: trackBufferSource,
                resolveTrackedBuffer: resolveTrackedBuffer,
                byteLengthOf: function (value) {
                    var buffer = asBufferSource(value);
                    if (buffer == null) {
                        return 0;
                    }
                    return typeof buffer.byteLength === 'number' ? buffer.byteLength : 0;
                },
                byteOffsetOf: function (value) {
                    if (ArrayBuffer.isView(value)) {
                        return value.byteOffset;
                    }
                    return 0;
                },
                readU8: function (value, offset) {
                    return asDataView(value).getUint8(offset >>> 0);
                },
                writeU8: function (value, offset, input) {
                    asDataView(value).setUint8(offset >>> 0, input >>> 0);
                    return value;
                },
                readI32: function (value, offset, littleEndian) {
                    return asDataView(value).getInt32(offset >>> 0, littleEndian !== false);
                },
                writeI32: function (value, offset, input, littleEndian) {
                    asDataView(value).setInt32(offset >>> 0, input | 0, littleEndian !== false);
                    return value;
                },
                readF32: function (value, offset, littleEndian) {
                    return asDataView(value).getFloat32(offset >>> 0, littleEndian !== false);
                },
                writeF32: function (value, offset, input, littleEndian) {
                    asDataView(value).setFloat32(offset >>> 0, +input, littleEndian !== false);
                    return value;
                },
                readF64: function (value, offset, littleEndian) {
                    return asDataView(value).getFloat64(offset >>> 0, littleEndian !== false);
                },
                writeF64: function (value, offset, input, littleEndian) {
                    asDataView(value).setFloat64(offset >>> 0, +input, littleEndian !== false);
                    return value;
                },
            };

            var proxyExtensions = [];
            var proxyInstances = new Map();
            var nextProxyId = 1;

            function ctorName(ctor) {
                return (ctor && (ctor.__typeName__ || ctor.name)) || 'Object';
            }

            var typeDescriptorCache = Object.create(null);

            function describeWinRTType(typeName) {
                if (!typeName || typeof globalThis.__nsDescribeWinRTType !== 'function') {
                    return null;
                }
                if (Object.prototype.hasOwnProperty.call(typeDescriptorCache, typeName)) {
                    return typeDescriptorCache[typeName];
                }
                try {
                    var raw = globalThis.__nsDescribeWinRTType(typeName);
                    typeDescriptorCache[typeName] = raw ? JSON.parse(raw) : null;
                } catch (_) {
                    typeDescriptorCache[typeName] = null;
                }
                return typeDescriptorCache[typeName];
            }

            function buildFallbackParameterMetadata(fn) {
                var params = [];
                var count = typeof fn === 'function' && Number.isFinite(fn.length) ? fn.length : 0;
                for (var i = 0; i < count; i++) {
                    params.push({ name: 'arg' + i, type: 'Object' });
                }
                return params;
            }

            function normalizeMethodMetadata(name, value, descriptors) {
                for (var i = 0; i < descriptors.length; i++) {
                    var descriptor = descriptors[i];
                    if (!descriptor || !Array.isArray(descriptor.methods)) {
                        continue;
                    }
                    for (var j = 0; j < descriptor.methods.length; j++) {
                        var method = descriptor.methods[j];
                        if (method && method.name === name) {
                            return {
                                name: method.name,
                                returnType: method.returnType || method.return_type || 'Void',
                                parameters: Array.isArray(method.parameters) ? method.parameters : [],
                            };
                        }
                    }
                }

                return {
                    name: name,
                    returnType: name === 'init' ? 'Void' : 'Object',
                    parameters: buildFallbackParameterMetadata(value),
                };
            }

            function normalizePropertyMetadata(name, descriptors) {
                for (var i = 0; i < descriptors.length; i++) {
                    var descriptor = descriptors[i];
                    if (!descriptor || !Array.isArray(descriptor.properties)) {
                        continue;
                    }
                    for (var j = 0; j < descriptor.properties.length; j++) {
                        var property = descriptor.properties[j];
                        if (property && property.name === name) {
                            return {
                                name: property.name,
                                propType: property.propType || property.prop_type || 'Object',
                                readable: property.readable !== false,
                                writable: property.writable !== false,
                            };
                        }
                    }
                }

                return {
                    name: name,
                    propType: 'Object',
                    readable: true,
                    writable: true,
                };
            }

            function collectProxyMethods(overrides, descriptors) {
                var methods = [];
                for (var key in overrides) {
                    if (!Object.prototype.hasOwnProperty.call(overrides, key)) {
                        continue;
                    }
                    if (key === 'interfaces') {
                        continue;
                    }
                    if (typeof overrides[key] === 'function') {
                        methods.push(normalizeMethodMetadata(key, overrides[key], descriptors));
                    }
                }
                methods.sort(function (left, right) {
                    return String(left && left.name || '').localeCompare(String(right && right.name || ''));
                });
                return methods;
            }

            function collectProxyProperties(overrides, descriptors) {
                var props = [];
                for (var key in overrides) {
                    if (!Object.prototype.hasOwnProperty.call(overrides, key)) {
                        continue;
                    }
                    if (key === 'interfaces') {
                        continue;
                    }
                    if (typeof overrides[key] !== 'function') {
                        props.push(normalizePropertyMetadata(key, descriptors));
                    }
                }
                props.sort(function (left, right) {
                    return String(left && left.name || '').localeCompare(String(right && right.name || ''));
                });
                return props;
            }

            function safeIdentifier(name) {
                return String(name || '')
                    .replace(/[^A-Za-z0-9_]/g, '_')
                    .replace(/^([^A-Za-z_])/, '_$1') || 'ProxyType';
            }

            function autoProxyTypeName(baseCtor) {
                var baseType = ctorName(baseCtor) || 'Object';
                var baseShort = safeIdentifier(baseType.split('.').pop());
                var baseNamespace = 'windows';
                var namespaceIndex = baseType.lastIndexOf('.');
                if (namespaceIndex >= 0) {
                    baseNamespace = baseType
                        .slice(0, namespaceIndex)
                        .toLowerCase()
                        .replace(/[^a-z0-9_.]/g, '_');
                }
                return 'com.tns.gen.winrt.' + baseNamespace + '.' + baseShort + '_AutoProxy_' + (proxyExtensions.length + 1);
            }

            function renderProxyCSharp(meta) {
                var typeName = meta.typeName || ('GeneratedProxy' + (proxyExtensions.length + 1));
                var safeTypeName = safeIdentifier(typeName.split('.').pop());
                var baseType = meta.baseType || 'object';
                var methodStubs = '';
                for (var i = 0; i < meta.methods.length; i++) {
                    var methodMeta = meta.methods[i];
                    var methodName = safeIdentifier((methodMeta && methodMeta.name) || methodMeta);
                    methodStubs +=
                        '    public object __ns_' + methodName + '(params object[] args)\\n' +
                        '    {\\n' +
                        '        return ProxyDispatcher.Invoke(this.__proxyId, "' + methodName + '", args);\\n' +
                        '    }\\n\\n';
                }

                return (
                    'using System;\\n\\n' +
                    'namespace NativeScriptGeneratedProxies\\n' +
                    '{\\n' +
                    '    public static class ProxyDispatcher\\n' +
                    '    {\\n' +
                    '        public static Func<int, string, object[], object> JsInvoke;\\n' +
                    '        public static object Invoke(int id, string method, object[] args)\\n' +
                    '        {\\n' +
                    '            var cb = JsInvoke;\\n' +
                    '            if (cb == null) throw new InvalidOperationException("JsInvoke callback is not registered.");\\n' +
                    '            return cb(id, method, args);\\n' +
                    '        }\\n' +
                    '    }\\n\\n' +
                    '    public class ' + safeTypeName + ' : ' + baseType + '\\n' +
                    '    {\\n' +
                    '        private readonly int __proxyId;\\n\\n' +
                    '        public ' + safeTypeName + '(int proxyId)\\n' +
                    '        {\\n' +
                    '            this.__proxyId = proxyId;\\n' +
                    '        }\\n\\n' +
                    methodStubs +
                    '    }\\n' +
                    '}\\n'
                );
            }

            function renderProxyCsproj(meta) {
                var asmName = safeIdentifier((meta.typeName || 'GeneratedProxy').split('.').pop());
                return (
                    '<Project Sdk="Microsoft.NET.Sdk">\\n' +
                    '  <PropertyGroup>\\n' +
                    '    <TargetFramework>net8.0-windows10.0.19041.0</TargetFramework>\\n' +
                    '    <AssemblyName>' + asmName + '</AssemblyName>\\n' +
                    '    <RootNamespace>NativeScriptGeneratedProxies</RootNamespace>\\n' +
                    '    <ImplicitUsings>enable</ImplicitUsings>\\n' +
                    '    <Nullable>disable</Nullable>\\n' +
                    '    <LangVersion>latest</LangVersion>\\n' +
                    '  </PropertyGroup>\\n' +
                    '</Project>\\n'
                );
            }

            function buildProxyMetadata(baseCtor, typeName, overrides, Extended) {
                var baseType = ctorName(baseCtor);
                var interfaceNames = Array.isArray(overrides.interfaces)
                    ? overrides.interfaces.map(function (iface) { return ctorName(iface); })
                    : [];
                var descriptors = [describeWinRTType(baseType)];
                for (var i = 0; i < interfaceNames.length; i++) {
                    descriptors.push(describeWinRTType(interfaceNames[i]));
                }
                var namespace = '';
                var className = typeName || '';
                if (typeName) {
                    var splitIndex = typeName.lastIndexOf('.');
                    if (splitIndex >= 0) {
                        namespace = typeName.slice(0, splitIndex);
                        className = typeName.slice(splitIndex + 1);
                    }
                }
                var meta = {
                    kind: 'windows-proxy',
                    typeName: typeName || '',
                    className: className || safeIdentifier((typeName || baseType || 'GeneratedProxy').split('.').pop()),
                    namespace: namespace || null,
                    baseType: baseType,
                    baseClass: baseType,
                    interfaces: interfaceNames,
                    methods: collectProxyMethods(overrides, descriptors.filter(Boolean)),
                    properties: collectProxyProperties(overrides, descriptors.filter(Boolean)),
                    isAutoGeneratedName: !typeName,
                    registeredAt: new Date().toISOString(),
                    registered: false,
                    generated: null,
                };
                try {
                    Object.defineProperty(Extended, '__proxyMetadata__', {
                        value: meta,
                        writable: true,
                        configurable: true,
                        enumerable: false,
                    });
                } catch (_) {
                    Extended.__proxyMetadata__ = meta;
                }
                proxyExtensions.push(meta);
                if (typeof globalThis.__nsProxyAutoCapture === 'function') {
                    try {
                        globalThis.__nsProxyAutoCapture(JSON.stringify(proxyExtensions));
                    } catch (_) {
                        // Capture is best-effort; runtime behavior should remain unaffected.
                    }
                }
                return meta;
            }

            function ensureProxyInstance(instance, overrides, ctor) {
                if (!instance || typeof instance !== 'object') {
                    return -1;
                }

                var proxyId = instance.__proxyId;
                if (typeof proxyId !== 'number' || !Number.isFinite(proxyId)) {
                    proxyId = nextProxyId++;
                    try {
                        Object.defineProperty(instance, '__proxyId', {
                            value: proxyId,
                            writable: false,
                            configurable: true,
                            enumerable: false,
                        });
                    } catch (_) {
                        instance.__proxyId = proxyId;
                    }
                }

                proxyInstances.set(proxyId, {
                    instance: instance,
                    overrides: overrides,
                    constructor: ctor,
                });

                return proxyId;
            }

            function makeExtendedConstructor(baseCtor, nameOrOverrides, maybeOverrides) {
                var hasName = typeof nameOrOverrides === 'string';
                var explicitTypeName = hasName ? nameOrOverrides : '';
                var typeName = explicitTypeName || autoProxyTypeName(baseCtor);
                var overrides = hasName ? maybeOverrides : nameOrOverrides;
                if (!overrides || typeof overrides !== 'object') {
                    overrides = {};
                }

                function Extended() {
                    var instance;
                    var args = Array.prototype.slice.call(arguments);
                    try {
                        instance = Reflect.construct(baseCtor, args);
                    } catch (_) {
                        instance = {};
                    }

                    for (var key in overrides) {
                        if (key === 'interfaces') {
                            continue;
                        }
                        var value = overrides[key];
                        if (typeof value === 'function') {
                            try {
                                Object.defineProperty(instance, key, {
                                    value: value,
                                    writable: true,
                                    configurable: true,
                                    enumerable: true,
                                });
                            } catch (_) {
                                instance[key] = value;
                            }
                        } else {
                            instance[key] = value;
                        }
                    }

                    if (typeof overrides.init === 'function') {
                        try {
                            var initResult = overrides.init.apply(instance, args);
                            if (initResult && typeof initResult === 'object') {
                                instance = initResult;
                            }
                        } catch (_) {
                            // Keep constructor resilient; init errors should not crash runtime.
                        }
                    }

                    if (Array.isArray(overrides.interfaces)) {
                        try {
                            Object.defineProperty(instance, '__interfaces__', {
                                value: overrides.interfaces.slice(),
                                writable: false,
                                configurable: true,
                                enumerable: false,
                            });
                        } catch (_) {
                            instance.__interfaces__ = overrides.interfaces.slice();
                        }
                    }

                    ensureProxyInstance(instance, overrides, Extended);

                    return instance;
                }

                Extended.prototype = Object.create((baseCtor && baseCtor.prototype) || Object.prototype);
                Extended.prototype.constructor = Extended;

                for (var protoKey in overrides) {
                    if (protoKey === 'interfaces') {
                        continue;
                    }
                    Extended.prototype[protoKey] = overrides[protoKey];
                }

                if (typeName) {
                    try {
                        Object.defineProperty(Extended, 'name', {
                            value: typeName,
                            configurable: true,
                        });
                    } catch (_) {
                        // Non-critical metadata assignment.
                    }
                }

                Extended.__typeName__ = typeName || ctorName(baseCtor);
                var metadata = buildProxyMetadata(baseCtor, typeName, overrides, Extended);

                Extended.extend = function (nextNameOrOverrides, nextMaybeOverrides) {
                    return makeExtendedConstructor(Extended, nextNameOrOverrides, nextMaybeOverrides);
                };

                try {
                    Object.defineProperty(Extended, 'emitProxy', {
                        value: function (outDir) {
                            return NSWinRT.proxy.emit(metadata, outDir);
                        },
                        writable: true,
                        configurable: true,
                        enumerable: false,
                    });
                } catch (_) {
                    Extended.emitProxy = function (outDir) {
                        return NSWinRT.proxy.emit(metadata, outDir);
                    };
                }

                return Extended;
            }

            if (typeof Function.prototype.extend !== 'function') {
                Object.defineProperty(Function.prototype, 'extend', {
                    value: function (nameOrOverrides, maybeOverrides) {
                        return makeExtendedConstructor(this, nameOrOverrides, maybeOverrides);
                    },
                    writable: true,
                    configurable: true,
                    enumerable: false,
                });
            }

            if (typeof Object.extend !== 'function') {
                Object.defineProperty(Object, 'extend', {
                    value: function (nameOrOverrides, maybeOverrides) {
                        return makeExtendedConstructor(Object, nameOrOverrides, maybeOverrides);
                    },
                    writable: true,
                    configurable: true,
                    enumerable: false,
                });
            }

            function defaultProxyOutDir(meta) {
                var typeName = (meta && meta.typeName) ? meta.typeName : 'GeneratedProxy';
                var safe = safeIdentifier(typeName.split('.').pop());
                return './generated-proxies/' + safe;
            }

            function emitProxy(meta, outDir) {
                if (!meta || typeof meta !== 'object') {
                    throw new Error('NSWinRT.proxy.emit(meta[, outDir]) expects a proxy metadata object');
                }
                if (typeof globalThis.__nsProxyWriteTextFile !== 'function') {
                    throw new Error('Host proxy file emitter is not available');
                }

                var dir = outDir || defaultProxyOutDir(meta);
                var csprojPath = dir + '/Proxy.csproj';
                var csPath = dir + '/Proxy.g.cs';
                var csproj = renderProxyCsproj(meta);
                var source = renderProxyCSharp(meta);

                globalThis.__nsProxyWriteTextFile(csprojPath, csproj);
                globalThis.__nsProxyWriteTextFile(csPath, source);

                meta.generated = {
                    dir: dir,
                    csprojPath: csprojPath,
                    csPath: csPath,
                };

                return meta.generated;
            }

            function compileProxy(meta, outDir, configuration) {
                if (typeof globalThis.__nsProxyCompileProject !== 'function') {
                    throw new Error('Host proxy compiler is not available');
                }
                var generated = emitProxy(meta, outDir);
                var result = globalThis.__nsProxyCompileProject(generated.csprojPath, configuration || 'Debug');
                generated.build = result;
                return generated;
            }

            function registerProxy(meta, outDir, configuration) {
                var generated = compileProxy(meta, outDir, configuration);
                var manifest = {
                    kind: 'windows-proxy',
                    typeName: meta.typeName,
                    baseType: meta.baseType,
                    interfaces: meta.interfaces,
                    methods: meta.methods,
                    properties: meta.properties,
                    generated: generated,
                    registration: {
                        hostCanLoadAssemblies: false,
                        note: 'Assembly build succeeded, but runtime CLR proxy activation is not wired yet. Dynamic JS fallback remains active.',
                    },
                };
                if (typeof globalThis.__nsProxyRegisterManifest === 'function') {
                    globalThis.__nsProxyRegisterManifest(JSON.stringify(manifest));
                }
                meta.registered = true;
                meta.registration = manifest.registration;
                return manifest;
            }

            function invokeProxyById(proxyId, methodName, argsArray) {
                var entry = proxyInstances.get(proxyId);
                if (!entry) {
                    throw new Error('Proxy instance not found for id ' + proxyId);
                }

                var target = entry.instance;
                var method = target && target[methodName];
                if (typeof method !== 'function') {
                    throw new Error('Proxy method "' + methodName + '" is not defined on proxy id ' + proxyId);
                }

                return method.apply(target, Array.isArray(argsArray) ? argsArray : []);
            }

            globalThis.__nsInvokeProxyJs = invokeProxyById;

            globalThis.NSWinRT.proxy = {
                getExtensions: function () {
                    return proxyExtensions.slice();
                },
                emit: emitProxy,
                compile: compileProxy,
                register: registerProxy,
                invokeById: invokeProxyById,
                listRegisteredManifests: function () {
                    if (typeof globalThis.__nsProxyListManifests === 'function') {
                        return globalThis.__nsProxyListManifests();
                    }
                    return [];
                },
            };

            function asDelegate(handler) {
                if (typeof handler === 'function') {
                    return handler;
                }
                if (handler && typeof handler.invoke === 'function') {
                    return handler.invoke.bind(handler);
                }
                throw new Error('NSWinRT.asDelegate(handler) expects a function or { invoke() } object');
            }

            function createEventEmitter() {
                var listeners = [];
                return {
                    add: function (handler) {
                        var normalized = asDelegate(handler);
                        listeners.push(normalized);
                        return {
                            dispose: function () {
                                var idx = listeners.indexOf(normalized);
                                if (idx >= 0) {
                                    listeners.splice(idx, 1);
                                }
                            },
                        };
                    },
                    emit: function () {
                        var args = Array.prototype.slice.call(arguments);
                        listeners.slice().forEach(function (listener) {
                            listener.apply(undefined, args);
                        });
                    },
                    count: function () {
                        return listeners.length;
                    },
                };
            }

            globalThis.NSWinRT.asDelegate = asDelegate;
            globalThis.NSWinRT.createEventEmitter = createEventEmitter;

            var moduleCache = new Map();

            function esmExportAliasAssignments(list) {
                if (!list || !list.trim()) {
                    return '';
                }

                var pairs = [];
                list.split(',').forEach(function (part) {
                    var token = part.trim();
                    if (!token) {
                        return;
                    }
                    var pieces = token.split(/\s+as\s+/i);
                    if (pieces.length === 2) {
                        pairs.push('exports.' + pieces[1].trim() + ' = ' + pieces[0].trim() + ';');
                    } else {
                        pairs.push('exports.' + token + ' = ' + token + ';');
                    }
                });

                return pairs.join('\n');
            }

            function transformEsmToRuntimeModule(source) {
                var transformed = String(source || '');

                transformed = transformed.replace(/^[ \t]*import\s+\*\s+as\s+([A-Za-z_$][\w$]*)\s+from\s+['\"]([^'\"]+)['\"];?[ \t]*$/gm,
                    'const $1 = __nsImport("$2", __filename);');
                transformed = transformed.replace(/^[ \t]*import\s+\{\s*([^}]+)\s*\}\s+from\s+['\"]([^'\"]+)['\"];?[ \t]*$/gm,
                    'const { $1 } = __nsImport("$2", __filename);');
                transformed = transformed.replace(/^[ \t]*import\s+([A-Za-z_$][\w$]*)\s+from\s+['\"]([^'\"]+)['\"];?[ \t]*$/gm,
                    'const $1 = (function(m){ return (m && Object.prototype.hasOwnProperty.call(m, "default")) ? m.default : m; })(__nsImport("$2", __filename));');
                transformed = transformed.replace(/^[ \t]*import\s+['\"]([^'\"]+)['\"];?[ \t]*$/gm,
                    '__nsImport("$1", __filename);');
                transformed = transformed.replace(/\bimport\s*\(/g, '__nsDynamicImport(');

                transformed = transformed.replace(/\bexport\s+default\s+/g, 'exports.default = ');
                transformed = transformed.replace(/^[ \t]*export\s+\{\s*([^}]+)\s*\};?[ \t]*$/gm, function (_, list) {
                    return esmExportAliasAssignments(list);
                });

                var exportedNames = [];
                transformed = transformed.replace(/^[ \t]*export\s+(const|let|var)\s+([A-Za-z_$][\w$]*)/gm, function (_, keyword, name) {
                    exportedNames.push(name);
                    return keyword + ' ' + name;
                });
                transformed = transformed.replace(/^[ \t]*export\s+function\s+([A-Za-z_$][\w$]*)/gm, function (_, name) {
                    exportedNames.push(name);
                    return 'function ' + name;
                });
                transformed = transformed.replace(/^[ \t]*export\s+class\s+([A-Za-z_$][\w$]*)/gm, function (_, name) {
                    exportedNames.push(name);
                    return 'class ' + name;
                });

                if (exportedNames.length > 0) {
                    transformed += '\n' + exportedNames.map(function (name) {
                        return 'exports.' + name + ' = ' + name + ';';
                    }).join('\n') + '\n';
                }

                return transformed;
            }

            function executeRuntimeModule(source, filename) {
                var modulePath = String(filename || '');
                if (!modulePath) {
                    throw new Error('executeRuntimeModule requires a module path');
                }
                if (moduleCache.has(modulePath)) {
                    return moduleCache.get(modulePath);
                }

                var module = { exports: {} };
                moduleCache.set(modulePath, module.exports);

                var transformed = transformEsmToRuntimeModule(source);
                var dirname = modulePath.replace(/[\\/][^\\/]*$/, '');
                var executor = new Function('exports', 'module', '__nsImport', '__nsDynamicImport', '__filename', '__dirname', transformed);
                executor(module.exports, module, __nsImport, __nsDynamicImport, modulePath, dirname);
                moduleCache.set(modulePath, module.exports);
                return module.exports;
            }

            function __nsImport(specifier, parentPath) {
                if (typeof globalThis.__nsResolveModulePath !== 'function') {
                    throw new Error('Module resolver host function is not available');
                }
                if (typeof globalThis.__nsReadTextFile !== 'function') {
                    throw new Error('Module file reader host function is not available');
                }

                var resolved = globalThis.__nsResolveModulePath(
                    String(specifier || ''),
                    parentPath ? String(parentPath) : '',
                    globalThis.__nsAppRoot || ''
                );
                if (!resolved) {
                    throw new Error('Unable to resolve module: ' + specifier);
                }
                if (moduleCache.has(resolved)) {
                    return moduleCache.get(resolved);
                }

                var source = globalThis.__nsReadTextFile(resolved);
                return executeRuntimeModule(source, resolved);
            }

            function __nsDynamicImport(specifier, parentPath) {
                if (typeof specifier !== 'string' ||
                    (!specifier.startsWith('https://') && !specifier.startsWith('http://'))) {
                    return Promise.resolve().then(function () {
                        return __nsImport(specifier, parentPath);
                    });
                }

                try {
                    var sec = {};
                    try {
                        var pkgText = globalThis.__nsReadTextFile(
                            (globalThis.__nsAppRoot || '') + '/package.json'
                        );
                        sec = ((JSON.parse(pkgText) || {}).nativescript || {}).security || {};
                    } catch (_) {}

                    if (!sec.allowRemoteModules) {
                        return Promise.reject(new Error(
                            'Remote module imports are disabled. ' +
                            'Set nativescript.security.allowRemoteModules=true in package.json'
                        ));
                    }

                    var allowlist = sec.remoteModuleAllowlist;
                    if (Array.isArray(allowlist) && allowlist.length > 0) {
                        var isAllowed = allowlist.some(function (p) {
                            return typeof p === 'string' && specifier.startsWith(p);
                        });
                        if (!isAllowed) {
                            return Promise.reject(new Error(
                                'Remote module URL is not in the allowlist: ' + specifier
                            ));
                        }
                    }

                    if (moduleCache.has(specifier)) {
                        return Promise.resolve(moduleCache.get(specifier));
                    }

                    var uri    = new Windows.Foundation.Uri(specifier);
                    var client = new Windows.Web.Http.HttpClient();

                    return NSWinRT.toPromise(client.getStringAsync(uri)).then(function (source) {
                        return executeRuntimeModule(String(source), specifier);
                    });
                } catch (e) {
                    return Promise.reject(e instanceof Error ? e : new Error(String(e)));
                }
            }

            globalThis.__nsInvalidateModuleCacheEntry = function (resolvedPath) {
                moduleCache.delete(String(resolvedPath || ''));
            };

            globalThis.__nsClearModuleCache = function () {
                moduleCache.clear();
            };

            globalThis.__nsEvalAsModule = function (source, filename) {
                return executeRuntimeModule(source, filename);
            };
            globalThis.NSWinRT.import = __nsImport;
            globalThis.__nsDynamicImport = __nsDynamicImport;
            globalThis.NSWinRT.dynamicImport = __nsDynamicImport;
        })();

        // ── runtime metadata ──────────────────────────────────────────────────
        globalThis.__runtimeVersion = "1.0.0";

        // ── setTimeout / clearTimeout / setInterval / clearInterval ───────────
        // Implemented on top of Windows.UI.Xaml.DispatcherTimer which fires on
        // the UI thread, making it safe to invoke JS callbacks directly.
        (function () {
            var _nextId = 0;
            var _timers = new Map();

            function _span(ms) {
                // TimeSpan.Duration is in 100-ns ticks; 1 ms = 10 000 ticks.
                return new Windows.Foundation.TimeSpan({
                    Duration: Math.max(1, Math.floor(ms || 0)) * 10000
                });
            }

            globalThis.setTimeout = function setTimeout(fn, delay) {
                if (typeof fn !== 'function') return 0;
                var id = ++_nextId;
                var extra = Array.prototype.slice.call(arguments, 2);
                var t = new Windows.UI.Xaml.DispatcherTimer();
                t.Interval = _span(delay);
                t.Tick = NSWinRT.asDelegate(function () {
                    t.Stop();
                    _timers.delete(id);
                    try { fn.apply(undefined, extra); } catch (e) {
                        console.log('setTimeout error:', e && e.message || e);
                    }
                });
                _timers.set(id, t);
                t.Start();
                return id;
            };

            globalThis.clearTimeout = function clearTimeout(id) {
                var t = _timers.get(id);
                if (t) { t.Stop(); _timers.delete(id); }
            };

            globalThis.setInterval = function setInterval(fn, delay) {
                if (typeof fn !== 'function') return 0;
                var id = ++_nextId;
                var extra = Array.prototype.slice.call(arguments, 2);
                var ms = Math.max(1, Math.floor(delay || 1));
                var t = new Windows.UI.Xaml.DispatcherTimer();
                t.Interval = _span(ms);
                t.Tick = NSWinRT.asDelegate(function () {
                    if (!_timers.has(id)) return;
                    try { fn.apply(undefined, extra); } catch (e) {
                        console.log('setInterval error:', e && e.message || e);
                    }
                });
                _timers.set(id, t);
                t.Start();
                return id;
            };

            globalThis.clearInterval = function clearInterval(id) {
                var t = _timers.get(id);
                if (t) { t.Stop(); _timers.delete(id); }
            };
        })();

        // ── requestAnimationFrame / cancelAnimationFrame ──────────────────────
        // Uses __nsDwmFlush() — the Windows equivalent of Choreographer /
        // CADisplayLink.  DwmFlush() blocks the calling thread until the next
        // monitor VSync, giving frame-perfect timing at any refresh rate
        // (60 / 120 / 144 / 240 Hz) with no timer overhead.
        //
        // On headless systems DwmFlush() returns immediately (composition
        // disabled), so rAF callbacks fire as fast as microtasks drain —
        // ideal for tests and headless rendering scenarios.
        (function () {
            var _nextId  = 0;
            var _pending = new Map();
            var _running = false;

            function _flush() {
                if (_pending.size === 0) { _running = false; return; }
                // Block until next VSync; returns ms timestamp.
                var ts = (typeof __nsDwmFlush === 'function')
                    ? __nsDwmFlush()
                    : performance.now();
                var cbs = Array.from(_pending);
                _pending.clear();
                for (var i = 0; i < cbs.length; i++) {
                    try { cbs[i][1](ts); } catch (e) {
                        console.log('rAF error:', e && e.message || e);
                    }
                }
                if (_pending.size > 0) queueMicrotask(_flush);
                else _running = false;
            }

            globalThis.requestAnimationFrame = function requestAnimationFrame(callback) {
                if (typeof callback !== 'function') return 0;
                var id = ++_nextId;
                _pending.set(id, callback);
                if (!_running) { _running = true; queueMicrotask(_flush); }
                return id;
            };

            globalThis.cancelAnimationFrame = function cancelAnimationFrame(id) {
                _pending.delete(id);
            };
        })();

        // ── Blob / File ───────────────────────────────────────────────────────
        (function () {
            // Minimal UTF-8 encoder (covers BMP + surrogates) for string parts.
            function _utf8(str) {
                var out = [];
                for (var i = 0; i < str.length; ) {
                    var c = str.codePointAt(i);
                    if      (c < 0x80)    { out.push(c); i += 1; }
                    else if (c < 0x800)   { out.push(0xC0|(c>>6), 0x80|(c&0x3F)); i += 1; }
                    else if (c < 0x10000) { out.push(0xE0|(c>>12), 0x80|((c>>6)&0x3F), 0x80|(c&0x3F)); i += 1; }
                    else                  { out.push(0xF0|(c>>18), 0x80|((c>>12)&0x3F), 0x80|((c>>6)&0x3F), 0x80|(c&0x3F)); i += 2; }
                }
                return new Uint8Array(out).buffer;
            }

            function _toAb(part) {
                if (typeof part === 'string') return _utf8(part);
                if (part instanceof globalThis.Blob) return part._buf;
                if (part instanceof ArrayBuffer) return part;
                if (ArrayBuffer.isView(part))
                    return part.buffer.slice(part.byteOffset, part.byteOffset + part.byteLength);
                return new ArrayBuffer(0);
            }

            function _concat(parts) {
                var total = 0;
                var bufs = (parts || []).map(function (p) { var ab = _toAb(p); total += ab.byteLength; return ab; });
                var out = new Uint8Array(total);
                var off = 0;
                for (var i = 0; i < bufs.length; i++) { out.set(new Uint8Array(bufs[i]), off); off += bufs[i].byteLength; }
                return out.buffer;
            }

            function Blob(blobParts, options) {
                this._buf = _concat(blobParts);
                this.type = (options && options.type) ? String(options.type).toLowerCase() : '';
                this.size = this._buf.byteLength;
            }
            Blob.prototype.arrayBuffer = function () { return Promise.resolve(this._buf.slice(0)); };
            Blob.prototype.text = function () {
                // Simple Latin-1 decode; good enough for ASCII/binary; a real
                // TextDecoder (if polyfilled later) would be more correct.
                var b = new Uint8Array(this._buf), s = '';
                for (var i = 0; i < b.length; i++) s += String.fromCharCode(b[i]);
                return Promise.resolve(s);
            };
            Blob.prototype.slice = function (start, end, type) {
                var sz = this.size;
                var s = start == null ? 0 : (start < 0 ? Math.max(0, sz + start) : Math.min(start, sz));
                var e = end   == null ? sz : (end   < 0 ? Math.max(0, sz + end)  : Math.min(end, sz));
                return new Blob([this._buf.slice(s, e)], { type: type || '' });
            };
            Blob.prototype.stream = function () {
                var buf = this._buf, done = false;
                return {
                    getReader: function () {
                        return {
                            read:        function () { if (done) return Promise.resolve({ done: true, value: undefined }); done = true; return Promise.resolve({ done: false, value: new Uint8Array(buf.slice(0)) }); },
                            cancel:      function () { done = true; return Promise.resolve(); },
                            releaseLock: function () {}
                        };
                    }
                };
            };
            globalThis.Blob = Blob;

            function File(fileBits, fileName, options) {
                Blob.call(this, fileBits, options);
                this.name = String(fileName);
                this.lastModified = (options && options.lastModified != null)
                    ? Number(options.lastModified) : Date.now();
            }
            File.prototype = Object.create(Blob.prototype);
            File.prototype.constructor = File;
            globalThis.File = File;
        })();

        // ── URL / URLSearchParams / URLPattern post-install wiring ────────────
        // The native classes are installed by install_url_globals() before this
        // script runs.  This block adds the JS-layer searchParams accessor
        // (mutation propagation + caching), Symbol.iterator, and the blob-URL API.
        (function () {
            // searchParams: lazily created, mutations propagate back to URL.search
            Object.defineProperty(URL.prototype, 'searchParams', {
                get: function () {
                    if (this._searchParams == null) {
                        var self = this;
                        var sp = new URLSearchParams(this.search);
                        Object.defineProperty(sp, '_url', { enumerable: false, writable: false, value: self });
                        var _append = sp.append.bind(sp);
                        sp.append = function (name, value) { _append(name, value); self.search = sp.toString(); };
                        var _delete = sp.delete.bind(sp);
                        sp.delete = function (name, value) { _delete(name, value); self.search = sp.toString(); };
                        var _set = sp.set.bind(sp);
                        sp.set = function (name, value) { _set(name, value); self.search = sp.toString(); };
                        var _sort = sp.sort.bind(sp);
                        sp.sort = function () { _sort(); self.search = sp.toString(); };
                        this._searchParams = sp;
                    }
                    return this._searchParams;
                },
                configurable: true,
            });

            // Make URLSearchParams iterable (entries by default)
            URLSearchParams.prototype[Symbol.iterator] = URLSearchParams.prototype.entries;

            // ── URL.createObjectURL / revokeObjectURL ─────────────────────────
            var BLOB_STORE = new Map();
            var _blobSeq = 0;
            URL.createObjectURL = function (object, options) {
                if (!(object instanceof globalThis.Blob)) return null;
                var id = (++_blobSeq).toString(16).padStart(8, '0') + '-'
                    + Math.floor(Math.random() * 0x10000).toString(16).padStart(4, '0') + '-4'
                    + Math.floor(Math.random() * 0x1000).toString(16).padStart(3, '0') + '-'
                    + (Math.floor(Math.random() * 4) + 8).toString(16)
                    + Math.floor(Math.random() * 0x1000).toString(16).padStart(3, '0') + '-'
                    + Math.floor(Math.random() * 0x1000000000000).toString(16).padStart(12, '0');
                var url = 'blob:nativescript/' + id;
                BLOB_STORE.set(url, { blob: object, type: object.type, ext: options && options.ext });
                return url;
            };
            URL.revokeObjectURL = function (url) { BLOB_STORE.delete(url); };
            URL.InternalAccessor = {
                getData: function (url) { return BLOB_STORE.get(url) || null; },
            };
        })();

        // ── URLPattern polyfill ───────────────────────────────────────────────
        // ada-url (C++) doesn't expose URLPattern; this JS polyfill covers the
        // common cases: :name capture groups, * / ** wildcards, literal segments.
        (function () {
            if (typeof URLPattern !== 'undefined') return;

            function _esc(s) {
                return s.replace(/[-[\]{}()*+?.,\\^$|#\s]/g, '\\$&');
            }

            function _compile(pat) {
                if (pat === undefined || pat === null) pat = '*';
                var s = String(pat);
                var groups = [];
                var re = '';
                var i = 0;
                while (i < s.length) {
                    var c = s[i];
                    if (c === ':') {
                        var j = i + 1;
                        while (j < s.length && /\w/.test(s[j])) j++;
                        var name = s.slice(i + 1, j);
                        groups.push(name);
                        re += '([^/?#]+?)';
                        i = j;
                    } else if (c === '*' && s[i + 1] === '*') {
                        groups.push('0');
                        re += '(.*)';
                        i += 2;
                    } else if (c === '*') {
                        groups.push('0');
                        re += '([^/?#]*)';
                        i++;
                    } else {
                        re += _esc(c);
                        i++;
                    }
                }
                return { re: new RegExp('^' + re + '$'), groups: groups };
            }

            function _match(compiled, value) {
                var m = String(value || '').match(compiled.re);
                if (!m) return null;
                var groups = {};
                for (var i = 0; i < compiled.groups.length; i++) {
                    groups[compiled.groups[i]] = m[i + 1] !== undefined ? m[i + 1] : undefined;
                }
                return { input: String(value || ''), groups: groups };
            }

            function URLPattern(init, baseURL) {
                if (typeof init === 'string') {
                    try {
                        var u = new URL(init, baseURL || 'http://_placeholder_');
                        init = {
                            protocol: u.protocol.slice(0, -1),
                            hostname: u.hostname,
                            port: u.port,
                            pathname: u.pathname,
                            search: u.search ? u.search.slice(1) : '',
                            hash: u.hash ? u.hash.slice(1) : '',
                        };
                    } catch (e) {
                        init = { pathname: init };
                    }
                }
                init = init || {};
                var opt = function (v, d) { return v !== undefined && v !== null ? String(v) : (d !== undefined ? d : '*'); };
                this.protocol = opt(init.protocol);
                this.username  = opt(init.username);
                this.password  = opt(init.password);
                this.hostname  = opt(init.hostname);
                this.port      = opt(init.port);
                this.pathname  = opt(init.pathname, '/*');
                this.search    = opt(init.search);
                this.hash      = opt(init.hash);
                this._pc = _compile(this.protocol);
                this._uc = _compile(this.username);
                this._pw = _compile(this.password);
                this._hc = _compile(this.hostname);
                this._oc = _compile(this.port);
                this._ac = _compile(this.pathname);
                this._sc = _compile(this.search);
                this._xc = _compile(this.hash);
            }

            URLPattern.prototype.test = function (input, baseURL) {
                return this.exec(input, baseURL) !== null;
            };

            URLPattern.prototype.exec = function (input, baseURL) {
                var url;
                try {
                    if (typeof input === 'string') {
                        url = new URL(input, baseURL || undefined);
                    } else if (input && input.href) {
                        url = new URL(input.href);
                    } else {
                        url = input || {};
                    }
                } catch (e) { return null; }

                var rm = _match(this._pc, url.protocol ? url.protocol.slice(0, -1) : '');
                var um = _match(this._uc, url.username || '');
                var pm = _match(this._pw, url.password || '');
                var hm = _match(this._hc, url.hostname || '');
                var om = _match(this._oc, url.port || '');
                var am = _match(this._ac, url.pathname || '/');
                var sm = _match(this._sc, url.search ? url.search.slice(1) : '');
                var xm = _match(this._xc, url.hash ? url.hash.slice(1) : '');

                if (!rm || !um || !pm || !hm || !om || !am || !sm || !xm) return null;
                return {
                    inputs:   [input],
                    protocol: rm, username: um, password: pm,
                    hostname: hm, port: om,     pathname: am,
                    search:   sm, hash: xm,
                };
            };

            globalThis.URLPattern = URLPattern;
        })();

        // ── NSWinRT.dotnet — BCL / arbitrary .NET dispatch ───────────────────
        // Requires the dotnet-bridge project to be published into
        //   <app-root>/dotnet-bridge/publish/DotNetBridge.dll
        (function () {
            if (typeof globalThis.__nsDotNetInvoke !== 'function') return;

            function _invoke(req) {
                var json = globalThis.__nsDotNetInvoke(JSON.stringify(req));
                var res  = JSON.parse(json);
                if (res.error) throw new Error(res.error);
                return res.result;
            }

            // ── Type-metadata cache ──────────────────────────────────────────
            // Populated lazily on first access; avoids repeated bridge round-trips.
            var _typeInfoCache = {};
            var _emptyInfo = { methods: [], properties: [], staticMethods: [], staticProperties: [] };

            // ── Auto-release via FinalizationRegistry ────────────────────────
            // When the JS GC collects a DotNet proxy the registry fires the
            // callback with the managed handle id, releasing the CLR reference.
            // Explicit sw.release() still works for deterministic teardown.
            var _dotNetFinalizers = typeof FinalizationRegistry === 'function'
                ? new FinalizationRegistry(function (handle) {
                    try { _invoke({ handle: handle, method: '__release', args: [] }); } catch (e) {}
                  })
                : null;

            function _getTypeInfo(assembly, typeName) {
                if (!typeName) return _emptyInfo;
                var cached = _typeInfoCache[typeName];
                if (cached !== undefined) return cached;
                try {
                    var info = _invoke({ assembly: assembly || typeName.split('.')[0], typeName: typeName, method: '__members__', args: [] });
                    _typeInfoCache[typeName] = (info && typeof info === 'object') ? info : _emptyInfo;
                } catch (e) {
                    _typeInfoCache[typeName] = _emptyInfo;
                }
                return _typeInfoCache[typeName];
            }

            function _unwrap(v) {
                if (v && typeof v === 'object' && typeof v.__handle === 'number') return { __handle: v.__handle };
                return v;
            }

            // ── Instance Proxy ───────────────────────────────────────────────
            // Makes sw.Stop() and sw.Elapsed both work naturally.
            // The proxy is registered with _dotNetFinalizers so the CLR reference
            // is released automatically when JS GC collects the proxy.
            function _makeDotNetInstance(handle, assembly, typeName) {
                var info = _getTypeInfo(assembly, typeName);
                var proxy = new Proxy({}, {
                    get: function (_, prop) {
                        if (typeof prop === 'symbol') return undefined;
                        if (prop === '__handle') return handle;
                        if (prop === '__type')   return typeName;
                        if (prop === 'release') return function () {
                            _invoke({ handle: handle, method: '__release', args: [] });
                        };
                        if (prop === 'toString') return function () {
                            return '[DotNetObject ' + typeName + ' #' + handle + ']';
                        };
                        // Re-read info in case it was populated after construction.
                        var i = _typeInfoCache[typeName] || _emptyInfo;
                        if (i.properties && i.properties.indexOf(prop) >= 0) {
                            // Property: resolve value immediately.
                            return _wrap(_invoke({ handle: handle, method: 'get_' + prop, args: [] }));
                        }
                        // Method (or unknown): return a callable.
                        return function () {
                            var args = Array.prototype.slice.call(arguments).map(_unwrap);
                            return _wrap(_invoke({ handle: handle, method: prop, args: args }));
                        };
                    },
                    set: function (_, prop, value) {
                        var i = _typeInfoCache[typeName] || _emptyInfo;
                        if (i.properties && i.properties.indexOf(prop) >= 0) {
                            _invoke({ handle: handle, method: 'set_' + prop, args: [_unwrap(value)] });
                        }
                        return true;
                    },
                });
                if (_dotNetFinalizers) _dotNetFinalizers.register(proxy, handle);
                return proxy;
            }

            function _wrap(value) {
                if (value == null) return null;
                if (Array.isArray(value)) return value.map(_wrap);
                if (typeof value === 'object' && typeof value.__handle === 'number') {
                    var typeName = value.__type || '';
                    var assembly = typeName.split('.')[0] || '';
                    return _makeDotNetInstance(value.__handle, assembly, typeName);
                }
                return value;
            }

            globalThis.NSWinRT.dotnet = {
                invoke: function (assembly, typeName, method, args) {
                    return _wrap(_invoke({ assembly: assembly, typeName: typeName, method: method, args: (args || []).map(_unwrap) }));
                },
                get: function (assembly, typeName, prop) {
                    return _wrap(_invoke({ assembly: assembly, typeName: typeName, method: 'get_' + prop, args: [] }));
                },
                fromHandle: function (handle, typeName) {
                    var assembly = (typeName || '').split('.')[0] || '';
                    return _makeDotNetInstance(handle, assembly, typeName || '');
                },
            };

            // ── Natural namespace proxies ────────────────────────────────────
            // System.Diagnostics.Stopwatch.StartNew()    →  static method call
            // System.Environment.MachineName             →  static property get
            // new System.Text.StringBuilder(64)          →  constructor
            // sw.Stop()                                  →  instance method
            // sw.Elapsed                                 →  instance property
            function _makeNamespaceProxy(path) {
                function _node() {}
                return new Proxy(_node, {
                    get: function (_, prop) {
                        if (typeof prop === 'symbol') return undefined;
                        var assembly = path.split('.')[0];
                        var info = _getTypeInfo(assembly, path);
                        // Static property: resolve value immediately.
                        if (info.staticProperties && info.staticProperties.indexOf(prop) >= 0) {
                            return _wrap(_invoke({ assembly: assembly, typeName: path, method: 'get_' + prop, args: [] }));
                        }
                        // Static method: return a callable.
                        if (info.staticMethods && info.staticMethods.indexOf(prop) >= 0) {
                            return function () {
                                var args = Array.prototype.slice.call(arguments).map(_unwrap);
                                return _wrap(_invoke({ assembly: assembly, typeName: path, method: prop, args: args }));
                            };
                        }
                        // Namespace / sub-type: keep descending.
                        return _makeNamespaceProxy(path + '.' + prop);
                    },
                    apply: function (_, _this, args) {
                        var lastDot  = path.lastIndexOf('.');
                        var typeName = path.substring(0, lastDot);
                        var method   = path.substring(lastDot + 1);
                        var assembly = typeName.split('.')[0];
                        return _wrap(_invoke({ assembly: assembly, typeName: typeName, method: method, args: args.map(_unwrap) }));
                    },
                    construct: function (_, args) {
                        var assembly = path.split('.')[0];
                        return _wrap(_invoke({ assembly: assembly, typeName: path, method: '.ctor', args: args.map(_unwrap) }));
                    },
                });
            }
            globalThis.System    = _makeNamespaceProxy('System');
            globalThis.Microsoft = _makeNamespaceProxy('Microsoft');
        })();

        // ── NSWinRT.win32 — dynamic Win32 FFI via libffi ────────────────────
        (function () {
            if (typeof globalThis.__nsWin32Call !== 'function') return;

            // Auto-type a bare JS value to a {type, value} descriptor.
            function _autoType(v) {
                if (v === null || v === undefined) return { type: 'pointer', value: 0 };
                if (typeof v === 'object' && typeof v.type === 'string') return v;
                if (typeof v === 'string')  return { type: 'wstr', value: v };
                if (typeof v === 'boolean') return { type: 'bool', value: v };
                if (typeof v === 'number')
                    return { type: Number.isInteger(v) ? 'i32' : 'f64', value: v };
                return { type: 'i32', value: Number(v) };
            }

            globalThis.NSWinRT.win32 = {
                /**
                 * Low-level call with explicit typed args.
                 *   NSWinRT.win32.call('user32.dll', 'MessageBoxW', 'i32',
                 *     {type:'pointer',value:0}, {type:'wstr',value:'Hello'}, ...)
                 */
                call: function (dll, fn_name, returnType) {
                    var args = Array.prototype.slice.call(arguments, 3);
                    var json = JSON.stringify({ dll: dll, fn: fn_name, returnType: returnType, args: args });
                    return JSON.parse(globalThis.__nsWin32Call(json)).value;
                },

                /**
                 * Returns a Proxy where every property is a callable Win32 function.
                 * Args are auto-typed from their JS values; strings become wstr,
                 * numbers become i32 (or f64 if non-integer), null becomes pointer(0).
                 *
                 *   const user32 = NSWinRT.win32.bind('user32.dll');
                 *   const { MessageBoxW } = NSWinRT.win32.bind('user32.dll');
                 *   MessageBoxW(null, 'Hello!', 'Title', 0);
                 */
                bind: function (dll, returnType) {
                    var ret = returnType || 'i32';
                    var win32 = globalThis.NSWinRT.win32;
                    return new Proxy({}, {
                        get: function (_, name) {
                            if (typeof name !== 'string') return undefined;
                            return function () {
                                var args = Array.prototype.slice.call(arguments).map(_autoType);
                                return win32.call.apply(win32, [dll, name, ret].concat(args));
                            };
                        },
                    });
                },

                /**
                 * Returns an object with typed function wrappers from explicit signatures.
                 *
                 *   const api = NSWinRT.win32.define('user32.dll', {
                 *       MessageBoxW:      ['pointer','wstr','wstr','u32'],
                 *       GetSystemMetrics: ['i32'],
                 *   }, 'i32');
                 *   api.MessageBoxW(null, 'Hello', 'Title', 0);
                 */
                define: function (dll, signatures, defaultReturnType) {
                    var ret = defaultReturnType || 'i32';
                    var win32 = globalThis.NSWinRT.win32;
                    var obj = {};
                    Object.keys(signatures).forEach(function (name) {
                        var argTypes = signatures[name];
                        obj[name] = function () {
                            var jsArgs = Array.prototype.slice.call(arguments);
                            var typedArgs = jsArgs.map(function (v, i) {
                                var ty = argTypes[i];
                                return ty ? { type: ty, value: v } : _autoType(v);
                            });
                            return win32.call.apply(win32, [dll, name, ret].concat(typedArgs));
                        };
                    });
                    return obj;
                },

                /**
                 * Enumerates all exports of a DLL and installs each one as a global
                 * function on `globalThis`, auto-typing args.
                 *
                 *   NSWinRT.win32.import('user32.dll');
                 *   MessageBoxW(null, 'Hello!', 'Title', 0); // ← now a plain global
                 */
                import: function (dll, returnType) {
                    if (typeof globalThis.__nsWin32Exports !== 'function') return;
                    var exports = JSON.parse(globalThis.__nsWin32Exports(dll));
                    var bound   = globalThis.NSWinRT.win32.bind(dll, returnType || 'i32');
                    exports.forEach(function (name) {
                        if (!(name in globalThis))
                            Object.defineProperty(globalThis, name, {
                                value: bound[name], writable: true, configurable: true,
                            });
                    });
                },
            };
        })();
        "#;

// ── __nsDotNetInvoke ──────────────────────────────────────────────────────────

/// Calls the .NET bridge with a JSON request string and returns the JSON
/// response string.  Throws a JS error if the host is not initialised or the
/// call fails at the hosting layer (application-level errors are returned as
/// `{"error":"…"}` inside the JSON, matching the bridge contract).
pub(crate) fn handle_dotnet_invoke(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 1 {
        throw_js_error(scope, "__nsDotNetInvoke: expected a JSON string argument");
        return;
    }
    let Some(json_v) = args.get(0).to_string(scope) else {
        throw_js_error(scope, "__nsDotNetInvoke: argument must be a string");
        return;
    };
    let json = json_v.to_rust_string_lossy(scope);
    match crate::dotnet::call_dotnet(&json) {
        Ok(result) => {
            if let Some(s) = v8::String::new(scope, &result) {
                retval.set(s.into());
            }
        }
        Err(e) => throw_js_error(scope, &e),
    }
}

// ── __nsWin32Exports ──────────────────────────────────────────────────────────

/// Returns a JSON array of exported function names for the given DLL, e.g.
/// `["MessageBoxW","CreateWindowExW",…]`.  Used by `NSWinRT.win32.import()`.
pub(crate) fn handle_win32_exports(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let Some(dll_v) = args.get(0).to_string(scope) else { return };
    let dll = dll_v.to_rust_string_lossy(scope);
    let names = match crate::win32::list_exports(&dll) {
        Ok(v) => v,
        Err(_) => vec![],
    };
    let json = serde_json::to_string(&names).unwrap_or_else(|_| "[]".to_string());
    if let Some(s) = v8::String::new(scope, &json) {
        retval.set(s.into());
    }
}

// ── __nsDwmFlush ──────────────────────────────────────────────────────────────

/// Blocks the calling thread until the next DWM VSync (monitor refresh), then
/// returns the elapsed milliseconds since process start as a `f64`.
///
/// This is the Windows equivalent of Android's `Choreographer` / iOS's
/// `CADisplayLink`: it yields exactly at the display's refresh boundary,
/// giving requestAnimationFrame perfect frame timing at any Hz (60/120/144/240).
///
/// On headless systems where DWM composition is disabled, `DwmFlush` returns
/// immediately with an error, so this call is non-blocking in that case.
pub(crate) fn handle_dwm_flush(
    _scope: &mut v8::PinScope<'_, '_>,
    _args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    use windows::Win32::Graphics::Dwm::DwmFlush;
    // Best-effort: ignore DWM_E_COMPOSITIONDISABLED on headless systems.
    let _ = unsafe { DwmFlush() };
    let ts = crate::globals::time::PROCESS_START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_nanos() as f64 / 1_000_000.0;
    retval.set_double(ts);
}

// ── __nsUUID ──────────────────────────────────────────────────────────────────

pub(crate) fn handle_ns_uuid(
    scope: &mut v8::PinScope<'_, '_>,
    _args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    match windows::core::GUID::new() {
        Ok(g) => {
            let s = format!(
                "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
                g.data1, g.data2, g.data3,
                g.data4[0], g.data4[1], g.data4[2], g.data4[3],
                g.data4[4], g.data4[5], g.data4[6], g.data4[7],
            );
            if let Some(v) = v8::String::new(scope, &s) {
                retval.set(v.into());
            }
        }
        Err(_) => retval.set(v8::undefined(scope).into()),
    }
}

// ── __nsWin32Call ─────────────────────────────────────────────────────────────

/// Calls an arbitrary Win32 function via libffi dynamic dispatch.
/// Accepts a JSON string `{dll, fn, returnType, args:[{type,value}…]}` and
/// returns `{"value": <result>}` or `{"error": "…"}`.
pub(crate) fn handle_win32_call(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 1 {
        throw_js_error(scope, "__nsWin32Call: expected a JSON string argument");
        return;
    }
    let Some(json_v) = args.get(0).to_string(scope) else {
        throw_js_error(scope, "__nsWin32Call: argument must be a string");
        return;
    };
    let json = json_v.to_rust_string_lossy(scope);
    let result_json = match crate::win32::call_win32_json(&json) {
        Ok(v)  => v,
        Err(e) => format!(r#"{{"error":{}}}"#, serde_json::to_string(&e).unwrap()),
    };
    if let Some(s) = v8::String::new(scope, &result_json) {
        retval.set(s.into());
    }
}

// ── Context initialisation ────────────────────────────────────────────────────

/// Installs all host-side global functions and the runtime bootstrap JS into `scope`.
pub(crate) fn init_async_helpers(
    scope: &mut v8::ContextScope<v8::HandleScope<v8::Context>>,
    app_root: &str,
) {
    let global = scope.get_current_context().global(scope);

    // One macro call per builtin replaces 4 lines of boilerplate.
    macro_rules! register {
        ($name:literal, $handler:expr) => {{
            if let (Some(n), Some(f)) = (v8::String::new(scope, $name), v8::Function::new(scope, $handler)) {
                global.define_own_property(scope, n.into(), f.into(), v8::PropertyAttribute::READ_ONLY);
            }
        }};
    }

    register!("__nsHostWaitForAsync",           handle_host_wait_for_async);
    register!("__nsEnqueueMicrotask",           handle_enqueue_microtask);
    register!("__nsPointerKey",                 handle_pointer_key);
    register!("__nsBufferToPointer",            handle_buffer_to_pointer);
    register!("__nsProxyWriteTextFile",         handle_proxy_write_text_file);
    register!("__nsProxyCompileProject",        handle_proxy_compile_project);
    register!("__nsProxyRegisterManifest",      handle_proxy_register_manifest);
    register!("__nsProxyListManifests",         handle_proxy_list_manifests);
    register!("__nsProxyAutoCapture",           handle_proxy_auto_capture);
    register!("__nsReadTextFile",               handle_read_text_file);
    register!("__nsResolveModulePath",          handle_resolve_module_path);
    register!("__nsDescribeWinRTType",          handle_describe_winrt_type);
    register!("__nsWorkerCreateThreaded",       handle_worker_create_threaded);
    register!("__nsWorkerPostMessage",          handle_worker_post_message);
    register!("__nsWorkerPollMessages",         handle_worker_poll_messages);
    register!("__nsWorkerTerminate",            handle_worker_terminate);
    register!("__nsWorkerPollMessagesBlocking", handle_worker_poll_messages_blocking);
    register!("__nsLiveSyncCopyFile",           handle_livesync_copy_file);
    register!("__nsDotNetInvoke",               handle_dotnet_invoke);
    register!("__nsWin32Call",                  handle_win32_call);
    register!("__nsWin32Exports",               handle_win32_exports);
    register!("__nsDwmFlush",                   handle_dwm_flush);
    register!("__nsUUID",                       handle_ns_uuid);

    if let (Some(k), Some(v)) = (v8::String::new(scope, "__nsAppRoot"), v8::String::new(scope, app_root)) {
        global.define_own_property(scope, k.into(), v.into(), v8::PropertyAttribute::READ_ONLY);
    }

    // Improve Windows timer resolution from the default ~15 ms to ~1 ms.
    // This benefits all DispatcherTimer-based setTimeout/setInterval calls.
    // Ignored on failure (e.g., sandboxed environments).
    let _ = crate::win32::call_win32_json(
        r#"{"dll":"winmm.dll","fn":"timeBeginPeriod","returnType":"u32","args":[{"type":"u32","value":1}]}"#,
    );

    // Attempt to initialise the .NET BCL host in the background; failures are
    // deferred so the runtime still starts without .NET installed.
    crate::dotnet::try_init_dotnet(app_root);

    crate::globals::url::install_url_globals(scope);

    if let Some(src) = v8::String::new(scope, HELPER_SOURCE) {
        if let Some(script) = v8::Script::compile(scope, src, None) {
            script.run(scope);
        }
    }

    crate::message_port::install_message_port_runtime(scope);
    crate::worker_support::install_worker_runtime(scope);
    crate::hmr_support::install_hmr_support(scope);
    crate::livesync::install_livesync_support(scope);
}
