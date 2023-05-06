use std::any::Any;
use std::fmt::{Debug, Formatter, Pointer};
use std::sync::Arc;
use parking_lot::RwLock;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::System::WinRT::Metadata::{CorTokenType, IMetaDataImport2};
use crate::declaration_factory::DeclarationFactory;
use crate::declarations::declaration::{Declaration, DeclarationKind};
use crate::declarations::delegate_declaration::{DelegateDeclaration, DelegateDeclarationImpl};
use crate::declarations::method_declaration::MethodDeclaration;
use crate::declarations::type_declaration::TypeDeclaration;
use crate::prelude::*;

#[derive(Clone)]
pub struct EventDeclaration {
    base: TypeDeclaration,
    // Arc ??
    type_: Option<Box<dyn DelegateDeclarationImpl>>,
    add_method: MethodDeclaration,
    remove_method: MethodDeclaration,
    full_name: String,
}

impl Debug for EventDeclaration {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventDeclaration")
            .field("type_", &self.type_.as_ref())
            .field("add_method", &self.add_method)
            .field("remove_method", &self.remove_method)
            .field("full_name", &self.full_name)
            .finish()
    }
}

impl EventDeclaration {
    pub fn make_add_method(
        metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
        token: CorTokenType,
    ) -> MethodDeclaration {
        let mut add_method_token = 0 as u32;
        if let Some(metadata) = Option::as_ref(&metadata) {
            let metadata = metadata.read();

            let result = unsafe {
                metadata.GetEventProps(
                    token.0 as u32,
                    0 as _,
                    None,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                    &mut add_method_token,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                )
            };
            debug_assert!(result.is_ok());
        }
        MethodDeclaration::new(
            Option::as_ref(&metadata).map(|v| Arc::clone(v)),
            CorTokenType(add_method_token as i32),
        )
    }

    pub fn make_remove_method(
        metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
        token: CorTokenType,
    ) -> MethodDeclaration {
        let mut remove_method_token = 0_u32;
        match Option::as_ref(&metadata) {
            None => {}
            Some(metadata) => {
                let metadata = metadata.read();
                let result = unsafe {
                    metadata.GetEventProps(
                        token.0 as u32,
                        0 as _,
                        None,
                        0 as _,
                        0 as _,
                        0 as _,
                        0 as _,
                        0 as _,
                        &mut remove_method_token,
                        0 as _,
                        0 as _,
                        0 as _,
                        0 as _,
                    )
                };

                debug_assert!(result.is_ok());
            }
        }
        MethodDeclaration::new(
            Option::as_ref(&metadata).map(|v| Arc::clone(v)),
            CorTokenType(remove_method_token as i32),
        )
    }

    pub fn make_type(
        metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
        token: CorTokenType,
    ) -> Option<Box<dyn DelegateDeclarationImpl>> {
        let mut delegate_token = 0 as u32;
        match Option::as_ref(&metadata) {
            None => {}
            Some(metadata) => {
                let metadata = metadata.read();
                let result = unsafe {
                    metadata.GetEventProps(
                        token.0 as u32,
                        0 as _,
                        None,
                        0 as _,
                        0 as _,
                        0 as _,
                        &mut delegate_token,
                        0 as _,
                        0 as _,
                        0 as _,
                        0 as _,
                        0 as _,
                        0 as _,
                    )
                };
                debug_assert!(result.is_ok());
            }
        }
        return DeclarationFactory::make_delegate_declaration(
            Option::as_ref(&metadata).map(|v| Arc::clone(v)),
            CorTokenType(delegate_token as i32),
        );
    }

    pub fn new(metadata: Option<Arc<RwLock<IMetaDataImport2>>>, token: CorTokenType) -> Self {
        let mut full_name = String::new();

        if let Some(metadata) = metadata.as_ref() {
            let metadata = metadata.read();

            let mut name_data = [0_u16; MAX_IDENTIFIER_LENGTH];
            let mut name_data_length = 0;

            let name = PCWSTR(name_data.as_mut_ptr());

            let result = unsafe {
                metadata.GetEventProps(
                    token.0 as u32,
                    0 as _,
                    name,
                    name_data.len() as u32,
                    &mut name_data_length,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                )
            };
            debug_assert!(result.is_ok());

            if name_data_length > 0 {
                full_name = HSTRING::from_wide(&name_data[..name_data_length as usize]).unwrap().to_string();
            }
        }


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
            full_name,
        }
    }

    pub fn is_static(&self) -> bool {
        self.add_method.is_static()
    }

    pub fn is_sealed(&self) -> bool {
        self.add_method.is_sealed()
    }

    pub fn type_(&self) -> Option<&DelegateDeclaration> {
        self.type_.map(|f|f.as_declaration().as_any().downcast_ref::<DelegateDeclaration>())
            .flatten()
    }

    pub fn add_method(&self) -> &MethodDeclaration {
        &self.add_method
    }

    pub fn remove_method(&self) -> &MethodDeclaration {
        &self.remove_method
    }
}

impl Declaration for EventDeclaration {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn is_exported(&self) -> bool {
        let mut flags = 0;
        if let Some(metadata) = self.base.metadata() {
            let result = unsafe { metadata.GetEventProps(
                self.base.token().0 as u32,
                0 as _,
                None,
                0 as _,
                0 as _,
                &mut flags,
                0 as _,
                0 as _,
                0 as _,
                0 as _,
                0 as _,
                0 as _,
                0 as _,
            )};
            debug_assert!(result.is_ok());
        }
        if is_ev_special_name(flags as i32) {
            return false;
        }

        return true;
    }

    fn name(&self) -> &str {
        self.full_name()
    }

    fn full_name(&self) -> &str {
        self.full_name.as_str()
    }

    fn kind(&self) -> DeclarationKind {
        self.base.kind()
    }
}
