use std::any::Any;
use std::ptr::addr_of_mut;
use std::sync::{Arc};
use parking_lot::RwLock;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::System::WinRT::Metadata::{COR_CTOR_METHOD_NAME, COR_CTOR_METHOD_NAME_W, CorTokenType, IMetaDataImport2, mdtInterfaceImpl, mdtTypeDef};
use crate::declaration_factory::DeclarationFactory;
use crate::declarations::base_class_declaration::{BaseClassDeclaration, BaseClassDeclarationImpl};
use crate::declarations::declaration::{Declaration, DeclarationKind};
use crate::declarations::event_declaration::EventDeclaration;
use crate::declarations::interface_declaration::InterfaceDeclaration;
use crate::declarations::method_declaration::MethodDeclaration;
use crate::declarations::property_declaration::PropertyDeclaration;
use crate::declarations::type_declaration::TypeDeclaration;

use crate::prelude::*;

const DEFAULT_ATTRIBUTE: &str = "Windows.Foundation.Metadata.DefaultAttribute";

#[derive(Clone)]
pub struct ClassDeclaration {
    initializers: Vec<MethodDeclaration>,
    default_interface: Option<Box<dyn BaseClassDeclarationImpl>>,
    base: BaseClassDeclaration,
    base_full_name: String,
}

impl ClassDeclaration {
    pub fn make_initializer_declarations(
        metadata: Option<&IMetaDataImport2>,
        token: CorTokenType,
    ) -> Vec<MethodDeclaration> {
        let mut ret = Vec::new();

        if let Some(metadata) = metadata {

            let mut enumerator = std::ptr::null_mut();
            let mut count = 0;
            let mut tokens = [0_u32; 1024];

            let result = unsafe {
                metadata.EnumMembersWithName(
                    addr_of_mut!(enumerator),
                    token.0 as u32,
                    COR_CTOR_METHOD_NAME_W,
                    tokens.as_mut_ptr(),
                    tokens.len() as u32,
                    &mut count,
                )
            };

            debug_assert!(result.is_ok());

            debug_assert!(count < (tokens.len().saturating_sub(1)) as u32);

            unsafe { metadata.CloseEnum(enumerator) };

            for i in 0..count as usize {
                let method_token = tokens[i];

                // TODO: Make a InstanceInitializerDeclaration and check this in it's isExported method
                let mut flags = 0;
                let result_inner = unsafe {
                    metadata.GetMethodProps(
                        method_token,
                        0 as _,
                        None,
                        0 as _,
                        &mut flags,
                        0 as _,
                        0 as _,
                        0 as _,
                        0 as _,
                    )
                };
                debug_assert!(result_inner.is_ok());

                if !is_md_public(flags as i32) {
                    continue;
                }

                ret.push(MethodDeclaration::new(
                    Some(&metadata),
                    CorTokenType(method_token as i32),
                ));
            }
        }
        ret
    }

    pub fn make_default_interface(
        metadata: Option<&IMetaDataImport2>,
        token: CorTokenType,
    ) -> Option<Box<dyn BaseClassDeclarationImpl>> {
        match metadata {
            None => {}
            Some(metadata) => {

                let mut interface_impl_tokens = [0 as u32; 1024];
                let mut interface_impl_count = 0;
                let mut interface_enumerator = std::ptr::null_mut();
                let result = unsafe {
                    metadata.EnumInterfaceImpls(
                        addr_of_mut!(interface_enumerator),
                        token.0 as u32,
                        interface_impl_tokens.as_mut_ptr(),
                        interface_impl_tokens.len() as u32,
                        &mut interface_impl_count,
                    )
                };
                debug_assert!(result.is_ok());

                debug_assert!(interface_impl_count < interface_impl_tokens.len() as u32);
                unsafe { metadata.CloseEnum(interface_enumerator) };
                let attr = HSTRING::from(DEFAULT_ATTRIBUTE);
                let attr = PCWSTR(attr.as_ptr());
                for i in 0..interface_impl_count as usize {
                    let interface_impl_token = interface_impl_tokens[i];
                    let get_custom_attribute_result = unsafe {
                        metadata
                            .GetCustomAttributeByName(
                                interface_impl_token,
                                attr,
                                0 as _,
                                0 as _,
                            )
                    };
                    debug_assert!(get_custom_attribute_result.is_ok());
                    if get_custom_attribute_result.is_ok() {
                        let mut interface_token = 0_u32;
                        let result = unsafe {
                            metadata.GetInterfaceImplProps(
                                interface_impl_token,
                                0 as _,
                                &mut interface_token,
                            )
                        };
                        debug_assert!(result.is_ok());
                        return DeclarationFactory::make_interface_declaration(
                            Some(metadata),
                            CorTokenType(interface_token as i32),
                        )
                    }
                }
            }
        }

        // todo
        unreachable!();
        None
    }

    pub fn new(metadata: Option<&IMetaDataImport2>, token: CorTokenType) -> Self {
        let mut base_full_name = String::new();

        if let Some(metadata) = metadata {
            let mut parent_token = 0_u32;


            let result = unsafe {
                metadata.GetTypeDefProps(
                    token.0 as u32,
                    None,
                    0 as _,
                    0 as _,
                    &mut parent_token,
                )
            };
            debug_assert!(result.is_ok());
            base_full_name = get_type_name(metadata, CorTokenType(parent_token as i32));
        }

        Self {
            initializers: ClassDeclaration::make_initializer_declarations(
                metadata.clone(),
                token,
            ),
            default_interface: ClassDeclaration::make_default_interface(
                metadata.clone(),
                token,
            ),
            base: BaseClassDeclaration::new(DeclarationKind::Class, metadata.clone(), token),
            base_full_name,
        }
    }

    pub fn base_full_name(&self) -> &str {
        self.base_full_name.as_str()
    }

    pub fn default_interface(&self) -> Option<&InterfaceDeclaration> {
        self.default_interface.as_ref().map(|f|{
            let f = f.as_declaration();
           let declaration =  f;
            declaration.as_any().downcast_ref::<InterfaceDeclaration>()
        }).flatten()
    }

    pub fn is_instantiable(&self) -> bool {
        !self.initializers.is_empty()
    }

    pub fn is_sealed(&self) -> bool {
        let mut flags = 0_u32;
        if let Some(metadata) = self.base.base().metadata() {
            let result = unsafe {
                metadata.GetTypeDefProps(
                    self.base.base().token().0 as u32,
                    None,
                    0 as _,
                    &mut flags,
                    0 as _,
                )
            };
            debug_assert!(result.is_ok());
        }
        is_td_sealed(flags as i32)
    }

    pub fn initializers(&self) -> &[MethodDeclaration] {
        self.initializers.as_slice()
    }
}

impl Declaration for ClassDeclaration {
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

impl BaseClassDeclarationImpl for ClassDeclaration {
    fn as_declaration(&self) -> &dyn Declaration {
        self
    }

    fn as_declaration_mut(&mut self) -> &mut dyn Declaration {
        self
    }

    fn base(&self) -> &TypeDeclaration {
        self.base.base()
    }

    fn implemented_interfaces(&self) -> Vec<&InterfaceDeclaration>{
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
