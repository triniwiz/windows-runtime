use std::any::Any;
use std::sync::Arc;
use parking_lot::RwLock;
use windows::core::GUID;
use windows::Win32::System::WinRT::Metadata::{CorTokenType, IMetaDataImport2};
use crate::declarations::base_class_declaration::BaseClassDeclarationImpl;
use crate::declarations::declaration::{Declaration, DeclarationKind};
use crate::declarations::event_declaration::EventDeclaration;
use crate::declarations::interface_declaration::InterfaceDeclaration;
use crate::declarations::method_declaration::MethodDeclaration;
use crate::declarations::property_declaration::PropertyDeclaration;
use crate::declarations::type_declaration::TypeDeclaration;

#[derive(Clone, Debug)]
pub struct GenericInterfaceDeclaration {
    base: InterfaceDeclaration,
}

impl GenericInterfaceDeclaration {
    pub fn new(metadata: Option<Arc<RwLock<IMetaDataImport2>>>, token: CorTokenType) -> Self {
        Self {
            base: InterfaceDeclaration::new_with_kind(
                DeclarationKind::GenericInterface,
                metadata,
                token,
            ),
        }
    }

    pub fn number_of_generic_parameters(&self) -> usize {
        let mut count = 0_u32;
        let mut enumerator = std::ptr::null_mut();
        let enumerator_ptr = &mut enumerator;
        let base = self.base();
        match base.metadata() {
            None => {}
            Some(metadata) => {
                let result =
                    unsafe { metadata.EnumGenericParams(enumerator_ptr, base.token().0 as u32, 0 as _, 0 as _, 0 as _) };
                debug_assert!(result.is_ok());
                let result = unsafe { metadata.CountEnum(enumerator, &mut count) };
                debug_assert!(result.is_ok());
                unsafe { metadata.CloseEnum(enumerator) };
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

    fn implemented_interfaces(&self) -> &[InterfaceDeclaration] {
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
