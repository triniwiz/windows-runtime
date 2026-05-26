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

    let dispatcher: Option<Arc<dyn Fn(&str) -> bool + Send + Sync>> = Some(Arc::new(|msg: &str| {
        runtime::inspector::try_dispatch_inspector_message_to_js(msg)
    }));

    match DevtoolsServer::attach(&config, rt.isolate_mut(), &global_ctx, forwarder, dispatcher) {
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
/// Must be called regularly on the V8/UI thread so that `setTimeout` /
/// `setInterval` callbacks fire. XAML hosts wire this to
/// `CompositionTarget.Rendering`; console hosts call it in their own loop.
///
/// Automatically detects context:
/// - XAML host: flushes V8 microtasks only (Win32 messages are pumped by XAML).
/// - Console/self-hosted: also drains Win32 messages so WinRT async `Completed`
///   callbacks (e.g. `BitmapImage.SetSourceAsync`) can fire.
#[no_mangle]
pub extern "C" fn runtime_pump_timers() {
    let _ = std::panic::catch_unwind(|| {
        runtime::timers::pump();
        if runtime::ui_dispatcher::needs_win32_pump() {
            runtime::pump_messages();
        } else {
            runtime::pump_dispatcher();
        }
    });
}

/// Pump Win32 messages and flush V8 microtasks.
///
/// For console apps and other hosts that have no XAML event loop.
/// Call this in a tight loop (e.g. `MsgWaitForMultipleObjects` or plain sleep loop)
/// to let WinRT async `Completed` callbacks (e.g. `BitmapImage.SetSourceAsync`)
/// fire and their Promise continuations run.
///
/// Do NOT call from `CompositionTarget.Rendering` — use `runtime_pump_timers` there.
///
/// Returns `true` if at least one Win32 message was dispatched.
#[no_mangle]
pub extern "C" fn runtime_pump_messages() -> bool {
    std::panic::catch_unwind(|| {
        runtime::pump_messages()
    }).unwrap_or(false)
}

/// Free a string previously returned by `runtime_devtools_start`.
#[cfg(feature = "devtools")]
#[no_mangle]
pub extern "C" fn runtime_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        drop(unsafe { CString::from_raw(ptr) });
    }
}