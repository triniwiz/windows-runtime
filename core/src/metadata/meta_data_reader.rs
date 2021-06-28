use std::sync::{Arc, RwLock};

use windows::HSTRING;

use crate::bindings::{enums, helpers, imeta_data_import2, rometadataresolution};
use crate::metadata::declarations::class_declaration::ClassDeclaration;
use crate::metadata::declarations::declaration::Declaration;
use crate::metadata::declarations::delegate_declaration::DelegateDeclaration;
use crate::metadata::declarations::delegate_declaration::generic_delegate_declaration::GenericDelegateDeclaration;
use crate::metadata::declarations::enum_declaration::EnumDeclaration;
use crate::metadata::declarations::interface_declaration::generic_interface_declaration::GenericInterfaceDeclaration;
use crate::metadata::declarations::namespace_declaration::NamespaceDeclaration;
use crate::metadata::declarations::struct_declaration::StructDeclaration;
use crate::prelude::*;
use crate::metadata::declarations::interface_declaration::InterfaceDeclaration;

const WINDOWS: &str = "Windows";
const SYSTEM_ENUM: &str = "System.Enum";
const SYSTEM_VALUETYPE: &str = "System.ValueType";
const SYSTEM_MULTICASTDELEGATE: &str = "System.MulticastDelegate";

#[derive(Debug)]
pub struct MetadataReader {}

impl MetadataReader {
	pub fn find_by_name_w(full_name: PCWSTR) -> Option<Arc<RwLock<dyn Declaration>>> {
		let mut count = 0;
		helpers::to_string_length(full_name, &mut count);
		let slice = unsafe { std::slice::from_raw_parts(full_name, count) };
		let name = OsString::from_wide(slice).to_string_lossy();
		MetadataReader::find_by_name(name.as_ref())
	}
	pub fn find_by_name(full_name: &str) -> Option<Arc<RwLock<dyn Declaration>>> {
		if full_name.is_empty() {
			return Some(
				Arc::new(
					RwLock::new(NamespaceDeclaration::new(""))
				)
			);
		}
		let mut metadata = unsafe { std::mem::MaybeUninit::uninit() };
		let meta = &mut metadata.as_mut_ptr() as *mut *mut c_void;
		let mut token = mdTokenNil;
		let mut full_name_hstring = HSTRING::from(full_name);
		let get_metadata_file_result = rometadataresolution::ro_get_meta_data_file(
			&mut full_name_hstring, None, None, Some(meta), Some(&mut token),
		);

		if get_metadata_file_result.is_err() {
			if get_metadata_file_result == windows::HRESULT::from_win32(RO_E_METADATA_NAME_IS_NAMESPACE as u32) {
				return Some(
					Arc::new(
						RwLock::new(NamespaceDeclaration::new(full_name))
					)
				);
			}
			return None;
		}

		let mut metadata = unsafe { metadata.assume_init() };

		let mut flags = 0;
		let mut parent_token = mdTokenNil;
		{
			let result = imeta_data_import2::get_type_def_props(
				&mut metadata, token, None, None, None, Some(&mut flags), Some(&mut parent_token),
			);
			debug_assert!(result.is_ok());
		}


		if helpers::is_td_class(flags) {
			let mut parent_name = [0_u16; MAX_IDENTIFIER_LENGTH];

			match CorTokenType::from(enums::type_from_token(parent_token)) {
				CorTokenType::mdtTypeDef => {
					let result = imeta_data_import2::get_type_def_props(
						&mut metadata, parent_token, Some(&mut parent_name), Some(parent_name.len() as u32),
						None, None, None,
					);
					debug_assert!(
						result.is_ok()
					);
				}
				CorTokenType::mdtTypeRef => {
					let result = imeta_data_import2::get_type_ref_props(
						&mut metadata, parent_token, None, Some(&mut parent_name), Some(parent_name.len() as u32), None,
					);
					debug_assert!(
						result.is_ok()
					);
				}
				_ => {
					core_unreachable()
				}
			}

			let mut parent_name_string = OsString::from_wide(&parent_name).to_string_lossy().as_ref();
			let metadata = unsafe {
				Arc::new(
					RwLock::new(
						metadata
					)
				)
			};
			if parent_name_string == SYSTEM_ENUM {
				return Some(
					Arc::new(
						RwLock::new(EnumDeclaration::new(Some(metadata), token))
					)
				);
			}

			if parent_name_string == SYSTEM_VALUETYPE {
				return Some(
					Arc::new(
						RwLock::new(StructDeclaration::new(Some(metadata), token))
					)
				);
			}

			if parent_name_string == SYSTEM_MULTICASTDELEGATE {
				return if full_name.contains("`") {
					Some(
						Arc::new(
							RwLock::new(GenericDelegateDeclaration::new(Some(metadata), token))
						)
					)
				} else {
					Some(
						Arc::new(
							RwLock::new(DelegateDeclaration::new(Some(metadata), token))
						)
					)
				}
			}

			return Some(
				Arc::new(
					RwLock::new(ClassDeclaration::new(Some(metadata), token))
				)
			);
		}

		if helpers::is_td_interface(flags) {
			let metadata = unsafe {
				Arc::new(
					RwLock::new(
						metadata
					)
				)
			};
			return if full_name.contains("`") {
				Some(
					Arc::new(
						RwLock::new(GenericInterfaceDeclaration::new(metadata, token))
					)
				)
			} else {
				Some(
					Arc::new(
						Mutex::new(InterfaceDeclaration::new(metadata, token))
					)
				)
			};
		}

		std::unreachable!();
	}
}