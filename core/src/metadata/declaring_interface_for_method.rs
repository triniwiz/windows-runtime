use crate::prelude::*;
use crate::bindings::{enums, imeta_data_import2, helpers};
use std::borrow::Cow;
use crate::metadata::com_helpers::get_string_value_from_blob;

#[derive(Debug)]
pub struct DeclaringInterfaceForMethod {}

impl DeclaringInterfaceForMethod {
	pub fn get_method_containing_class_token(metadata: *mut IMetaDataImport2, method_token: mdMethodDef) -> u32 {
		let mut class_token = mdTokenNil;

		match CorTokenType::from(enums::type_from_token(method_token)) {
			CorTokenType::mdtMethodDef => {
				debug_assert!(
					imeta_data_import2::get_method_props(
						metadata, method_token, Some(&mut class_token), None,
						None, None, None, None,
						None, None, None,
					).is_ok()
				);
			}
			CorTokenType::mdtMemberRef => {
				debug_assert!(
					imeta_data_import2::get_member_ref_props(
						metadata, method_token, Some(&mut class_token), None, None,
						None, None, None,
					).is_ok()
				);
			}
			_ => {
				std::unreachable!()
			}
		}

		return class_token;
	}

	pub fn get_custom_attribute_constructor_token(metadata: *mut IMetaDataImport2, custom_attribute: mdCustomAttribute) -> u32 {
		let mut constructor_token = mdTokenNil;
		debug_assert!(
			imeta_data_import2::get_custom_attribute_props(
				metadata, custom_attribute, None, Some(&mut constructor_token),
				None, None,
			).is_ok()
		);
		constructor_token
	}

	pub fn get_custom_attribute_class_token(metadata: *mut IMetaDataImport2, custom_attribute: mdCustomAttribute) -> u32 {
		DeclaringInterfaceForMethod::get_method_containing_class_token(metadata, DeclaringInterfaceForMethod::get_custom_attribute_constructor_token(metadata, custom_attribute))
	}

	pub fn get_method_signature<'a>(metadata: *mut IMetaDataImport2, token: mdMethodDef) -> Cow<'a, u8> {
		let mut signature = [0_u8; MAX_IDENTIFIER_LENGTH];
		let signature_ptr = &mut signature.as_mut_ptr();
		let mut signature_size = 0;
		match CorTokenType::from(enums::type_from_token(token)) {
			CorTokenType::mdtMethodDef => {
				imeta_data_import2::get_method_props(
					metadata, token,
					None, None, None, None,
					None, Some(signature_ptr as *mut *const u8),
					Some(&mut signature_size), None, None,
				).is_ok()
			}
			CorTokenType::mdtMemberRef => {
				imeta_data_import2::get_member_ref_props(
					metadata, token, None,
					None, None, None, Some(signature_ptr as *mut *const u8),
					Some(&mut signature_size),
				).is_ok()
			}
			_ => {
				std::unreachable!()
			}
		}
		// todo clean up
		let mut new_signature: [u8] = signature[..signature_size];
		let os = unsafe { new_signature.as_ptr().offset(1) };
		unsafe { std::slice::from_raw_parts(os, new_signature.len() - 1).into() }
	}

	pub fn get_signature_argument_count(metadata: *mut IMetaDataImport2, signature: PCCOR_SIGNATURE) -> u32 {
		helpers::cor_sig_uncompress_data(signature)
	}

	pub fn get_method_argument_count(metadata: *mut IMetaDataImport2, token: mdToken) {
		let signature = DeclaringInterfaceForMethod::get_method_signature(metadata, token).into_owned();
		DeclaringInterfaceForMethod::get_signature_argument_count(metadata, &signature);
	}


	pub fn get_custom_attributes_with_name(metadata: *mut IMetaDataImport2, token: mdTypeDef, attribute_name: *const wchar_t) {
		// mdCustomAttribute
		let mut attributes = [0;512];
	let mut attributes_count = 0;
	let mut attributes_enumerator = std::mem::MaybeUninit::uninit();
	let attributes_enumerator_ptr = &mut attributes_enumerator;

		debug_assert!(
			imeta_data_import2::enum_custom_attributes(
			metadata, Some(attributesEnumerator_ptr), token, None,
			Some(attributes.as_mut_ptr()), Some(attributes.len()),
			Some(&mut attributesCount);
		).is_ok()
		);
		debug_assert!(
			attributes_count < attributes.len() -1
		);
		imeta_data_import2::close_enum(
			metadata, attributes_enumerator
		);


	let mut filtered_attributes = Vec::new();
		let new_attributes: [i32] = attributes[..attributes_count];
		for attribute in new_attributes.iter() {
			let class_attribute_class_token = DeclaringInterfaceForMethod::get_custom_attribute_class_token(metadata, attribute);
			let mut name = [0_u16; MAX_IDENTIFIER_LENGTH];
			let length = helpers::get_type_name(metadata, class_attribute_class_token, name.as_mut_ptr(), name.len() as u32);
			let mut class_attribute_class_name:[u16] = name[..length];
			// TODO
			if class_attribute_class_name.as_ptr() != attribute_name {
				continue;
			}

			filtered_attributes.push(
				attribute
			);

			filtered_attributes.push_back(attribute);
		}
		filtered_attributes
	}

	pub fn get_custom_attribute_type_argument(metadata: *mut IMetaDataImport2, token: mdCustomAttribute) {
	let mut attribute_value = std::mem::MaybeUninit::uninit();
		let attribute_value_ptr = &mut attribute_value;
	let mut attribute_value_size = 0;
		debug_assert!(imeta_data_import2::get_custom_attribute_props(
			metadata, token,
			None, None, Some(attribute_value_ptr as *mut *const u8), Some(&mut attribute_value_size)
		).is_ok());

		let type_name = get_string_value_from_blob(
			metadata, attribute_value.as_ptr().offset(2)
		);


	let mut typeToken = mdTokenNil;
		debug_assert!(imeta_data_import2::find_type_def_by_name(
			metadata, type_name_data, None, Some(&mut typeToken)
		).is_ok())
	return typeToken;
	}

}