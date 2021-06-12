#![allow(non_upper_case_globals)]

use crate::prelude::*;
use crate::bindings::helpers;
use std::borrow::Cow;


const Guid: &str = "Guid";

#[derive(Debug)]
pub struct Signature {}

impl Signature {
	pub fn consume_type(signature: PCCOR_SIGNATURE) -> PCCOR_SIGNATURE {
		let start = signature;
		let element_type = helpers::cor_sig_uncompress_element_type(signature);
		use crate::enums::CorElementType;
		match element_type {
			CorElementType::ElementTypeVoid |
			CorElementType::ElementTypeBoolean |
			CorElementType::ElementTypeChar |
			CorElementType::ElementTypeI1 |
			CorElementType::ElementTypeU1 |
			CorElementType::ElementTypeI2 |
			CorElementType::ElementTypeU2 |
			CorElementType::ElementTypeI4 |
			CorElementType::ElementTypeU4 |
			CorElementType::ElementTypeI8 |
			CorElementType::ElementTypeU8 |
			CorElementType::ElementTypeR4 |
			CorElementType::ElementTypeR8 |
			CorElementType::ElementTypeString => start,
			CorElementType::ElementTypeValuetype => {
				helpers::cor_sig_uncompress_token(signature);
				start
			}
			CorElementType::ElementTypeClass => {
				helpers::cor_sig_uncompress_token(signature);
				start
			}
			CorElementType::ElementTypeObject => start,
			CorElementType::ElementTypeSzarray => {
				Signature::consume_type(signature);
				start
			}
			CorElementType::ElementTypeVar => {
				helpers::cor_sig_uncompress_data(signature);
				start
			}
			CorElementType::ElementTypeGenericinst => {
				helpers::cor_sig_uncompress_element_type(signature);
				helpers::cor_sig_uncompress_token(signature);

				let generic_arguments_count = helpers::cor_sig_uncompress_data(signature);
				for i in 0..generic_arguments_count {
					Signature::consume_type(signature);
				}
				start
			}
			CorElementType::ElementTypeByref => {
				Signature::consume_type(signature);
				start
			}
			_ => {
				std::unreachable!()
			}
		}
	}

	pub fn to_string<'a>(metadata: *mut IMetaDataImport2, signature: PCCOR_SIGNATURE) -> Cow<'a, str> {
		let element_type = helpers::cor_sig_uncompress_element_type(signature);
		use crate::enums::CorElementType;
		return match element_type {
			CorElementType::ElementTypeVoid => "Void".into(),
			CorElementType::ElementTypeBoolean => "Boolean".into(),
			CorElementType::ElementTypeChar => "Char16".into(),
			CorElementType::ElementTypeI1 => "Int8".into(),
			CorElementType::ElementTypeU1 => "UInt8".into(),
			CorElementType::ElementTypeI2 => "Int16".into(),
			CorElementType::ElementTypeU2 => "UInt16".into(),
			CorElementType::ElementTypeI4 => "Int32".into(),
			CorElementType::ElementTypeU4 => "UInt32".into(),
			CorElementType::ElementTypeI8 => "Int64".into(),
			CorElementType::ElementTypeU8 => "UInt64".into(),
			CorElementType::ElementTypeR4 => "Single".into(),
			CorElementType::ElementTypeR8 => "Double".into(),
			CorElementType::ElementTypeString => "String".into(),
			CorElementType::ElementTypeValuetype => {
				let token = helpers::cor_sig_uncompress_token(signature);
				let mut class_name_data = [0_u16; MAX_IDENTIFIER_LENGTH];
				let length = helpers::get_type_name(metadata, token, class_name_data.as_mut_ptr(), class_name_data.len() as u32);

				let class_name = OsString::from_wide(&class_name_data[..length as usize]).to_string_lossy().as_ref();
				if class_name.eq("System.Guid") {
					Guid.into()
				} else {
					class_name.into()
				}
			}
			CorElementType::ElementTypeClass => {
				let token = helpers::cor_sig_uncompress_token(signature);
				let mut class_name_data = [0_u16; MAX_IDENTIFIER_LENGTH];
				let length = helpers::get_type_name(metadata, token, class_name_data.as_mut_ptr(), class_name_data.len() as u32);
				OsString::from_wide(&class_name_data[..length as usize]).to_string_lossy()
			}
			CorElementType::ElementTypeObject => "Object".into(),
			CorElementType::ElementTypeSzarray => {
				let result = Signature::to_string(metadata, signature);
				(result.to_owned() + "[]").into()
			}
			CorElementType::ElementTypeVar => {
				let index = helpers::cor_sig_uncompress_data(signature);
				let mut result_data = [0_u16; MAX_IDENTIFIER_LENGTH];
				let length = helpers::to_wstring(index, result_data.as_mut_ptr());
				("Var!".to_owned() + OsString::from_wide(&result_data[..length as usize]).to_string_lossy().as_ref()).into()
			}
			CorElementType::ElementTypeGenericinst => {
				let generic_type = helpers::cor_sig_uncompress_element_type(signature);
				debug_assert!(generic_type == CorElementType::ElementTypeClass);

				let token = helpers::cor_sig_uncompress_token(signature);


				let mut class_name_data = [0_u16; MAX_IDENTIFIER_LENGTH];
				let length = helpers::get_type_name(metadata, token, class_name_data.as_mut_ptr(), class_name_data.len() as u32);

				let mut result = OsString::from_wide(&class_name_data[..length as usize]).to_string_lossy().as_ref().to_owned();

				result += "<";

				let generic_arguments_count = helpers::cor_sig_uncompress_data(signature);
				for i in 0..generic_arguments_count {
					let sig_type = Signature::consume_type(signature);
					let data = Signature::to_string(metadata, sig_type);
					result += data.as_ref();
					if i == generic_arguments_count - 1 {
						result += ", ";
					}
				}

				result += ">";

				result.into()
			}
			CorElementType::ElementTypeByref => {
				let mut result = "ByRef ".to_owned();
				result += Signature::to_string(metadata, signature).as_ref();
				result.into()
			}
			_ => {
				std::unreachable!()
			}
		};
	}
}