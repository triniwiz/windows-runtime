use std::borrow::Cow;
use std::fmt::{Debug, Formatter};
use cxx::UniquePtr;
use windows::core::GUID;
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

        type GUID;

        pub unsafe fn GetGUID(data: *const u8) -> UniquePtr<GUID>;

        pub fn GetData1(guid: &GUID) -> u32;

        pub fn GetData2(guid: &GUID) -> u16;

        pub fn GetData3(guid: &GUID) -> u16;

        pub fn GetData4(guid: &GUID) -> &[u8];
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