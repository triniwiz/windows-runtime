use std::ffi::c_void;
use windows::core::{GUID, HSTRING, IUnknown, Interface};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::System::WinRT::Metadata::{
    CLSID_CorMetaDataDispenser, IMetaDataDispenserEx, IMetaDataImport2, ofRead,
};

pub mod com_helpers;
pub mod declarations;
pub mod prelude;
pub mod meta_data_reader;
pub mod value;
pub mod signature;
pub mod generic_instance_id_builder;
pub mod declaration_factory;
pub mod declaring_interface_for_method;

// ---------------------------------------------------------------------------
// Vtable / COM helpers
// ---------------------------------------------------------------------------

pub fn get_method(iface: &IUnknown, index: usize, method: *mut *mut c_void) {
    if method.is_null() {
        return;
    }

    unsafe {
        let vtable = *(iface.as_raw() as *mut *mut *mut c_void);
        *method = *vtable.add(index);
    }
}

pub fn query_interface(
    index: usize,
    factory: &IUnknown,
    guid: &GUID,
    activation_factory: &mut IUnknown,
    func: *mut *mut c_void,
) {
    let _ = activation_factory;
    if func.is_null() {
        return;
    }

    unsafe {
        *func = std::ptr::null_mut();

        let mut queried = std::ptr::null_mut();
        if factory.query(guid, &mut queried).is_err() || queried.is_null() {
            return;
        }

        let queried = IUnknown::from_raw(queried);
        let vtable = *(queried.as_raw() as *mut *mut *mut c_void);
        *func = *vtable.add(index);
    }
}

// ---------------------------------------------------------------------------
// GUID helpers
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

    unsafe {
        let dispenser: IMetaDataDispenserEx =
            CoCreateInstance(&CLSID_CorMetaDataDispenser, None, CLSCTX_INPROC_SERVER).ok()?;
        let path = HSTRING::from(path_str.as_ref());
        let unknown = dispenser
            .OpenScope(&path, ofRead.0 as u32, &IMetaDataImport2::IID)
            .ok()?;

        Some(IMetaDataImport2::from_raw(unknown.into_raw()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_guid_without_cxx_bridge() {
        let guid = GUID {
            data1: 0x12345678,
            data2: 0x9abc,
            data3: 0xdef0,
            data4: [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0],
        };

        assert_eq!(
            guid_to_string(&guid),
            "{12345678-9ABC-DEF0-1234-56789ABCDEF0}"
        );
    }

    #[test]
    fn reads_unaligned_windows_guid_bytes() {
        let bytes = [
            0x78, 0x56, 0x34, 0x12, 0xbc, 0x9a, 0xf0, 0xde, 0x12, 0x34, 0x56, 0x78,
            0x9a, 0xbc, 0xde, 0xf0,
        ];

        let guid = unsafe { get_guid(bytes.as_ptr()) };

        assert_eq!(guid.data1, 0x12345678);
        assert_eq!(guid.data2, 0x9abc);
        assert_eq!(guid.data3, 0xdef0);
        assert_eq!(guid.data4, [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0]);
    }
}
