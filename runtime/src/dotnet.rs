/// BCL / arbitrary .NET hosting via the .NET hostfxr API.
///
/// On first call to `try_init_dotnet` the runtime locates hostfxr.dll,
/// initialises a CLR context from the bridge project's runtimeconfig.json,
/// loads DotNetBridge.dll, and binds function pointers to the two exported
/// entry points:
///
///   `ns_dotnet_invoke(request_ptr, request_len, &response_ptr, &response_len) -> i32`
///   `ns_dotnet_free(ptr)`
///
/// The bridge uses UTF-8 throughout: request is a raw UTF-8 byte slice (no
/// allocation on the Rust side), and the response is a UTF-8 byte buffer
/// freed via `ns_dotnet_free`.  This eliminates the UTF-16 encode/decode
/// round-trip that the original char* ABI required.

use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::OnceLock;

use windows::Win32::Foundation::HMODULE;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::core::{PCSTR, PCWSTR};

// ── hostfxr ABI ───────────────────────────────────────────────────────────────

type FnInitForRuntimeConfig = unsafe extern "C" fn(
    runtime_config_path: *const u16,
    parameters: *const c_void,
    host_context_handle: *mut *mut c_void,
) -> i32;

type FnGetRuntimeDelegate = unsafe extern "C" fn(
    host_context_handle: *mut c_void,
    delegate_type: i32,
    delegate: *mut *mut c_void,
) -> i32;

type FnClose = unsafe extern "C" fn(host_context_handle: *mut c_void) -> i32;

type FnLoadAsmAndGetFnPtr = unsafe extern "C" fn(
    assembly_path: *const u16,
    type_name: *const u16,
    method_name: *const u16,
    delegate_type_name: *const u16,
    reserved: *const c_void,
    delegate: *mut *mut c_void,
) -> i32;

// ── bridge entry-point signatures (UTF-8 ABI) ─────────────────────────────────
//
// Switching from char* (UTF-16) to byte* (UTF-8) eliminates:
//   • encode_utf16().collect() → Vec<u16> on every call
//   • String::from_utf16_lossy on every response
// The bridge uses JsonSerializer.Deserialize(ReadOnlySpan<byte>) and
// JsonSerializer.SerializeToUtf8Bytes(), both of which are zero-copy on the
// managed side.

type FnBridgeInvoke = unsafe extern "C" fn(
    request_ptr: *const u8,
    request_len: i32,
    response_ptr: *mut *mut u8,
    response_len: *mut i32,
) -> i32;

// Binary ABI: same signature, different entry point and wire format.
type FnBridgeInvokeBinary = unsafe extern "C" fn(
    request_ptr: *const u8,
    request_len: i32,
    response_ptr: *mut *mut u8,
    response_len: *mut i32,
) -> i32;

type FnBridgeFree = unsafe extern "C" fn(ptr: *mut u8);

// RegisterJsCallback: C# stores this pointer so managed delegates can call
// back into V8.  Signature: (callback_id, args_ptr, args_len, &resp_ptr, &resp_len) -> void
pub(crate) type FnJsCallback =
    unsafe extern "C" fn(i32, *const u8, i32, *mut *mut u8, *mut i32);
type FnRegisterJsCallback = unsafe extern "C" fn(callback: FnJsCallback) -> i32;

// ── host state ────────────────────────────────────────────────────────────────

struct DotNetHost {
    _hostfxr: HMODULE,
    invoke: FnBridgeInvoke,
    invoke_binary: FnBridgeInvokeBinary,
    free: FnBridgeFree,
    register_js_callback: FnRegisterJsCallback,
}

// SAFETY: only ever accessed from the main JS/UI thread.
unsafe impl Send for DotNetHost {}
unsafe impl Sync for DotNetHost {}

static DOTNET_HOST: OnceLock<Result<DotNetHost, String>> = OnceLock::new();

// ── helpers ───────────────────────────────────────────────────────────────────

fn to_wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn find_hostfxr() -> Option<PathBuf> {
    if let Ok(root) = std::env::var("DOTNET_ROOT") {
        if let Some(p) = scan_fxr_dir(&PathBuf::from(root).join("host").join("fxr")) {
            return Some(p);
        }
    }
    for base in [
        r"C:\Program Files\dotnet\host\fxr",
        r"C:\Program Files (x86)\dotnet\host\fxr",
    ] {
        if let Some(p) = scan_fxr_dir(&PathBuf::from(base)) {
            return Some(p);
        }
    }
    if let Ok(home) = std::env::var("USERPROFILE") {
        let p = PathBuf::from(home).join(".dotnet").join("host").join("fxr");
        if let Some(found) = scan_fxr_dir(&p) {
            return Some(found);
        }
    }
    None
}

fn scan_fxr_dir(dir: &PathBuf) -> Option<PathBuf> {
    let mut versions: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.path())
        .collect();

    versions.sort_by(|a, b| {
        let av = a.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let bv = b.file_name().and_then(|n| n.to_str()).unwrap_or("");
        bv.cmp(av)
    });

    versions.into_iter().find_map(|v| {
        let candidate = v.join("hostfxr.dll");
        candidate.exists().then_some(candidate)
    })
}

// ── initialisation ────────────────────────────────────────────────────────────

fn build_host(bridge_dll: &str, runtime_config: &str) -> Result<DotNetHost, String> {
    let hostfxr_path = find_hostfxr()
        .ok_or_else(|| "hostfxr.dll not found — is .NET 6+ installed?".to_string())?;

    let hostfxr_wide = to_wide_null(&hostfxr_path.to_string_lossy());
    let hostfxr = unsafe {
        LoadLibraryW(PCWSTR(hostfxr_wide.as_ptr()))
            .map_err(|e| format!("LoadLibraryW(hostfxr.dll): {}", e))?
    };

    macro_rules! resolve {
        ($sym:literal, $ty:ty) => {{
            let name = concat!($sym, "\0");
            let ptr = unsafe { GetProcAddress(hostfxr, PCSTR(name.as_ptr())) }
                .ok_or_else(|| format!("GetProcAddress({}) failed", $sym))?;
            unsafe { std::mem::transmute::<_, $ty>(ptr) }
        }};
    }

    let fn_init: FnInitForRuntimeConfig =
        resolve!("hostfxr_initialize_for_runtime_config", FnInitForRuntimeConfig);
    let fn_get_delegate: FnGetRuntimeDelegate =
        resolve!("hostfxr_get_runtime_delegate", FnGetRuntimeDelegate);
    let _fn_close: FnClose = resolve!("hostfxr_close", FnClose);

    let config_wide = to_wide_null(runtime_config);
    let mut ctx: *mut c_void = std::ptr::null_mut();
    let hr = unsafe { fn_init(config_wide.as_ptr(), std::ptr::null(), &mut ctx) };
    if hr < 0 {
        return Err(format!("hostfxr_initialize_for_runtime_config: HRESULT 0x{:08X}", hr as u32));
    }

    let mut load_fn_ptr: *mut c_void = std::ptr::null_mut();
    let hr = unsafe { fn_get_delegate(ctx, 5, &mut load_fn_ptr) };
    if hr < 0 {
        return Err(format!("hostfxr_get_runtime_delegate: HRESULT 0x{:08X}", hr as u32));
    }
    let load_asm: FnLoadAsmAndGetFnPtr = unsafe { std::mem::transmute(load_fn_ptr) };

    let dll_wide  = to_wide_null(bridge_dll);
    let type_wide = to_wide_null("NativeScriptBridge.Bridge, DotNetBridge");

    let bind = |method: &str| -> Result<*mut c_void, String> {
        let method_wide = to_wide_null(method);
        let mut fn_out: *mut c_void = std::ptr::null_mut();
        let hr = unsafe {
            load_asm(
                dll_wide.as_ptr(),
                type_wide.as_ptr(),
                method_wide.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                &mut fn_out,
            )
        };
        if hr < 0 {
            return Err(format!("load_assembly_and_get_function_pointer({method}): HRESULT 0x{hr:08X}"));
        }
        if fn_out.is_null() {
            return Err(format!("load_assembly_and_get_function_pointer({method}): returned null"));
        }
        Ok(fn_out)
    };

    let invoke:               FnBridgeInvoke          = unsafe { std::mem::transmute(bind("Invoke")?) };
    let invoke_binary:        FnBridgeInvokeBinary     = unsafe { std::mem::transmute(bind("InvokeBinary")?) };
    let free:                 FnBridgeFree             = unsafe { std::mem::transmute(bind("Free")?) };
    let register_js_callback: FnRegisterJsCallback     = unsafe { std::mem::transmute(bind("RegisterJsCallback")?) };

    Ok(DotNetHost { _hostfxr: hostfxr, invoke, invoke_binary, free, register_js_callback })
}

// ── public API ────────────────────────────────────────────────────────────────

pub(crate) fn try_init_dotnet(app_root: &str) {
    DOTNET_HOST.get_or_init(|| {
        let bridge = PathBuf::from(app_root)
            .join("dotnet-bridge").join("publish").join("DotNetBridge.dll");
        let config = PathBuf::from(app_root)
            .join("dotnet-bridge").join("publish").join("DotNetBridge.runtimeconfig.json");

        if !bridge.exists() {
            return Err(format!(
                "DotNetBridge.dll not found at {path} — run `dotnet publish` in dotnet-bridge/",
                path = bridge.display()
            ));
        }
        if !config.exists() {
            return Err(format!(
                "DotNetBridge.runtimeconfig.json not found at {}",
                config.display()
            ));
        }

        build_host(&bridge.to_string_lossy(), &config.to_string_lossy())
    });
}

/// Calls the managed bridge with a JSON request string and returns the JSON
/// response string.  Uses the UTF-8 ABI: request bytes are passed directly
/// (`as_bytes()` — no allocation), response is decoded with
/// `from_utf8_unchecked` (the bridge always outputs valid JSON UTF-8).
pub(crate) fn call_dotnet(request_json: &str) -> Result<String, String> {
    let host = DOTNET_HOST
        .get()
        .ok_or_else(|| "DotNet host not yet initialised".to_string())?
        .as_ref()
        .map_err(|e| e.clone())?;

    // Zero-copy: borrow the str's bytes directly — no Vec<u16> allocation.
    let bytes = request_json.as_bytes();
    let mut resp_ptr: *mut u8 = std::ptr::null_mut();
    let mut resp_len: i32 = 0;

    let hr = unsafe {
        (host.invoke)(
            bytes.as_ptr(),
            bytes.len() as i32,
            &mut resp_ptr,
            &mut resp_len,
        )
    };
    if hr < 0 {
        return Err(format!("DotNetBridge.Invoke HRESULT 0x{:08X}", hr as u32));
    }
    if resp_ptr.is_null() || resp_len < 0 {
        return Err("DotNetBridge.Invoke returned null/empty response".to_string());
    }

    let slice = unsafe { std::slice::from_raw_parts(resp_ptr, resp_len as usize) };
    // SAFETY: the bridge serialises JSON which is always valid UTF-8.
    let result = unsafe { std::str::from_utf8_unchecked(slice) }.to_owned();
    unsafe { (host.free)(resp_ptr) };
    Ok(result)
}

/// Registers the Rust V8 callback function with the managed bridge so that
/// .NET delegates created via opcode 0x09 can call back into JavaScript.
/// Must be called after `try_init_dotnet` succeeds.
pub(crate) fn init_js_callbacks(callback: FnJsCallback) {
    let host = match DOTNET_HOST.get() {
        Some(Ok(h)) => h,
        _ => return,
    };
    unsafe { (host.register_js_callback)(callback) };
}

/// Calls the managed bridge with a pre-built binary request packet and returns
/// the raw binary response bytes.  No JSON involved on either side.
pub(crate) fn call_dotnet_binary(request: &[u8]) -> Result<Vec<u8>, String> {
    let host = DOTNET_HOST
        .get()
        .ok_or_else(|| "DotNet host not yet initialised".to_string())?
        .as_ref()
        .map_err(|e| e.clone())?;

    let mut resp_ptr: *mut u8 = std::ptr::null_mut();
    let mut resp_len: i32 = 0;

    let hr = unsafe {
        (host.invoke_binary)(
            request.as_ptr(),
            request.len() as i32,
            &mut resp_ptr,
            &mut resp_len,
        )
    };
    if hr < 0 {
        return Err(format!("DotNetBridge.InvokeBinary HRESULT 0x{:08X}", hr as u32));
    }
    if resp_ptr.is_null() || resp_len < 0 {
        return Err("DotNetBridge.InvokeBinary returned null/empty response".to_string());
    }

    let slice = unsafe { std::slice::from_raw_parts(resp_ptr, resp_len as usize) };
    let result = slice.to_vec();
    unsafe { (host.free)(resp_ptr) };
    Ok(result)
}
