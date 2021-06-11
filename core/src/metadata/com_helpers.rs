use crate::prelude::*;
use crate::bindings::{helpers, imeta_data_import2, enums, rometadataresolution};
use std::borrow::Cow;


const GUID_ATTRIBUTE: &str = "Windows.Foundation.Metadata.GuidAttribute";

pub const SYSTEM_TYPE: &str = "System.Type";
pub const STATIC_ATTRIBUTE: &str = "Windows.Foundation.Metadata.StaticAttribute";
pub const ACTIVATABLE_ATTRIBUTE: &str = "Windows.Foundation.Metadata.ActivatableAttribute";
pub const COMPOSABLE_ATTRIBUTE: &str = "Windows.Foundation.Metadata.ComposableAttribute";


pub fn get_string_value_from_blob<'a>(metadata: *mut IMetaDataImport2, signature: PCCOR_SIGNATURE) -> Cow<'a, str> {
	debug_assert!(!metadata.is_null());
	debug_assert!(!signature.is_null());

	if signature as u8 == u8::MAX {
		return "".into();
	}

	let size = helpers::cor_sig_uncompress_data(signature);

	let slice = unsafe { std::slice::from_raw_parts(signature as *mut u16, size as usize) };

	windows::HSTRING::from_wide(slice).to_string_lossy().into()
}


pub fn get_unary_custom_attribute_string_value<'a>(metadata: *mut c_void, token: mdToken, attribute_name: &str) -> Cow<'a, str> {
	debug_assert!(!metadata.is_null());
	debug_assert!(token != mdTokenNil);
	debug_assert!(!attribute_name.is_null());


	let mut data = vec![0_u8; MAX_IDENTIFIER_LENGTH];
	let name = OsString::from(attribute_name.to_owned()).to_wide();
	let data_ptr = &data.as_mut_ptr() as *mut *const c_void;
	let result = imeta_data_import2::get_custom_attribute_by_name(
		metadata, token, Some(name.as_ptr()), Some(data_ptr), None,
	);
	debug_assert!(
		result.is_ok()
	);
	if result.is_err() {
		return "".into();
	}

	get_string_value_from_blob(metadata, data + 2)
}


pub fn resolve_type_ref(metadata: *mut IMetaDataImport2, token: mdTypeRef, external_metadata: *mut *mut c_void, external_token: *mut mdTypeDef) -> bool {
	debug_assert!(!metadata.is_null());
	debug_assert!(enums::type_from_token(token) == CorTokenType::mdtTypeRef as u32);
	debug_assert!(!external_metadata.is_null());
	debug_assert!(!external_token.is_null());

	let mut data = vec![0_u16; MAX_IDENTIFIER_LENGTH];
	debug_assert!(
		imeta_data_import2::get_type_ref_props(
			metadata, token, None, Some(data.as_mut_ptr()), Some(data.len() as u32), None,
		).is_ok()
	);
	let mut string = windows::HSTRING::from_wide(data.as_slice());
	rometadataresolution::ro_get_meta_data_file(
		&mut string, None, None, Some(external_metadata), Some(external_token),
	).is_ok()
}


pub fn get_guid_attribute_value(metadata: *mut IMetaDataImport2, token: mdToken) -> GUID {
	debug_assert!(!metadata.is_null());
	debug_assert!(token != mdTokenNil);
	let mut size = 0;
	let mut data = vec![0_u8; MAX_IDENTIFIER_LENGTH];
	let name = OsString::from(GUID_ATTRIBUTE).to_wide();
	let data_ptr = &data.as_mut_ptr() as *mut *const c_void;
	imeta_data_import2::get_custom_attribute_by_name(
		metadata, token, Some(name.as_ptr()), Some(data_ptr), Some(&mut size),
	);
	data.resize(size as _, 0);

	// Skip prolog
	let os_data = unsafe { data.as_mut_ptr().offset(2) };
	let mut guid = GUID::default();
	helpers::bytes_to_guid(
		os_data, &mut guid,
	);
	guid
}

pub fn get_type_name<'a>(metadata: *mut IMetaDataImport2, token: mdToken) -> Cow<'a, str> {
	debug_assert!(!metadata.is_null());
	debug_assert!(token != mdTokenNil);

	let mut name_data = [0_u16; MAX_IDENTIFIER_LENGTH];
	let mut name_length = 0;
	match CorTokenType::from(enums::type_from_token(token)) {
		CorTokenType::mdtTypeDef => {
			debug_assert!(imeta_data_import2::get_type_def_props(
				metadata, token, Some(name_data.as_mut_ptr()), Some(name_data.len() as u32), Some(&mut name_length), None, None,
			).is_ok());
		}
		CorTokenType::mdtTypeRef => {
			debug_assert!(
				imeta_data_import2::get_type_ref_props(
					metadata, token, None, Some(name_data.as_mut_ptr()), Some(name_data.len() as u32), Some(&mut name_length),
				).is_ok()
			);
		}
		_ => {
			std::unreachable!()
		}
	}

	OsString::from_wide(name_data[..name_length]).into()
}