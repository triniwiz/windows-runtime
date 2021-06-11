use crate::prelude::*;
use crate::bindings::{enums, imeta_data_import2, helpers};
use std::borrow::Cow;
use crate::metadata::com_helpers::{get_string_value_from_blob, get_type_name, SYSTEM_TYPE, COMPOSABLE_ATTRIBUTE, ACTIVATABLE_ATTRIBUTE, STATIC_ATTRIBUTE, resolve_type_ref};
use crate::metadata::declarations::interface_declaration::InterfaceDeclaration;
use crate::metadata::declarations::interface_declaration::generic_interface_instance_declaration::GenericInterfaceInstanceDeclaration;
use crate::metadata::declarations::base_class_declaration::BaseClassDeclarationImpl;
use std::sync::{Arc, Mutex};
use crate::metadata::declarations::method_declaration::MethodDeclaration;

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

	pub fn get_method_signature<'a>(metadata: *mut IMetaDataImport2, token: mdMethodDef) -> Cow<'a, [u8]> {
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

	pub fn get_signature_argument_count(metadata: *mut IMetaDataImport2, signature: Cow<[u8]>) -> u32 {
		helpers::cor_sig_uncompress_data(signature.as_ptr())
	}

	pub fn get_method_argument_count(metadata: *mut IMetaDataImport2, token: mdToken) -> u32 {
		let signature = DeclaringInterfaceForMethod::get_method_signature(metadata, token);
		DeclaringInterfaceForMethod::get_signature_argument_count(metadata, signature)
	}


	pub fn get_custom_attributes_with_name(metadata: *mut IMetaDataImport2, token: mdTypeDef, attribute_name: &str) -> Vec<u32> {
		// mdCustomAttribute
		let mut attributes = [0; 512];
		let mut attributes_count = 0;
		let mut attributes_enumerator = std::ptr::null_mut();
		let attributes_enumerator_ptr = &mut attributes_enumerator;

		debug_assert!(
			imeta_data_import2::enum_custom_attributes(
				metadata, Some(attributes_enumerator_ptr), Some(token), None,
				Some(attributes.as_mut_ptr()), Some(attributes.len() as u32),
				Some(&mut attributes_count),
			).is_ok()
		);
		debug_assert!(
			attributes_count < (attributes.len() - 1) as u32
		);
		imeta_data_import2::close_enum(
			metadata, attributes_enumerator,
		);


		let mut filtered_attributes: Vec<u32> = Vec::new();
		let new_attributes: [u32] = attributes[..attributes_count];
		for attribute in new_attributes.iter() {
			let class_attribute_class_token = DeclaringInterfaceForMethod::get_custom_attribute_class_token(metadata, attribute);
			let mut name = [0_u16; MAX_IDENTIFIER_LENGTH];
			let length = helpers::get_type_name(metadata, class_attribute_class_token, name.as_mut_ptr(), name.len() as u32);
			let mut class_attribute_class_name = windows::HSTRING::from_wide(name[..length]);
			// TODO
			if class_attribute_class_name != attribute_name {
				continue;
			}

			filtered_attributes.push(
				attribute
			);

			filtered_attributes.push_back(attribute);
		}
		filtered_attributes
	}

	pub fn get_custom_attribute_type_argument(metadata: *mut IMetaDataImport2, token: mdCustomAttribute) -> u32 {
		let mut attribute_value = std::mem::MaybeUninit::uninit();
		let attribute_value_ptr = &mut attribute_value;
		let mut attribute_value_size = 0;
		debug_assert!(imeta_data_import2::get_custom_attribute_props(
			metadata, token,
			None, None, Some(attribute_value_ptr as *mut *const u8), Some(&mut attribute_value_size),
		).is_ok());

		let type_name = get_string_value_from_blob(
			metadata, unsafe { attribute_value.as_ptr().offset(2) },
		);

		let type_name_data = windows::HSTRING::from(type_name);
		let type_name_data = type_name_data.as_wide();

		let mut type_token = mdTokenNil;
		debug_assert!(imeta_data_import2::find_type_def_by_name(
			metadata, Some(type_name_data.as_ptr()), None, Some(&mut type_token),
		).is_ok());
		return type_token;
	}


	pub fn get_class_methods<'a>(metadata: *mut IMetaDataImport2, token: mdTypeDef) -> Cow<'a, [u32]> {
		let mut methods = [0; 1024];
		let mut methods_count = 0;
		let mut methods_enumerator = std::ptr::null_mut();
		let methods_enumerator_ptr = &mut methods_enumerator;
		debug_assert!(
			imeta_data_import2::enum_methods(
				metadata, methods_enumerator_ptr, token, methods.as_mut_ptr(),
				methods.len() as u32, &mut methods_count,
			).is_ok()
		);
		debug_assert!(
			methods_count < (methods.len() - 1) as u32
		);
		imeta_data_import2::close_enum(
			metadata, methods_enumerator,
		);

		methods[..methods_count]
	}


	pub fn has_method_first_type_argument(metadata: *mut IMetaDataImport2, token: mdToken) -> bool {
		let mut signature = DeclaringInterfaceForMethod::get_method_signature(
			metadata, token,
		);
		let argument_count = helpers::cor_sig_uncompress_data(signature.as_ptr());

		if argument_count == 0 {
			return false;
		}

		let return_type = helpers::cor_sig_uncompress_element_type(signature.as_ptr());

		debug_assert!(
			return_type == crate::enums::CorElementType::ElementTypeVoid
		);

		let first_argument = helpers::cor_sig_uncompress_element_type(signature.as_ptr());


		if first_argument == crate::enums::CorElementType::ElementTypeClass {
			return false;
		}

		let first_argument_token = helpers::cor_sig_uncompress_token(signature.as_ptr());
		let first_argument_type_name = get_type_name(
			metadata, first_argument_token,
		);

		if first_argument_type_name != SYSTEM_TYPE {
			return false;
		}

		return true;
	}


	pub fn declaring_interface_for_initializer<'a>(metadata: *mut IMetaDataImport2, method_token: mdMethodDef, out_index: &mut usize) -> Option<Arc<Mutex<dyn BaseClassDeclarationImpl>>> {
		// InterfaceDeclaration
		let method_argument_count = DeclaringInterfaceForMethod::get_method_argument_count(metadata, method_token);

		let class_token = DeclaringInterfaceForMethod::get_method_containing_class_token(metadata, method_token);

		debug_assert!(
			CorTokenType::from(enums::type_from_token(class_token)) == crate::enums::CorTokenType::mdtTypeDef
		);

		let composable_attributes = DeclaringInterfaceForMethod::get_custom_attributes_with_name(
			metadata, class_token, COMPOSABLE_ATTRIBUTE,
		);

		for attributeToken in composable_attributes.iter() {
			let factory_token = DeclaringInterfaceForMethod::get_custom_attribute_type_argument(
				metadata, attributeToken,
			);

			let factory_methods = DeclaringInterfaceForMethod::get_class_methods(
				metadata, attributeToken,
			);

			for (i, factoryMethod) in factory_methods.iter().enumerate() {
				let factory_method_arguments_count = DeclaringInterfaceForMethod::get_method_argument_count(metadata, factoryMethod);
				if factory_method_arguments_count - 2 != method_argument_count {
					continue;
				}

				*out_index = i;
				return Some(
					Arc::new(
						Mutex::new(
							InterfaceDeclaration::new(metadata, factory_token)
						)
					)
				);
			}
		}

		if method_argument_count == 0 {
			*out_index = usize::MAX;
			return None;
		}


		let mut activatable_attributes = DeclaringInterfaceForMethod::get_custom_attributes_with_name(
			metadata, class_token, ACTIVATABLE_ATTRIBUTE,
		);
		for attributeToken in activatable_attributes.iter() {
			let attribute_constructor_token = DeclaringInterfaceForMethod::get_custom_attribute_constructor_token(
				metadata, attributeToken,
			);

			if !DeclaringInterfaceForMethod::has_method_first_type_argument(metadata, attribute_constructor_token) {
				continue;
			}

			let factory_token = DeclaringInterfaceForMethod::get_custom_attribute_type_argument(metadata, attributeToken);


			let factory_methods = DeclaringInterfaceForMethod::get_class_methods(
				metadata, factory_token,
			);

			for (i, factory_method) in factory_methods.iter().enumerate() {
				let factory_method_arguments_count = DeclaringInterfaceForMethod::get_method_argument_count(
					metadata, factory_method,
				);

				if factory_method_arguments_count != method_argument_count {
					continue;
				}

				*out_index = i;
				return Some(
					Arc::new(
						Mutex::new(
							InterfaceDeclaration::new(metadata, factory_token)
						)
					)
				);
			}
		}

		// return Option ?
		std::unreachable!();
	}


	pub fn declaring_interface_for_static_method<'a>(metadata: *mut IMetaDataImport2, method_token: mdMethodDef, out_index: &mut usize) -> Option<Arc<Mutex<dyn BaseClassDeclarationImpl>>> {
		let method_signature = DeclaringInterfaceForMethod::get_method_signature(
			metadata, method_token,
		);
		let class_token = DeclaringInterfaceForMethod::get_method_containing_class_token(
			metadata, method_token,
		);
		debug_assert!(
			CorTokenType::from(enums::type_from_token(class_token)) == CorTokenType::mdtTypeDef
		);

		let static_attributes = DeclaringInterfaceForMethod::get_custom_attributes_with_name(
			metadata, class_token, STATIC_ATTRIBUTE,
		);

		for attributeToken in static_attributes.iter() {
			let statics_token = DeclaringInterfaceForMethod::get_custom_attribute_type_argument(
				metadata, attributeToken,
			);

			let static_methods = DeclaringInterfaceForMethod::get_class_methods(
				metadata, statics_token,
			);

			for (i, staticMethod) in static_methods.iter().enumerate() {
				let mut static_signature = [0_u8; MAX_IDENTIFIER_LENGTH];
				let static_signature_ptr = &mut static_signature.as_mut_ptr();
				let mut static_signature_size = 0;
				debug_assert!(imeta_data_import2::get_method_props(
					metadata, staticMethod,
					None, None, None, None, None,
					Some(static_signature_ptr as *mut *const u8), Some(&mut static_signature_size),
					None, None,
				));
				let static_signature: &[u8] = &static_signature[..static_signature_size as usize];

				unsafe {
					if libc::memcmp(
						static_signature.as_ptr().offset(1) as _,
						method_signature.as_ptr() as _,
						static_signature_size as usize,
					) != 0 {
						continue;
					}
				}
				*out_index = i;
				return Some(Arc::new(Mutex::new(InterfaceDeclaration::new(metadata, statics_token))));
			}
		}

		std::unreachable!();
	}


	pub fn find_method_index(metadata: *mut IMetaDataImport2, class_token: mdTypeDef, method_token: mdMethodDef) -> usize {
		let mut first_method = 0;

		let mut methods_enumerator = std::ptr::null_mut();
		let methods_enumerator_ptr = &mut methods_enumerator;
		debug_assert!(
			imeta_data_import2::enum_methods(
				metadata, methods_enumerator_ptr, class_token, &mut first_method, 1, 0 as _,
			).is_ok()
		);

		imeta_data_import2::close_enum(metadata, methods_enumerator);


		return (method_token - first_method) as usize;
	}


	pub fn declaring_interface_for_instance_method(metadata: *mut IMetaDataImport2, method_token: mdMethodDef, out_index: &mut usize) -> Option<Arc<Mutex<dyn BaseClassDeclarationImpl>>> {
		let class_token = DeclaringInterfaceForMethod::get_method_containing_class_token(
			metadata, method_token,
		);
		debug_assert!(
			CorTokenType::from(enums::type_from_token(class_token)) == CorTokenType::mdtTypeDef
		);
		let mut method_body_tokens = [0; 1024];
		let mut method_decl_tokens = [0; 1024];
		let mut method_impls_count = 0;
		let mut method_impls_enumerator = std::ptr::null_mut();
		let method_impls_enumerator_ptr = &mut method_impls_enumerator;
		debug_assert!(
			imeta_data_import2::enum_method_impls(
				metadata, method_impls_enumerator_ptr, Some(class_token),
				Some(&mut method_body_tokens),
				Some(&mut method_decl_tokens),
				Some(method_body_tokens.len() as u32),
				Some(&mut method_impls_count),
			).is_ok()
		);

		debug_assert!(
			method_impls_count < (method_body_tokens.len() - 1) as u32
		);

		imeta_data_import2::close_enum(metadata, method_impls_enumerator);

		let mut result: Option<Arc<Mutex<dyn BaseClassDeclarationImpl>>> = None;
		for i in 0..method_impls_count {
			let method_body_token = method_body_tokens[i];
			debug_assert!(
				CorTokenType::from(enums::type_from_token(method_body_token)) == CorTokenType::mdtMethodDef
			);

			if method_token != method_body_token {
				continue;
			}

			let method_decl_token = method_decl_tokens[i];
			match CorTokenType::from(enums::type_from_token(method_decl_token)) {
				CorTokenType::mdtMethodDef => {
					let mut declaring_interface_token = mdTokenNil;
					debug_assert!(
						imeta_data_import2::get_method_props(
							metadata, method_decl_token,
							Some(&mut declaring_interface_token), None,
							None, None, None, None,
							None, None, None,
						).is_ok()
					);
					*out_index = DeclaringInterfaceForMethod::find_method_index(
						metadata, declaring_interface_token, method_decl_token,
					);
					result = Some(
						Arc::new(
							Mutex::new(
								InterfaceDeclaration::new(metadata, declaring_interface_token)
							)
						)
					);
				}
				CorTokenType::mdtMemberRef => {
					let mut parent_token = 0;
					debug_assert!(
						imeta_data_import2::get_member_ref_props(
							metadata, method_decl_token, Some(&mut parent_token), None,
							None, None, None, None,
						).is_ok()
					);

					match CorTokenType::from(enums::type_from_token(parent_token)) {
						CorTokenType::mdtTypeRef => {
							let mut external_metadata = std::ptr::null_mut();
							let external_metadata_ptr = &mut external_metadata;
							let mut declaring_interface_token = 0;
							let is_resolved = resolve_type_ref(
								metadata, parent_token, external_metadata_ptr, &mut declaring_interface_token,
							);
							debug_assert!(
								is_resolved
							);

							let mut declaring_method_name = [0_u16; MAX_IDENTIFIER_LENGTH];
							let mut signature = [0_u8; MAX_IDENTIFIER_LENGTH];
							let signature_ptr = &mut signature.as_mut_ptr();
							let mut signature_size = 0;
							debug_assert!(
								imeta_data_import2::get_member_ref_props(
									metadata, method_decl_token, None,
									Some(declaring_method_name.as_mut_ptr()),
									Some(declaring_method_name.len() as u32),
									None, Some(signature_ptr as *mut *const u8), Some(&mut signature_size),
								)
							);
							let mut declaring_method = 0;
							debug_assert!(
								imeta_data_import2::find_method(
									external_metadata as _, declaring_interface_token, declaring_method_name.as_mut_ptr(), Some(signature.as_ptr()), Some(signature_size), Some(&mut declaring_method),
								).is_ok()
							);
							*out_index = DeclaringInterfaceForMethod::find_method_index(
								external_metadata as _, declaring_interface_token, declaring_method,
							);
							result = Some(
								Arc::new(
									Mutex::new(
										InterfaceDeclaration::new(external_metadata as _, declaring_interface_token)
									)
								)
							);
						}
						CorTokenType::mdtTypeSpec => {
							let mut type_spec_signature = std::ptr::null_mut();
							let type_spec_signature_ptr = &mut type_spec_signature;
							let mut type_spec_signature_size = 0;
							debug_assert!(
								imeta_data_import2::get_type_spec_from_token(
									metadata, parent_token, type_spec_signature_ptr as *mut *const u8, &mut type_spec_signature_size,
								).is_ok()
							);


							let mut declaring_method_name = [0_u16; MAX_IDENTIFIER_LENGTH];
							let mut signature = std::ptr::null_mut();
							let signature_ptr = &mut signature;
							let mut signature_size = 0;
							debug_assert!(
								imeta_data_import2::get_member_ref_props(
									metadata, method_decl_token, None, Some(
										declaring_method_name.as_mut_ptr()
									),
									Some(declaring_method_name.len() as u32),
									None, Some(signature_ptr as *mut *const u8), Some(&mut signature_size),
								).is_ok()
							);
							let type1 = helpers::cor_sig_uncompress_element_type(type_spec_signature.as_ptr());
							debug_assert!(
								type1 == crate::enums::CorElementType::ElementTypeGenericinst
							);

							let type2 = helpers::cor_sig_uncompress_element_type(type_spec_signature.as_ptr());
							debug_assert!(
								type2 == crate::enums::CorElementType::ElementTypeClass
							);

							// TODO: Use signature in matching
							let open_generic_class_token = helpers::cor_sig_uncompress_token(type_spec_signature.as_ptr());
							match CorTokenType::from(enums::type_from_token(open_generic_class_token)) {
								CorTokenType::mdtTypeDef => {
									let mut declaring_method = 0;
									debug_assert!(imeta_data_import2::find_method(
										metadata, open_generic_class_token, declaring_method_name.as_ptr(), None, None, Some(&mut declaring_method),
									).is_ok());
									*out_index = DeclaringInterfaceForMethod::find_method_index(metadata, open_generic_class_token, 0);

									result = Some(
										Arc::new(
											Mutex::new(
												GenericInterfaceInstanceDeclaration::new(
													metadata as _, open_generic_class_token, metadata as _, parent_token,
												)
											)
										)
									);
								}
								CorTokenType::mdtTypeRef => {
									let mut external_metadata = std::ptr::null_mut();
									let mut external_metadata_ptr = &mut external_metadata;
									let mut external_class_token = mdTokenNil;

									let is_resolved = resolve_type_ref(
										metadata, open_generic_class_token, external_metadata_ptr, &mut external_class_token,
									);
									debug_assert!(
										is_resolved
									);

									let mut declaring_method = 0;
									debug_assert!(
										imeta_data_import2::find_method(
											external_metadata as _, external_class_token, declaring_method_name.as_ptr(), None, None, Some(&mut declaring_method),
										).is_ok()
									);

									*out_index = DeclaringInterfaceForMethod::find_method_index(external_metadata as _, external_class_token, declaring_method);
									result = Some(
										Arc::new(
											Mutex::new(
												GenericInterfaceInstanceDeclaration::new(external_metadata, external_class_token, metadata as _, parent_token)
											)
										)
									);
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
				_ => {
					std::unreachable!();
				}
			}
		};
		result
	}

	pub fn find_declaring_interface_for_method(method: &MethodDeclaration, out_index: &mut usize) -> Option<Arc<Mutex<dyn BaseClassDeclarationImpl>>> {
		debug_assert!(
			out_index
		);

		let metadata = method.base().metadata;
		let method_token = method.base().token;

		if method.is_static() {
			DeclaringInterfaceForMethod::declaring_interface_for_static_method(
				metadata, method_token, out_index,
			)
		} else if method.is_initializer() {
			DeclaringInterfaceForMethod::declaring_interface_for_initializer(
				metadata, method_token, out_index,
			)
		} else {
			DeclaringInterfaceForMethod::declaring_interface_for_instance_method(
				metadata, method_token, out_index,
			)
		}
	}
}