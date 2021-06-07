use crate::prelude::*;
use crate::bindings::enums;
use crate::metadata::declarations::delegate_declaration::DelegateDeclaration;
use crate::bindings::rometadataresolution;
use crate::metadata::com_helpers::resolve_type_ref;
use std::sync::Arc;
use crate::metadata::declarations::interface_declaration::InterfaceDeclaration;

pub struct DeclarationFactory {}

impl DeclarationFactory {
	pub fn make_delegate_declaration(metadata: *mut c_void, token: mdToken) -> DelegateDeclaration{
		match enums::type_from_token(token) as CorTokenType
		{
			CorTokenType::mdtTypeDef => DelegateDeclaration::new(metadata, token),
			CorTokenType::mdtTypeRef => {
				let mut external_metadata = std::ptr::null_mut();
				let mut external_delegate_token = mdTokenNil;
				let is_resolved = resolve_type_ref(metadata, token, &mut external_metadata,&mut external_delegate_token);
				debug_assert!(is_resolved);
				DelegateDeclaration::new(external_metadata, external_delegate_token)
			}
			CorTokenType::mdtTypeSpec => {}
			_ => {
				std::unreachable!()
			}
		}
	}
	pub fn make_interface_declaration(metadata: *mut c_void, token: mdToken):  {

		match enums::type_from_token(token){
			mdtTypeDef => {
				return Some(
					Arc::new(
						InterfaceDeclaration::new(metadata, token)
					)
				)
			}
		}
		switch (TypeFromToken(token)) {
			case mdtTypeDef: {
				return make_unique<InterfaceDeclaration>(metadata, token);
			}

			case mdtTypeRef: {
				ComPtr<IMetaDataImport2> externalMetadata;
				mdTypeDef externalInterfaceToken{ mdTokenNil };

				bool isResolved{ resolveTypeRef(metadata, token, externalMetadata.GetAddressOf(), &externalInterfaceToken) };
				ASSERT(isResolved);

				return make_unique<InterfaceDeclaration>(externalMetadata.Get(), externalInterfaceToken);
			}

			case mdtTypeSpec: {
				PCCOR_SIGNATURE signature{ nullptr };
				ULONG signatureSize{ 0 };
				ASSERT_SUCCESS(metadata->GetTypeSpecFromToken(token, &signature, &signatureSize));

				CorElementType type1{ CorSigUncompressElementType(signature) };
				ASSERT(type1 == ELEMENT_TYPE_GENERICINST);

				CorElementType type2{ CorSigUncompressElementType(signature) };
				ASSERT(type2 == ELEMENT_TYPE_CLASS);

				mdToken openGenericDelegateToken{ CorSigUncompressToken(signature) };
				switch (TypeFromToken(openGenericDelegateToken)) {
					case mdtTypeDef: {
						return make_unique<GenericInterfaceInstanceDeclaration>(metadata, openGenericDelegateToken, metadata, token);
					}

					case mdtTypeRef: {
						ComPtr<IMetaDataImport2> externalMetadata;
						mdTypeDef externalDelegateToken{ mdTokenNil };

						bool isResolved{ resolveTypeRef(metadata, openGenericDelegateToken, externalMetadata.GetAddressOf(), &externalDelegateToken) };
						ASSERT(isResolved);

						return make_unique<GenericInterfaceInstanceDeclaration>(externalMetadata.Get(), externalDelegateToken, metadata, token);
					}

					default:
						ASSERT_NOT_REACHED();
				}

				break;
			}

			default:
				ASSERT_NOT_REACHED();
		}
	}
}