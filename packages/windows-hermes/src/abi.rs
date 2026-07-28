//! `nativescript.dll` C ABI for the Hermes engine — the WinUI 3 .NET host P/Invokes exactly this
//! surface (`runtime_init`, `runtime_runscript`, `runtime_pump_timers`, …), identical to the
//! classic V8 runtime's DLL, so an app swaps `@nativescript/windows` for
//! `@nativescript/windows-hermes` with no code change. Built into the cdylib only under the
//! `host_dll` feature (`build.ps1 -Engine hermes`); the default build still produces the standalone
//! `nativescript-windows.exe`.
//!
//! Same shape as the reference (`windows-quickjs/src/abi.rs`): the engine-neutral work (WinRT
//! init, globals, `Windows` namespace, event-loop turn) lives in `runtime::napi_engine::host_abi`;
//! the three engine-specific pieces (create the env, evaluate a script, drain microtasks) come
//! from `crate::host`.

use std::cell::Cell;
use std::ffi::{c_char, c_int, c_void, CStr, CString};

use napi::Env;
use runtime::napi_engine::host_abi;

use crate::host;

thread_local! {
    /// The Hermes `napi_env` for this thread, captured at `runtime_init`. `runtime_pump_timers`
    /// takes no handle (the classic ABI is stateless there), so the env is kept thread-local for
    /// the pump to reach it.
    static HOST_ENV: Cell<*mut c_void> = const { Cell::new(std::ptr::null_mut()) };
}

fn host_env() -> Option<*mut c_void> {
    let raw = HOST_ENV.with(|c| c.get());
    (!raw.is_null()).then_some(raw)
}

/// Create the runtime on Hermes and bring WinRT up. Returns a non-zero handle on success (the
/// classic host treats `0` as failure). `app_root` is accepted for ABI parity.
#[no_mangle]
pub extern "C" fn runtime_init(app_root: *const c_char) -> i64 {
    let result = std::panic::catch_unwind(|| unsafe {
        // Populate napi-sys's symbol table from Hermes's `napi_*`, which this module forward-exports
        // to hermes.dll (build.rs's /EXPORT:napi_*=hermes.napi_* linker args, applied to the cdylib
        // as well as the exe). Resolving THIS module rather than the process .exe requires the
        // vendored napi-sys patch (packages/vendor/napi-sys): stock napi-sys queries
        // GetModuleHandleExW(0, NULL) = the .NET host .exe, which exports no napi_*, so every call
        // would abort. The patch's probe follows the forwarders into hermes.dll. Validated by a C#
        // P/Invoke harness (LoadLibrary nativescript.dll → runtime_init → real WinRT round-trip).
        std::mem::forget(napi::sys::setup());

        let app_root = if app_root.is_null() {
            String::new()
        } else {
            CStr::from_ptr(app_root).to_string_lossy().into_owned()
        };

        let raw = host::shared_env_ptr();
        if raw.is_null() {
            return 0;
        }
        let env = Env::from_raw(raw as napi::sys::napi_env);
        if host_abi::initialize_runtime(&env, &app_root).is_err() {
            return 0;
        }
        // Engine-specific JS setup that needs the engine's own eval: URL polyfill + runtime prelude
        // (queueMicrotask + NSWinRT.toPromise over the loop keep-alive natives).
        if let Err(e) = host::run_script_checked(raw, ns_windows_common::url_polyfill::POLYFILL) {
            runtime::store_last_js_error(format!("[url_polyfill] {e}"));
        }
        if let Err(e) = host::run_script_checked(raw, ns_windows_common::prelude::PRELUDE) {
            runtime::store_last_js_error(format!("[prelude] {e}"));
        }

        HOST_ENV.with(|c| c.set(raw));
        // A stable non-zero token; per-instance state is process-global (one leaked runtime), so
        // the handle only needs to be truthy and round-trip through the host.
        1
    });
    result.unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn runtime_deinit(_runtime: i64) {
    // The Hermes runtime + env are process-lifetime (leaked, as a real host keeps them); nothing
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
        return;
    }
    let _ = std::panic::catch_unwind(|| unsafe {
        if let Some(raw) = host_env() {
            let code = CStr::from_ptr(script).to_string_lossy();
            if let Err(e) = host::run_script_checked(raw, &code) {
                eprintln!("[NativeScript] script error: {e}");
                runtime::store_last_js_error(e);
            }
        }
    });
}

/// Drive one turn of the event loop (timers, WinRT async completions, microtasks). The WinUI 3
/// host calls this each frame from `CompositionTarget.Rendering`.
#[no_mangle]
pub extern "C" fn runtime_pump_timers() {
    let _ = std::panic::catch_unwind(|| unsafe {
        if let Some(raw) = host_env() {
            let env = Env::from_raw(raw as napi::sys::napi_env);
            let mut drain = || host::drain_microtasks(raw);
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
            let _ = host::run_script_checked(raw, &code);
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
