use std::ffi::{c_char, CStr, CString};
use std::sync::Once;
use runtime::Runtime;

// ─── Devtools (compiled only with the `devtools` feature) ────────────────────
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
    CTRL_C_INIT.call_once(move || {
        let _ = ctrlc::set_handler(move || {
            println!("Ctrl+C received, shutting down runtime...");
            std::process::exit(exit_code);
        });
    });
}

#[no_mangle]
pub extern "C" fn runtime_init(app_root: *const c_char) -> i64 {
    let mut boxed = if app_root.is_null() {
        Box::new(Runtime::new(""))
    } else {
        let string = unsafe { CStr::from_ptr(app_root) }.to_string_lossy();
        Box::new(Runtime::new(string.as_ref()))
    };
    boxed.register_delegate_isolate_ptr();
    Box::into_raw(boxed) as i64
}

#[no_mangle]
pub extern "C" fn runtime_deinit(runtime: i64) {
    if runtime != 0 {
        let runtime: *mut Runtime = runtime as _;
        let _ = unsafe { Box::from_raw(runtime) };
    }
}

#[no_mangle]
pub extern "C" fn runtime_runscript(runtime: i64, script: *const c_char, filename: *const c_char) {
    if runtime != 0 {
        let runtime: *mut Runtime = runtime as _;
        let runtime = unsafe { &mut *runtime };
        let script = unsafe { CStr::from_ptr(script) }.to_string_lossy();
        let filename = if filename.is_null() {
            "main.js".to_string()
        } else {
            unsafe { CStr::from_ptr(filename) }.to_string_lossy().into_owned()
        };
        runtime.run_script(script.as_ref(), &filename);
    }
}

// ─── Devtools FFI ─────────────────────────────────────────────────────────────

/// Start the Chrome DevTools Protocol server on `port` for the given runtime.
///
/// Returns a null-terminated WebSocket URL on success (e.g.
/// `ws://127.0.0.1:9229/devtools/page/runtime\0`), or NULL on failure.
/// The caller must free the returned string with `runtime_free_string`.
///
/// Only available in builds compiled with the `devtools` feature.
#[cfg(feature = "devtools")]
#[no_mangle]
pub extern "C" fn runtime_devtools_start(runtime: i64, port: u16) -> *mut c_char {
    if runtime == 0 { return std::ptr::null_mut(); }
    let rt = unsafe { &mut *(runtime as *mut Runtime) };

    let config = DevtoolsServerConfig { host: "127.0.0.1".to_string(), port };
    // Split borrows: copy the Global handle first, then take the mutable isolate borrow.
    let global_ctx = rt.global_context().clone();
    match DevtoolsServer::attach(&config, rt.isolate_mut(), &global_ctx) {
        Err(_) => std::ptr::null_mut(),
        Ok(server) => {
            let ws_url = server.endpoint().websocket_url.clone();

            // Register the pump hook so the async-wait loop auto-drains messages.
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

/// Pump pending DevTools messages for the given runtime.
///
/// Call this periodically from the host's event loop (e.g. every 16 ms) when
/// the runtime is not blocked inside an async operation, since the internal
/// pump hook only fires during async waits.
///
/// Only available in builds compiled with the `devtools` feature.
#[cfg(feature = "devtools")]
#[no_mangle]
pub extern "C" fn runtime_devtools_pump(_runtime: i64) {
    DEVTOOLS.with(|d| {
        if let Ok(mut guard) = d.try_borrow_mut() {
            if let Some(s) = guard.as_mut() { s.pump_messages(); }
        }
    });
}

/// Free a string previously returned by `runtime_devtools_start`.
#[cfg(feature = "devtools")]
#[no_mangle]
pub extern "C" fn runtime_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        drop(unsafe { CString::from_raw(ptr) });
    }
}