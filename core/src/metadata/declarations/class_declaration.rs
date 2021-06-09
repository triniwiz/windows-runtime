use std::borrow::Cow;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use crate::{
    metadata::declarations::method_declaration::MethodDeclaration
};
use crate::bindings::{helpers, imeta_data_import2};
use crate::bindings::helpers::get_type_name;
use crate::metadata::declaration_factory::DeclarationFactory;
use crate::metadata::declarations::base_class_declaration::{BaseClassDeclaration, BaseClassDeclarationImpl};
use crate::metadata::declarations::declaration::{Declaration, DeclarationKind};
use crate::prelude::*;
use crate::metadata::declarations::interface_declaration::InterfaceDeclaration;

const DEFAULT_ATTRIBUTE:&'static str = "Windows.Foundation.Metadata.DefaultAttribute";

#[derive(Clone)]
pub struct ClassDeclaration<'a> {
    initializers: Vec<MethodDeclaration<'a>>,
    default_interface: Option<Arc<dyn BaseClassDeclarationImpl>>,
    base: BaseClassDeclaration<'a>
}

impl<'a> ClassDeclaration<'a> {

    fn make_initializer_declarations<'b>(metadata: *mut c_void, token: mdTypeDef) -> Vec<MethodDeclaration<'b>>{
        let mut enumerator = std::mem::MaybeUninit::uninit();
        let enumerator_ptr = &mut enumerator;
        let mut count = 0;
        let mut tokens = [0;1024];

        let name = windows::HSTRING::from(COR_CTOR_METHOD_NAME);
        let name = name.as_wide();
        debug_assert!(
            imeta_data_import2::enum_methods_with_name(
                metadata, enumerator_ptr, token, name.as_ptr(), tokens.as_mut_ptr(),
                tokens.len() as u32, &mut count
            ).is_ok()
        );

         debug_assert!(count < (tokens.len() - 1) as u32);

        imeta_data_import2::close_enum(metadata, enumerator);

        let mut result = Vec::new();

        for i in 0..count as usize {
            let method_token = tokens[i];

            // TODO: Make a InstanceInitializerDeclaration and check this in it's isExported method
            let mut flags = 0;
            debug_assert!(
                imeta_data_import2::get_method_props(
                    metadata, method_token, None, None,
                    None, None, Some(&mut flags), None, None,
                    None, None
                ).is_ok()
            );

            if helpers::is_md_public(flags) {
                continue;
            }

            result.push(
                MethodDeclaration::new(metadata, method_token)
            );
        }

        return result;
    }

    fn make_default_interface(metadata: *mut c_void, token: mdTypeDef) -> Arc<Mutex<dyn BaseClassDeclarationImpl>> {
        let mut interface_impl_tokens = [0;1024];
        let mut interface_impl_count = 0;
        let mut interface_enumerator = std::mem::MaybeUninit::uninit();
        let interface_enumerator_ptr = &mut interface_enumerator;
        debug_assert!(
            imeta_data_import2::enum_interface_impls(
                metadata, interface_enumerator_ptr, Some(token),
                Some(interface_impl_tokens.as_mut_ptr()), Some(interface_impl_tokens.len() as u32),
                Some(&mut interface_impl_count)
            ).is_ok()
        );

        debug_assert!(interface_impl_count < interface_impl_tokens.len() as u32);
        imeta_data_import2::close_enum(metadata, interface_enumerator);
        let attr = windows::HSTRING::from(DEFAULT_ATTRIBUTE);
        let attr = attr.as_wide();
        for i in 0..interface_impl_count as usize {
           let interface_impl_token = interface_impl_tokens[i];
            let get_custom_attribute_result = imeta_data_import2::get_custom_attribute_by_name(
                metadata, interface_impl_token, Some(attr.as_ptr()), None, None
            );
            debug_assert!(get_custom_attribute_result.is_ok());
            if get_custom_attribute_result.is_ok() {
               let mut interface_token = mdTokenNil;
                debug_assert!(
                    imeta_data_import2::get_interface_impl_props(
                        metadata, interface_impl_token, None, Some(&mut interface_token)
                    ).is_ok()
                );
                return DeclarationFactory::make_interface_declaration(metadata, interface_token)
            }
        }

        std::unreachable!()
    }

    pub fn new(metadata: *mut c_void, token: mdTypeDef)-> Self {
        Self {
            initializers: ClassDeclaration::make_initializer_declarations(metadata, token),
            default_interface: ClassDeclaration::make_default_interface(metadata, token),
            base: BaseClassDeclaration::new(
                DeclarationKind::Class, metadata, token
            )
        }
    }

    pub fn base_full_name<'b>(&self) -> Cow<'b, str> {
        let mut parent_token = mdTokenNil;
        let base = self.base.base();
        debug_assert!(
         imeta_data_import2::get_type_def_props(
             base.metadata,
             base.token,
             None, None,None,None,
             Some(&mut parent_token)
         ).is_ok()
        );
        let mut name = [0_u16; MAX_IDENTIFIER_LENGTH];
        let count = get_type_name(base.metadata, parent_token, name.as_mut_ptr(), name.len() as u32);
        OsString::from_wide(&name[..count as usize]).to_string_lossy()
    }

    pub fn default_interface(&self) -> &InterfaceDeclaration{
        &self.default_interface
    }

    pub fn is_instantiable(&self) -> bool {
        !self.initializers.is_empty()
    }

    pub fn is_sealed(&self) -> bool {
        let mut flags = 0;
        let base = self.base.base();
        debug_assert!(
            imeta_data_import2::get_type_def_props(
                base.metadata, base.token, None, None, None, Some(&mut flags), None
            ).is_ok()
        );
        helpers::is_td_sealed(flags)
    }

    pub fn initializers(&self) -> &[MethodDeclaration<'a>] {
       self.initializers.as_slice()
    }
}


impl <'a> Declaration for ClassDeclaration <'a> {
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