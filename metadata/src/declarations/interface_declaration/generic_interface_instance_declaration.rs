use std::any::Any;
use std::sync::Arc;
use parking_lot::RwLock;
use windows::core::GUID;
use windows::Win32::System::WinRT::Metadata::{CorTokenType, IMetaDataImport2, mdtTypeSpec};
use crate::declarations::base_class_declaration::BaseClassDeclarationImpl;
use crate::declarations::declaration::{Declaration, DeclarationKind};
use crate::declarations::event_declaration::EventDeclaration;
use crate::declarations::interface_declaration::InterfaceDeclaration;
use crate::declarations::method_declaration::MethodDeclaration;
use crate::declarations::property_declaration::PropertyDeclaration;
use crate::declarations::type_declaration::TypeDeclaration;
use crate::generic_instance_id_builder::GenericInstanceIdBuilder;
use crate::prelude::*;
use crate::signature::Signature;

#[derive(Clone, Debug)]
pub struct GenericInterfaceInstanceDeclaration {
    base: InterfaceDeclaration,
    closed_metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
    closed_token: CorTokenType,
}

impl GenericInterfaceInstanceDeclaration {
    pub fn new(
        open_metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
        open_token: CorTokenType,
        closed_metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
        closed_token: CorTokenType,
    ) -> Self {
        debug_assert!(closed_metadata.is_some());
        debug_assert!(
            type_from_token(closed_token) == mdtTypeSpec.0 as i32
        );
        debug_assert!(closed_token.0 != 0);


        let mut full_name = String::new();

        if let Some(metadata) = closed_metadata.as_ref() {
            let metadata = metadata.read();

            let mut signature = std::ptr::null_mut();//PCCOR_SIGNATURE::default();
            //let mut signature = [0_u8; MAX_IDENTIFIER_LENGTH];
            //let signature_ptr = &mut signature;
            let mut signature_size = 0;


            let result = unsafe {
                metadata.GetTypeSpecFromToken(
                    closed_token.0 as u32,
                    &mut signature,
                    &mut signature_size,
                )
            };
            debug_assert!(result.is_ok());
            if signature_size > 0 {
                let mut signature = PCCOR_SIGNATURE::from_ptr(signature);
               // let signature = unsafe { std::slice::from_raw_parts(signature, signature_size as usize) };
                full_name = Signature::to_string(&metadata, &signature);
            }
        }

        Self {
            base: InterfaceDeclaration::new_with_kind(
                DeclarationKind::GenericInterfaceInstance,
                open_metadata,
                open_token,
            ),
            closed_metadata,
            closed_token,
        }
    }
    pub fn id(&self) -> GUID {
        return GenericInstanceIdBuilder::generate_id(self);
    }
}

impl BaseClassDeclarationImpl for GenericInterfaceInstanceDeclaration {
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

impl Declaration for GenericInterfaceInstanceDeclaration {
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
