use crate::prelude::*;
use crate::bindings::helpers;
use crate::metadata::enums::CorElementType;

pub struct Signature {}

impl Signature {
	pub fn consume_type(signature: PCCOR_SIGNATURE) -> PCCOR_SIGNATURE {
		let start = signature;
		let element_type = helpers::cor_sig_uncompress_element_type(signature);
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
			CorElementType::ElementTypeVar =>{
				helpers::cor_sig_uncompress_data(signature);
				start
			}
			CorElementType::ElementTypeGenericinst =>{
				helpers::cor_sig_uncompress_element_type(signature);
				helpers::cor_sig_uncompress_token(signature);

				let generic_arguments_count = helpers::cor_sig_uncompress_data(signature);
				for i in 0..generic_arguments_count {
					Signature::consume_type(signature)
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

	pub fn to_string<'a>(metadata: *mut c_void, signature: PCCOR_SIGNATURE) -> &'a str {
		let element_type = helpers::cor_sig_uncompress_element_type(signature);
		match element_type {
			CorElementType::ElementTypeVoid => "Void",
			CorElementType::ElementTypeBoolean => "Boolean",
			CorElementType::ElementTypeChar => "Char16",
			CorElementType::ElementTypeI1 => "Int8",
			CorElementType::ElementTypeU1 => "UInt8",
			CorElementType::ElementTypeI2 => "Int16",
			CorElementType::ElementTypeU2 => "UInt16",
			CorElementType::ElementTypeI4 => "Int32",
			CorElementType::ElementTypeU4 => "UInt32",
			CorElementType::ElementTypeI8 => "Int64",
			CorElementType::ElementTypeU8 => "UInt64",
			CorElementType::ElementTypeR4 => "Single",
			CorElementType::ElementTypeR8 => "Double",
			CorElementType::ElementTypeString => "String",
			CorElementType::ElementTypeValuetype => {
				let token = helpers::cor_sig_uncompress_token(signature);
				let mut class_name_data = vec![0_u16; MAX_IDENTIFIER_LENGTH];
				let length = helpers::get_type_name(metadata, token, class_name_data.as_mut_ptr(), class_name_data.len() as u32);
				class_name_data.resize(length as usize, 0);
				let class_name = OsString::from_wide(name.as_slice()).to_string_lossy().as_ref();
				if class_name.eq("System.Guid") {
					"Guid"
				}else {
					class_name
				}
			}
			CorElementType::ElementTypeClass => {
				let token = helpers::cor_sig_uncompress_token(signature);
				let mut class_name_data = vec![0_u16; MAX_IDENTIFIER_LENGTH];
				let length = helpers::get_type_name(metadata, token, class_name_data.as_mut_ptr(), class_name_data.len() as u32);
				class_name_data.resize(length as usize, 0);
				OsString::from_wide(name.as_slice()).to_string_lossy().as_ref()
			}
			CorElementType::ElementTypeObject => "Object",
			CorElementType::ElementTypeSzarray => {
				let result = Signature::to_string(metadata, signature);
				result.to_owned() + "[]"
			}
			CorElementType::ElementTypeVar =>{
				let index = helpers::cor_sig_uncompress_data(signature);
				let mut result_data = vec![0_u16; MAX_IDENTIFIER_LENGTH];
				let length = helpers::to_wstring(index, result_data.as_mut_ptr());
				result_data.resize(length, 0);
				"Var!".to_owned() + OsString::from_wide(result_data.as_slice()).to_string_lossy().as_ref()
			}
			CorElementType::ElementTypeGenericinst => {
				let generic_type = helpers::cor_sig_uncompress_element_type(signature);
				assert(generic_type == CorElementType::ElementTypeClass);

				let token = helpers::cor_sig_uncompress_token(signature);


				let mut class_name_data = vec![0_u16; MAX_IDENTIFIER_LENGTH];
				let length = helpers::get_type_name(metadata, token, class_name_data.as_mut_ptr(), class_name_data.len() as u32);
				class_name_data.resize(length as usize, 0);


				let mut result = OsString::from_wide(name.as_slice()).to_string_lossy().as_ref().to_owned();

				result += "<";

				let generic_arguments_count = helpers::cor_sig_uncompress_data(signature);
				for i in 0..generic_arguments_count {
					let sig_type = Signature::consume_type(signature);
					let data = Signature::to_string(metadata, sig_type);
					result += data;
					if i == generic_arguments_count - 1 {
						result += ", ";
					}
				}

				result += ">";

				result
			}
			CorElementType::ElementTypeByref => {
				let mut result = "ByRef ".to_owned();
				result += Signature::to_string(metadata, signature);
				result
			}
			_ => {
				std::unreachable!()
			}
		}
	}
}