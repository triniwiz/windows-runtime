use crate::prelude::*;
use crate::{
    metadata::declarations::method_declaration::MethodDeclaration,
    metadata::declarations::interface_declaration::InterfaceDeclaration
};

use crate::bindings::{helpers, imeta_data_import2};
use std::str::FromStr;
use crate::metadata::declaration_factory::DeclarationFactory;
use std::sync::Arc;
use crate::metadata::declarations::base_class_declaration::BaseClassDeclaration;
use crate::metadata::declarations::declaration::DeclarationKind;


const DEFAULT_ATTRIBUTE:&'static str = "Windows.Foundation.Metadata.DefaultAttribute";

pub struct ClassDeclaration<'a> {
    initializers: Vec<MethodDeclaration<'a>>,
    default_interface: InterfaceDeclaration<'a>,
    base: BaseClassDeclaration<'a>
}

impl ClassDeclaration {

    fn make_initializer_declarations(metadata: *mut c_void, token: mdTypeDef) -> Vec<MethodDeclaration>{
        let enumerator = std::ptr::null_mut();
        let mut count = 0;
        let mut tokens:Vec<mdProperty> = vec![0;1024];

        let name = OsString::from_str(COR_CTOR_METHOD_NAME).unwrap();
        let name = name.to_wide();
        debug_assert!(
            imeta_data_import2::enum_methods_with_name(
                metadata, enumerator, token, name.as_ptr(), tokens.as_mut_ptr(),
                tokens.len(), &mut count
            ).is_ok()
        );

         debug_assert!(count < tokens.len() - 1);

        imeta_data_import2::close_enum(metadata, enumerator);

        let mut result = Vec::new();
        for i in 0..count {
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

    fn make_default_interface(metadata: *mut c_void, token: mdTypeDef) -> Option<Arc<>> {
        let mut interface_impl_tokens = vec![0_1024];
        let mut interface_impl_count = 0;
        let interface_enumerator = std::ptr::null_mut();
        debug_assert!(
            imeta_data_import2::enum_interface_impls(
                metadata, interface_enumerator, token,
                Some(interface_impl_tokens.as_mut_ptr()), Some(interface_impl_tokens.len()),
                Some(&mut interface_impl_count)
            ).is_ok()
        );

        debug_assert!(interface_impl_count < interface_impl_tokens.size());
        imeta_data_import2::close_enum(metadata, interface_enumerator);
        let attr = OsString::from_str(DEFAULT_ATTRIBUTE).unwrap().to_wide();
        for i in 0..interface_impl_count {
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
                return Some(
                    Arc::new(
                        DeclarationFactory::make_interface_declaration(metadata, interface_token)
                    )
                )
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

    pub fn base_full_name(&self) -> &str {
        let mut parentToken = mdTokenNil;
        debug_assert!(
         imeta_data_import2::get_type_def_props(
             self.base
         )
        );
        ASSERT_SUCCESS(_metadata->GetTypeDefProps(_token, nullptr, 0, nullptr, nullptr, &parentToken));

        return getTypeName(_metadata.Get(), parentToken);
    }

    pub fn default_interface(&self) -> &InterfaceDeclaration{
        &self.default_interface
    }

    pub fn is_instantiable(&self) -> bool {}

    pub fn is_sealed(&self) -> bool {}

    pub fn initializers(&self) -> {
       // self.initializers.iter()
    }
}