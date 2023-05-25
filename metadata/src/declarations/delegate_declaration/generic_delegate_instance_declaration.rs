use std::any::Any;
use std::borrow::Cow;

use crate::prelude::*;
use parking_lot::RwLock;
use windows::core::GUID;
use windows::Win32::System::WinRT::Metadata::{CorTokenType, IMetaDataImport2, mdtTypeSpec};
use crate::declarations::declaration::{Declaration, DeclarationKind};
use crate::declarations::delegate_declaration::{DelegateDeclaration, DelegateDeclarationImpl};
use crate::declarations::method_declaration::MethodDeclaration;
use crate::declarations::type_declaration::TypeDeclaration;
use crate::generic_instance_id_builder::GenericInstanceIdBuilder;
use crate::signature::Signature;

#[derive(Clone, Debug)]
pub struct GenericDelegateInstanceDeclaration {
    base: DelegateDeclaration,
    closed_token: CorTokenType,
    closed_metadata: Option<IMetaDataImport2>,
    full_name: String,
}

impl GenericDelegateInstanceDeclaration {
    pub fn new(
        open_metadata: Option<&IMetaDataImport2>,
        open_token: CorTokenType,
        closed_metadata: Option<&IMetaDataImport2>,
        closed_token: CorTokenType,
    ) -> Self {
        assert!(closed_metadata.is_some());
        assert_eq!(type_from_token(closed_token), mdtTypeSpec.0);
        assert_ne!(closed_token.0, 0);

        let full_name = match closed_metadata {
            None => String::new(),
            Some(metadata) => {
                let mut signature = PCCOR_SIGNATURE::default(); //std::ptr::null_mut();
                //let mut sig = &mut signature;
                let mut signature_size = 0;
                let result = unsafe {
                    metadata.GetTypeSpecFromToken(
                        closed_token.0 as u32,
                        &mut signature.as_abi_mut(),
                        &mut signature_size,
                    )
                };
                assert!(result.is_ok());
                // let signature = unsafe { std::slice::from_raw_parts(signature as *const u8, signature_size as usize) };
                Signature::to_string(metadata, &signature)
            }
        };

        Self {
            base: DelegateDeclaration::new_overload(
                DeclarationKind::GenericDelegateInstance,
                open_metadata,
                open_token,
            ),
            closed_token,
            closed_metadata: closed_metadata.map(|f| f.clone()),
            full_name,
        }
    }
}

impl Declaration for GenericDelegateInstanceDeclaration {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn name(&self) -> &str {
        self.base.name()
    }

    fn full_name(&self) -> &str {
        self.full_name.as_ref()
    }

    fn kind(&self) -> DeclarationKind {
        self.base.kind()
    }
}

impl DelegateDeclarationImpl for GenericDelegateInstanceDeclaration {
    fn as_declaration(&self) -> &dyn Declaration {
        self
    }

    fn as_declaration_mut(&mut self) -> &mut dyn Declaration {
        self
    }

    fn base(&self) -> &TypeDeclaration {
        &self.base.base
    }

    fn id(&self) -> GUID {
        GenericInstanceIdBuilder::generate_id(self)
    }

    fn invoke_method<'b>(&self) -> &MethodDeclaration {
        &self.base.invoke_method
    }
}