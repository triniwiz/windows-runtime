use std::borrow::Cow;

use std::sync::{Arc, RwLock};

use crate::metadata::declarations::interface_declaration::InterfaceDeclaration;
use crate::{
    metadata::declaration_factory::DeclarationFactory,
    metadata::declarations::declaration::{Declaration, DeclarationKind},
    metadata::declarations::event_declaration::EventDeclaration,
    metadata::declarations::method_declaration::MethodDeclaration,
    metadata::declarations::property_declaration::PropertyDeclaration,
    metadata::declarations::type_declaration::TypeDeclaration,
    prelude::*,
};

#[derive(Clone)]
pub struct BaseClassDeclaration {
    base: TypeDeclaration,
    implemented_interfaces: Vec<Arc<RwLock<dyn BaseClassDeclarationImpl>>>,
    methods: Vec<MethodDeclaration>,
    properties: Vec<PropertyDeclaration>,
    events: Vec<EventDeclaration>,
}

impl BaseClassDeclaration {
    pub fn metadata(&self) -> Option<&IMetaDataImport2> {
        self.base.metadata()
    }
    
    pub fn make_implemented_interfaces_declarations(
        metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
        token: mdTypeDef,
    ) -> Vec<dyn BaseClassDeclarationImpl> {
        let mut result = Vec::new();
        match Option::as_ref(&metadata) {
            None => {}
            Some(metadata) => {
                let meta = Arc::clone(metadata);
                match metadata.try_read() {
                    Ok(metadata) => {
                        let mut enumerator = std::ptr::null_mut();
                        let enumerator_ptr = &mut enumerator;
                        let mut count = 0;
                        let mut tokens = [0; 1024];

                        let result_inner = metadata.enum_interface_impls(
                            enumerator_ptr,
                            Some(token),
                            Some(&mut tokens),
                            Some(tokens.len() as u32),
                            Some(&mut count),
                        );
                        debug_assert!(result_inner.is_ok());

                        debug_assert!(count < (tokens.len() - 1) as u32);
                        metadata.close_enum(enumerator);

                        for token in tokens.into_iter() {
                            let mut interface_token = mdTokenNil;
                            let result_inner = metadata.get_interface_impl_props(
                                *token,
                                None,
                                Some(&mut interface_token),
                            );
                            debug_assert!(result_inner.is_ok());
                            result.push(DeclarationFactory::make_interface_declaration(
                                Some(Arc::clone(&meta)),
                                interface_token,
                            ));
                        }
                    }
                    Err(_) => {}
                }
            }
        }
        result
    }

    pub fn make_method_declarations(
        metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
        token: mdTypeDef,
    ) -> Vec<MethodDeclaration> {
        let mut result = Vec::new();
        match Option::as_ref(&metadata) {
            None => {}
            Some(metadata) => {
                let meta = Arc::clone(&metadata);
                match metadata.try_read() {
                    Ok(metadata) => {
                        let mut enumerator = std::ptr::null_mut();
                        let enumerator_ptr = &mut enumerator;
                        let mut count = 0;
                        let mut tokens = [0; 1024];
                        let result_inner = metadata.enum_methods(
                            enumerator_ptr,
                            token,
                            &mut tokens,
                            tokens.len() as u32,
                            &mut count,
                        );
                        debug_assert!(result_inner.is_ok());

                        debug_assert!(count < (tokens.len() - 1) as u32);
                        metadata.close_enum(enumerator);

                        for token in tokens.iter() {
                            let method = MethodDeclaration::new(Some(Arc::clone(&meta)), *token);
                            if !method.is_exported() {
                                continue;
                            }
                            result.push(method);
                        }
                    }
                    Err(_) => {}
                }
            }
        }

        return result;
    }

    pub fn make_property_declarations<'b>(
        metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
        token: mdTypeDef,
    ) -> Vec<PropertyDeclaration> {
        let mut result = Vec::new();
        match Option::as_ref(&metadata) {
            None => {}
            Some(metadata) => {
                let meta = Arc::clone(&metadata);
                match metadata.try_read() {
                    Ok(metadata) => {
                        let mut enumerator = std::ptr::null_mut();
                        let enumerator_ptr = &mut enumerator;
                        let mut count = 0;
                        let mut tokens = [0; 1024];
                        let result_inner = metadata.enum_properties(
                            enumerator_ptr,
                            token,
                            &mut tokens,
                            tokens.len() as u32,
                            &mut count,
                        );
                        debug_assert!(result_inner.is_ok());
                        debug_assert!(count < (tokens.len() - 1) as u32);
                        metadata.close_enum(enumerator);

                        for token in tokens.iter() {
                            let property =
                                PropertyDeclaration::new(Some(Arc::clone(&meta)), *token);
                            if !property.is_exported() {
                                continue;
                            }
                            result.push(property);
                        }
                    }
                    Err(_) => {}
                }
            }
        }
        result
    }

    pub fn make_event_declarations(
        metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
        token: mdTypeDef,
    ) -> Vec<EventDeclaration> {
        let mut result = Vec::new();
        if let Some(metadata) = Option::as_ref(&metadata) {
            let meta = Arc::clone(metadata);
            match metadata.try_read() {
                Ok(metadata) => {
                    let mut enumerator = std::ptr::null_mut();
                    let enumerator_ptr = &mut enumerator;
                    let mut count = 0;
                    let mut tokens = [0; 1024];

                    let result_inner = metadata.enum_events(
                        enumerator_ptr,
                        token,
                        &mut tokens,
                        tokens.len() as u32,
                        &mut count,
                    );
                    debug_assert!(result_inner.is_ok());
                    debug_assert!(count < (tokens.len() - 1) as u32);
                    metadata.close_enum(enumerator);

                    for token in tokens.iter() {
                        let event = EventDeclaration::new(Some(Arc::clone(&meta)), *token);
                        if !event.is_exported() {
                            continue;
                        }
                        result.push(event);
                    }
                }
                Err(_) => {}
            }
        }

        result
    }

    pub fn new(
        kind: DeclarationKind,
        metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
        token: mdTypeDef,
    ) -> Self {
        Self {
            base: TypeDeclaration::new(
                kind,
                Option::as_ref(&metadata).map(|v| Arc::clone(v)),
                token,
            ),
            implemented_interfaces: BaseClassDeclaration::make_implemented_interfaces_declarations(
                Option::as_ref(&metadata).map(|v| Arc::clone(v)),
                token,
            ),
            methods: BaseClassDeclaration::make_method_declarations(
                Option::as_ref(&metadata).map(|v| Arc::clone(v)),
                token,
            ),
            properties: BaseClassDeclaration::make_property_declarations(
                Option::as_ref(&metadata).map(|v| Arc::clone(v)),
                token,
            ),
            events: BaseClassDeclaration::make_event_declarations(
                Option::as_ref(&metadata).map(|v| Arc::clone(v)),
                token,
            ),
        }
    }
}

pub trait BaseClassDeclarationImpl {
    fn base(&self) -> &TypeDeclaration;

    fn implemented_interfaces<'a>(&self) -> &[InterfaceDeclaration];

    fn methods<'a>(&self) -> &[MethodDeclaration];

    fn properties<'a>(&self) -> &[PropertyDeclaration];

    fn events<'a>(&self) -> &[EventDeclaration];

    fn find_members_with_name(&self, name: &str) -> Vec<dyn Declaration> {
        debug_assert!(!name.is_empty());

        let mut result: Vec<Box<dyn Declaration>> = Vec::new();

        // let mut methods = self.find_methods_with_name(name).into_iter().map(|item| Box::new(item)).collect();
        // result.append(&mut methods);

        let mut methods = self.find_methods_with_name(name);
        for method in methods.into_iter() {
            result.push(Box::new(method));
        }

        // let mut properties = self.properties().into_iter().filter(|prop| prop.full_name() == name).collect();
        // result.append(&mut properties);

        let mut properties = self.properties().clone();

        for property in properties.into_iter() {
            if property.full_name() == name {
                result.push(Box::new(property));
            }
        }

        // let mut events = self.events().into_iter().filter(|event| event.full_name() == name).collect();
        // result.append(&mut events);

        let mut events = self.events().clone();

        for event in events {
            if event.full_name() == name {
                result.push(Box::new(event))
            }
        }

        return result;
    }

    fn find_methods_with_name(&self, name: &str) -> Vec<MethodDeclaration> {
        debug_assert!(!name.is_empty());
        let mut method_tokens = [0; 1024];
        let mut meta = None;
        if let Some(metadata) = self.base().metadata_shared() {
            meta = Some(Arc::clone(&metadata));
            let mut enumerator = std::ptr::null_mut();
            let enumerator_ptr = &mut enumerator;

            let mut methods_count = 0;
            let name = windows::HSTRING::from(name);
            let name = name.as_wide();
            let base = self.base();
            let result = metadata.enum_methods_with_name(
                enumerator_ptr,
                base.token(),
                name.as_ptr(),
                &mut method_tokens,
                method_tokens.len() as u32,
                &mut methods_count,
            );
            debug_assert!(result.is_ok());
            metadata.close_enum(enumerator);
        }

        method_tokens
            .iter()
            .map(|method_token| {
                MethodDeclaration::new(Option::as_ref(&meta).map(|v| Arc::clone(v)), *method_token)
            })
            .collect()
    }
}

impl BaseClassDeclarationImpl for BaseClassDeclaration {
    fn base(&self) -> &TypeDeclaration {
        &self.base
    }

    fn implemented_interfaces(&self) -> &[Arc<RwLock<dyn BaseClassDeclarationImpl>>] {
        &self.implemented_interfaces
    }

    fn methods(&self) -> &[MethodDeclaration] {
        &self.methods
    }

    fn properties(&self) -> &[PropertyDeclaration] {
        &self.properties
    }

    fn events(&self) -> &[EventDeclaration] {
        &self.events
    }
}

impl Declaration for BaseClassDeclaration {
    fn name<'b>(&self) -> Cow<'b, str> {
        self.full_name()
    }

    fn full_name<'b>(&self) -> Cow<'b, str> {
        self.base().full_name()
    }

    fn kind(&self) -> DeclarationKind {
        self.base().kind()
    }
}
