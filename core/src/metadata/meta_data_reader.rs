use std::sync::Arc;

use windows::HSTRING;

use crate::bindings::{enums, helpers, imeta_data_import2, rometadataresolution};
use crate::metadata::declarations::class_declaration::ClassDeclaration;
use crate::metadata::declarations::declaration::Declaration;
use crate::metadata::declarations::delegate_declaration::DelegateDeclaration;
use crate::metadata::declarations::delegate_declaration::generic_delegate_declaration::GenericDelegateDeclaration;
use crate::metadata::declarations::enum_declaration::EnumDeclaration;
use crate::metadata::declarations::interface_declaration::generic_interface_declaration::GenericInterfaceDeclaration;
use crate::metadata::declarations::interface_declaration::interface_declaration::InterfaceDeclaration;
use crate::metadata::declarations::namespace_declaration::NamespaceDeclaration;
use crate::metadata::declarations::struct_declaration::StructDeclaration;
use crate::prelude::*;

const WINDOWS: &str = "Windows";
const SYSTEM_ENUM: &str = "System.Enum";
const SYSTEM_VALUETYPE: &str = "System.ValueType";
const SYSTEM_MULTICASTDELEGATE: &str = "System.MulticastDelegate";

#[derive(Debug)]
pub struct MetadataReader {}

impl MetadataReader {
	pub fn find_by_name_w(full_name: PCWSTR) -> Option<Arc<dyn Declaration>> {
		let mut count = 0;
		helpers::to_string_length(full_name, &mut count);
		let slice = unsafe { std::slice::from_raw_parts(full_name, count) };
		let name = OsString::from_wide(slice).to_string_lossy();
		MetadataReader::find_by_name(name.as_ref())
	}
	pub fn find_by_name(full_name: &str) -> Option<Arc<dyn Declaration>> {
		if full_name.is_empty() {
			return Some(
				Arc::new(
					NamespaceDeclaration::new("")
				)
			);
		}
		let mut metadata = std::ptr::null_mut();
		let meta = &mut metadata;
		let mut token = mdTokenNil;
		let mut full_name_hstring = HSTRING::from(full_name);
		let get_metadata_file_result = rometadataresolution::ro_get_meta_data_file(
			&mut full_name_hstring, None, None, Some(meta), Some(&mut token),
		);

		if get_metadata_file_result.is_err() {
			if get_metadata_file_result == windows::HRESULT::from_win32(RO_E_METADATA_NAME_IS_NAMESPACE as u32) {
				return Some(
					Arc::new(NamespaceDeclaration::new(full_name))
				);
			}
			return None;
		}

		let mut flags = 0;
		let mut parent_token = mdTokenNil;
		debug_assert!(imeta_data_import2::get_type_def_props(
			metadata, token, None, None, None, Some(&mut flags), Some(&mut parent_token),
		).is_ok());


		if helpers::is_td_class(flags) {
			let mut parent_name = vec![0_u16; MAX_IDENTIFIER_LENGTH];


			match CorTokenType::from(enums::type_from_token(parent_token)) {
				CorTokenType::mdtTypeDef => {
					debug_assert!(
						imeta_data_import2::get_type_def_props(
							metadata, parent_token, Some(parent_name.as_mut_ptr()), Some(parent_name.len() as u32),
							None, None, None,
						).is_ok()
					);
				}
				CorTokenType::mdtTypeRef => {
					debug_assert!(
						imeta_data_import2::get_type_ref_props(
							metadata, parent_token, None, Some(parent_name.as_mut_ptr()), Some(parent_name.len() as u32), None,
						).is_ok()
					)
				}
				_ => {
					std::unreachable!()
				}
			}

			let mut parent_name_string = OsString::from_wide(parent_name.as_slice()).to_string_lossy().as_ref();

			if parent_name_string == SYSTEM_ENUM {
				return Some(
					Arc::new(
						EnumDeclaration::new(metadata, token)
					)
				);
			}

			if parent_name_string == SYSTEM_VALUETYPE {
				return Some(
					Arc::new(
						StructDeclaration::new(metadata, token)
					)
				);
			}

			if parent_name_string == SYSTEM_MULTICASTDELEGATE {
				return if full_name.contains("`") {
					Some(
						Arc::new(
							GenericDelegateDeclaration::new(metadata, token)
						)
					)
				} else {
					Some(
						Arc::new(
							DelegateDeclaration::new(metadata, token)
						)
					)
				}
			}

			return Some(
				Arc::new(
					ClassDeclaration::new(metadata, token)
				)
			);
		}

		if helpers::is_td_interface(flags) {
			return if full_name.contains("`") {
				Some(
					Arc::new(
						GenericInterfaceDeclaration::new(metadata, token)
					)
				)
			} else {
				Some(
					Arc::new(
						InterfaceDeclaration::new(metadata, token)
					)
				)
			};
		}

		std::unreachable!();
	}
}