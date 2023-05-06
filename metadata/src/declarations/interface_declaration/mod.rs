use std::any::Any;
use std::sync::Arc;
use parking_lot::RwLock;
use windows::core::GUID;
use windows::Win32::System::WinRT::Metadata::{CorTokenType, IMetaDataImport2};
use crate::declarations::base_class_declaration::{BaseClassDeclaration, BaseClassDeclarationImpl};
use crate::declarations::declaration::{Declaration, DeclarationKind};
use crate::declarations::event_declaration::EventDeclaration;
use crate::declarations::method_declaration::MethodDeclaration;
use crate::declarations::property_declaration::PropertyDeclaration;
use crate::declarations::type_declaration::TypeDeclaration;
use crate::prelude::*;
pub mod generic_interface_declaration;
pub mod generic_interface_instance_declaration;

#[derive(Clone, Debug)]
pub struct InterfaceDeclaration {
    base: BaseClassDeclaration,
}

impl InterfaceDeclaration {
    pub fn new(metadata: Option<Arc<RwLock<IMetaDataImport2>>>, token: CorTokenType) -> Self {
        Self::new_with_kind(DeclarationKind::Interface, metadata, token)
    }

    pub fn new_with_kind(
        kind: DeclarationKind,
        metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
        token: CorTokenType,
    ) -> Self {
        Self {
            base: BaseClassDeclaration::new(kind, metadata, token),
        }
    }

    pub fn id(&self) -> GUID {
        let base = self.base.base();
        match base.metadata.as_ref() {
            None => GUID::zeroed(),
            Some(metadata) => {
                let metadata = metadata.read();
                get_guid_attribute_value(Some(&*metadata), base.token())
            }
        }
    }
}

impl Declaration for InterfaceDeclaration {
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

impl BaseClassDeclarationImpl for InterfaceDeclaration {
    fn as_declaration(&self) -> &dyn Declaration {
        self
    }

    fn as_declaration_mut(&mut self) -> &mut dyn Declaration {
        self
    }

    fn base(&self) -> &TypeDeclaration {
        self.base.base()
    }

    fn implemented_interfaces(&self) -> Vec<&InterfaceDeclaration> {
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
