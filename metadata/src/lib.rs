use std::ffi::c_void;
use std::fmt::{Debug, Formatter};
use windows::core::{GUID, IUnknown};
use windows::Win32::System::WinRT::Metadata::IMetaDataImport2;

pub mod com_helpers;
pub mod declarations;
pub mod prelude;
pub mod meta_data_reader;
pub mod value;
pub mod signature;
pub mod generic_instance_id_builder;
pub mod declaration_factory;
pub mod declaring_interface_for_method;


#[cxx::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("metadata/src/bindings.h");

        type c_void;

        type IUnknown;

        // Returns the function pointer at vtable slot `index` of `iface`.
        pub unsafe fn GetMethod(iface: *mut IUnknown, index: usize, method: *mut *mut c_void);

        // Calls QueryInterface on `factory` using the given GUID, then returns the vtable
        // function pointer at `index` from the resulting interface via `func`.
        pub unsafe fn QueryInterface(
            index: usize,
            factory: *mut c_void,
            data1: u32,
            data2: u16,
            data3: u16,
            data4: &[u8],
            activation_factory: *mut c_void,
            func: *mut *mut c_void,
        );

        // Opens an IMetaDataImport2 scope for any CLI metadata file (.dll, .winmd, .exe).
        // Returns an AddRef'd raw pointer; caller must Release() it.  Returns null on error.
        pub unsafe fn OpenMetadataScope(path: &str) -> *mut c_void;
    }
}

// ---------------------------------------------------------------------------
// Vtable / COM helpers
// ---------------------------------------------------------------------------

pub fn get_method(iface: &IUnknown, index: usize, method: *mut *mut c_void) {
    unsafe {
        ffi::GetMethod(std::mem::transmute_copy(iface), index, std::mem::transmute(method))
    }
}

pub fn query_interface(
    index: usize,
    factory: &IUnknown,
    guid: &GUID,
    activation_factory: &mut IUnknown,
    func: *mut *mut c_void,
) {
    unsafe {
        ffi::QueryInterface(
            index,
            std::mem::transmute_copy(factory),
            guid.data1,
            guid.data2,
            guid.data3,
            guid.data4.as_slice(),
            std::mem::transmute_copy(activation_factory),
            std::mem::transmute(func),
        )
    }
}

// ---------------------------------------------------------------------------
// GUID helpers — pure Rust, no C++ required
// ---------------------------------------------------------------------------

/// Formats a GUID as `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}`.
pub fn guid_to_string(value: &GUID) -> String {
    format!(
        "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        value.data1,
        value.data2,
        value.data3,
        value.data4[0],
        value.data4[1],
        value.data4[2],
        value.data4[3],
        value.data4[4],
        value.data4[5],
        value.data4[6],
        value.data4[7],
    )
}

/// Reads a GUID from a raw 16-byte blob in Windows byte order.
///
/// # Safety
/// `data` must point to at least 16 readable bytes in Windows GUID layout.
pub unsafe fn get_guid(data: *const u8) -> GUID {
    std::ptr::read_unaligned(data as *const GUID)
}

// ---------------------------------------------------------------------------
// Metadata file loading
// ---------------------------------------------------------------------------

/// Opens a CLI metadata scope (IMetaDataImport2) for any file that carries
/// embedded CLI metadata (.dll, .winmd, .exe).  Returns `None` if the file
/// cannot be opened or does not contain CLI metadata.
///
/// This uses `IMetaDataDispenserEx::OpenScope` via the system CLR dispenser,
/// so it works for arbitrary .NET assemblies, not just registered WinRT types.
pub fn open_metadata_scope_from_file(path: &std::path::Path) -> Option<IMetaDataImport2> {
    let path_str = path.to_string_lossy();
    let raw = unsafe { ffi::OpenMetadataScope(&path_str) };
    if raw.is_null() {
        return None;
    }
    // SAFETY: OpenMetadataScope returns a valid AddRef'd IMetaDataImport2 COM pointer.
    // IMetaDataImport2 in windows-rs is #[repr(transparent)] over a single pointer,
    // so transmuting from *mut c_void with the same pointer value is well-defined.
    Some(unsafe { std::mem::transmute(raw) })
}
