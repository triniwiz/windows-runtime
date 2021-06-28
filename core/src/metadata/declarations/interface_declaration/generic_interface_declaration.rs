use std::borrow::Cow;

use crate::metadata::declarations::base_class_declaration::BaseClassDeclarationImpl;
use crate::metadata::declarations::declaration::{Declaration, DeclarationKind};
use crate::metadata::declarations::event_declaration::EventDeclaration;
use crate::metadata::declarations::interface_declaration::InterfaceDeclaration;
use crate::metadata::declarations::method_declaration::MethodDeclaration;
use crate::metadata::declarations::property_declaration::PropertyDeclaration;
use crate::metadata::declarations::type_declaration::TypeDeclaration;
use crate::prelude::*;
use core_bindings::{mdToken, GUID};
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug)]
pub struct GenericInterfaceDeclaration {
    base: InterfaceDeclaration,
}

impl GenericInterfaceDeclaration {
    pub fn new(metadata: Option<Arc<RwLock<IMetaDataImport2>>>, token: mdToken) -> Self {
        Self {
            base: InterfaceDeclaration::new_with_kind(
                DeclarationKind::GenericInterface,
                metadata,
                token,
            ),
        }
    }

    pub fn number_of_generic_parameters(&self) -> usize {
        let mut count = 0;
        let mut enumerator = std::ptr::null_mut();
        let enumerator_ptr = &mut enumerator;
        let base = self.base();
        match self.base().metadata() {
            None => {}
            Some(metadata) => {
                let result_1 =
                    metadata.enum_generic_params(enumerator_ptr, base.token(), None, None, None);
                debug_assert!(result_1.is_ok());
                let result_2 = metadata.count_enum(enumerator, &mut count);
                debug_assert!(result_2.is_ok());
                let result_1 =
                    metadata.enum_generic_params(enumerator_ptr, base.token(), None, None, None);
                debug_assert!(result_1.is_ok());
                let result_2 = metadata.count_enum(enumerator, &mut count);
                debug_assert!(result_2.is_ok());
            }
        }
        return count as usize;
    }

    pub fn id(&self) -> GUID {
        self.base.id()
    }
}

impl BaseClassDeclarationImpl for GenericInterfaceDeclaration {
    fn base(&self) -> &TypeDeclaration {
        self.base.base()
    }

    fn implemented_interfaces(&self) -> &Vec<InterfaceDeclaration> {
        self.base.implemented_interfaces()
    }

    fn methods(&self) -> &[MethodDeclaration] {
        self.base.methods()
    }

    fn properties(&self) -> &[PropertyDeclaration] {
        self.base.properties()
    }

    fn events(&self) -> &[EventDeclaration] {
        self.base.events()
    }
}

impl Declaration for GenericInterfaceDeclaration {
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
