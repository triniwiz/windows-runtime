use std::ffi::{c_char, CStr, CString, c_void};
use std::sync::{Once, OnceLock};
use std::sync::Arc;
use runtime::Runtime;

static PANIC_HOOK_INIT: Once = Once::new();
static APP_ROOT: OnceLock<String> = OnceLock::new();
static LOCAL_FOLDER: OnceLock<String> = OnceLock::new();

fn install_panic_hook() {
    PANIC_HOOK_INIT.call_once(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let msg = if let Some(s) = info.payload().downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = info.payload().downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic".to_string()
            };
            let location = info.location()
                .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
                .unwrap_or_else(|| "<unknown location>".to_string());
            let full = format!("[NativeScript] PANIC at {}: {}\n", location, msg);
            eprint!("{}", full);
            let log_dir = LOCAL_FOLDER.get()
                .map(|s| s.as_str())
                .or_else(|| APP_ROOT.get().map(|s| s.as_str()));
            let log_written = if let Some(dir) = log_dir {
                let path = std::path::Path::new(dir).join("nativescript-panic.log");
                std::fs::OpenOptions::new()
                    .create(true).append(true)
                    .open(&path)
                    .and_then(|mut f| { use std::io::Write; f.write_all(full.as_bytes()) })
                    .is_ok()
            } else {
                false
            };
            if !log_written {
                let temp = std::env::temp_dir().join("nativescript-panic.log");
                let _ = std::fs::OpenOptions::new()
                    .create(true).append(true)
                    .open(&temp)
                    .and_then(|mut f| { use std::io::Write; f.write_all(full.as_bytes()) });
            }
            prev(info);
        }));
    });
}

#[cfg(feature = "devtools")]
use runtime_devtools::{DevtoolsServer, DevtoolsServerConfig};

#[cfg(feature = "devtools")]
thread_local! {
    static DEVTOOLS: std::cell::RefCell<Option<DevtoolsServer>> =
        std::cell::RefCell::new(None);
}

static CTRL_C_INIT: Once = Once::new();

#[no_mangle]
pub extern "C" fn runtime_install_ctrlc_handler(exit_code: i32) {
    install_panic_hook();
    CTRL_C_INIT.call_once(move || {
        let _ = ctrlc::set_handler(move || {
            println!("Ctrl+C received, shutting down runtime...");
            std::process::exit(exit_code);
        });
    });
}

#[no_mangle]
pub extern "C" fn runtime_set_local_folder(path: *const c_char) {
    if path.is_null() { return; }
    let s = unsafe { CStr::from_ptr(path) }.to_string_lossy().to_string();
    let _ = LOCAL_FOLDER.set(s);
}

#[no_mangle]
pub extern "C" fn runtime_init(app_root: *const c_char) -> i64 {
    let result = std::panic::catch_unwind(|| {
        let mut boxed = if app_root.is_null() {
            Box::new(Runtime::new(""))
        } else {
            let string = unsafe { CStr::from_ptr(app_root) }.to_string_lossy();
            let _ = APP_ROOT.set(string.to_string());
            Box::new(Runtime::new(string.as_ref()))
        };
        boxed.register_delegate_isolate_ptr();
        Box::into_raw(boxed) as i64
    });
    match result {
        Ok(ptr) => ptr,
        Err(e) => {
            let msg = if let Some(s) = e.downcast_ref::<&str>() { s.to_string() }
                      else if let Some(s) = e.downcast_ref::<String>() { s.clone() }
                      else { "unknown panic".to_string() };
            eprintln!("[NativeScript] runtime_init panic: {}", msg);
            0
        }
    }
}

#[no_mangle]
pub extern "C" fn runtime_deinit(runtime: i64) {
    if runtime != 0 {
        let _ = std::panic::catch_unwind(|| {
            let runtime: *mut Runtime = runtime as _;
            let _ = unsafe { Box::from_raw(runtime) };
        });
    }
}

#[no_mangle]
pub extern "C" fn runtime_runscript(runtime: i64, script: *const c_char, filename: *const c_char) {
    if runtime != 0 {
        let _ = std::panic::catch_unwind(|| {
            let runtime: *mut Runtime = runtime as _;
            let runtime = unsafe { &mut *runtime };
            let script = unsafe { CStr::from_ptr(script) }.to_string_lossy();
            let filename = if filename.is_null() {
                "main.js".to_string()
            } else {
                unsafe { CStr::from_ptr(filename) }.to_string_lossy().into_owned()
            };
            runtime.run_script(script.as_ref(), &filename);
        });
    }
}

/// Returns the last JS error (message + stack) or NULL. Caller must free with `runtime_free_js_error`.
#[no_mangle]
pub extern "C" fn runtime_get_last_js_error() -> *mut c_char {
    match runtime::get_last_js_error() {
        Some(s) => CString::new(s).map(|c| c.into_raw()).unwrap_or(std::ptr::null_mut()),
        None => std::ptr::null_mut(),
    }
}

/// Free a string previously returned by `runtime_get_last_js_error`.
#[no_mangle]
pub extern "C" fn runtime_free_js_error(ptr: *mut c_char) {
    if !ptr.is_null() {
        drop(unsafe { CString::from_raw(ptr) });
    }
}

// ─── Devtools FFI ─────────────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn runtime_has_devtools() -> bool {
    cfg!(feature = "devtools")
}

/// Returns a null-terminated WebSocket URL on success, or NULL. Caller must free with `runtime_free_string`.
#[cfg(feature = "devtools")]
#[no_mangle]
pub extern "C" fn runtime_devtools_start(runtime: i64, port: u16) -> *mut c_char {
    if runtime == 0 { return std::ptr::null_mut(); }
    let rt = unsafe { &mut *(runtime as *mut Runtime) };

    let config = DevtoolsServerConfig { host: "127.0.0.1".to_string(), port };
    // Split borrows: copy the Global handle first, then take the mutable isolate borrow.
    let global_ctx = rt.global_context().clone();
    let forwarder: Option<Arc<dyn Fn(&str) + Send + Sync>> = Some(Arc::new(|s: &str| {
        runtime::debug_output(s);
    }));

    match DevtoolsServer::attach(&config, rt.isolate_mut(), &global_ctx, forwarder) {
        Err(_) => std::ptr::null_mut(),
        Ok(server) => {
            let ws_url = server.endpoint().websocket_url.clone();

            runtime::ASYNC_PUMP_HOOK.with(|hook| {
                *hook.borrow_mut() = Some(Box::new(|| {
                    DEVTOOLS.with(|d| {
                        if let Ok(mut guard) = d.try_borrow_mut() {
                            if let Some(s) = guard.as_mut() { s.pump_messages(); }
                        }
                    });
                }));
            });

            DEVTOOLS.with(|d| { *d.borrow_mut() = Some(server); });

            CString::new(ws_url).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
        }
    }
}

#[cfg(feature = "devtools")]
#[no_mangle]
pub extern "C" fn runtime_devtools_pump(_runtime: i64) {
    let _ = std::panic::catch_unwind(|| {
        DEVTOOLS.with(|d| {
            if let Ok(mut guard) = d.try_borrow_mut() {
                if let Some(s) = guard.as_mut() { s.pump_messages(); }
            }
        });
    });
}

/// Drain the JS timer queue on the calling thread.
///
/// Must be called regularly on the V8/UI thread (e.g. every render frame) so
/// that `setTimeout` / `setInterval` callbacks fire.  The C# host wires this
/// to `CompositionTarget.Rendering` which fires at the display refresh rate.
#[no_mangle]
pub extern "C" fn runtime_pump_timers() {
    let _ = std::panic::catch_unwind(|| {
        runtime::timers::pump();
    });
}

/// Attach a (cached) container visual to the supplied `UIElement` pointer and
/// return the raw visual pointer as an `i64`. Returns 0 on error.
#[no_mangle]
pub extern "C" fn runtime_attach_border_container(element: *mut c_void) -> i64 {
    if element.is_null() { return 0; }
    let result = std::panic::catch_unwind(|| {
        match runtime::composition_border::ensure_container_for_element(element) {
            Ok(ptr) => ptr,
            Err(err) => {
                eprintln!("[NativeScript] runtime_attach_border_container error: {:?}", err);
                0
            }
        }
    });
    match result {
        Ok(v) => v,
        Err(_) => 0,
    }
}

/// Create a border helper instance attached to `element` and return an opaque id.
#[no_mangle]
pub extern "C" fn runtime_create_border_instance(element: *mut c_void) -> i64 {
    if element.is_null() { return 0; }
    match runtime::composition_border::create_border_instance(element) {
        Ok(id) => id,
        Err(err) => {
            eprintln!("[NativeScript] runtime_create_border_instance error: {:?}", err);
            0
        }
    }
}

/// Set border on previously created instance. Returns 1 on success, 0 on failure.
#[no_mangle]
pub extern "C" fn runtime_set_border(
    instance_id: i64,
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
    color: u32,
    radius_tl: f32,
    radius_tr: f32,
    radius_br: f32,
    radius_bl: f32,
) -> i32 {
    match runtime::composition_border::set_border(instance_id, left, top, right, bottom, color, radius_tl, radius_tr, radius_br, radius_bl) {
        Ok(()) => 1,
        Err(err) => {
            eprintln!("[NativeScript] runtime_set_border error: {:?}", err);
            0
        }
    }
}

/// Free a border instance previously created.
#[no_mangle]
pub extern "C" fn runtime_free_border_instance(instance_id: i64) {
    let _ = runtime::composition_border::free_border_instance(instance_id);
}

/// Free a string previously returned by `runtime_devtools_start`.
#[cfg(feature = "devtools")]
#[no_mangle]
pub extern "C" fn runtime_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        drop(unsafe { CString::from_raw(ptr) });
    }
}