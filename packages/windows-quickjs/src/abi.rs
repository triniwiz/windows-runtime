//! `nativescript.dll` C ABI for the QuickJS engine — the WinUI 3 .NET host P/Invokes exactly this
//! surface (`runtime_init`, `runtime_runscript`, `runtime_pump_timers`, …), identical to the
//! classic V8 runtime's DLL, so an app swaps `@nativescript/windows` for
//! `@nativescript/windows-quickjs` with no code change. Built into the cdylib only under the
//! `host_dll` feature (`build.ps1 -Engine quickjs`); the default build still produces the
//! standalone `nativescript-windows.exe`.
//!
//! This is the reference implementation of the engine → DLL adapter. The engine-neutral work
//! (WinRT init, globals, `Windows` namespace, event-loop turn) lives in
//! `runtime::napi_engine::host_abi`; only the three engine-specific pieces are wired here —
//! creating the env (`shim::shared_env_ptr`), evaluating a script (`shim::run_script_checked`),
//! and draining microtasks (`shim::drain_microtasks`). The other engine packages follow this same
//! shape with their own shim.

use std::cell::Cell;
use std::ffi::{c_char, c_int, c_void, CStr, CString};

use napi::Env;
use runtime::napi_engine::host_abi;

use crate::shim;

thread_local! {
    /// The QuickJS `napi_env` for this thread, captured at `runtime_init`. `runtime_pump_timers`
    /// takes no handle (the classic ABI is stateless there), so the env is kept thread-local for
    /// the pump to reach it.
    static HOST_ENV: Cell<*mut c_void> = const { Cell::new(std::ptr::null_mut()) };
}

fn host_env() -> Option<*mut c_void> {
    let raw = HOST_ENV.with(|c| c.get());
    (!raw.is_null()).then_some(raw)
}

/// Create the runtime on QuickJS and bring WinRT up. Returns a non-zero handle on success (the
/// classic host treats `0` as failure). `app_root` is accepted for ABI parity.
#[no_mangle]
pub extern "C" fn runtime_init(app_root: *const c_char) -> i64 {
    let result = std::panic::catch_unwind(|| unsafe {
        // Populate napi-sys's symbol table from THIS module's own napi_* (the statically-linked,
        // dllexported shim). Resolving this module rather than the process .exe requires the
        // vendored napi-sys patch (packages/vendor/napi-sys): stock napi-sys queries
        // GetModuleHandleExW(0, NULL) = the .NET host .exe, which exports no napi_*, so every call
        // would abort ("Node-API symbol has not been loaded"). Validated by a C# P/Invoke harness
        // (LoadLibrary nativescript.dll → runtime_init → real WinRT round-trip).
        std::mem::forget(napi::sys::setup());

        let app_root = if app_root.is_null() {
            String::new()
        } else {
            CStr::from_ptr(app_root).to_string_lossy().into_owned()
        };

        let raw = shim::shared_env_ptr();
        if raw.is_null() {
            runtime::debug_output("[NativeScript] runtime_init: shared_env_ptr() returned null\n");
            return 0;
        }
        let env = Env::from_raw(raw as napi::sys::napi_env);
        if let Err(e) = host_abi::initialize_runtime(&env, &app_root) {
            runtime::debug_output(&format!("[NativeScript] runtime_init: initialize_runtime() failed: {e}\n"));
            return 0;
        }
        // Engine-specific JS setup that needs the engine's own eval: URL polyfill + runtime prelude
        // (queueMicrotask + NSWinRT.toPromise over the loop keep-alive natives).
        if let Err(e) = shim::run_script_checked(raw, ns_windows_common::url_polyfill::POLYFILL) {
            runtime::debug_output(&format!("[NativeScript] runtime_init: url_polyfill failed: {e}\n"));
            runtime::store_last_js_error(format!("[url_polyfill] {e}"));
        }
        if let Err(e) = shim::run_script_checked(raw, ns_windows_common::prelude::PRELUDE) {
            runtime::debug_output(&format!("[NativeScript] runtime_init: prelude failed: {e}\n"));
            runtime::store_last_js_error(format!("[prelude] {e}"));
        }

        HOST_ENV.with(|c| c.set(raw));
        runtime::debug_output("[NativeScript] runtime_init: initialize_runtime OK, HOST_ENV set\n");
        // A stable non-zero token; per-instance state is process-global for QuickJS (one leaked
        // runtime), so the handle only needs to be truthy and round-trip through the host.
        1
    });
    match result {
        Ok(v) => v,
        Err(e) => {
            let msg = if let Some(s) = e.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = e.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic".to_string()
            };
            runtime::debug_output(&format!("[NativeScript] runtime_init: PANICKED: {msg}\n"));
            0
        }
    }
}

#[no_mangle]
pub extern "C" fn runtime_deinit(_runtime: i64) {
    // The QuickJS runtime + env are process-lifetime (leaked, as a real host keeps them); nothing
    // per-call to free. Present for ABI parity with the classic runtime.
    HOST_ENV.with(|c| c.set(std::ptr::null_mut()));
}

#[no_mangle]
pub extern "C" fn runtime_runscript(
    _runtime: i64,
    script: *const c_char,
    _filename: *const c_char,
) {
    if script.is_null() {
        runtime::debug_output("[NativeScript] runtime_runscript: script is null\n");
        return;
    }
    let outcome = std::panic::catch_unwind(|| unsafe {
        match host_env() {
            Some(raw) => {
                let code = CStr::from_ptr(script).to_string_lossy();
                runtime::debug_output(&format!(
                    "[NativeScript] runtime_runscript: executing {} bytes\n",
                    code.len()
                ));
                match shim::run_script_checked(raw, &code) {
                    Ok(_) => {
                        runtime::debug_output("[NativeScript] runtime_runscript: OK\n");
                    }
                    Err(e) => {
                        runtime::debug_output(&format!(
                            "[NativeScript] runtime_runscript: script error: {e}\n"
                        ));
                        runtime::store_last_js_error(e);
                    }
                }
            }
            None => {
                runtime::debug_output(
                    "[NativeScript] runtime_runscript: host_env() is None, skipping\n",
                );
            }
        }
    });
    if let Err(e) = outcome {
        let msg = if let Some(s) = e.downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = e.downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown panic".to_string()
        };
        runtime::debug_output(&format!("[NativeScript] runtime_runscript: PANICKED: {msg}\n"));
    }
}

/// Drive one turn of the event loop (timers, WinRT async completions, microtasks). The WinUI 3
/// host calls this each frame from `CompositionTarget.Rendering`.
#[no_mangle]
pub extern "C" fn runtime_pump_timers() {
    let _ = std::panic::catch_unwind(|| unsafe {
        if let Some(raw) = host_env() {
            let env = Env::from_raw(raw as napi::sys::napi_env);
            let mut drain = || shim::drain_microtasks(raw);
            host_abi::pump_once(&env, &mut drain);
        }
    });
}

/// Forward a host lifecycle event into JS via `globalThis.__nsOnAppEvent(kind, message)`.
#[no_mangle]
pub extern "C" fn runtime_notify_app_event(_runtime: i64, kind: c_int, message: *const c_char) {
    let _ = std::panic::catch_unwind(|| unsafe {
        if let Some(raw) = host_env() {
            let msg = if message.is_null() {
                "null".to_string()
            } else {
                let m = CStr::from_ptr(message).to_string_lossy().replace('`', "\\`");
                format!("`{m}`")
            };
            let code = format!(
                "typeof __nsOnAppEvent==='function' && __nsOnAppEvent({kind}, {msg})"
            );
            let _ = shim::run_script_checked(raw, &code);
        }
    });
}

#[no_mangle]
pub extern "C" fn runtime_set_local_folder(path: *const c_char) {
    if path.is_null() {
        return;
    }
    let s = unsafe { CStr::from_ptr(path) }.to_string_lossy().into_owned();
    runtime::set_log_dir(s);
}

#[no_mangle]
pub extern "C" fn runtime_install_ctrlc_handler(_exit_code: i32) {
    // No-op on the engine hosts: the WinUI 3 process owns Ctrl+C. Present for ABI parity.
}

#[no_mangle]
pub extern "C" fn runtime_has_devtools() -> bool {
    false
}

/// Returns the last JS error (message + stack) or NULL. Caller frees with `runtime_free_js_error`.
#[no_mangle]
pub extern "C" fn runtime_get_last_js_error() -> *mut c_char {
    match runtime::get_last_js_error() {
        Some(s) => CString::new(s)
            .map(|c| c.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        None => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn runtime_free_js_error(ptr: *mut c_char) {
    if !ptr.is_null() {
        drop(unsafe { CString::from_raw(ptr) });
    }
}
