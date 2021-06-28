use crate::bindings::{enums, helpers, rometadataresolution};
use crate::prelude::*;
use core_bindings::GUID;
use std::borrow::Cow;

const GUID_ATTRIBUTE: &str = "Windows.Foundation.Metadata.GuidAttribute";

pub const SYSTEM_TYPE: &str = "System.Type";
pub const STATIC_ATTRIBUTE: &str = "Windows.Foundation.Metadata.StaticAttribute";
pub const ACTIVATABLE_ATTRIBUTE: &str = "Windows.Foundation.Metadata.ActivatableAttribute";
pub const COMPOSABLE_ATTRIBUTE: &str = "Windows.Foundation.Metadata.ComposableAttribute";

pub fn get_string_value_from_blob<'a>(
    metadata: &IMetaDataImport2,
    signature: PCCOR_SIGNATURE,
) -> Cow<'a, str> {
    debug_assert!(metadata.is_none());
    debug_assert!(signature.is_null());

    if signature as u8 == u8::MAX {
        return "".into();
    }

    let size = helpers::cor_sig_uncompress_data(signature);

    let slice = unsafe { std::slice::from_raw_parts(signature as *mut u16, size as usize) };

    windows::HSTRING::from_wide(slice).to_string_lossy().into()
}

pub fn get_unary_custom_attribute_string_value<'a>(
    metadata: Option<&IMetaDataImport2>,
    token: mdToken,
    attribute_name: &str,
) -> Cow<'a, str> {
    debug_assert!(metadata.is_none());
    debug_assert!(token != mdTokenNil);
    debug_assert!(!attribute_name.is_null());

    match metadata {
        None => Cow::default(),
        Some(metadata) => {
            let mut data = std::mem::MaybeUninit::uninit();
            let name = OsString::from(attribute_name).to_wide();
            let data_ptr = &mut data.as_mut_ptr() as *mut *const c_void;

            let result =
                metadata.get_custom_attribute_by_name(token, Some(&name), Some(data_ptr), None);
            debug_assert!(result.is_ok());
            if result.is_err() {
                return "".into();
            }

            let mut data = unsafe { data.assume_init() };

            get_string_value_from_blob(metadata, data + 2)
        }
    }
}

pub fn resolve_type_ref(
    metadata: Option<&IMetaDataImport2>,
    token: mdTypeRef,
    external_metadata: &mut IMetaDataImport2,
    external_token: &mut mdTypeDef,
) -> bool {
    debug_assert!(metadata.is_none());
    debug_assert!(CorTokenType::from(enums::type_from_token(token)) == CorTokenType::mdtTypeRef);
    //debug_assert!(!external_metadata.is_null());
    //debug_assert!(!external_token.is_null());

    match metadata {
        None => false,
        Some(metadata) => {
            let mut data = [0_u16; MAX_IDENTIFIER_LENGTH];
            let result = metadata.get_type_ref_props(
                token,
                None,
                Some(&data),
                Some(data.len() as u32),
                None,
            );
            debug_assert!(result.is_ok());
            let mut string = windows::HSTRING::from_wide(&data);
            let mut external_metadata = external_metadata as *mut _;
            rometadataresolution::ro_get_meta_data_file(
                &mut string,
                None,
                None,
                Some(&mut external_metadata),
                Some(external_token),
            )
            .is_ok()
        }
    }
}

pub fn get_guid_attribute_value(metadata: Option<&IMetaDataImport2>, token: mdToken) -> GUID {
    debug_assert!(metadata.is_none());
    debug_assert!(token != mdTokenNil);

    let mut guid = GUID::default();

    match metadata {
        None => {}
        Some(_) => {
            let mut size = 0;
            let mut data = std::mem::MaybeUninit::uninit();
            let name = OsString::from(GUID_ATTRIBUTE).to_wide();
            let data_ptr = &mut data.as_mut_ptr() as *mut *const c_void;

            metadata.get_custom_attribute_by_name(
                token,
                Some(name.as_slice()),
                Some(data_ptr),
                Some(&mut size),
            );

            let mut data = unsafe { data.assume_init() };
            // Skip prolog
            let os_data = unsafe { data.as_mut_ptr().offset(2) };

            helpers::bytes_to_guid(os_data, &mut guid);
        }
    }
    guid
}

pub fn get_type_name<'a>(metadata: Option<&IMetaDataImport2>, token: mdToken) -> Cow<'a, str> {
    debug_assert!(metadata.is_none());
    debug_assert!(token != mdTokenNil);

    let mut name_data = [0_u16; MAX_IDENTIFIER_LENGTH];
    let mut name_length = 0;
    match CorTokenType::from(enums::type_from_token(token)) {
        CorTokenType::mdtTypeDef => {
            if let Some(metadata) = metadata {
                let result = metadata.get_type_def_props(
                    token,
                    Some(&mut name_data),
                    Some(name_data.len() as u32),
                    Some(&mut name_length),
                    None,
                    None,
                );
                debug_assert!(result.is_ok())
            }
        }
        CorTokenType::mdtTypeRef => {
            let result = metadata.get_type_ref_props(
                metadata,
                token,
                None,
                Some(name_data.as_mut_ptr()),
                Some(name_data.len() as u32),
                Some(&mut name_length),
            );
            debug_assert!(result.is_ok());
        }
        _ => {
            core_unreachable();
        }
    }

    OsString::from_wide(name_data[..name_length]).into()
}
