use std::any::Any;
use crate::prelude::*;
use std::sync::{Arc};
use parking_lot::RwLock;
use windows::Win32::System::WinRT::Metadata::{CorTokenType, IMetaDataImport2};
use crate::declarations::declaration::{Declaration, DeclarationKind};
use crate::declarations::delegate_declaration::{DelegateDeclaration, DelegateDeclarationImpl};
use crate::declarations::method_declaration::MethodDeclaration;
use crate::declarations::type_declaration::TypeDeclaration;

#[derive(Clone, Debug)]
pub struct GenericDelegateDeclaration {
    base: DelegateDeclaration,
}

impl GenericDelegateDeclaration {
    pub fn new(metadata: Option<&IMetaDataImport2>, token: CorTokenType) -> Self {
        Self {
            base: DelegateDeclaration::new_overload(
                DeclarationKind::GenericDelegate,
                metadata,
                token,
            ),
        }
    }

    pub fn number_of_generic_parameters(&self) -> usize {
        let mut count = 0;

        if let Some(metadata) = self.base.base.metadata() {
            let mut enumerator = std::ptr::null_mut();
            let enumerator_ptr = &mut enumerator;
            let result = unsafe { metadata.EnumGenericParams(
                enumerator_ptr,
                self.base.base.token().0 as u32,
                0 as _,
                0,
                0 as _,
            )};
            assert!(result.is_ok());

            let result = unsafe { metadata.CountEnum(enumerator, &mut count)};
            assert!(result.is_ok());
            unsafe { metadata.CloseEnum(enumerator)};
        }
        return count as usize;
    }
}

impl Declaration for GenericDelegateDeclaration {
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
        self.base.full_name()
    }

    fn kind(&self) -> DeclarationKind {
        self.base.kind()
    }
}

impl DelegateDeclarationImpl for GenericDelegateDeclaration {
    fn as_declaration(&self) -> &dyn Declaration {
        self
    }

    fn as_declaration_mut(&mut self) -> &mut dyn Declaration {
        self
    }

    fn base(&self) -> &TypeDeclaration {
        &self.base.base
    }

    fn invoke_method(&self) -> &MethodDeclaration {
        &self.base.invoke_method
    }
}