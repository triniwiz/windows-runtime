use std::sync::{Arc, Mutex, RwLock};

use crate::bindings::{enums, helpers};
use crate::metadata::com_helpers::resolve_type_ref;
use crate::metadata::declarations::base_class_declaration::BaseClassDeclarationImpl;

use crate::metadata::declarations::delegate_declaration::{DelegateDeclaration, DelegateDeclarationImpl};
use crate::metadata::declarations::delegate_declaration::generic_delegate_instance_declaration::GenericDelegateInstanceDeclaration;
use crate::metadata::declarations::interface_declaration::generic_interface_instance_declaration::GenericInterfaceInstanceDeclaration;
use crate::prelude::*;
use crate::metadata::declarations::interface_declaration::InterfaceDeclaration;

#[derive(Clone, Debug)]
pub struct DeclarationFactory {}

impl DeclarationFactory {
	pub fn make_delegate_declaration(metadata: Option<Arc<RwLock<IMetaDataImport2>>>, token: mdToken) -> Option<Arc<RwLock<dyn DelegateDeclarationImpl>>> {
		match CorTokenType::from(enums::type_from_token(token))
		{
			CorTokenType::mdtTypeDef => Box::new(DelegateDeclaration::new(metadata, token)),
			CorTokenType::mdtTypeRef => {
				match get_lock_value(&metadata) {
					None => {}
					Some(metadata) => {
						let mut external_metadata = std::mem::MaybeUninit::uninit();
						let external_metadata_ptr = &mut external_metadata.as_mut_ptr();
						let mut external_delegate_token = mdTokenNil;
						let is_resolved = resolve_type_ref(Some(metadata), token, external_metadata_ptr, &mut external_delegate_token);
						debug_assert!(is_resolved);
						let external_metadata = unsafe { external_metadata.assume_init() };
						Some(
							Arc::new(
								RwLock::new(
									external_metadata
								)
							)
						)
					}
				}
			}
			CorTokenType::mdtTypeSpec => {
				let mut signature = [0_u8; MAX_IDENTIFIER_LENGTH];
				let mut signature_ptr = signature.as_ptr();
				let mut signature_size = 0;
				debug_assert!(
					imeta_data_import2::get_type_spec_from_token(
						metadata, token, &mut signature_ptr, &mut signature_size,
					).is_ok()
				);

				let type1 = helpers::cor_sig_uncompress_element_type(signature.as_ptr());
				debug_assert!(
					type1 == crate::enums::CorElementType::ElementTypeGenericinst
				);

				let type2 = helpers::cor_sig_uncompress_element_type(signature.as_ptr());
				debug_assert!(
					type2 == crate::enums::CorElementType::ElementTypeClass
				);

				let open_generic_delegate_token = helpers::cor_sig_uncompress_token(signature.as_ptr());
				match CorTokenType::from(enums::type_from_token(open_generic_delegate_token)) {
					CorTokenType::mdtTypeDef => {
						Box::new(GenericDelegateInstanceDeclaration::new(metadata, open_generic_delegate_token, metadata, token))
					}
					CorTokenType::mdtTypeRef => {
						let mut external_metadata: std::mem::MaybeUninit<IMetaDataImport2> = std::mem::MaybeUninit::uninit();
						let external_metadata_ptr = &mut external_metadata.as_mut_ptr();
						let mut external_delegate_token = mdTokenNil;

						let is_resolved = resolve_type_ref(
							metadata, open_generic_delegate_token, external_metadata_ptr, &mut external_delegate_token,
						);

						debug_assert!(is_resolved);
						Box::new(GenericDelegateInstanceDeclaration::new(external_metadata, external_delegate_token, metadata, token))
					}
					_ => {
						std::unreachable!()
					}
				}
			}
			_ => {
				std::unreachable!()
			}
		}
	}
	pub fn make_interface_declaration(metadata: Option<Arc<RwLock<IMetaDataImport2>>>, token: mdToken) -> Option<Arc<RwLock<dyn BaseClassDeclarationImpl>>> {
		match CorTokenType::from(enums::type_from_token(token)) {
			CorTokenType::mdtTypeDef => {
				Some(
					Arc::new(
						RwLock::new(InterfaceDeclaration::new(metadata, token))
					)
				)
			}
			CorTokenType::mdtTypeRef => {
				let mut external_metadata = std::mem::MaybeUninit::uninit();
				let external_metadata_ptr = &mut external_metadata;
				let mut external_interface_token = mdTokenNil;

				let is_resolved = resolve_type_ref(metadata, token, external_metadata_ptr, &mut external_interface_token);

				debug_assert!(is_resolved);

				Arc::new(
					Mutex::new(InterfaceDeclaration::new(
						external_metadata, external_interface_token,
					))
				)
			}
			CorTokenType::mdtTypeSpec => {
				let mut signature = [0_u8; MAX_IDENTIFIER_LENGTH];
				let mut signature_ptr = &mut signature.as_mut_ptr();
				let mut signature_size = 0;
				debug_assert!(
					imeta_data_import2::get_type_spec_from_token(
						metadata, token, signature_ptr as *mut _ as *mut *const u8, &mut signature_size,
					).is_ok()
				);

				let type1 = helpers::cor_sig_uncompress_element_type(signature.as_ptr());
				debug_assert!(
					type1 == crate::enums::CorElementType::ElementTypeGenericinst
				);

				let type2 = helpers::cor_sig_uncompress_element_type(signature.as_ptr());

				debug_assert!(
					type2 == crate::enums::CorElementType::ElementTypeClass
				);
				let open_generic_delegate_token = helpers::cor_sig_uncompress_token(signature.as_ptr());

				match CorTokenType::from(enums::type_from_token(token)) {
					CorTokenType::mdtTypeSpec => match CorTokenType::from(enums::type_from_token(open_generic_delegate_token)) {
						CorTokenType::mdtTypeDef => {
							Arc::new(
								Mutex::new(GenericInterfaceInstanceDeclaration::new(
									metadata, open_generic_delegate_token, metadata, token,
								))
							)
						}
						CorTokenType::mdtTypeRef => {
							let mut external_metadata = std::mem::MaybeUninit::uninit();
							let external_metadata_ptr = &mut external_metadata;
							let mut external_delegate_token = mdTokenNil;

							let is_resolved = resolve_type_ref(
								metadata, open_generic_delegate_token, external_metadata_ptr, &mut external_delegate_token,
							);

							debug_assert!(
								is_resolved
							);

							Arc::new(
								Mutex::new(
									GenericInterfaceInstanceDeclaration::new(
										external_metadata, external_delegate_token, metadata, token,
									)
								)
							)
						}
						_ => {
							std::unreachable!()
						}
					}
					_ => {
						std::unreachable!()
					}
				}
			}
			_ => {
				std::unreachable!()
			}
		}
	}
}