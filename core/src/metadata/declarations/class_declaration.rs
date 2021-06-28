use std::borrow::Cow;
use std::sync::{Arc, RwLock, TryLockError};

use crate::bindings::helpers;
use crate::metadata::declaration_factory::DeclarationFactory;
use crate::metadata::declarations::base_class_declaration::{
    BaseClassDeclaration, BaseClassDeclarationImpl,
};
use crate::metadata::declarations::declaration::{Declaration, DeclarationKind};
use crate::metadata::declarations::interface_declaration::InterfaceDeclaration;
use crate::metadata::declarations::method_declaration::MethodDeclaration;
use crate::prelude::*;

const DEFAULT_ATTRIBUTE: &str = "Windows.Foundation.Metadata.DefaultAttribute";

#[derive(Clone)]
pub struct ClassDeclaration {
    initializers: Vec<MethodDeclaration>,
    default_interface: Option<Arc<dyn BaseClassDeclarationImpl>>,
    base: BaseClassDeclaration,
}

impl ClassDeclaration {
    pub fn make_initializer_declarations(
        metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
        token: mdTypeDef,
    ) -> Vec<MethodDeclaration> {
        let mut result = Vec::new();

        if let Some(metadata) = Option::as_ref(&metadata) {
            let meta = Arc::clone(metadata);
            match metadata.try_read() {
                Ok(metadata) => {
                    let mut enumerator = std::ptr::null_mut();
                    let enumerator_ptr = &mut enumerator;
                    let mut count = 0;
                    let mut tokens = [0; 1024];

                    let name = windows::HSTRING::from(COR_CTOR_METHOD_NAME);
                    let name = name.as_wide();

                    let result = metadata.enum_methods_with_name(
                        enumerator_ptr,
                        token,
                        name.as_ptr(),
                        &mut tokens,
                        tokens.len() as u32,
                        &mut count,
                    );
                    debug_assert!(result.is_ok());

                    debug_assert!(count < (tokens.len() - 1) as u32);

                    metadata.close_enum(enumerator);

                    for i in 0..count as usize {
                        let method_token = tokens[i];

                        // TODO: Make a InstanceInitializerDeclaration and check this in it's isExported method
                        let mut flags = 0;
                        let result_inner = metadata.get_method_props(
                            method_token,
                            None,
                            None,
                            None,
                            None,
                            Some(&mut flags),
                            None,
                            None,
                            None,
                            None,
                        );
                        debug_assert!(result_inner.is_ok());

                        if helpers::is_md_public(flags) {
                            continue;
                        }

                        result.push(MethodDeclaration::new(
                            Some(Arc::clone(&meta)),
                            method_token,
                        ));
                    }
                }
                Err(_) => {}
            }
        }

        result
    }

    pub fn make_default_interface(
        metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
        token: mdTypeDef,
    ) -> Option<Arc<RwLock<dyn BaseClassDeclarationImpl>>> {
        match Option::as_ref(&metadata) {
            None => {}
            Some(metadata) => {
                let meta = Arc::clone(metadata);

                match metadata.try_read() {
                    Ok(metadata) => {
                        let mut interface_impl_tokens = [0; 1024];
                        let mut interface_impl_count = 0;
                        let mut interface_enumerator = std::ptr::null_mut();
                        let interface_enumerator_ptr = &mut interface_enumerator;
                        let result = metadata.enum_interface_impls(
                            interface_enumerator_ptr,
                            Some(token),
                            Some(&mut interface_impl_tokens),
                            Some(interface_impl_tokens.len() as u32),
                            Some(&mut interface_impl_count),
                        );
                        debug_assert!(result.is_ok());

                        debug_assert!(interface_impl_count < interface_impl_tokens.len() as u32);
                        metadata.close_enum(interface_enumerator);
                        let attr = windows::HSTRING::from(DEFAULT_ATTRIBUTE);
                        let attr = attr.as_wide();
                        for i in 0..interface_impl_count as usize {
                            let interface_impl_token = interface_impl_tokens[i];
                            let get_custom_attribute_result = metadata
                                .get_custom_attribute_by_name(
                                    interface_impl_token,
                                    Some(attr),
                                    None,
                                    None,
                                );
                            debug_assert!(get_custom_attribute_result.is_ok());
                            if get_custom_attribute_result.is_ok() {
                                let mut interface_token = mdTokenNil;
                                let result = metadata.get_interface_impl_props(
                                    interface_impl_token,
                                    None,
                                    Some(&mut interface_token),
                                );
                                debug_assert!(result.is_ok());
                                return DeclarationFactory::make_interface_declaration(
                                    Some(Arc::clone(&meta)),
                                    interface_token,
                                );
                            }
                        }
                    }
                    Err(_) => {}
                }
            }
        }

        core_unreachable();
        None
    }

    pub fn new(metadata: Option<Arc<RwLock<IMetaDataImport2>>>, token: mdTypeDef) -> Self {
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
        }
    }

    pub fn base_full_name<'b>(&self) -> Cow<'b, str> {
        let mut name = [0_u16; MAX_IDENTIFIER_LENGTH];
        let mut count = 0;
        let mut parent_token = mdTokenNil;
        if let Some(metadata) = self.base.metadata() {
            let result = metadata.get_type_def_props(
                self.base.base().token(),
                None,
                None,
                None,
                None,
                Some(&mut parent_token),
            );
            debug_assert!(result.is_ok());
            count = helpers::get_type_name(metadata, parent_token, &mut name);
        }
        OsString::from_wide(&name[..count as usize]).to_string_lossy()
    }

    pub fn default_interface(&self) -> &InterfaceDeclaration {
        &self.default_interface
    }

    pub fn is_instantiable(&self) -> bool {
        !self.initializers.is_empty()
    }

    pub fn is_sealed(&self) -> bool {
        let mut flags = 0;
        if let Some(metadata) = self.base.base().metadata() {
            let result = metadata.get_type_def_props(
                self.base.base().token(),
                None,
                None,
                None,
                Some(&mut flags),
                None,
            );
            debug_assert!(result.is_ok());
        }
        helpers::is_td_sealed(flags)
    }

    pub fn initializers(&self) -> &[MethodDeclaration] {
        self.initializers.as_slice()
    }
}

impl Declaration for ClassDeclaration {
    fn name<'b>(&self) -> Cow<'b, str> {
        self.base.name()
    }

    fn full_name<'b>(&self) -> Cow<'b, str> {
        self.base.full_name()
    }

    fn kind(&self) -> DeclarationKind {
        self.base.kind()
    }
}
