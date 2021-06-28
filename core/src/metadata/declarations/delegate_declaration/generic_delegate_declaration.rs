use std::borrow::Cow;

use crate::metadata::declarations::declaration::{Declaration, DeclarationKind};
use crate::metadata::declarations::delegate_declaration::{
    DelegateDeclaration, DelegateDeclarationImpl,
};
use crate::metadata::declarations::method_declaration::MethodDeclaration;
use crate::metadata::declarations::type_declaration::TypeDeclaration;
use crate::prelude::*;
use core_bindings::mdToken;
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug)]
pub struct GenericDelegateDeclaration {
    base: DelegateDeclaration,
}

impl GenericDelegateDeclaration {
    pub fn new(metadata: Option<Arc<RwLock<IMetaDataImport2>>>, token: mdToken) -> Self {
        Self {
            base: DelegateDeclaration::new_overload(
                DeclarationKind::GenericDelegate,
                Option::as_ref(&metadata).map(|v| Arc::clone(v)),
                token,
            ),
        }
    }

    pub fn number_of_generic_parameters(&self) -> usize {
        let mut count = 0;

        if let Some(metadata) = self.base.base.metadata() {
            let mut enumerator = std::ptr::null_mut();
            let enumerator_ptr = &mut enumerator;
            let result = metadata.enum_generic_params(
                enumerator_ptr,
                self.base.base.token(),
                None,
                None,
                None,
            );
            debug_assert!(result.is_ok());

            let result = metadata.count_enum(enumerator, &mut count);
            debug_assert!(result.is_ok());
            metadata.close_enum(enumerator);
        }
        return count as usize;
    }
}

impl Declaration for GenericDelegateDeclaration {
    fn name<'b>(&self) -> Cow<'b, str> {
        self.base.name()
    }

    fn full_name<'b>(&self) -> Cow<'b, str> {
        self.base.full_name()
    }

    fn kind(&self) -> DeclarationKind {
        self.base.kind()
    }
}

impl DelegateDeclarationImpl for GenericDelegateDeclaration {
    fn base<'b>(&self) -> &'b TypeDeclaration {
        &self.base.base
    }

    fn invoke_method<'b>(&self) -> &'b MethodDeclaration {
        &self.base.invoke_method
    }
}
