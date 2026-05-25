use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::collections::HashMap;
use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, MSG, PeekMessageW, PM_REMOVE, TranslateMessage};
use windows::core::{IUnknown, Interface};
use std::mem::ManuallyDrop;
use runtime_binding_gen::{RuntimeExtensionMetadata, RuntimeExtensionRegistry};

use crate::{throw_js_error, Runtime, ASYNC_PUMP_HOOK, proxy_manifests};
use crate::type_description::build_runtime_type_descriptor;
use std::cell::RefCell;
use std::sync::Arc;
use serde_json::Value as JsonValue;
use std::ffi::c_void;

extern "system" {
    fn LocalAlloc(uFlags: u32, uBytes: usize) -> *mut c_void;
}

const LMEM_FIXED: u32 = 0x0000;

#[cfg(feature = "devtools")]
use runtime_devtools::{DevtoolsServer, DevtoolsServerConfig};

#[cfg(feature = "devtools")]
thread_local!(static DEVTOOLS_SERVER: RefCell<Option<DevtoolsServer>> = RefCell::new(None));

thread_local!(static INSPECTOR_DOMAIN_DISPATCHERS: RefCell<HashMap<String, v8::Global<v8::Value>>> = RefCell::new(HashMap::new()));

const INSPECTOR_DISPATCHERS_GLOBAL: &str = "__nsInspectorDomainDispatchers";

// ── Private helpers ───────────────────────────────────────────────────────────

pub(crate) fn value_to_string(scope: &mut v8::PinScope<'_, '_>, value: v8::Local<v8::Value>) -> Option<String> {
    let value = value.to_string(scope)?;
    Some(value.to_rust_string_lossy(scope))
}

pub(crate) fn handle_run_on_ui_thread(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    if args.length() < 1 {
        throw_js_error(scope, "__nsRunOnUIThread(fn): expected a function argument");
        return;
    }

    let Ok(func) = v8::Local::<v8::Function>::try_from(args.get(0)) else {
        throw_js_error(scope, "__nsRunOnUIThread: argument must be a function");
        return;
    };

    // v8::Global is !Send; store it by ID so only the i32 crosses into the closure.
    let cb_id = crate::DOTNET_NEXT_CB_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    crate::DOTNET_JS_CALLBACKS.with(|m| {
        m.borrow_mut().insert(cb_id, v8::Global::new(scope, func));
    });

    crate::ui_dispatcher::post_to_ui_thread(move || {
        let isolate_ptr = crate::DELEGATE_ISOLATE_PTR.with(|c| c.get());
        if isolate_ptr.is_null() {
            crate::DOTNET_JS_CALLBACKS.with(|m| { m.borrow_mut().remove(&cb_id); });
            return;
        }
        let isolate: &mut v8::Isolate = unsafe { &mut *isolate_ptr };
        v8::scope!(scope, isolate);
        let ctx_global = match scope.get_slot::<v8::Global<v8::Context>>() {
            Some(g) => g.clone(),
            None => {
                crate::DOTNET_JS_CALLBACKS.with(|m| { m.borrow_mut().remove(&cb_id); });
                return;
            }
        };
        let context = v8::Local::new(scope, &ctx_global);
        let scope = &mut v8::ContextScope::new(scope, context);
        v8::tc_scope!(tc, scope);

        let func_global = crate::DOTNET_JS_CALLBACKS.with(|m| {
            m.borrow().get(&cb_id).cloned()
        });
        let Some(func_global) = func_global else {
            return;
        };

        let func = v8::Local::new(tc, &func_global);
        let recv: v8::Local<v8::Value> = v8::undefined(tc).into();
        let _ = func.call(tc, recv, &[]);

        if tc.has_caught() {
            if let Some(ex) = tc.exception() {
                let msg = ex.to_rust_string_lossy(tc);
                crate::store_last_js_error(msg);
            }
            tc.reset();
        }

        crate::DOTNET_JS_CALLBACKS.with(|m| { m.borrow_mut().remove(&cb_id); });
    });
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
                // deadline == None means timeout_ms == 0: non-blocking check requested.
                // Return the op as-is rather than spinning forever.
                if deadline.is_none() {
                    retval.set(op_value);
                    return;
                }
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

// Inspector helpers moved to `inspector.rs`.
pub(crate) use crate::inspector::{
    handle_register_domain_dispatcher,
    handle_inspector_send_event,
    handle_inspector_timestamp,
    try_dispatch_inspector_message_to_js,
    maybe_attach_devtools,
};

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
                        if (typeof globalThis.__ns__setTimeout === 'function') {
                            globalThis.__ns__setTimeout(function () { throw err; }, 0);
                        } else if (typeof globalThis.setTimeout === 'function') {
                            globalThis.setTimeout(function () { throw err; }, 0);
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

                if (op && typeof op.__handle === 'number' && op.__isTask === true
                    && typeof globalThis.__nsDotNetAwaitTask === 'function') {
                    return new Promise(function (resolve, reject) {
                        globalThis.__nsDotNetAwaitTask(op.__handle, resolve, reject);
                    });
                }

                if (typeof op.then === 'function' && !('Completed' in op)) {
                    return op;
                }

                return new Promise(function (resolve, reject) {
                    var settled = false;

                    function settleFromStatus(overrideStatus) {
                        if (settled) { return; }
                        try {
                            var status = normalizeStatus(
                                overrideStatus !== undefined ? overrideStatus : (op && op.Status)
                            );

                            if (status === statusEnum.Completed || status === 1) {
                                settled = true;
                                resolve(getResults(op));
                                return;
                            }
                            if (status === statusEnum.Canceled || status === 2) {
                                settled = true;
                                reject(new Error('WinRT async operation was canceled'));
                                return;
                            }
                            if (status === statusEnum.Error || status === 3) {
                                settled = true;
                                reject((op && op.ErrorCode) || new Error('WinRT async operation failed'));
                                return;
                            }
                            // status === 0 (Started): still running — do not settle yet.
                        } catch (err) {
                            settled = true;
                            reject(err);
                        }
                    }

                    try {
                        if (op && 'Completed' in op) {
                            // Settle immediately if the operation already finished before we
                            // could register the handler — WinRT will not fire Completed
                            // retroactively once the operation has left the Started state.
                            var initialStatus = normalizeStatus(op.Status);
                            if (!Number.isNaN(initialStatus) && initialStatus !== 0) {
                                settleFromStatus(initialStatus);
                                return;
                            }

                            op.Completed = function (asyncInfo, asyncStatus) {
                                settleFromStatus(asyncStatus);
                            };

                            // Re-check after assignment: the op may have completed in the
                            // narrow window between the status read and the handler assignment.
                            var raceStatus = normalizeStatus(op.Status);
                            if (!Number.isNaN(raceStatus) && raceStatus !== 0) {
                                settleFromStatus(raceStatus);
                            }
                            return;
                        }
                    } catch (_) {
                        // Completed setter not available; fall through to synchronous wait.
                    }

                    // Synchronous wait fallback — only safe when a finite timeout is given.
                    // Calling wait() with timeout=0 enters an infinite spin in the host.
                    var timeoutMs = normalizeTimeoutMs(options);
                    if (timeoutMs === 0) {
                        var currentStatus = normalizeStatus(op && op.Status);
                        if (!Number.isNaN(currentStatus) && currentStatus !== 0) {
                            settleFromStatus(currentStatus);
                        } else {
                            reject(new Error(
                                'Cannot await this WinRT async operation: it has no Completed property ' +
                                'and no timeoutMs was specified. Pass { timeoutMs: N } as the second argument.'
                            ));
                        }
                        return;
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

            // Expose top-level `interop` alias.
            // This lets code reference `globalThis.interop` while keeping backwards
            // compatibility with `globalThis.NSWinRT`.
            globalThis.interop = globalThis.interop || globalThis.NSWinRT.interop;
            // Ensure NSWinRT.interop is populated when `interop` was already present.
            globalThis.NSWinRT = globalThis.NSWinRT || {};
            globalThis.NSWinRT.interop = globalThis.NSWinRT.interop || globalThis.interop;

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

            // Create a managed-backed subclass factory. This returns a constructor
            // that, when invoked, asks the managed bridge to instantiate a real
            // C# subclass whose vtable forwards virtual calls back into the
            // provided JS `overrides` object via the DotNet bridge.
            function makeManagedConstructor(baseCtor, nameOrOverrides, maybeOverrides) {
                var hasName = typeof nameOrOverrides === 'string';
                var explicitTypeName = hasName ? nameOrOverrides : '';
                var typeName = explicitTypeName || autoProxyTypeName(baseCtor);
                var overrides = hasName ? maybeOverrides : nameOrOverrides;
                if (!overrides || typeof overrides !== 'object') overrides = {};

                function Managed() {
                    var args = Array.prototype.slice.call(arguments);
                    // Pass the actual base ctor type name as the assembly param so the
                    // bridge can fall back to it when no static proxy exists for typeName.
                    var baseTypeName = ctorName(baseCtor) || '';
                    var obj = NSWinRT.proxy.createManagedSubclass(baseTypeName, typeName || '', overrides);
                    if (typeof overrides.init === 'function') {
                        try {
                            var initResult = overrides.init.apply(obj, args);
                            if (initResult && typeof initResult === 'object') obj = initResult;
                        } catch (_) { }
                    }
                    return obj;
                }

                Managed.prototype = Object.create((baseCtor && baseCtor.prototype) || Object.prototype);
                Managed.prototype.constructor = Managed;
                Managed.extend = function (nextNameOrOverrides, nextMaybeOverrides) {
                    return makeManagedConstructor(Managed, nextNameOrOverrides, nextMaybeOverrides);
                };
                try {
                    Object.defineProperty(Managed, 'name', { value: typeName || ctorName(baseCtor), configurable: true });
                } catch (_) { }
                Managed.__typeName__ = typeName || ctorName(baseCtor);
                return Managed;
            }

            if (typeof Function.prototype.extendManaged !== 'function') {
                Object.defineProperty(Function.prototype, 'extendManaged', {
                    value: function (nameOrOverrides, maybeOverrides) {
                        return makeManagedConstructor(this, nameOrOverrides, maybeOverrides);
                    },
                    writable: true,
                    configurable: true,
                    enumerable: false,
                });
            }

            if (typeof Object.extendManaged !== 'function') {
                Object.defineProperty(Object, 'extendManaged', {
                    value: function (nameOrOverrides, maybeOverrides) {
                        return makeManagedConstructor(Object, nameOrOverrides, maybeOverrides);
                    },
                    writable: true,
                    configurable: true,
                    enumerable: false,
                });
            }

            // Convenience alias for hosts that expect a global BaseClass helper.
            globalThis.BaseClass = globalThis.BaseClass || {};
            if (typeof globalThis.BaseClass.extend !== 'function') {
                globalThis.BaseClass.extend = function (baseCtor, nameOrOverrides, maybeOverrides) {
                    return makeManagedConstructor(baseCtor || Object, nameOrOverrides, maybeOverrides);
                };
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
                createManagedSubclass: function (assembly, typeName, overrides) {
                    if (typeof globalThis.__nsDotNetCreateJsSubclass !== 'function') {
                        throw new Error('__nsDotNetCreateJsSubclass is not available in this runtime');
                    }
                    if (!overrides || typeof overrides !== 'object') overrides = {};
                    var dispatcher = function () {
                        var a = Array.prototype.slice.call(arguments);
                        try {
                            var target = _wrap(a[0]);
                            var method = a[1];
                            var margs = a[2] || [];
                            if (target && typeof method === 'string') {
                                var fn = overrides[method];
                                if (typeof fn === 'function') return fn.apply(target, Array.isArray(margs) ? margs : []);
                            }
                        } catch (_) { }
                    };
                    var handle = globalThis.__nsDotNetCreateJsSubclass(assembly || '', typeName || '', dispatcher);
                    var obj = _wrap(handle);
                    ensureProxyInstance(obj, overrides, function () { });
                    return obj;
                },
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

        // ── Timers bootstrap (runtime-registered defaults, embedders may override) ─
        // The runtime registers host-facing timer entrypoints into the JS global:
        //   global.__ns__setTimeout
        //   global.__ns__setInterval
        //   global.__ns__clearTimeout
        //   global.__ns__clearInterval
        // Embedders may replace these with
        // platform-specific implementations. When no host implementation is
        // available, the bootstrap provides default behaviors (throws for
        // setTimeout/setInterval and no-op for clearTimeout/clearInterval).
        (function () {
            if (typeof globalThis.__ns__setTimeout !== 'function') {
                globalThis.__ns__setTimeout = function () {
                    throw new Error('Timers are not available: embedder must provide global.__ns__setTimeout');
                };
            }
            if (typeof globalThis.__ns__setInterval !== 'function') {
                globalThis.__ns__setInterval = function () {
                    throw new Error('Timers are not available: embedder must provide global.__ns__setInterval');
                };
            }
            if (typeof globalThis.__ns__clearTimeout !== 'function') {
                globalThis.__ns__clearTimeout = function () { };
            }
            if (typeof globalThis.__ns__clearInterval !== 'function') {
                globalThis.__ns__clearInterval = function () { };
            }
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
            if (typeof globalThis.__nsDotNetInvokeBin !== 'function') return;

            // Binary protocol: no JSON.stringify / JSON.parse.
            // __nsDotNetInvokeBin(handle, typeName, assembly, method, ...args)
            // throws on error, returns the result value directly.
            function _invoke(req) {
                var handle = (req.handle !== undefined && req.handle !== null) ? req.handle : -1;
                var args   = req.args || [];
                return globalThis.__nsDotNetInvokeBin(
                    handle,
                    req.typeName  || '',
                    req.assembly  || '',
                    req.method    || '',
                    ...args
                );
            }

            // ── Type-metadata cache ──────────────────────────────────────────
            // Populated lazily on first access; avoids repeated bridge round-trips.
            var _typeInfoCache = {};
            var _emptyInfo = { methods: [], properties: [], staticMethods: [], staticProperties: [], readonlyProperties: [], readonlyStaticProperties: [], writeonlyProperties: [], writeonlyStaticProperties: [] };
            // Optional mapping for namespace root → assembly simple-name. Use
            // NSWinRT.dotnet.registerNamespace('com', 'My.Assembly.Name') to map
            // top-level roots (e.g. `com`) to the assembly that contains types.
            var _namespaceAssemblyMap = Object.create(null);

            // ── Auto-release via FinalizationRegistry ────────────────────────
            // When the JS GC collects a DotNet proxy the registry fires the
            // callback with the managed handle id, releasing the CLR reference.
            // Explicit sw.release() still works for deterministic teardown.
            var _dotNetFinalizers = typeof FinalizationRegistry === 'function'
                ? new FinalizationRegistry(function (handle) {
                    try { _invoke({ handle: handle, method: '__release', args: [] }); } catch (e) {}
                  })
                : null;

            // Auto-populate namespace→assembly map from the managed side.
            // GetNamespaceAssemblyMapJson scans the app directory for assemblies and
            // returns a JSON map of { rootNamespace: assemblySimpleName }.
            // Runs once at IIFE init; any failure is silently ignored.
            try {
                var _autoNsJson = _invoke({ assembly: '', typeName: 'NativeScriptBridge.Bridge', method: 'GetNamespaceAssemblyMapJson', args: [] });
                if (typeof _autoNsJson === 'string' && _autoNsJson) {
                    var _parsed = JSON.parse(_autoNsJson);
                    if (_parsed && typeof _parsed === 'object') {
                        for (var _k in _parsed) {
                            if (Object.prototype.hasOwnProperty.call(_parsed, _k) && typeof _parsed[_k] === 'string') {
                                _namespaceAssemblyMap[_k] = _parsed[_k];
                            }
                        }
                    }
                }
            } catch (_e) {}

            function _getTypeInfo(assembly, typeName) {
                if (!typeName) return _emptyInfo;
                var cached = _typeInfoCache[typeName];
                if (cached !== undefined) return cached;
                try {
                    // Respect an explicitly-provided assembly name. When empty,
                    // let the managed side attempt resolution (BCL types via Type.GetType).
                    var asm = (typeof assembly === 'string') ? assembly : '';
                    var info = _invoke({ assembly: asm, typeName: typeName, method: '__members__', args: [] });
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
            function _makeDotNetInstance(handle, assembly, typeName, isTask, nativePtr) {
                var info = _getTypeInfo(assembly, typeName);
                var proxy = new Proxy({}, {
                    get: function (_, prop) {
                        if (typeof prop === 'symbol') return undefined;
                        if (prop === '__handle') return handle;
                        if (prop === '__type')   return typeName;
                        if (prop === '__isTask') return isTask === true;
                        if (prop === '__native_ptr') return nativePtr;
                        if (prop === 'release') return function () {
                            _invoke({ handle: handle, method: '__release', args: [] });
                        };
                        if (prop === 'toString') return function () {
                            return '[DotNetObject ' + typeName + ' #' + handle + ']';
                        };
                        if (prop === 'then') return undefined;
                        // Re-read info in case it was populated after construction.
                        var i = _typeInfoCache[typeName] || _emptyInfo;
                        // Write-only: has setter but no getter — reading it is an error.
                        if (i.writeonlyProperties && i.writeonlyProperties.indexOf(prop) >= 0)
                            throw new TypeError('Cannot read write-only property \'' + prop + '\' of .NET type \'' + typeName + '\'');
                        if (i.properties && i.properties.indexOf(prop) >= 0)
                            return _wrap(_invoke({ handle: handle, method: 'get_' + prop, args: [] }));
                        // Not a native property — return a callable for method dispatch.
                        return function () {
                            var args = Array.prototype.slice.call(arguments).map(_unwrap);
                            return _wrap(_invoke({ handle: handle, method: prop, args: args }));
                        };
                    },
                    set: function (_, prop, value) {
                        if (typeof prop === 'symbol') return true;
                        var i = _typeInfoCache[typeName] || _emptyInfo;
                        // Read-only: has getter but no setter — assignment is an error.
                        if (i.readonlyProperties && i.readonlyProperties.indexOf(prop) >= 0)
                            throw new TypeError('Cannot assign to read-only property \'' + prop + '\' of .NET type \'' + typeName + '\'');
                        // Writable (read-write or write-only) — invoke the setter.
                        if ((i.properties && i.properties.indexOf(prop) >= 0) ||
                            (i.writeonlyProperties && i.writeonlyProperties.indexOf(prop) >= 0))
                            _invoke({ handle: handle, method: 'set_' + prop, args: [_unwrap(value)] });
                        // Not a native property — don't intercept, let JS do its thing.
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
                    var root = (typeName || '').split('.')[0];
                    var assembly = _namespaceAssemblyMap[root] || '';
                    return _makeDotNetInstance(value.__handle, assembly, typeName, value.__isTask === true, value.__native_ptr);
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
                    var root = (typeName || '').split('.')[0];
                    var assembly = _namespaceAssemblyMap[root] || '';
                    return _makeDotNetInstance(handle, assembly, typeName || '');
                },
                // Lazily register a top-level namespace root and optional assembly mapping.
                // Example: NSWinRT.dotnet.registerNamespace('com', 'NativeScript');
                registerNamespace: function(rootName, assemblyName) {
                    if (!rootName || typeof rootName !== 'string') return;
                    var root = String(rootName).split('.')[0];
                    if (assemblyName && typeof assemblyName === 'string') {
                        _namespaceAssemblyMap[root] = assemblyName;
                    }
                    if (root in globalThis) return;
                    try {
                        Object.defineProperty(globalThis, root, {
                            configurable: true,
                            enumerable: true,
                            get: function() {
                                var proxy = _makeNamespaceProxy(root);
                                try { Object.defineProperty(globalThis, root, { value: proxy, writable: true, configurable: true, enumerable: true }); } catch (_) {}
                                return proxy;
                            }
                        });
                    } catch (_) {
                        try { globalThis[root] = _makeNamespaceProxy(root); } catch (_) {}
                    }
                },
                registerNamespaces: function(arr) {
                    if (!Array.isArray(arr)) return;
                    for (var i = 0; i < arr.length; i++) this.registerNamespace(arr[i]);
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
                        // Prevent string-coercion methods from descending into sub-proxies.
                        // V8's console.log / JSON.stringify call toString/valueOf on unknown objects;
                        // returning a sub-proxy causes the apply trap to fire with a non-existent
                        // type name, producing a spurious "Type not found" bridge error.
                        if (prop === 'toString' || prop === 'valueOf')
                            return function() { return '[.NET ' + path + ']'; };
                        var root = path.split('.')[0];
                        var assembly = _namespaceAssemblyMap[root] || '';
                        var info = _getTypeInfo(assembly, path);
                        // Write-only static property — reading it is an error.
                        if (info.writeonlyStaticProperties && info.writeonlyStaticProperties.indexOf(prop) >= 0)
                            throw new TypeError('Cannot read write-only property \'' + prop + '\' of .NET type \'' + path + '\'');
                        // Readable static property: resolve value immediately.
                        if (info.staticProperties && info.staticProperties.indexOf(prop) >= 0)
                            return _wrap(_invoke({ assembly: assembly, typeName: path, method: 'get_' + prop, args: [] }));
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
                    set: function (_, prop, value) {
                        if (typeof prop === 'symbol') return true;
                        var root = path.split('.')[0];
                        var assembly = _namespaceAssemblyMap[root] || '';
                        var info = _getTypeInfo(assembly, path);
                        // Read-only static property — assignment is an error.
                        if (info.readonlyStaticProperties && info.readonlyStaticProperties.indexOf(prop) >= 0)
                            throw new TypeError('Cannot assign to read-only property \'' + prop + '\' of .NET type \'' + path + '\'');
                        // Writable (read-write or write-only) — invoke the setter.
                        if ((info.staticProperties && info.staticProperties.indexOf(prop) >= 0) ||
                            (info.writeonlyStaticProperties && info.writeonlyStaticProperties.indexOf(prop) >= 0))
                            _invoke({ assembly: assembly, typeName: path, method: 'set_' + prop, args: [_unwrap(value)] });
                        // Not a native property — don't intercept.
                        return true;
                    },
                    apply: function (_, _this, args) {
                        var lastDot  = path.lastIndexOf('.');
                        // No dot means this is a top-level name being called directly —
                        // there is no type+method pair to dispatch, so bail out.
                        if (lastDot <= 0) return undefined;
                        var typeName = path.substring(0, lastDot);
                        var method   = path.substring(lastDot + 1);
                        var root = typeName.split('.')[0];
                        var assembly = _namespaceAssemblyMap[root] || '';
                        return _wrap(_invoke({ assembly: assembly, typeName: typeName, method: method, args: args.map(_unwrap) }));
                    },
                    construct: function (_, args) {
                        var root = path.split('.')[0];
                        var assembly = _namespaceAssemblyMap[root] || '';
                        return _wrap(_invoke({ assembly: assembly, typeName: path, method: '.ctor', args: args.map(_unwrap) }));
                    },
                });
            }
            globalThis.System         = _makeNamespaceProxy('System');
            globalThis.Microsoft      = _makeNamespaceProxy('Microsoft');
            globalThis.NativeScript   = _makeNamespaceProxy('NativeScript');

            // ── NSWinRT.dotnet.asDelegate ────────────────────────────────────
            // Creates a typed .NET BCL delegate (not a WinRT COM delegate).
            // Use this when the API expects a managed System.Action,
            // System.EventHandler, etc. rather than a WinRT delegate interface.
            // For WinRT delegate types use NSWinRT.asDelegate instead.
            if (typeof globalThis.__nsDotNetAwaitTask === 'function') {
                globalThis.NSWinRT.dotnet.taskToPromise = function (obj) {
                    var h = obj && typeof obj.__handle === 'number' ? obj.__handle
                          : typeof obj === 'number' ? obj : -1;
                    if (h < 0) return Promise.resolve(obj);
                    return new Promise(function (resolve, reject) {
                        globalThis.__nsDotNetAwaitTask(h, resolve, reject);
                    });
                };
            }

            if (typeof globalThis.__nsDotNetCreateDelegate === 'function') {
                globalThis.NSWinRT.dotnet.asDelegate = function(typeNameOrFn, fn) {
                    var typeName, callback;
                    if (typeof typeNameOrFn === 'function') {
                        typeName = '';
                        callback = typeNameOrFn;
                    } else {
                        typeName = typeNameOrFn || '';
                        callback = fn;
                    }
                    if (typeof callback !== 'function')
                        throw new TypeError('NSWinRT.dotnet.asDelegate: callback must be a function');
                    var wrapped = function() {
                        var args = Array.prototype.slice.call(arguments).map(_wrap);
                        return callback.apply(null, args);
                    };
                    return globalThis.__nsDotNetCreateDelegate(typeName, wrapped);
                };
            }

            if (typeof globalThis.__nsRunOnUIThread === 'function') {
                globalThis.NSWinRT.runOnUIThread = function(fn) {
                    if (typeof fn !== 'function') throw new TypeError('NSWinRT.runOnUIThread: expected a function');
                    return globalThis.__nsRunOnUIThread(fn);
                };
            }
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
                    if (typeof globalThis.__nsWin32CallRaw === 'function') {
                        var rawArgs = [dll, fn_name, returnType];
                        for (var i = 0; i < args.length; i++) {
                            rawArgs.push(args[i].type, args[i].value);
                        }
                        return globalThis.__nsWin32CallRaw.apply(null, rawArgs);
                    }
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

        // ── NSWinRT.asDelegate ───────────────────────────────────────────────
        // Wraps a JS function as a typed WinRT delegate via the native JsDelegate
        // COM bridge using WinRT metadata for GUID and parameter-type resolution.
        //
        //   NSWinRT.asDelegate('Windows.UI.Popups.UICommandInvokedHandler', fn)
        //
        // Returns {handle} which can be passed directly to any WinRT event-add
        // or method that expects that delegate type.
        // Callback parameters that carry COM object pointers arrive as
        // {handle: External}; wrap them with the appropriate WinRT constructor
        // to get a full proxy.
        // For managed BCL delegates (System.Action etc.) use
        // NSWinRT.dotnet.asDelegate() instead.
        (function () {
            globalThis.NSWinRT.asDelegate = function(typeNameOrFn, fn) {
                // Two-argument form: asDelegate(typeName, fn) → COM-backed JsDelegate
                if (typeof typeNameOrFn === 'string') {
                    if (typeof fn !== 'function')
                        throw new TypeError('NSWinRT.asDelegate: callback must be a function');
                    if (typeof globalThis.__nsAsDelegate === 'function')
                        return globalThis.__nsAsDelegate(typeNameOrFn, fn);
                    return fn;
                }
                // One-argument form: asDelegate(fn) or asDelegate({invoke(){}})
                if (typeof typeNameOrFn === 'function') {
                    return typeNameOrFn;
                }
                if (typeNameOrFn && typeof typeNameOrFn.invoke === 'function') {
                    return typeNameOrFn.invoke.bind(typeNameOrFn);
                }
                throw new TypeError('NSWinRT.asDelegate: expected a function, { invoke() } object, or (typeName, fn) pair');
            };
        })();

        // ── CommonJS require() ────────────────────────────────────────────────
        (function () {
            var cjsCache = new Map();

            function resolveSpecifier(specifier, callerFile) {
                if (typeof globalThis.__nsResolveModulePath !== 'function') return null;
                var appRoot = (globalThis.__nsAppRoot || '').replace(/[\\/]+$/, '');

                // NativeScript tilde alias: ~/foo → {appRoot}/app/foo
                if (specifier.charAt(0) === '~' && specifier.charAt(1) === '/') {
                    var abs = appRoot + '\\app\\' + specifier.substring(2).replace(/\//g, '\\');
                    // Pass absolute path; resolver treats it as-is and tries extensions.
                    return globalThis.__nsResolveModulePath(abs, '', appRoot) || abs;
                }

                // Relative (./foo, ../foo) or bare name: use native resolver with caller context.
                // Fall back to app/bundle.js as parent so top-level require('./chunk.js') works.
                var parent = callerFile || (appRoot + '\\app\\bundle.js');
                return globalThis.__nsResolveModulePath(specifier, parent, appRoot);
            }

            function makeRequire(callerFile) {
                return function require(specifier) {
                    var resolved = resolveSpecifier(specifier, callerFile);
                    if (!resolved) throw new Error('Cannot find module: ' + specifier);

                    var key = resolved.replace(/\\/g, '/').toLowerCase();
                    if (cjsCache.has(key)) return cjsCache.get(key).exports;

                    var mod = { id: resolved, filename: resolved, exports: {} };
                    cjsCache.set(key, mod); // set before eval to break circular deps

                    var isJson = key.endsWith('.json');
                    var content = typeof globalThis.__nsReadTextFile === 'function'
                        ? globalThis.__nsReadTextFile(resolved) : null;

                    if (isJson) {
                        try { mod.exports = JSON.parse(content || '{}'); } catch (_) { mod.exports = {}; }
                        return mod.exports;
                    }

                    if (content === null || content === undefined)
                        throw new Error('Failed to read module: ' + resolved);

                    var dirName = resolved.replace(/\//g, '\\').replace(/\\[^\\]*$/, '');
                    var childRequire = makeRequire(resolved);

                    try {
                        var factory = new Function('module', 'exports', 'require', '__filename', '__dirname', content);
                        factory(mod, mod.exports, childRequire, resolved, dirName);
                    } catch (e) {
                        cjsCache.delete(key);
                        throw e;
                    }
                    cjsCache.set(key, mod);
                    return mod.exports;
                };
            }

            globalThis.require = makeRequire(null);

            // Top-level CJS globals for scripts executed outside a factory wrapper
            // (e.g., when the host calls runtime_runscript directly with a CJS file).
            if (typeof globalThis.module === 'undefined') {
                var _topMod = { id: '<main>', exports: {} };
                Object.defineProperty(globalThis, 'module',  { value: _topMod, writable: true, configurable: true });
                Object.defineProperty(globalThis, 'exports', { value: _topMod.exports, writable: true, configurable: true });
            }

            // Provide __dirname / __filename globals for webpack target:'node' bundles.
            // webpack leaves these undefined when building for node (expects Node.js to
            // provide them via its module wrapper).  Our runtime is not Node.js, so we
            // supply the app directory as a reasonable fallback value.  Modules that need
            // the exact source path will still work because @nativescript/core/globals only
            // stores the value on `global.__dirname` for downstream use.
            if (typeof globalThis.__dirname === 'undefined') {
                var _appRoot2 = (globalThis.__nsAppRoot || '').replace(/[\\/]+$/, '');
                var _appDir = _appRoot2 + '\\app';
                Object.defineProperty(globalThis, '__dirname',  { value: _appDir, writable: true, configurable: true });
                Object.defineProperty(globalThis, '__filename', { value: _appDir + '\\bundle.js', writable: true, configurable: true });
            }
        })();
        
        (function () {
            if (typeof WeakRef === 'undefined') return;
            WeakRef.prototype.get = WeakRef.prototype.deref;
            WeakRef.prototype.__hasWarnedAboutClear = false;
            WeakRef.prototype.clear = function () {
                if (WeakRef.prototype.__hasWarnedAboutClear) return;
                WeakRef.prototype.__hasWarnedAboutClear = true;
                console.warn('WeakRef.clear() is non-standard and has been deprecated. It does nothing and the call can be safely removed.');
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

// ── __nsDotNetInvokeBin ───────────────────────────────────────────────────────

/// Binary-protocol bridge: builds a compact request packet from structured V8
/// arguments, calls the C# InvokeBinary entry point, and converts the binary
/// response directly into a V8 value — no JSON.stringify / JSON.parse on either
/// side.
///
/// Argument convention (matches the JS _invoke wrapper):
///   args[0]  handle: i32 (≥0) or null/undefined (−1 ⇒ static)
///   args[1]  typeName: string  (empty for instance calls)
///   args[2]  assembly: string  (empty for instance calls)
///   args[3]  method:   string
///   args[4…] method arguments as raw JS values
pub(crate) fn handle_dotnet_invoke_binary(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 4 {
        throw_js_error(scope, "__nsDotNetInvokeBin: expected (handle, typeName, assembly, method, ...args)");
        return;
    }

    let handle_v8 = args.get(0);
    let handle: i32 = if handle_v8.is_null_or_undefined() {
        -1
    } else if let Ok(n) = v8::Local::<v8::Integer>::try_from(handle_v8) {
        n.value() as i32
    } else if let Ok(n) = v8::Local::<v8::Number>::try_from(handle_v8) {
        n.value() as i32
    } else {
        -1
    };

    let type_name = args.get(1)
        .to_string(scope)
        .map(|s| s.to_rust_string_lossy(scope))
        .unwrap_or_default();

    let assembly = args.get(2)
        .to_string(scope)
        .map(|s| s.to_rust_string_lossy(scope))
        .unwrap_or_default();

    let method = args.get(3)
        .to_string(scope)
        .map(|s| s.to_rust_string_lossy(scope))
        .unwrap_or_default();

    let mut req: Vec<u8> = Vec::with_capacity(64);

    // Determine opcode.
    let op: u8 = if handle >= 0 {
        match method.as_str() {
            "__release"   => 0x04,
            "__members__" => 0x05,
            _             => 0x01,
        }
    } else {
        match method.as_str() {
            "__members__" => 0x06,
            ".ctor"       => 0x03,
            _             => 0x02,
        }
    };

    req.push(op);

    match op {
        0x01 | 0x04 | 0x05 => req.extend_from_slice(&handle.to_le_bytes()),
        _ => {
            bin_write_str16(&mut req, type_name.as_bytes());
            bin_write_str16(&mut req, assembly.as_bytes());
        }
    }

    if op == 0x01 || op == 0x02 {
        bin_write_str16(&mut req, method.as_bytes());
    }

    if matches!(op, 0x01 | 0x02 | 0x03) {
        let arg_count = (args.length() - 4).max(0) as u8;
        req.push(arg_count);
        for i in 4..4 + arg_count as i32 {
            bin_write_v8_arg(&mut req, scope, args.get(i));
        }
    }

    match crate::dotnet::call_dotnet_binary(&req) {
        Ok(response) => match bin_read_response(scope, &response) {
            Ok(v8_val) => retval.set(v8_val),
            Err(e)     => throw_js_error(scope, &e),
        },
        Err(e) => throw_js_error(scope, &e),
    }
}

// ── __nsDotNetCreateDelegate ──────────────────────────────────────────────────

/// Called by the Rust runtime (via stored function pointer) when a managed .NET
/// delegate fires.  Runs on the V8/JS isolate thread — same pattern as the
/// existing WinRT JsDelegate COM bridge.
///
/// Wire format for args_ptr: [count: u8] [tagged values...]  (response-tag scheme)
pub(crate) unsafe extern "C" fn invoke_dotnet_js_callback(
    callback_id: i32,
    args_ptr: *const u8,
    args_len: i32,
    _resp_ptr: *mut *mut u8,
    _resp_len: *mut i32,
) {
    let isolate_ptr = crate::DELEGATE_ISOLATE_PTR.with(|c| c.get());
    if isolate_ptr.is_null() { return; }

    let isolate: &mut v8::Isolate = &mut *isolate_ptr;
    v8::scope!(scope, isolate);
    let ctx_global = match scope.get_slot::<v8::Global<v8::Context>>() {
        Some(g) => g.clone(),
        None => return,
    };
    let context = v8::Local::new(scope, &ctx_global);
    let scope = &mut v8::ContextScope::new(scope, context);
    v8::tc_scope!(tc, scope);

    let func_global = crate::DOTNET_JS_CALLBACKS.with(|m| {
        m.borrow().get(&callback_id).cloned()
    });
    let Some(func_global) = func_global else { return; };
    let func = v8::Local::new(tc, &func_global);
    let recv: v8::Local<v8::Value> = v8::undefined(tc).into();

    let args_slice = if args_len > 0 {
        std::slice::from_raw_parts(args_ptr, args_len as usize)
    } else {
        &[]
    };
    let js_args = parse_dotnet_callback_args(tc, args_slice);

    let result_val = func.call(tc, recv, &js_args);
    if tc.has_caught() {
        if let Some(ex) = tc.exception() {
            let msg = ex.to_rust_string_lossy(tc);
            crate::store_last_js_error(msg);
        }
        tc.reset();
    }
    // Serialize the returned value (if any) into the binary response format
    // and return it to managed via resp_ptr/resp_len. Managed will call
    // Bridge.Free (Marshal.FreeHGlobal) to release this memory, so allocate
    // with the Windows LocalAlloc/LMEM_FIXED allocator for compatibility.
    let mut out_buf: Vec<u8> = Vec::new();
    match result_val {
        Some(rv) => {
            // Convert the returned V8 value into a single tagged binary value.
            bin_write_v8_value(&mut out_buf, tc, rv);
        }
        None => {
            // No return value: null tag.
            out_buf.push(0x00u8);
        }
    }

    if out_buf.is_empty() {
        // No response: leave resp pointers null / zero.
        if !_resp_ptr.is_null() { *_resp_ptr = std::ptr::null_mut(); }
        if !_resp_len.is_null() { *_resp_len = 0; }
    } else {
        // Allocate memory using LocalAlloc so managed Marshal.FreeHGlobal can free it.
        let size = out_buf.len();
        unsafe {
            let p = LocalAlloc(LMEM_FIXED, size);
            if p.is_null() {
                if !_resp_ptr.is_null() { *_resp_ptr = std::ptr::null_mut(); }
                if !_resp_len.is_null() { *_resp_len = 0; }
            } else {
                let dest = p as *mut u8;
                std::ptr::copy_nonoverlapping(out_buf.as_ptr(), dest, size);
                if !_resp_ptr.is_null() { *_resp_ptr = dest; }
                if !_resp_len.is_null() { *_resp_len = size as i32; }
            }
        }
    }
    // If this callback was registered as a one-shot, remove it now to avoid leaks.
    crate::DOTNET_ONESHOT_JS_CALLBACKS.with(|s| {
        let mut set = s.borrow_mut();
        if set.contains(&callback_id) {
            // Remove the stored JS function and clear the oneshot marker.
            crate::DOTNET_JS_CALLBACKS.with(|m| { m.borrow_mut().remove(&callback_id); });
            set.remove(&callback_id);
        }
    });
}

fn parse_dotnet_callback_args<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    bytes: &[u8],
) -> Vec<v8::Local<'s, v8::Value>> {
    if bytes.is_empty() { return vec![]; }
    let count = bytes[0] as usize;
    let mut pos = 1usize;
    let mut result = Vec::with_capacity(count);
    for _ in 0..count {
        if pos >= bytes.len() { break; }
        match bin_read_value(scope, bytes, &mut pos) {
            Ok(v) => result.push(v),
            Err(_) => break,
        }
    }
    result
}

/// Registers a JS function as a typed .NET delegate and returns the managed
/// handle.  The handle can be passed to any .NET method that accepts a delegate.
///
/// Args: args[0] = typeName (string, "" → System.Action), args[1] = function
pub(crate) fn handle_dotnet_create_delegate(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 2 {
        throw_js_error(scope, "__nsDotNetCreateDelegate(typeName, fn): expected 2 arguments");
        return;
    }

    let type_name = args.get(0)
        .to_string(scope)
        .map(|s| s.to_rust_string_lossy(scope))
        .unwrap_or_default();

    let Ok(func) = v8::Local::<v8::Function>::try_from(args.get(1)) else {
        throw_js_error(scope, "__nsDotNetCreateDelegate: second argument must be a function");
        return;
    };

    let cb_id = crate::DOTNET_NEXT_CB_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    crate::DOTNET_JS_CALLBACKS.with(|m| {
        m.borrow_mut().insert(cb_id, v8::Global::new(scope, func));
    });

    // opcode 0x09 | type_name (str16) | callback_id (i32)
    let mut req: Vec<u8> = Vec::with_capacity(32);
    req.push(0x09);
    bin_write_str16(&mut req, type_name.as_bytes());
    req.extend_from_slice(&cb_id.to_le_bytes());

    match crate::dotnet::call_dotnet_binary(&req) {
        Ok(response) => match bin_read_response(scope, &response) {
            Ok(v)  => retval.set(v),
            Err(e) => throw_js_error(scope, &e),
        },
        Err(e) => throw_js_error(scope, &e),
    }
}

/// Create a managed subclass instance backed by JS overrides.
/// Args: args[0] = assembly (string or ''), args[1] = typeName (string), args[2] = function or object (callable to receive invocations)
pub(crate) fn handle_dotnet_create_js_subclass(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 3 {
        throw_js_error(scope, "__nsDotNetCreateJsSubclass(assembly, typeName, fn): expected 3 arguments");
        return;
    }

    let assembly = args.get(0)
        .to_string(scope)
        .map(|s| s.to_rust_string_lossy(scope))
        .unwrap_or_default();

    let type_name = args.get(1)
        .to_string(scope)
        .map(|s| s.to_rust_string_lossy(scope))
        .unwrap_or_default();

    // Accept a function as the callback.
    let cb_val = args.get(2);
    let Ok(cb_fn) = v8::Local::<v8::Function>::try_from(cb_val) else {
        throw_js_error(scope, "__nsDotNetCreateJsSubclass: third argument must be a function");
        return;
    };

    let cb_id = crate::DOTNET_NEXT_CB_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    crate::DOTNET_JS_CALLBACKS.with(|m| {
        m.borrow_mut().insert(cb_id, v8::Global::new(scope, cb_fn));
    });

    let mut req: Vec<u8> = Vec::with_capacity(64);
    req.push(0x0A);
    bin_write_str16(&mut req, assembly.as_bytes());
    bin_write_str16(&mut req, type_name.as_bytes());
    req.extend_from_slice(&cb_id.to_le_bytes());

    match crate::dotnet::call_dotnet_binary(&req) {
        Ok(response) => match bin_read_response(scope, &response) {
            Ok(v)  => retval.set(v),
            Err(e) => throw_js_error(scope, &e),
        },
        Err(e) => throw_js_error(scope, &e),
    }
}

fn bin_write_str16(buf: &mut Vec<u8>, bytes: &[u8]) {
    buf.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
    buf.extend_from_slice(bytes);
}

pub(crate) fn handle_dotnet_await_task(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    _retval: v8::ReturnValue,
) {
    if args.length() < 3 {
        throw_js_error(scope, "__nsDotNetAwaitTask(handleId, resolve, reject): expected 3 arguments");
        return;
    }

    let handle_id: i32 = if let Ok(n) = v8::Local::<v8::Integer>::try_from(args.get(0)) {
        n.value() as i32
    } else if let Ok(n) = v8::Local::<v8::Number>::try_from(args.get(0)) {
        n.value() as i32
    } else {
        throw_js_error(scope, "__nsDotNetAwaitTask: first argument must be a handle id (integer)");
        return;
    };

    let Ok(resolve_fn) = v8::Local::<v8::Function>::try_from(args.get(1)) else {
        throw_js_error(scope, "__nsDotNetAwaitTask: second argument must be a resolve function");
        return;
    };
    let Ok(reject_fn) = v8::Local::<v8::Function>::try_from(args.get(2)) else {
        throw_js_error(scope, "__nsDotNetAwaitTask: third argument must be a reject function");
        return;
    };

    let resolve_id = crate::DOTNET_NEXT_CB_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let reject_id  = crate::DOTNET_NEXT_CB_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    crate::DOTNET_JS_CALLBACKS.with(|m| {
        let mut map = m.borrow_mut();
        map.insert(resolve_id, v8::Global::new(scope, resolve_fn));
        map.insert(reject_id,  v8::Global::new(scope, reject_fn));
    });

    // Binary instance call: 0x01 | handle(i32) | "__dotnet_await__"(str16) | 2 | i32 resolveId | i32 rejectId
    let mut req: Vec<u8> = Vec::with_capacity(32);
    req.push(0x01u8);
    req.extend_from_slice(&handle_id.to_le_bytes());
    bin_write_str16(&mut req, b"__dotnet_await__");
    req.push(2u8);
    req.push(0x03u8); req.extend_from_slice(&resolve_id.to_le_bytes());
    req.push(0x03u8); req.extend_from_slice(&reject_id.to_le_bytes());

    if let Err(e) = crate::dotnet::call_dotnet_binary(&req) {
        crate::DOTNET_JS_CALLBACKS.with(|m| {
            let mut map = m.borrow_mut();
            map.remove(&resolve_id);
            map.remove(&reject_id);
        });
        throw_js_error(scope, &e);
    }
}

fn bin_write_v8_arg(buf: &mut Vec<u8>, scope: &mut v8::PinScope<'_, '_>, arg: v8::Local<v8::Value>) {
    if arg.is_null_or_undefined() { buf.push(0x00); return; }

    if arg.is_boolean() {
        buf.push(if arg.is_true() { 0x02 } else { 0x01 });
        return;
    }

    // Integer (Smi) check before general Number to preserve i32 tag where possible.
    if let Ok(n) = v8::Local::<v8::Integer>::try_from(arg) {
        let v = n.value();
        if v >= i32::MIN as i64 && v <= i32::MAX as i64 {
            buf.push(0x03);
            buf.extend_from_slice(&(v as i32).to_le_bytes());
            return;
        }
    }

    if let Ok(n) = v8::Local::<v8::Number>::try_from(arg) {
        buf.push(0x04);
        buf.extend_from_slice(&n.value().to_bits().to_le_bytes());
        return;
    }

    if arg.is_string() {
        if let Some(s) = arg.to_string(scope) {
            let bytes = s.to_rust_string_lossy(scope).into_bytes();
            buf.push(0x05);
            bin_write_str16(buf, &bytes);
            return;
        }
    }

    if arg.is_object() {
        if let Some(obj) = arg.to_object(scope) {
            // Dotnet bridge handle: { __handle: i32 }
            if let Some(key) = v8::String::new(scope, "__handle") {
                if let Some(hval) = obj.get(scope, key.into()) {
                    if let Ok(n) = v8::Local::<v8::Integer>::try_from(hval) {
                        buf.push(0x06);
                        buf.extend_from_slice(&(n.value() as i32).to_le_bytes());
                        return;
                    }
                    if let Ok(n) = v8::Local::<v8::Number>::try_from(hval) {
                        buf.push(0x06);
                        buf.extend_from_slice(&(n.value() as i32).to_le_bytes());
                        return;
                    }
                }
            }
            // WinRT COM pointer: BigInt exposed as __native_ptr on WinRT proxy objects.
            // Encoded as tag 0x0A + 8-byte little-endian u64 so C# can call
            // Marshal.GetObjectForIUnknown to obtain a managed RCW.
            if let Some(key) = v8::String::new(scope, "__native_ptr") {
                if let Some(pval) = obj.get(scope, key.into()) {
                    if let Ok(bi) = v8::Local::<v8::BigInt>::try_from(pval) {
                        let (ptr, _) = bi.u64_value();
                        if ptr != 0 {
                            // Ensure ownership semantics: AddRef the raw COM pointer
                            // before emitting it to managed code so the managed side
                            // can safely wrap and Release it later.
                            unsafe {
                                let unknown = ManuallyDrop::new(IUnknown::from_raw(ptr as *mut _));
                                let vtable = unknown.vtable();
                                ((*vtable).AddRef)(unknown.as_raw());
                            }
                            buf.push(0x0A);
                            buf.extend_from_slice(&ptr.to_le_bytes());
                            return;
                        }
                    }
                }
            }
        }

        // Fallback: stringify object
        if let Some(sv) = arg.to_string(scope) {
            let bytes = sv.to_rust_string_lossy(scope).into_bytes();
            buf.push(0x05);
            bin_write_str16(buf, &bytes);
            return;
        }
        }

    // Final fallback: null
    buf.push(0x00);
    }

fn bin_write_str32(buf: &mut Vec<u8>, bytes: &[u8]) {
    buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    buf.extend_from_slice(bytes);
}

fn bin_write_v8_value(buf: &mut Vec<u8>, scope: &mut v8::PinScope<'_, '_>, arg: v8::Local<v8::Value>) {
    if arg.is_null_or_undefined() { buf.push(0x00); return; }
    if arg.is_boolean() { buf.push(if arg.is_true() { 0x02 } else { 0x01 }); return; }

    // Integer before general number
    if let Ok(n) = v8::Local::<v8::Integer>::try_from(arg) {
        let v = n.value();
        if v >= i32::MIN as i64 && v <= i32::MAX as i64 {
            buf.push(0x03);
            buf.extend_from_slice(&(v as i32).to_le_bytes());
            return;
        }
    }
    if let Ok(n) = v8::Local::<v8::Number>::try_from(arg) {
        buf.push(0x04);
        buf.extend_from_slice(&n.value().to_bits().to_le_bytes());
        return;
    }

    if arg.is_string() {
        if let Some(s) = arg.to_string(scope) {
            let bytes = s.to_rust_string_lossy(scope).into_bytes();
            buf.push(0x05);
            bin_write_str32(buf, &bytes);
            return;
        }
    }

    if arg.is_array() {
        if let Some(arr) = v8::Local::<v8::Array>::try_from(arg).ok() {
            let len = arr.length() as u32;
            buf.push(0x07);
            buf.extend_from_slice(&len.to_le_bytes());
            for i in 0..len {
                if let Some(it) = arr.get_index(scope, i) {
                    bin_write_v8_value(buf, scope, it);
                } else {
                    buf.push(0x00);
                }
            }
            return;
        }
    }

    if arg.is_object() {
        if let Some(obj) = arg.to_object(scope) {
            // Dotnet bridge handle: { __handle: i32 }
            if let Some(key) = v8::String::new(scope, "__handle") {
                if let Some(hval) = obj.get(scope, key.into()) {
                    if let Ok(n) = v8::Local::<v8::Integer>::try_from(hval) {
                        buf.push(0x06);
                        buf.extend_from_slice(&(n.value() as i32).to_le_bytes());
                        return;
                    }
                    if let Ok(n) = v8::Local::<v8::Number>::try_from(hval) {
                        buf.push(0x06);
                        buf.extend_from_slice(&(n.value() as i32).to_le_bytes());
                        return;
                    }
                }
            }
            // WinRT COM pointer: __native_ptr BigInt -> 0x0A + 8-byte u64
            if let Some(key) = v8::String::new(scope, "__native_ptr") {
                if let Some(pval) = obj.get(scope, key.into()) {
                    if let Ok(bi) = v8::Local::<v8::BigInt>::try_from(pval) {
                        let (ptr, _) = bi.u64_value();
                        if ptr != 0 {
                            // Emit native pointer tag (0x0A) + 8-byte pointer value.
                            buf.push(0x0A);
                            buf.extend_from_slice(&ptr.to_le_bytes());
                            return;
                        }
                    }
                }
            }
            // fallback: stringify
            if let Some(sv) = arg.to_string(scope) {
                let bytes = sv.to_rust_string_lossy(scope).into_bytes();
                buf.push(0x05);
                bin_write_str32(buf, &bytes);
                return;
            }
        }
    }

    // final fallback: null
    buf.push(0x00);
}

fn bin_read_response<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    bytes: &[u8],
) -> Result<v8::Local<'s, v8::Value>, String> {
    if bytes.is_empty() { return Ok(v8::null(scope).into()); }
    let mut pos = 0usize;
    bin_read_value(scope, bytes, &mut pos)
}

fn bin_read_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    bytes: &[u8],
    pos: &mut usize,
) -> Result<v8::Local<'s, v8::Value>, String> {
    let tag = *bytes.get(*pos).ok_or("response truncated")?;
    *pos += 1;

    match tag {
        0x00 => Ok(v8::null(scope).into()),
        0x01 => Ok(v8::Boolean::new(scope, false).into()),
        0x02 => Ok(v8::Boolean::new(scope, true).into()),

        0x03 => {
            let v = i32::from_le_bytes(bytes[*pos..*pos+4].try_into().map_err(|_| "i32 read")?);
            *pos += 4;
            Ok(v8::Integer::new(scope, v).into())
        }

        0x04 => {
            let bits = u64::from_le_bytes(bytes[*pos..*pos+8].try_into().map_err(|_| "f64 read")?);
            *pos += 8;
            Ok(v8::Number::new(scope, f64::from_bits(bits)).into())
        }

        0x05 => { // string: u32 len + utf8
            let len = u32::from_le_bytes(bytes[*pos..*pos+4].try_into().map_err(|_| "str len")?) as usize;
            *pos += 4;
            let s = std::str::from_utf8(&bytes[*pos..*pos+len]).map_err(|_| "utf8")?;
            *pos += len;
            Ok(v8::String::new(scope, s).map(Into::into).unwrap_or_else(|| v8::null(scope).into()))
        }

        0x06 | 0x0C => { // handle → JS object {__handle, __type} ; 0x0C adds __isTask:true
            let id = i32::from_le_bytes(bytes[*pos..*pos+4].try_into().map_err(|_| "handle id")?);
            *pos += 4;
            let type_len = u16::from_le_bytes(bytes[*pos..*pos+2].try_into().map_err(|_| "type len")?) as usize;
            *pos += 2;
            let type_name = std::str::from_utf8(&bytes[*pos..*pos+type_len]).map_err(|_| "utf8")?;
            *pos += type_len;

            let obj = v8::Object::new(scope);
            let hk = v8::String::new(scope, "__handle").ok_or("v8 str")?;
            let tk = v8::String::new(scope, "__type").ok_or("v8 str")?;
            let tv = v8::String::new(scope, type_name).ok_or("v8 str")?;
            obj.set(scope, hk.into(), v8::Integer::new(scope, id).into());
            obj.set(scope, tk.into(), tv.into());
            if tag == 0x0C {
                let ik = v8::String::new(scope, "__isTask").ok_or("v8 str")?;
                obj.set(scope, ik.into(), v8::Boolean::new(scope, true).into());
            }

            // Optional native pointer included by the bridge.
            // Read a presence flag byte, then a little-endian i64 pointer.
            if bytes.len() - *pos >= 1 {
                let flag = bytes[*pos];
                *pos += 1;
                if flag != 0 {
                    if bytes.len() - *pos >= 8 {
                        let raw = i64::from_le_bytes(bytes[*pos..*pos+8].try_into().map_err(|_| "i64 read")?);
                        *pos += 8;
                        let nk = v8::String::new(scope, "__native_ptr").ok_or("v8 str")?;
                        if raw >= 0 {
                            obj.set(scope, nk.into(), v8::BigInt::new_from_u64(scope, raw as u64).into());
                        } else {
                            obj.set(scope, nk.into(), v8::BigInt::new_from_i64(scope, raw).into());
                        }
                    }
                }
            }
            Ok(obj.into())
        }

        0x07 => { // array: u32 count + N items
            let count = u32::from_le_bytes(bytes[*pos..*pos+4].try_into().map_err(|_| "arr count")?) as usize;
            *pos += 4;
            let arr = v8::Array::new(scope, count as i32);
            for i in 0..count {
                let item = bin_read_value(scope, bytes, pos)?;
                arr.set_index(scope, i as u32, item);
            }
            Ok(arr.into())
        }

        0x08 => { // members: 8 string arrays — methods/props/staticMethods/staticProps + readonly/writeonly variants
            let obj = v8::Object::new(scope);
            for key in ["methods", "properties", "staticMethods", "staticProperties",
                        "readonlyProperties", "readonlyStaticProperties",
                        "writeonlyProperties", "writeonlyStaticProperties"] {
                let count = u16::from_le_bytes(bytes[*pos..*pos+2].try_into().map_err(|_| "member count")?) as usize;
                *pos += 2;
                let arr = v8::Array::new(scope, count as i32);
                for i in 0..count {
                    let len = u16::from_le_bytes(bytes[*pos..*pos+2].try_into().map_err(|_| "str len")?) as usize;
                    *pos += 2;
                    let s = std::str::from_utf8(&bytes[*pos..*pos+len]).map_err(|_| "utf8")?;
                    *pos += len;
                    if let Some(sv) = v8::String::new(scope, s) {
                        arr.set_index(scope, i as u32, sv.into());
                    }
                }
                let k = v8::String::new(scope, key).ok_or("v8 str")?;
                obj.set(scope, k.into(), arr.into());
            }
            Ok(obj.into())
        }

        0xFF => { // error: u32 len + utf8 message
            let len = u32::from_le_bytes(bytes[*pos..*pos+4].try_into().map_err(|_| "err len")?) as usize;
            *pos += 4;
            let msg = std::str::from_utf8(&bytes[*pos..*pos+len]).map_err(|_| "utf8")?;
            Err(msg.to_string())
        }

        t => Err(format!("Unknown binary response tag 0x{t:02X}")),
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

// ── __tns_uptime
pub(crate) fn handle_tns_uptime(
    _scope: &mut v8::PinScope<'_, '_>,
    _args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
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

// ── __nsIsUiThread ───────────────────────────────────────────────────────────

pub(crate) fn handle_is_ui_thread(
    _scope: &mut v8::PinScope<'_, '_>,
    _args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let is_ui = crate::ui_dispatcher::is_ui_thread();
    retval.set_bool(is_ui);
}

// ── __nsThreadInfo ──────────────────────────────────────────────────────────
pub(crate) fn handle_thread_info(
    scope: &mut v8::PinScope<'_, '_>,
    _args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let os_tid = crate::ui_dispatcher::get_ui_thread_os_tid();
    let rust_tid = crate::ui_dispatcher::get_ui_thread_rust_tid();
    let is_ui = crate::ui_dispatcher::is_ui_thread();

    let obj = v8::Object::new(scope);
    // osTid: number | null
    if let Some(k) = v8::String::new(scope, "osTid") {
        if let Some(t) = os_tid {
            obj.set(scope, k.into(), v8::Number::new(scope, t as f64).into());
        } else {
            obj.set(scope, k.into(), v8::null(scope).into());
        }
    }
    // rustTid: string | null
    if let Some(k) = v8::String::new(scope, "rustTid") {
        if let Some(s) = rust_tid {
            if let Some(v) = v8::String::new(scope, &s) {
                obj.set(scope, k.into(), v.into());
            }
        } else {
            obj.set(scope, k.into(), v8::null(scope).into());
        }
    }
    // isUiThread: bool
    if let Some(k) = v8::String::new(scope, "isUiThread") {
        obj.set(scope, k.into(), v8::Boolean::new(scope, is_ui).into());
    }

    retval.set(obj.into());
}

// ── __nsGetLastJsError ──────────────────────────────────────────────────────
pub(crate) fn handle_get_last_js_error(
    scope: &mut v8::PinScope<'_, '_>,
    _args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if let Some(err) = crate::get_last_js_error() {
        if let Some(s) = v8::String::new(scope, &err) {
            retval.set(s.into());
            return;
        }
    }
    retval.set(v8::null(scope).into());
}


// ── __nsWin32CallRaw ──────────────────────────────────────────────────────────

/// Fast typed Win32 dispatch — no JSON round-trip.
/// Args: (dll, fn_name, retType, argType₀, argVal₀, argType₁, argVal₁, …)
/// Returns the result as a typed V8 value (number, bool, or null for void).
pub(crate) fn handle_win32_call_raw(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 3 {
        throw_js_error(scope, "__nsWin32CallRaw: expected (dll, fn, retType, ...)");
        return;
    }
    let Some(dll_v) = args.get(0).to_string(scope) else {
        throw_js_error(scope, "__nsWin32CallRaw: dll must be a string"); return;
    };
    let Some(fn_v) = args.get(1).to_string(scope) else {
        throw_js_error(scope, "__nsWin32CallRaw: fn must be a string"); return;
    };
    let Some(ret_v) = args.get(2).to_string(scope) else {
        throw_js_error(scope, "__nsWin32CallRaw: retType must be a string"); return;
    };
    let dll      = dll_v.to_rust_string_lossy(scope);
    let fn_name  = fn_v.to_rust_string_lossy(scope);
    let ret_type = ret_v.to_rust_string_lossy(scope);

    let pair_count = ((args.length() - 3) / 2) as usize;
    let mut type_strs: Vec<String>                    = Vec::with_capacity(pair_count);
    let mut raw_vals:  Vec<crate::win32::RawArgValue> = Vec::with_capacity(pair_count);

    let mut i: i32 = 3;
    while i + 1 < args.length() {
        let Some(ty_v) = args.get(i).to_string(scope) else { break };
        let ty_str = ty_v.to_rust_string_lossy(scope);
        let v8_val = args.get(i + 1);
        let raw_val = if ty_str == "wstr" || ty_str == "str" {
            let s = v8_val.to_string(scope)
                .map(|sv| sv.to_rust_string_lossy(scope))
                .unwrap_or_default();
            crate::win32::RawArgValue::Str(s)
        } else {
            let n = v8_val.number_value(scope).unwrap_or(0.0);
            crate::win32::RawArgValue::Number(n)
        };
        type_strs.push(ty_str);
        raw_vals.push(raw_val);
        i += 2;
    }

    let arg_pairs: Vec<(&str, crate::win32::RawArgValue)> = type_strs.iter()
        .map(|s| s.as_str())
        .zip(raw_vals)
        .collect();

    match crate::win32::call_win32_direct(&dll, &fn_name, &ret_type, &arg_pairs) {
        Ok(result) => match result {
            crate::win32::Win32Result::Null    => retval.set(v8::null(scope).into()),
            crate::win32::Win32Result::Bool(b) => retval.set_bool(b),
            crate::win32::Win32Result::I64(v)  => retval.set_double(v as f64),
            crate::win32::Win32Result::U64(v)  => retval.set_double(v as f64),
            crate::win32::Win32Result::F64(v)  => retval.set_double(v),
        },
        Err(e) => throw_js_error(scope, &e),
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
    register!("__nsAsDelegate",                 crate::handle_as_delegate);
    register!("__nsDotNetInvoke",               handle_dotnet_invoke);
    register!("__nsDotNetInvokeBin",            handle_dotnet_invoke_binary);
    register!("__nsDotNetCreateDelegate",       handle_dotnet_create_delegate);
    register!("__nsDotNetCreateJsSubclass",     handle_dotnet_create_js_subclass);
    register!("__nsDotNetAwaitTask",            handle_dotnet_await_task);
    register!("__nsRunOnUIThread",              handle_run_on_ui_thread);
    register!("__nsWin32Call",                  handle_win32_call);
    register!("__nsWin32CallRaw",               handle_win32_call_raw);
    register!("__nsWin32Exports",               handle_win32_exports);
    register!("__ns__setTimeout",               crate::timers::handle_ns_set_timeout);
    register!("__ns__setInterval",              crate::timers::handle_ns_set_interval);
    register!("__ns__clearTimeout",             crate::timers::handle_ns_clear_timeout);
    register!("__ns__clearInterval",            crate::timers::handle_ns_clear_interval);
    register!("__nsDwmFlush",                   handle_dwm_flush);
    register!("__tns_uptime",                   handle_tns_uptime);
    register!("__nsUUID",                       handle_ns_uuid);
    register!("__nsIsUiThread",                 handle_is_ui_thread);
    register!("__nsThreadInfo",                 handle_thread_info);
    register!("__nsGetLastJsError",             handle_get_last_js_error);

    // DevTools host hooks: allow JS to register domain dispatchers and
    // post events/timestamps to the DevTools server when enabled.
    register!("__registerDomainDispatcher",     handle_register_domain_dispatcher);
    register!("__inspectorSendEvent",           handle_inspector_send_event);
    register!("__inspectorTimestamp",           handle_inspector_timestamp);

    // Initialize native timers scheduler (non-blocking). This registers a
    // pump into `ASYNC_PUMP_HOOK` so blocking waits will also process timers.
    crate::timers::init();

    if let (Some(k), Some(v)) = (v8::String::new(scope, "__nsAppRoot"), v8::String::new(scope, app_root)) {
        global.define_own_property(scope, k.into(), v.into(), v8::PropertyAttribute::READ_ONLY);
    }

    // This benefits host-provided timers that rely on system timer resolution.
    // Ignored on failure (e.g., sandboxed environments).
    let _ = crate::win32::call_win32_json(
        r#"{"dll":"winmm.dll","fn":"timeBeginPeriod","returnType":"u32","args":[{"type":"u32","value":1}]}"#,
    );

    // Store app root for optional .NET initialization. Actual host initialisation
    // is performed lazily on first .NET use.
    crate::dotnet::set_app_root(app_root);

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
