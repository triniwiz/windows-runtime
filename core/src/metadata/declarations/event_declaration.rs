use crate::bindings::{helpers};
use crate::metadata::declaration_factory::DeclarationFactory;
use crate::metadata::declarations::declaration::{Declaration, DeclarationKind};
use crate::metadata::declarations::delegate_declaration::{
    DelegateDeclaration, DelegateDeclarationImpl,
};
use crate::metadata::declarations::method_declaration::MethodDeclaration;
use crate::metadata::declarations::type_declaration::TypeDeclaration;
use crate::prelude::*;
use std::borrow::Cow;
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug)]
pub struct EventDeclaration {
    base: TypeDeclaration,
    // Arc ??
    type_: Option<Arc<RwLock<dyn DelegateDeclarationImpl>>>,
    add_method: MethodDeclaration,
    remove_method: MethodDeclaration,
}

impl EventDeclaration {
    pub fn make_add_method(
        metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
        token: mdEvent,
    ) -> MethodDeclaration {
        let mut add_method_token = mdTokenNil;
        if let Some(metadata) = Option::as_ref(&metadata) {
            match metadata.try_read() {
                Ok(metadata) => {
                    let result = metadata.get_event_props(
                        token,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        Some(&mut add_method_token),
                        None,
                        None,
                        None,
                        None,
                        None,
                    );
                    debug_assert!(result.is_ok());
                }
                Err(_) => {}
            }
        }
        MethodDeclaration::new(
            Option::as_ref(&metadata).map(|v| Arc::clone(v)),
            add_method_token,
        )
    }

    pub fn make_remove_method(
        metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
        token: mdEvent,
    ) -> MethodDeclaration {
        let mut remove_method_token = mdTokenNil;
        match Option::as_ref(&metadata) {
            None => {}
            Some(metadata) => match metadata.try_read() {
                Ok(metadata) => {
                    let result = metadata.get_event_props(
                        token,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        Some(&mut remove_method_token),
                        None,
                        None,
                        None,
                        None,
                    );

                    debug_assert!(result.is_ok());
                }
                Err(_) => {}
            },
        }
        MethodDeclaration::new(
            Option::as_ref(&metadata).map(|v| Arc::clone(v)),
            remove_method_token,
        )
    }

    pub fn make_type(
        metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
        token: mdEvent,
    ) -> Option<Arc<RwLock<dyn DelegateDeclarationImpl>>> {
        let mut delegate_token = mdTokenNil;
        match Option::as_ref(&metadata) {
            None => {}
            Some(metadata) => match metadata.try_read() {
                Ok(metadata) => {
                    let result = metadata.get_event_props(
                        token,
                        None,
                        None,
                        None,
                        None,
                        None,
                        Some(&mut delegate_token),
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                    );
                    debug_assert!(result.is_ok());
                }
                Err(_) => {}
            },
        }
        return DeclarationFactory::make_delegate_declaration(
            Option::as_ref(&metadata).map(|v| Arc::clone(v)),
            delegate_token,
        );
    }

    pub fn new(metadata: Option<Arc<RwLock<IMetaDataImport2>>>, token: mdEvent) -> Self {
        Self {
            base: TypeDeclaration::new(
                DeclarationKind::Event,
                Option::as_ref(&metadata).map(|v| Arc::clone(v)),
                token,
            ),
            type_: EventDeclaration::make_type(
                Option::as_ref(&metadata).map(|v| Arc::clone(v)),
                token,
            ),
            add_method: EventDeclaration::make_add_method(
                Option::as_ref(&metadata).map(|v| Arc::clone(v)),
                token,
            ),
            remove_method: EventDeclaration::make_remove_method(
                Option::as_ref(&metadata).map(|v| Arc::clone(v)),
                token,
            ),
        }
    }

    pub fn is_static(&self) -> bool {
        self.add_method.is_static()
    }

    pub fn is_sealed(&self) -> bool {
        self.add_method.is_sealed()
    }

    pub fn type_(&self) -> &DelegateDeclaration {
        &self.type_
    }

    pub fn add_method(&self) -> &MethodDeclaration {
        &self.add_method
    }

    pub fn remove_method(&self) -> &MethodDeclaration {
        &self.remove_method
    }
}

impl Declaration for EventDeclaration {
    fn is_exported(&self) -> bool {
        let mut flags = 0;
        if let Some(metadata) = self.base.metadata() {
            let result = metadata.get_event_props(
                self.base.token(),
                None,
                None,
                None,
                None,
                Some(&mut flags),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            );
            debug_assert!(result.is_ok());
        }
        if helpers::is_ev_special_name(flags) {
            return false;
        }

        return true;
    }

    fn name<'b>(&self) -> Cow<'b, str> {
        self.full_name()
    }

    fn full_name<'b>(&self) -> Cow<'b, str> {
        let mut name_data = [0_u16; MAX_IDENTIFIER_LENGTH];
        let mut name_data_length = 0;

        if let Some(metadata) = self.base.metadata() {
            let result = metadata.get_event_props(
                self.base.token(),
                None,
                Some(name_data.as_mut_ptr()),
                Some(name_data.len() as u32),
                Some(&mut name_data_length),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            );
            debug_assert!(result.is_ok());
        }
        OsString::from_wide(&name_data[..name_data_length]).into()
    }

    fn kind(&self) -> DeclarationKind {
        self.base.kind()
    }
}
