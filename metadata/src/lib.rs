use std::borrow::Cow;
use std::ffi::{c_void, OsString};
use std::fmt::{Debug, Formatter};
use std::os::windows::ffi::EncodeWide;
use std::os::windows::prelude::OsStrExt;
use cxx::UniquePtr;
use windows::core::{GUID, HSTRING, IUnknown, PCWSTR, Type};
use windows::Win32::System::Com::{CLSIDFromProgID, CLSIDFromString};
use windows::Win32::System::WinRT::Metadata::{CorElementType, CorTokenType, IMetaDataImport2, MDTypeRefToDef};
use crate::prelude::PCCOR_SIGNATURE;

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

        type HSTRING;

        type GUID;

        type c_void;

        pub unsafe fn GetGUID(data: *const u8) -> UniquePtr<GUID>;

        pub fn GetData1(guid: &GUID) -> u32;

        pub fn GetData2(guid: &GUID) -> u16;

        pub fn GetData3(guid: &GUID) -> u16;

        pub fn GetData4(guid: &GUID) -> &[u8];

        pub fn GUIDToString(data1: u32, data2: u16, data3: u16, data4: &[u8]) -> String;

        pub unsafe fn QueryInterface(index: usize, factory: *mut c_void, data1: u32, data2: u16, data3: u16, data4: &[u8], activation_factory: *mut c_void, func: *mut *mut c_void);
    }
}

impl Debug for ffi::GUID {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        unsafe {
            f.debug_struct(
                "GUID"
            )
                .field("Data1", &ffi::GetData1(self))
                .field("Data2", &ffi::GetData2(self))
                .field("Data3", &ffi::GetData3(self))
                .field("Data4", &ffi::GetData4(self))
                .finish()
        }
    }
}

pub fn guid_to_string(value: &GUID) -> String {
    unsafe { ffi::GUIDToString(value.data1, value.data2, value.data3, &value.data4) }
}

pub fn get_guid(data: *const u8) -> GUID {
    let guid: UniquePtr<ffi::GUID> = unsafe { ffi::GetGUID(data) };

    unsafe {
        GUID::from_values(
            ffi::GetData1(&guid),
            ffi::GetData2(&guid),
            ffi::GetData3(&guid),
            ffi::GetData4(&guid).try_into().unwrap(),
        )
    }
}

pub fn query_interface(index: usize, factory: &IUnknown, guid: &GUID, activation_factory: &mut IUnknown, func: *mut *mut c_void) {
    unsafe { ffi::QueryInterface(index, std::mem::transmute_copy(factory), guid.data1, guid.data2, guid.data3, guid.data4.as_slice(), std::mem::transmute_copy(activation_factory), std::mem::transmute(func)) }
}