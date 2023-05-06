use std::any::Any;
use std::sync::{Arc};
use parking_lot::RwLock;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::System::WinRT::Metadata::{COR_CTOR_METHOD_NAME, COR_CTOR_METHOD_NAME_W, CorTokenType, IMetaDataImport2};
use crate::declaration_factory::DeclarationFactory;
use crate::declarations::base_class_declaration::{BaseClassDeclaration, BaseClassDeclarationImpl};
use crate::declarations::declaration::{Declaration, DeclarationKind};
use crate::declarations::interface_declaration::InterfaceDeclaration;
use crate::declarations::method_declaration::MethodDeclaration;

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
        metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
        token: CorTokenType,
    ) -> Vec<MethodDeclaration> {
        let mut ret = Vec::new();

        if let Some(metadata) = Option::as_ref(&metadata) {
            let meta = Arc::clone(metadata);
            let metadata = metadata.read();
            let mut enumerator = std::ptr::null_mut();
            let enumerator_ptr = &mut enumerator;
            let mut count = 0;
            let mut tokens = [0_u32; 1024];

            let result = unsafe {
                metadata.EnumMembersWithName(
                    enumerator_ptr,
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

                if is_md_public(flags as i32) {
                    continue;
                }

                ret.push(MethodDeclaration::new(
                    Some(Arc::clone(&meta)),
                    CorTokenType(method_token as i32),
                ));
            }
        }
        ret
    }

    pub fn make_default_interface(
        metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
        token: CorTokenType,
    ) -> Option<Box<dyn BaseClassDeclarationImpl>> {
        match Option::as_ref(&metadata) {
            None => {}
            Some(metadata) => {
                let meta = Arc::clone(metadata);
                let metadata = metadata.read();

                let mut interface_impl_tokens = [0_u32; 1024];
                let mut interface_impl_count = 0;
                let mut interface_enumerator = std::ptr::null_mut();
                let interface_enumerator_ptr = &mut interface_enumerator;
                let result = unsafe {
                    metadata.EnumInterfaceImpls(
                        interface_enumerator_ptr,
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
                            Some(Arc::clone(&meta)),
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

    pub fn new(metadata: Option<Arc<RwLock<IMetaDataImport2>>>, token: CorTokenType) -> Self {
        let mut base_full_name = String::new();

        if let Some(metadata) = metadata.as_ref() {
            let metadata = metadata.read();

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
            base_full_name = get_type_name(&*metadata, CorTokenType(parent_token as i32));
        }

        Self {
            initializers: ClassDeclaration::make_initializer_declarations(
                Option::as_ref(&metadata).map(|v| Arc::clone(v)),
                token,
            ),
            default_interface: ClassDeclaration::make_default_interface(
                Option::as_ref(&metadata).map(|v| Arc::clone(v)),
                token,
            ),
            base: BaseClassDeclaration::new(DeclarationKind::Class, metadata, token),
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
