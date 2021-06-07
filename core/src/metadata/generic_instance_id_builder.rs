use crate::prelude::*;
use crate::metadata::declarations::declaration::{Declaration, DeclarationKind};
use crate::bindings::helpers;
use crate::metadata::meta_data_reader::MetadataReader;
pub struct GenericInstanceIdBuilder {}

pub use crate::bindings::{rometadataresolution, iro_simple_meta_data_builder};
use core_bindings::GUID;
use crate::metadata::declarations::class_declaration::ClassDeclaration;
use std::sync::Arc;
use windows::HRESULT;
use crate::metadata::declarations::interface_declaration::{InterfaceDeclaration, GenericInterfaceDeclaration};
use crate::metadata::declarations::enum_declaration::EnumDeclaration;
use crate::metadata::signature::Signature;
use crate::metadata::declarations::struct_declaration::StructDeclaration;
use crate::metadata::declarations::delegate_declaration::{DelegateDeclaration, GenericDelegateDeclaration};

impl GenericInstanceIdBuilder {
	extern "C" fn locator_impl(name: PCWSTR, builder: *mut IRoSimpleMetaDataBuilder) -> windows::HRESULT {
		let declaration =  MetadataReader::find_by_name_w(name);
		debug_assert!(declaration.is_some());

		if let Some(declaration) = declaration {
			match declaration.kind() {
				DeclarationKind::Class => {
					let mut class_declaration = declaration
						.as_any()
						.downcast_ref::<ClassDeclaration>()
						.unwrap();

					let default_interface = class_declaration.default_interface();
					let default_interface_id = default_interface.id();
					let full_name = OsString::from(default_interface.full_name());
					let full_name_w = full_name.to_wide();
					debug_assert!(
						iro_simple_meta_data_builder::set_runtime_class_simple_default(
							builder, name, full_name_w.as_ptr(),
							&default_interface_id
						)
					);
					return HRESULT::from_win32(0)
				}
				DeclarationKind::Interface => {
					let mut interface_declaration = declaration
						.as_any()
						.downcast_ref::<InterfaceDeclaration>()
						.unwrap();
					let interface_declaration_id = interface_declaration.id();
					iro_simple_meta_data_builder::set_win_rt_interface(
						builder,interface_declaration_id
					);
					HRESULT::from_win32(0)
				}
				DeclarationKind::GenericInterface => {
					let mut generic_interface_declaration = declaration
						.as_any()
						.downcast_ref::<GenericInterfaceDeclaration>()
						.unwrap();
					debug_assert!(
						iro_simple_meta_data_builder::set_parameterized_interface(
							builder, generic_interface_declaration.id(), generic_interface_declaration.number_of_generic_parameters() as u32
						).is_ok()
					);
					HRESULT::from_win32(0)
				}
				DeclarationKind::Enum => {
					let mut enum_declaration = declaration
						.as_any()
						.downcast_ref::<EnumDeclaration>()
						.unwrap();
					let type_ = enum_declaration.type_().into_owned();
					let full_name_w = enum_declaration.full_name().to_owned().to_wide();
					let signature = Signature::to_string(
						std::ptr::null() as _, type_.as_ptr()
					);
					let signature_w = signature.to_wide();
					debug_assert!(
						iro_simple_meta_data_builder::set_enum(
							builder, full_name_w.as_ptr(),
							signature_w.as_ptr()
						).is_ok()
					);
					HRESULT::from_win32(0)
				}
				DeclarationKind::Struct => {

					let mut struct_declaration = declaration
						.as_any()
						.downcast_ref::<StructDeclaration>()
						.unwrap();

					let mut field_names = Vec::new();
					for field in struct_declaration.fields().iter() {
						field_names.push(
							Signature::to_string(
								field.base.metadata, field.type_()
							).to_wide().as_ptr()
						)
					}

					let full_name = struct_declaration.full_name().into_owned().to_wide();
					debug_assert!(
						iro_simple_meta_data_builder::set_struct(
							builder, full_name.as_ptr(),
							struct_declaration.size() as u32,
							field_names.as_ptr()
						).is_ok()
					);

					HRESULT::from_win32(0)
				}
				DeclarationKind::Delegate => {
					let mut delegate_declaration = declaration
						.as_any()
						.downcast_ref::<DelegateDeclaration>()
						.unwrap();

					debug_assert!(
						iro_simple_meta_data_builder::set_delegate(
							builder, delegate_declaration.id()
						).is_ok()
					);
					HRESULT::from_win32(0)
				}
				DeclarationKind::GenericDelegate => {
					let mut generic_delegate_declaration = declaration
						.as_any()
						.downcast_ref::<GenericDelegateDeclaration>()
						.unwrap();
					debug_assert!(
						iro_simple_meta_data_builder::set_parameterized_delegate(
							builder, generic_delegate_declaration.id(), generic_delegate_declaration.number_of_generic_parameters() as u32
						)
					)
				}
				_ => {
					std::unreachable!()
				}
			}
		}
		std::unreachable!()
	}

	pub fn generate_id(declaration: &dyn Declaration) -> GUID {
		let mut declaration_full_name = declaration.full_name().into_owned().to_wide();
		let mut name_parts = [0_u16;MAX_IDENTIFIER_LENGTH];
		let mut name_parts_count = 0;
		let mut name_parts_w = &mut name_parts.as_ptr();

		helpers::generate_id_name(name_parts_w, declaration_full_name.as_mut_ptr(), &mut name_parts_count);

		let mut guid = GUID::default();
		debug_assert!(
			rometadataresolution::ro_get_parameterized_type_instance_iid(
				name_parts_count, namePartsW.as_mut_ptr(), Option(GenericInstanceIdBuilder::locator_impl), &mut guid, None
			)
		);

		return guid;
	}

}