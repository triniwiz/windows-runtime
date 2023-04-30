use std::borrow::Cow;
use windows::Win32::System::WinRT::Metadata::{IMetaDataImport2, MDTypeRefToDef};

mod com_helpers;
pub mod declarations;
mod prelude;
pub mod meta_data_reader;
pub mod value;


#[cxx::bridge]
mod ffi {

    unsafe extern "C++" {
        include!("metadata/src/bindings.h");

        pub fn BindingsCCorSigUncompressCallingConv(sig: &[u8])-> u32;

        pub fn BindingsCCorSigUncompressData(sig: &[u8])-> u32;

        pub fn BindingsCCorSigUncompressElementType(sig: &[u8])-> i32;

        pub fn BindingsCCorSigUncompressToken(sig: &[u8])-> i32;
    }
}

pub fn cor_sig_uncompress_calling_conv(sig: &[u8]) -> u32 {
    ffi::BindingsCCorSigUncompressCallingConv(sig)
}


pub fn cor_sig_uncompress_data(sig: &[u8]) -> u32 {
    ffi::BindingsCCorSigUncompressData(sig)
}

pub fn cor_sig_uncompress_element_type(sig: &[u8]) -> i32 {
    ffi::BindingsCCorSigUncompressElementType(sig)
}

pub fn cor_sig_uncompress_token(sig: &[u8]) -> i32 {
    ffi::BindingsCCorSigUncompressToken(sig)
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