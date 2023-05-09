use std::borrow::Cow;
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


#[cxx::bridge]
mod ffi {

    unsafe extern "C++" {
        include!("metadata/src/bindings.h");

        pub unsafe fn BindingsCCorSigUncompressCallingConv(sig: *mut u8)-> i32;

        pub unsafe fn BindingsCCorSigUncompressData(sig: *mut u8)-> i32;

        pub unsafe fn BindingsCCorSigUncompressElementType(sig: *mut u8)-> i32;

        pub unsafe fn BindingsCCorSigUncompressToken(sig: *mut u8)-> i32;

    }
}

pub fn cor_sig_uncompress_calling_conv(sig: &mut PCCOR_SIGNATURE) -> i32 {
    unsafe { ffi::BindingsCCorSigUncompressCallingConv(sig.as_abi_mut()) }
}

pub fn cor_sig_uncompress_data(sig: &mut PCCOR_SIGNATURE) -> i32 {
    unsafe { ffi::BindingsCCorSigUncompressData(sig.as_abi_mut()) }
}

pub fn cor_sig_uncompress_element_type(sig: &mut PCCOR_SIGNATURE) -> i32 {
    unsafe { ffi::BindingsCCorSigUncompressElementType(sig.as_abi_mut()) }
}

pub fn cor_sig_uncompress_token(sig: &mut PCCOR_SIGNATURE) -> i32 {
    unsafe { ffi::BindingsCCorSigUncompressToken(sig.as_abi_mut()) }
}

/*
pub fn get_string_value_from_blob<'a>(
    metadata: &IMetaDataImport2,
    signature: PCCOR_SIGNATURE,
) -> Cow<'a, str> {
    debug_assert!(metadata.is_none());
    debug_assert!(signature.is_null());

    if signature as u8 == u8::MAX {
        return "".into();
    }



   // PCCOR_SIGNATURE
    //CorSigUncompressData

    let size = helpers::cor_sig_uncompress_data(signature);

    let slice = unsafe { std::slice::from_raw_parts(signature as *mut u16, size as usize) };

    windows::HSTRING::from_wide(slice).to_string_lossy().into()
}
*/