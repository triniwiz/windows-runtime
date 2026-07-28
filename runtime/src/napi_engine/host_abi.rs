//! Engine-neutral pieces of the standalone-host / WinUI 3 DLL bring-up.
//!
//! A napi-backed runtime DLL (`nativescript.dll` built for a non-V8 engine) has to expose the
//! same C ABI the classic runtime does — `runtime_init` / `runtime_runscript` /
//! `runtime_pump_timers`, etc. — so the WinUI 3 .NET host can P/Invoke it unchanged. Two parts of
//! that are engine-specific and live in each engine package (they need the engine's shim):
//!
//!   * creating the engine + its `napi_env`, and
//!   * evaluating a script string + draining that engine's microtask queue.
//!
//! Everything else — initializing WinRT, installing the runtime globals and the `Windows`
//! namespace, and driving one turn of the event loop — is identical across engines, so it lives
//! here (in-workspace, compiled with the `napi_engine` feature) rather than being copy-pasted into
//! every engine package. The engine wrapper supplies the `napi_env`; these helpers do the rest.

use napi::Env;

use crate::napi_engine::{event_loop, globals, invoke, module_natives, ns_proxy};

/// Initializes the WinRT runtime on an already-created `napi_env`:
///   1. initialize the WinRT apartment (idempotent),
///   2. install the runtime globals (`console`, `performance`, timers, `interop`, …),
///   3. resolve and expose the root `Windows` namespace on `globalThis`,
///   4. install the event-loop keep-alive natives (`__nsLoopRetain` / `__nsLoopRelease`),
///   5. install the CommonJS module natives (`__nsAppRoot`, `__nsReadTextFile`,
///      `__nsResolveModulePath`) the JS prelude's `require`/`module`/`exports` shim needs —
///      without these, webpack `target: 'node'` bundles throw `ReferenceError: require is not
///      defined` on evaluation.
///
/// This is exactly what the standalone hosts do before running app code; the only thing left to
/// the caller is running the engine's JS prelude/polyfills (which need the engine's own eval).
pub fn initialize_runtime(env: &Env, app_root: &str) -> napi::Result<()> {
    invoke::ensure_winrt_initialized();
    // Classic (v8) sets this from global_fns::runtime_init's app_root arg; without it,
    // ensure_dotnet_initialized() falls back to "." (the process's actual cwd, not the app's
    // install dir in a packaged WinUI 3 host) and DotNetBridge.dll is never found — permanently,
    // since the lookup result is cached in a OnceLock on first access.
    crate::dotnet::set_app_root(app_root);
    globals::install_globals(env)?;
    let windows_ns = ns_proxy::create_namespace_proxy(env, "Windows")?;
    let mut global = env.get_global()?;
    global.set_named_property("Windows", windows_ns)?;
    event_loop::install_loop_natives(env)?;
    module_natives::install_module_natives(env, app_root)?;
    Ok(())
}

/// Drive one turn of the event loop (timers, WinRT async completions, microtasks). WinUI 3 hosts
/// call this once per frame from `CompositionTarget.Rendering`; it never blocks. `drain_microtasks`
/// is the engine's microtask pump (e.g. `js_execute_pending_jobs` / `jsr_drain_microtasks`).
pub fn pump_once<F: FnMut()>(env: &Env, drain_microtasks: &mut F) {
    event_loop::pump_once(env, drain_microtasks);
}

/// True when the loop has no outstanding work (no timers, zero keep-alive count) — a self-hosted
/// host can use this to decide when to exit.
pub fn is_idle() -> bool {
    event_loop::is_idle()
}
