use std::any::Any;
use std::ptr::addr_of_mut;
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
    closed_metadata: Option<IMetaDataImport2>,
    closed_token: CorTokenType,
    full_name: String,
    iid_full_name: String,
    cached_id: GUID,
}

impl GenericInterfaceInstanceDeclaration {
    pub fn new(
        open_metadata: Option<&IMetaDataImport2>,
        open_token: CorTokenType,
        closed_metadata: Option<&IMetaDataImport2>,
        closed_token: CorTokenType,
    ) -> Self {
        debug_assert!(closed_metadata.is_some());
        debug_assert!(
            type_from_token(closed_token) == mdtTypeSpec.0
        );
        debug_assert!(closed_token.0 != 0);

        let mut full_name = String::new();
        let mut iid_full_name = String::new();

        if let Some(metadata) = closed_metadata {
            let mut signature = PCCOR_SIGNATURE::default();
            let mut signature_size = 0;

            let result = unsafe {
                metadata.GetTypeSpecFromToken(
                    closed_token.0 as u32,
                    addr_of_mut!(signature.0),
                    &mut signature_size,
                )
            };
            debug_assert!(result.is_ok());
            if signature_size > 0 {
                full_name = Signature::to_string(metadata, &signature);
                iid_full_name = Signature::to_iid_string(metadata, &signature);
            }
        }

        let cached_id = if iid_full_name.is_empty() {
            GUID::zeroed()
        } else {
            GenericInstanceIdBuilder::generate_id_from_name(&iid_full_name)
        };

        Self {
            base: InterfaceDeclaration::new_with_kind(
                DeclarationKind::GenericInterfaceInstance,
                open_metadata,
                open_token,
            ),
            closed_metadata: closed_metadata.map(|f| f.clone()),
            closed_token,
            full_name,
            iid_full_name,
            cached_id,
        }
    }

    /// Build from the open-generic metadata/token and pre-computed type name strings.
    /// Used when we have the full closed-generic type name but no TypeSpec token.
    pub fn new_from_names(
        open_metadata: Option<&IMetaDataImport2>,
        open_token: CorTokenType,
        full_name: String,
        iid_full_name: String,
    ) -> Self {
        let cached_id = GenericInstanceIdBuilder::generate_id_from_name(&iid_full_name);
        Self {
            base: InterfaceDeclaration::new_with_kind(
                DeclarationKind::GenericInterfaceInstance,
                open_metadata,
                open_token,
            ),
            closed_metadata: None,
            closed_token: CorTokenType::default(),
            full_name,
            iid_full_name,
            cached_id,
        }
    }

    pub fn id(&self) -> GUID {
        self.cached_id
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
        self.full_name.as_str()
    }

    fn kind(&self) -> DeclarationKind {
        self.base.kind()
    }
}
