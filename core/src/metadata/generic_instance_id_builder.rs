use crate::prelude::*;
use crate::metadata::declarations::declaration::{Declaration, DeclarationKind};
use crate::bindings::helpers;
use crate::metadata::meta_data_reader::MetadataReader;
pub struct GenericInstanceIdBuilder {}

pub use crate::bindings::rometadataresolution;
use core_bindings::GUID;
use crate::metadata::declarations::class_declaration::ClassDeclaration;
use std::sync::Arc;


impl GenericInstanceIdBuilder {
	extern "C" fn locator_impl(name: PCWSTR, builder: *mut IRoSimpleMetaDataBuilder) -> windows::HRESULT {
		let declaration =  MetadataReader::find_by_name(name);
		debug_assert!(declaration.is_some());

		if let Some(declaration) = declaration {
			match declaration.kind() {
				DeclarationKind::Class => {
					let mut classDeclaration = declaration
						.as_any()
						.downcast_ref::<ClassDeclaration>()
						.unwrap();

					let defaultInterface = classDeclaration.default_interface();
					let defaultInterfaceId = defaultInterface.id();
					debug_assert!(
						
					);
					ASSERT_SUCCESS(builder.SetRuntimeClassSimpleDefault(name, defaultInterface.fullName().data(), &defaultInterfaceId));
					return S_OK;
				}
				DeclarationKind::Interface => {}
				DeclarationKind::GenericInterface => {}
				DeclarationKind::GenericInterfaceInstance => {}
				DeclarationKind::Enum => {}
				DeclarationKind::EnumMember => {}
				DeclarationKind::Struct => {}
				DeclarationKind::StructField => {}
				DeclarationKind::Delegate => {}
				DeclarationKind::GenericDelegate => {}
				DeclarationKind::GenericDelegateInstance => {}
				DeclarationKind::Event => {}
				DeclarationKind::Property => {}
				DeclarationKind::Method => {}
				DeclarationKind::Parameter => {}
			}
		}
		DeclarationKind kind{ declaration->kind() };
		switch (kind) {
			case DeclarationKind::Class: {
				ClassDeclaration* classDeclaration{ static_cast<ClassDeclaration*>(declaration.get()) };
				const InterfaceDeclaration& defaultInterface{ classDeclaration->defaultInterface() };
				IID defaultInterfaceId = defaultInterface.id();
				ASSERT_SUCCESS(builder.SetRuntimeClassSimpleDefault(name, defaultInterface.fullName().data(), &defaultInterfaceId));
				return S_OK;
			}

			case DeclarationKind::Interface: {
				InterfaceDeclaration* interfaceDeclaration{ static_cast<InterfaceDeclaration*>(declaration.get()) };
				ASSERT_SUCCESS(builder.SetWinRtInterface(interfaceDeclaration->id()));
				return S_OK;
			}

			case DeclarationKind::GenericInterface: {
				GenericInterfaceDeclaration* genericInterfaceDeclaration{ static_cast<GenericInterfaceDeclaration*>(declaration.get()) };
				ASSERT_SUCCESS(builder.SetParameterizedInterface(genericInterfaceDeclaration->id(), genericInterfaceDeclaration->numberOfGenericParameters()));
				return S_OK;
			}

			case DeclarationKind::Enum: {
				EnumDeclaration* enumDeclaration{ static_cast<EnumDeclaration*>(declaration.get()) };
				ASSERT_SUCCESS(builder.SetEnum(enumDeclaration->fullName().data(), Signature::toString(nullptr, enumDeclaration->type()).data()));
				return S_OK;
			}

			case DeclarationKind::Struct: {
				StructDeclaration* structDeclaration{ static_cast<StructDeclaration*>(declaration.get()) };

				vector<wstring> fieldNames;
				for (const StructFieldDeclaration& field : *structDeclaration) {
					fieldNames.push_back(Signature::toString(field._metadata.Get(), field.type()));
				}

				vector<wchar_t*> fieldNamesW;
				for (wstring& fieldName : fieldNames) {
					fieldNamesW.push_back(const_cast<wchar_t*>(fieldName.data()));
				}

				ASSERT_SUCCESS(builder.SetStruct(structDeclaration->fullName().data(), structDeclaration->size(), fieldNamesW.data()));
				return S_OK;
			}

			case DeclarationKind::Delegate: {
				DelegateDeclaration* delegateDeclaration{ static_cast<DelegateDeclaration*>(declaration.get()) };
				ASSERT_SUCCESS(builder.SetDelegate(delegateDeclaration->id()));
				return S_OK;
			}

			case DeclarationKind::GenericDelegate: {
				GenericDelegateDeclaration* genericDelegateDeclaration{ static_cast<GenericDelegateDeclaration*>(declaration.get()) };
				ASSERT_SUCCESS(builder.SetParameterizedDelegate(genericDelegateDeclaration->id(), genericDelegateDeclaration->numberOfGenericParameters()));
				return S_OK;
			}

			default:
				ASSERT_NOT_REACHED();
		}
	}

	pub fn generate_id(declaration: &dyn Declaration){
		let declaration_full_name = declaration.full_name();
		let mut name_ref = windows::HSTRING::from(declaration_full_name);
		let cap = vec![0_u16;MAX_IDENTIFIER_LENGTH];
		let mut nameParts = windows::HSTRING::from_wide(cap.as_slice());
		let mut name_parts_ref = &mut nameParts.as_mut_ptr();
		let mut namePartsCount = 0;
		debug_assert!(
			rometadataresolution::ro_parse_type_name(
				name_ref, &mut namePartsCount, name_parts_ref
			).is_ok()
		);

		let mut namePartsW = vec![0_u16; 128];
		debug_assert!(
			namePartsCount < namePartsW.len() as u32
		);

		for i in 0..namePartsCount {
			namePartsCount[i] = helpers::windows_get_string_raw_buffer(nameParts.as_wide()[i],None)
		}


		let mut guid = GUID::default();
		debug_assert!(
			rometadataresolution::ro_get_parameterized_type_instance_iid(
				namePartsCount, namePartsW.as_mut_ptr(), rometadataresolution::ro_locator(
					Option(GenericInstanceIdBuilder::locator_impl)
				), &mut guid, None
			)
		);

		/*
		for (size_t i = 0; i < namePartsCount; ++i) {
			ASSERT_SUCCESS(WindowsDeleteString(nameParts[i]));
		}
		*/

		//CoTaskMemFree(nameParts);

		return guid;
	}

	// HRESULT locatorImpl(PCWSTR name, IRoSimpleMetaDataBuilder& builder);
}