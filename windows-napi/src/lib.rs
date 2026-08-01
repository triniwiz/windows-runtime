//! Node-API binding for the NativeScript Windows (WinRT) runtime.
//!
//! Exposes the `runtime` crate to Node/Bun/Deno as a `.node` addon, providing the WinRT
//! interop surface (namespace proxies, value marshalling, delegates, the `interop.*` helpers)
//! to any Node-API-compatible host. See `docs/napi-consumption.md`.

use napi::Env;
use napi_derive::napi;
use std::cell::RefCell;
use std::mem::ManuallyDrop;
use runtime::Runtime;

mod console_test;
mod delegate_test;
mod invoke_test;
mod proxy_test;
mod value_test;

thread_local! {
    /// One runtime per JS thread. Node addon calls arrive on the main thread; the runtime's
    /// V8 isolate and WinRT apartment are thread-affine, so a thread-local keeps them together.
    ///
    /// `ManuallyDrop` so TLS destruction never runs `Runtime::drop`: it touches other
    /// thread-locals, which aborts the process once those are destroyed (observed when a
    /// consumer calls `process.exit()` without `deinit()` — Node skips env teardown on a hard
    /// exit). Explicit `deinit()`/env-cleanup drop it properly; otherwise it leaks at process
    /// death, which is harmless.
    static RT: RefCell<Option<ManuallyDrop<Box<Runtime>>>> = const { RefCell::new(None) };
}

fn drop_runtime() {
    RT.with(|cell| {
        if let Some(rt) = cell.borrow_mut().take() {
            drop(ManuallyDrop::into_inner(rt));
        }
    });
}

/// Create the runtime rooted at `appRoot` (defaults to the empty string). Idempotent: a second
/// call while a runtime already exists is a no-op and returns `true`.
#[napi]
pub fn init(mut env: Env, app_root: Option<String>) -> bool {
    RT.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_some() {
            return true;
        }
        let mut rt = Box::new(Runtime::new(app_root.as_deref().unwrap_or("")));
        rt.register_delegate_isolate_ptr();
        *slot = Some(ManuallyDrop::new(rt));
        // On a graceful env teardown (worker exit, embedder shutdown) drop the runtime
        // properly; a hard process.exit() never reaches this and leaks instead — by design.
        let _ = env.add_env_cleanup_hook((), |_| drop_runtime());
        true
    })
}

/// Evaluate `script` under `filename` (defaults to `main.js`) in the runtime's context.
#[napi]
pub fn run_script(script: String, filename: Option<String>) {
    RT.with(|cell| {
        if let Some(rt) = cell.borrow_mut().as_mut() {
            rt.run_script(&script, filename.as_deref().unwrap_or("main.js"));
        }
    });
}

/// Drain JS timers and pump the appropriate message/dispatcher loop for one tick. Wire this to
/// a libuv `check`/`prepare` handle (or an `setImmediate` loop) so `setTimeout`, WinRT async
/// `Completed` callbacks, and their promise continuations fire.
#[napi]
pub fn pump_timers() {
    runtime::timers::pump();
    if runtime::ui_dispatcher::needs_win32_pump() {
        runtime::pump_messages();
    } else {
        runtime::pump_dispatcher();
    }
}

/// Pump Win32 messages and flush microtasks. Returns `true` if a message was dispatched.
#[napi]
pub fn pump_messages() -> bool {
    runtime::pump_messages()
}

/// The last JS error (message + stack), if any.
#[napi]
pub fn last_error() -> Option<String> {
    runtime::get_last_js_error()
}

/// Install the runtime's globals (`__time`, and `performance`/`console` where the host lacks
/// them) onto the host's global object.
#[napi]
pub fn install_globals(env: Env) -> napi::Result<()> {
    runtime::napi_engine::globals::install_globals(&env)
}

/// Resolve a WinRT namespace root (e.g. `getNamespace('Windows')`) as a lazy proxy: member
/// access walks metadata, classes come back as constructable proxies.
#[napi]
pub fn get_namespace(env: Env, name: String) -> napi::Result<napi::JsObject> {
    runtime::napi_engine::invoke::ensure_winrt_initialized();
    runtime::napi_engine::ns_proxy::create_namespace_proxy(&env, &name)
}

/// Generate a fresh UUID string (CoCreateGuid).
#[napi]
pub fn ns_uuid() -> String {
    runtime::napi_engine::interop::ns_uuid()
}

/// Whether the WinRT class `name` is sealed (metadata flag). Test hook: lets suites assert
/// they are really covering the composable (non-sealed, null-outer) constructor path.
#[napi]
pub fn class_is_sealed(name: String) -> Option<bool> {
    use metadata::declarations::class_declaration::ClassDeclaration;
    let declaration = metadata::meta_data_reader::MetadataReader::find_by_name(&name)?;
    let lock = declaration.read();
    lock.as_any()
        .downcast_ref::<ClassDeclaration>()
        .map(|c| c.is_sealed())
}

/// Register a third-party `.winmd` file for metadata resolution (WebView2, app types, …).
#[napi]
pub fn register_winmd(path: String) -> napi::Result<()> {
    runtime::napi_engine::interop::register_winmd(&path)
        .map_err(|e| napi::Error::from_reason(e))
}

/// Register every `.winmd` in a directory (non-recursive); returns the count registered.
#[napi]
pub fn scan_winmd_dir(dir: String) -> u32 {
    runtime::napi_engine::interop::scan_winmd_dir(&dir) as u32
}

/// Wrap a `Windows.Storage.Streams.IBuffer` as a (zero-copy where supported) ArrayBuffer.
#[napi]
pub fn array_buffer_from_buffer(env: Env, buffer: napi::JsUnknown) -> napi::Result<napi::JsUnknown> {
    runtime::napi_engine::interop::array_buffer_from_buffer(&env, &buffer)
}

/// Install the `__ns*` interop natives and the `NSWinRT.interop` JS surface (Pointer/OutParam,
/// `reference` / typed-value boxing, buffer + DateTime utilities) without touching any other
/// host global. Idempotent; `nswinrt.js` calls this on load.
#[napi]
pub fn install_interop(env: Env) -> napi::Result<()> {
    runtime::napi_engine::invoke::ensure_winrt_initialized();
    runtime::napi_engine::interop::install_interop(&env)
}

/// Install the `.NET`/BCL bridge natives and the `NSWinRT.dotnet` JS surface
/// (invoke/get/fromHandle/registerNamespace, taskToPromise/asDelegate, `NSWinRT.runOnUIThread`).
/// Idempotent; `nswinrt.js` calls this on load. A no-op at the JS layer until a
/// `dotnet-bridge/publish/DotNetBridge.dll` exists next to the app.
#[napi]
pub fn install_dotnet(env: Env) -> napi::Result<()> {
    runtime::napi_engine::dotnet::install_dotnet(&env)
}

/// Tear the runtime down on this thread.
#[napi]
pub fn deinit() {
    drop_runtime();
}
