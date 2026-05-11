/// Dynamic Win32 FFI dispatch via libffi.
///
/// JS calls `__nsWin32Call(jsonRequest)` → Rust loads the DLL, resolves the
/// function pointer, builds a libffi CIF, marshals arguments, and invokes.
///
/// Request JSON:
///   { "dll": "user32.dll", "fn": "MessageBoxW", "returnType": "i32",
///     "args": [{"type":"pointer","value":0}, {"type":"wstr","value":"Hi"}, …] }
///
/// Response JSON:
///   { "value": 1 }   |   { "error": "…" }
///
/// Supported arg/return types:
///   "i8","i16","i32","i64","u8","u16","u32","u64","f32","f64",
///   "pointer","wstr","str","bool","void"

use std::collections::HashMap;
use std::ffi::{c_void, CString};
use libffi::middle::{Arg, Cif, CodePtr, Type};
use parking_lot::Mutex;
use serde_json::Value;
use windows::core::PCWSTR;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

// ── DLL handle cache ──────────────────────────────────────────────────────────

static DLL_CACHE: std::sync::OnceLock<Mutex<HashMap<String, usize>>> =
    std::sync::OnceLock::new();

fn dll_cache() -> &'static Mutex<HashMap<String, usize>> {
    DLL_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn load_dll(name: &str) -> Result<usize, String> {
    let mut cache = dll_cache().lock();
    if let Some(&h) = cache.get(name) {
        return Ok(h);
    }
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let hmod = unsafe {
        LoadLibraryW(PCWSTR(wide.as_ptr()))
            .map_err(|e| format!("LoadLibraryW({name}): {e}"))?
    };
    let handle = hmod.0 as usize;
    cache.insert(name.to_string(), handle);
    Ok(handle)
}

fn resolve_proc(dll_handle: usize, func_name: &str) -> Result<*mut c_void, String> {
    use windows::Win32::Foundation::HMODULE;
    use windows::core::PCSTR;
    let cname = format!("{func_name}\0");
    let hmod = HMODULE(dll_handle as *mut c_void);
    let ptr = unsafe { GetProcAddress(hmod, PCSTR(cname.as_ptr())) }
        .ok_or_else(|| format!("GetProcAddress({func_name}) failed"))?;
    Ok(ptr as *mut c_void)
}

// ── Type mapping ──────────────────────────────────────────────────────────────

fn ffi_type_for(ty: &str) -> Result<Type, String> {
    Ok(match ty {
        "void"    => Type::void(),
        "i8"      => Type::i8(),
        "i16"     => Type::i16(),
        "i32"     => Type::i32(),
        "i64"     => Type::i64(),
        "u8"      => Type::u8(),
        "u16"     => Type::u16(),
        "u32"     => Type::u32(),
        "u64"     => Type::u64(),
        "f32"     => Type::f32(),
        "f64"     => Type::f64(),
        "bool"    => Type::i32(),
        "pointer" => Type::pointer(),
        "wstr"    => Type::pointer(),
        "str"     => Type::pointer(),
        other     => return Err(format!("Unknown FFI type: {other}")),
    })
}

// ── Stored argument ───────────────────────────────────────────────────────────
//
// Each variant keeps its data alive for the duration of the call.  Pointer-like
// variants store the pointer VALUE in a field so that `Arg::new(&self.field)`
// gives libffi a stable reference to the pointer, which it will dereference once
// when building the call frame (standard libffi ABI: pass `&&value`).

enum StoredArg {
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    F32(f32),
    F64(f64),
    // Pointer args: the pointer value itself sits in `ptr_val`; we pass `&ptr_val`
    // so libffi can dereference it once to get the actual pointer for the call frame.
    Ptr { ptr_val: *const c_void },
    WStr { ptr_val: *const u16, _buf: Vec<u16> },
    Str  { ptr_val: *const i8,  _buf: CString  },
}

// SAFETY: StoredArg is only used within a single-threaded call site.
unsafe impl Send for StoredArg {}
unsafe impl Sync for StoredArg {}

impl StoredArg {
    fn as_ffi_arg(&self) -> Arg<'_> {
        match self {
            StoredArg::I8(v)               => Arg::new(v),
            StoredArg::I16(v)              => Arg::new(v),
            StoredArg::I32(v)              => Arg::new(v),
            StoredArg::I64(v)              => Arg::new(v),
            StoredArg::U8(v)               => Arg::new(v),
            StoredArg::U16(v)              => Arg::new(v),
            StoredArg::U32(v)              => Arg::new(v),
            StoredArg::U64(v)              => Arg::new(v),
            StoredArg::F32(v)              => Arg::new(v),
            StoredArg::F64(v)              => Arg::new(v),
            StoredArg::Ptr  { ptr_val, .. } => Arg::new(ptr_val),
            StoredArg::WStr { ptr_val, .. } => Arg::new(ptr_val),
            StoredArg::Str  { ptr_val, .. } => Arg::new(ptr_val),
        }
    }
}

fn marshal(ty: &str, val: &Value) -> Result<StoredArg, String> {
    let n = || val.as_f64().unwrap_or(0.0);
    Ok(match ty {
        "i8"  => StoredArg::I8 (n() as i8),
        "i16" => StoredArg::I16(n() as i16),
        "i32" => StoredArg::I32(n() as i32),
        "i64" => StoredArg::I64(n() as i64),
        "u8"  => StoredArg::U8 (n() as u8),
        "u16" => StoredArg::U16(n() as u16),
        "u32" => StoredArg::U32(n() as u32),
        "u64" => StoredArg::U64(val.as_u64().unwrap_or(n() as u64)),
        "f32" => StoredArg::F32(n() as f32),
        "f64" => StoredArg::F64(n()),
        "bool" => {
            let b = val.as_bool().unwrap_or(n() != 0.0);
            StoredArg::I32(if b { 1 } else { 0 })
        }
        "pointer" => {
            let addr = val.as_u64().unwrap_or(n() as u64) as usize;
            StoredArg::Ptr { ptr_val: addr as *const c_void }
        }
        "wstr" => {
            let s = val.as_str().unwrap_or("");
            let buf: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
            let ptr_val = buf.as_ptr();
            StoredArg::WStr { ptr_val, _buf: buf }
        }
        "str" => {
            let s = val.as_str().unwrap_or("");
            let buf = CString::new(s).unwrap_or_default();
            let ptr_val = buf.as_ptr();
            StoredArg::Str { ptr_val, _buf: buf }
        }
        other => return Err(format!("Unsupported arg type: {other}")),
    })
}

// ── PE export table enumeration ───────────────────────────────────────────────

/// Returns every named export from `dll` by parsing its PE export directory.
/// Used by `NSWinRT.win32.import(dll)` to inject all DLL functions as globals.
pub(crate) fn list_exports(dll: &str) -> Result<Vec<String>, String> {
    let handle = load_dll(dll)?;
    let base = handle as *const u8;

    let names = unsafe {
        // DOS header: first 2 bytes must be 'MZ' (0x5A4D).
        let dos_magic = base.cast::<u16>().read_unaligned();
        if dos_magic != 0x5A4D {
            return Err(format!("{dll}: not a PE file"));
        }

        // e_lfanew at offset 0x3C → offset of the NT headers.
        let e_lfanew = base.add(0x3C).cast::<i32>().read_unaligned();
        if !(4..=1_048_576).contains(&e_lfanew) {
            return Err(format!("{dll}: invalid e_lfanew"));
        }

        let nt = base.add(e_lfanew as usize);

        // NT signature must be 'PE\0\0' (0x00004550).
        if nt.cast::<u32>().read_unaligned() != 0x0000_4550 {
            return Err(format!("{dll}: bad PE signature"));
        }

        // Optional header magic at nt+24: 0x10B = PE32, 0x20B = PE32+.
        let opt_magic = nt.add(24).cast::<u16>().read_unaligned();

        // Export directory RVA is the first data-directory entry.
        // Its offset within the optional header is 96 (PE32) or 112 (PE32+).
        let export_dir_off: usize = if opt_magic == 0x020B { 24 + 112 } else { 24 + 96 };
        let export_rva = nt.add(export_dir_off).cast::<u32>().read_unaligned();
        if export_rva == 0 {
            return Ok(vec![]);
        }

        let exp = base.add(export_rva as usize);

        // IMAGE_EXPORT_DIRECTORY layout (all u32 unless noted):
        //   +0  Characteristics
        //   +4  TimeDateStamp
        //   +8  MajorVersion (u16)
        //  +10  MinorVersion (u16)
        //  +12  Name (RVA of module name string)
        //  +16  Base
        //  +20  NumberOfFunctions
        //  +24  NumberOfNames
        //  +28  AddressOfFunctions
        //  +32  AddressOfNames  ← array of RVAs to name strings
        //  +36  AddressOfNameOrdinals
        let number_of_names = exp.add(24).cast::<u32>().read_unaligned() as usize;
        let names_rva       = exp.add(32).cast::<u32>().read_unaligned();

        if names_rva == 0 || number_of_names == 0 {
            return Ok(vec![]);
        }

        let name_table = base.add(names_rva as usize) as *const u32;

        (0..number_of_names)
            .filter_map(|i| {
                let name_rva: u32 = name_table.add(i).read_unaligned();
                let name_ptr = base.add(name_rva as usize) as *const i8;
                std::ffi::CStr::from_ptr(name_ptr)
                    .to_str()
                    .ok()
                    .map(|s| s.to_owned())
            })
            .collect::<Vec<_>>()
    };

    Ok(names)
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Parse and execute a Win32 FFI call described by a JSON request string.
/// Returns `{"value":<result>}` or `{"error":"…"}`.
pub(crate) fn call_win32_json(json: &str) -> Result<String, String> {
    let req: Value = serde_json::from_str(json)
        .map_err(|e| format!("JSON parse error: {e}"))?;

    let dll       = req["dll"].as_str().ok_or("missing 'dll'")?;
    let func_name = req["fn"].as_str().ok_or("missing 'fn'")?;
    let ret_type  = req["returnType"].as_str().unwrap_or("i32");
    let args_arr  = req["args"].as_array().map(|a| a.as_slice()).unwrap_or(&[]);

    let dll_handle = load_dll(dll)?;
    let fn_ptr     = resolve_proc(dll_handle, func_name)?;

    let mut arg_types: Vec<Type>     = Vec::with_capacity(args_arr.len());
    let mut stored:    Vec<StoredArg> = Vec::with_capacity(args_arr.len());

    for a in args_arr {
        let ty  = a["type"].as_str().ok_or("arg missing 'type'")?;
        let val = &a["value"];
        arg_types.push(ffi_type_for(ty)?);
        stored.push(marshal(ty, val)?);
    }

    let cif  = Cif::new(arg_types, ffi_type_for(ret_type)?);
    let code = CodePtr(fn_ptr);
    let ffi_args: Vec<Arg<'_>> = stored.iter().map(|s| s.as_ffi_arg()).collect();

    let result = match ret_type {
        "void" => {
            unsafe { cif.call::<()>(code, &ffi_args) };
            r#"{"value":null}"#.to_string()
        }
        "i8"  => { let v: i8  = unsafe { cif.call(code, &ffi_args) }; format!(r#"{{"value":{v}}}"#) }
        "i16" => { let v: i16 = unsafe { cif.call(code, &ffi_args) }; format!(r#"{{"value":{v}}}"#) }
        "i32" => { let v: i32 = unsafe { cif.call(code, &ffi_args) }; format!(r#"{{"value":{v}}}"#) }
        "i64" => { let v: i64 = unsafe { cif.call(code, &ffi_args) }; format!(r#"{{"value":{v}}}"#) }
        "u8"  => { let v: u8  = unsafe { cif.call(code, &ffi_args) }; format!(r#"{{"value":{v}}}"#) }
        "u16" => { let v: u16 = unsafe { cif.call(code, &ffi_args) }; format!(r#"{{"value":{v}}}"#) }
        "u32" => { let v: u32 = unsafe { cif.call(code, &ffi_args) }; format!(r#"{{"value":{v}}}"#) }
        "u64" => { let v: u64 = unsafe { cif.call(code, &ffi_args) }; format!(r#"{{"value":{v}}}"#) }
        "f32" => { let v: f32 = unsafe { cif.call(code, &ffi_args) }; format!(r#"{{"value":{v}}}"#) }
        "f64" => { let v: f64 = unsafe { cif.call(code, &ffi_args) }; format!(r#"{{"value":{v}}}"#) }
        "bool"    => { let v: i32  = unsafe { cif.call(code, &ffi_args) }; format!(r#"{{"value":{}}}"#, v != 0) }
        "pointer" => { let v: usize = unsafe { cif.call(code, &ffi_args) }; format!(r#"{{"value":{v}}}"#) }
        other => return Err(format!("Unsupported return type: {other}")),
    };

    Ok(result)
}
