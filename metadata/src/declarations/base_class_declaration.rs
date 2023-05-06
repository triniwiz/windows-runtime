use std::any::Any;
use std::fmt::{Debug, Formatter, Pointer};
use std::sync::Arc;
use parking_lot::{MappedRwLockReadGuard, RwLock};
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::System::WinRT::Metadata::{CorTokenType, IMetaDataImport2};
use crate::declaration_factory::DeclarationFactory;
use crate::declarations::declaration::{Declaration, DeclarationKind};
use crate::declarations::event_declaration::EventDeclaration;
use crate::declarations::interface_declaration::InterfaceDeclaration;
use crate::declarations::method_declaration::MethodDeclaration;
use crate::declarations::property_declaration::PropertyDeclaration;
use crate::declarations::type_declaration::TypeDeclaration;

#[derive(Clone)]
pub struct BaseClassDeclaration {
    base: TypeDeclaration,
    implemented_interfaces: Vec<Box<dyn BaseClassDeclarationImpl>>,
    methods: Vec<MethodDeclaration>,
    properties: Vec<PropertyDeclaration>,
    events: Vec<EventDeclaration>,
}

impl Debug for BaseClassDeclaration {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(self.methods.iter())
            .entries(self.properties.iter())
            .entries(self.events.iter())
            .finish()
    }
}

impl BaseClassDeclaration {
    pub fn metadata(&self) -> Option<MappedRwLockReadGuard<'_, IMetaDataImport2>> {
        self.base.metadata()
    }

    pub fn make_implemented_interfaces_declarations(
        metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
        token: CorTokenType,
    ) -> Vec<Box<dyn BaseClassDeclarationImpl>> {
        let mut result = Vec::new();
        match Option::as_ref(&metadata) {
            None => {}
            Some(metadata) => {
                let meta = Arc::clone(metadata);
                let metadata = metadata.read();

                let mut enumerator = std::ptr::null_mut();
                let enumerator_ptr = &mut enumerator;
                let mut count = 0;
                let mut tokens = [0_u32; 1024];

                let result_inner = unsafe {
                    metadata.EnumInterfaceImpls(
                        enumerator_ptr,
                        token.0 as u32,
                        tokens.as_mut_ptr(),
                        tokens.len() as u32,
                        &mut count,
                    )
                };
                debug_assert!(result_inner.is_ok());

                debug_assert!(count < (tokens.len().saturating_sub(1)) as u32);

                unsafe { metadata.CloseEnum(enumerator) };

                for token in tokens.into_iter() {
                    let mut interface_token = 0_u32;
                    let result_inner = unsafe {
                        metadata.GetInterfaceImplProps(
                            token,
                            0 as _,
                            &mut interface_token,
                        )
                    };
                    debug_assert!(result_inner.is_ok());
                    if let Some(dec) = DeclarationFactory::make_interface_declaration(
                        Some(Arc::clone(&meta)),
                        CorTokenType(interface_token as i32),
                    )
                    {
                        result.push(dec);
                    }
                }
            }
        }
        result
    }

    pub fn make_method_declarations(
        metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
        token: CorTokenType,
    ) -> Vec<MethodDeclaration> {
        let mut result = Vec::new();
        match Option::as_ref(&metadata) {
            None => {}
            Some(metadata) => {
                let meta = Arc::clone(&metadata);
                let metadata = metadata.read();

                let mut enumerator = std::ptr::null_mut();
                let enumerator_ptr = &mut enumerator;
                let mut count = 0;
                let mut tokens = [0_u32; 1024];
                let result_inner = unsafe {
                    metadata.EnumMethods(
                        enumerator_ptr,
                        token.0 as u32,
                        tokens.as_mut_ptr(),
                        tokens.len() as u32,
                        &mut count,
                    )
                };
                debug_assert!(result_inner.is_ok());

                debug_assert!(count < (tokens.len().saturating_sub(1)) as u32);
                unsafe { metadata.CloseEnum(enumerator)};

                for token in tokens.iter() {
                    let method = MethodDeclaration::new(Some(Arc::clone(&meta)), CorTokenType(*token as i32));
                    if !method.is_exported() {
                        continue;
                    }
                    result.push(method);
                }
            }
        }

        return result;
    }

    pub fn make_property_declarations(
        metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
        token: CorTokenType,
    ) -> Vec<PropertyDeclaration> {
        let mut result = Vec::new();
        match Option::as_ref(&metadata) {
            None => {}
            Some(metadata) => {
                let meta = Arc::clone(&metadata);
                let metadata = metadata.read();

                let mut enumerator = std::ptr::null_mut();
                let enumerator_ptr = &mut enumerator;
                let mut count = 0;
                let mut tokens = [0_u32; 1024];
                let result_inner = unsafe {
                    metadata.EnumProperties(
                        enumerator_ptr,
                        token.0 as u32,
                        tokens.as_mut_ptr(),
                        tokens.len() as u32,
                        &mut count,
                    )
                };
                debug_assert!(result_inner.is_ok());
                debug_assert!(count < (tokens.len().saturating_sub(1)) as u32);
                unsafe { metadata.CloseEnum(enumerator) };

                for token in tokens.iter() {
                    let property =
                        PropertyDeclaration::new(Some(Arc::clone(&meta)), CorTokenType(*token as i32));
                    if !property.is_exported() {
                        continue;
                    }
                    result.push(property);
                }
            }
        }
        result
    }

    pub fn make_event_declarations(
        metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
        token: CorTokenType,
    ) -> Vec<EventDeclaration> {
        let mut result = Vec::new();
        if let Some(metadata) = Option::as_ref(&metadata) {
            let meta = Arc::clone(metadata);
            let metadata = metadata.read();

            let mut enumerator = std::ptr::null_mut();
            let enumerator_ptr = &mut enumerator;
            let mut count = 0;
            let mut tokens = [0_u32; 1024];

            let result_inner = unsafe {
                metadata.EnumEvents(
                    enumerator_ptr,
                    token.0 as u32,
                    tokens.as_mut_ptr(),
                    tokens.len() as u32,
                    &mut count,
                )
            };
            debug_assert!(result_inner.is_ok());
            debug_assert!(count < (tokens.len().saturating_sub(1)) as u32);
            unsafe { metadata.CloseEnum(enumerator) };

            for token in tokens.iter() {
                let event = EventDeclaration::new(Some(Arc::clone(&meta)), CorTokenType(*token as i32));
                if !event.is_exported() {
                    continue;
                }
                result.push(event);
            }
        }

        result
    }

    pub fn new(
        kind: DeclarationKind,
        metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
        token: CorTokenType,
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

    fn as_declaration(&self) -> &dyn Declaration;

    fn as_declaration_mut(&mut self) -> &mut dyn Declaration;

    fn base(&self) -> &TypeDeclaration;

    fn implemented_interfaces(&self) -> &[&InterfaceDeclaration];

    fn methods(&self) -> &[MethodDeclaration];

    fn properties(&self) -> &[PropertyDeclaration];

    fn events(&self) -> &[EventDeclaration];

    fn find_members_with_name(&self, name: &str) -> Vec<Box<dyn Declaration>> {
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

        let mut properties = self.properties().to_vec();

        for property in properties.into_iter() {
            if property.full_name() == name {
                result.push(Box::new(property));
            }
        }

        // let mut events = self.events().into_iter().filter(|event| event.full_name() == name).collect();
        // result.append(&mut events);

        let mut events = self.events().to_vec();

        for event in events {
            if event.full_name() == name {
                result.push(Box::new(event))
            }
        }

        return result;
    }

    fn find_methods_with_name(&self, name: &str) -> Vec<MethodDeclaration> {
        debug_assert!(!name.is_empty());
        let mut method_tokens = [0_u32; 1024];
        let mut meta = self.base().metadata.clone();
        if let Some(metadata) = self.base().metadata() {
            let mut enumerator = std::ptr::null_mut();
            let enumerator_ptr = &mut enumerator;

            let mut methods_count = 0;
            let name = HSTRING::from(name);
            let name = PCWSTR(name.as_ptr());
            let base = self.base();
            let result = unsafe {
                metadata.EnumMethodsWithName(
                    enumerator_ptr,
                    base.token().0 as u32,
                    name,
                    method_tokens.as_mut_ptr(),
                    method_tokens.len() as u32,
                    &mut methods_count,
                )
            };
            debug_assert!(result.is_ok());
            unsafe { metadata.CloseEnum(enumerator) };
        }

        method_tokens
            .into_iter()
            .map(|method_token| {
                MethodDeclaration::new(Option::as_ref(&meta).map(|v| Arc::clone(v)), CorTokenType(method_token as i32))
            })
            .collect()
    }
}

impl BaseClassDeclarationImpl for BaseClassDeclaration {
    fn as_declaration(&self) -> &dyn Declaration {
        self
    }

    fn as_declaration_mut(&mut self) -> &mut dyn Declaration {
        self
    }

    fn base(&self) -> &TypeDeclaration {
        &self.base
    }

    fn implemented_interfaces(&self) -> &[&InterfaceDeclaration]{
        let ret = self.implemented_interfaces
            .iter()
            .filter_map(|f| f.as_declaration().as_any().downcast_ref::<InterfaceDeclaration>())
            .collect::<Vec<_>>();

        ret.as_slice()
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
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn name(&self) -> &str {
        self.full_name()
    }

    fn full_name(&self) -> &str {
        self.base().full_name()
    }

    fn kind(&self) -> DeclarationKind {
        self.base().kind()
    }
}

impl Clone for Box<dyn BaseClassDeclarationImpl> {
    fn clone(&self) -> Self {
        self.clone()
    }
}
