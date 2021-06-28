use std::borrow::Cow;

use core_bindings::{mdTypeDef, mdTypeSpec, GUID};

use crate::{
    bindings::enums,
    enums::CorTokenType,
    metadata::declarations::base_class_declaration::BaseClassDeclarationImpl,
    metadata::declarations::declaration::{Declaration, DeclarationKind},
    metadata::declarations::event_declaration::EventDeclaration,
    metadata::declarations::interface_declaration::InterfaceDeclaration,
    metadata::declarations::method_declaration::MethodDeclaration,
    metadata::declarations::property_declaration::PropertyDeclaration,
    metadata::declarations::type_declaration::TypeDeclaration,
    metadata::generic_instance_id_builder::GenericInstanceIdBuilder,
    metadata::signature::Signature,
    prelude::*,
};
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug)]
pub struct GenericInterfaceInstanceDeclaration {
    base: InterfaceDeclaration,
    closed_metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
    closed_token: mdTypeSpec,
}

impl GenericInterfaceInstanceDeclaration {
    pub fn new(
        open_metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
        open_token: mdTypeDef,
        closed_metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
        closed_token: mdTypeSpec,
    ) -> Self {
        debug_assert!(closed_metadata.is_none());
        debug_assert!(
            CorTokenType::from(enums::type_from_token(closed_token)) == CorTokenType::mdtTypeSpec
        );
        debug_assert!(closed_token != mdTypeSpecNil);

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

impl Declaration for GenericInterfaceInstanceDeclaration {
    fn name<'b>(&self) -> Cow<'b, str> {
        self.base.name()
    }

    fn full_name<'b>(&self) -> Cow<'b, str> {
        let mut signature = std::mem::MaybeUninit::uninit();
        //let mut signature = [0_u8; MAX_IDENTIFIER_LENGTH];
        let signature_ptr = &mut signature.as_mut_ptr();
        let mut signature_size = 0;
        match Option::as_ref(&self.closed_metadata) {
            None => Cow::default(),
            Some(metadata) => match metadata.try_read() {
                Ok(metadata) => {
                    let result = metadata.get_type_spec_from_token(
                        self.closed_token,
                        signature_ptr as _,
                        &mut signature_size,
                    );
                    debug_assert!(result.is_ok());
                    Signature::to_string(&metadata, signature.as_ptr()).into()
                }
                Err(_) => Cow::default(),
            },
        }
    }

    fn kind(&self) -> DeclarationKind {
        self.base.kind()
    }
}
