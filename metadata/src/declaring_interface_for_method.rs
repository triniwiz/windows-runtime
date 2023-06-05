use std::ffi::{c_void, OsString};
use std::mem::MaybeUninit;
use std::os::windows::prelude::OsStrExt;
use std::ptr::addr_of_mut;
use std::sync::Arc;
use parking_lot::RwLock;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::System::WinRT::Metadata::{CorTokenType, ELEMENT_TYPE_CLASS, ELEMENT_TYPE_GENERICINST, ELEMENT_TYPE_VOID, IMetaDataImport2, mdtMemberRef, mdtMethodDef, mdtTypeDef, mdtTypeRef, mdtTypeSpec};
use crate::declarations::base_class_declaration::BaseClassDeclarationImpl;
use crate::declarations::interface_declaration::generic_interface_instance_declaration::GenericInterfaceInstanceDeclaration;
use crate::declarations::interface_declaration::InterfaceDeclaration;
use crate::declarations::method_declaration::MethodDeclaration;
use crate::prelude::*;


pub struct Metadata {}

impl Metadata {
    pub fn get_method_containing_class_token(metadata: &IMetaDataImport2, method_token: CorTokenType) -> u32 {
        let mut class_token = 0_u32;

        match CorTokenType(type_from_token(method_token)) {
            mdtMethodDef => {
                let result = unsafe {
                    metadata.GetMethodProps(
                        method_token.0 as u32,
                        &mut class_token,
                        None,
                        0 as _,
                        0 as _,
                        0 as _,
                        0 as _,
                        0 as _,
                        0 as _,
                    )
                };
                debug_assert!(
                    result.is_ok()
                );
            }
            mdtMemberRef => {
                let result = unsafe {
                    metadata.GetMemberRefProps(
                        method_token.0 as u32,
                        &mut class_token, None, 0 as _, 0 as _, 0 as _)
                };
                debug_assert!(
                    result.is_ok()
                );
            }
            _ => {
                std::unreachable!()
            }
        }

        class_token
    }

    pub fn get_custom_attribute_constructor_token(metadata: &IMetaDataImport2, custom_attribute: CorTokenType) -> u32 {
        let mut constructor_token = 0_u32;
        let result = unsafe {
            metadata.GetCustomAttributeProps(
                custom_attribute.0 as u32,
                0 as _, &mut constructor_token,
                0 as _, 0 as _,
            )
        };
        debug_assert!(
            result.is_ok()
        );
        constructor_token
    }

    pub fn get_custom_attribute_class_token(metadata: &IMetaDataImport2, custom_attribute: CorTokenType) -> u32 {
        let token = Metadata::get_custom_attribute_constructor_token(metadata, custom_attribute);
        Metadata::get_method_containing_class_token(metadata, CorTokenType(token as i32))
    }

    pub fn get_method_signature(metadata: &IMetaDataImport2, token: CorTokenType) -> PCCOR_SIGNATURE {
        let mut signature = std::ptr::null_mut();
        let mut signature_size = 0;

        match CorTokenType(type_from_token(token)) {
            mdtMethodDef => {
                let result = unsafe {
                    metadata.GetMethodProps(
                        token.0 as u32,
                        0 as _,
                        None,
                        0 as _,
                        0 as _,
                        addr_of_mut!(signature),
                        &mut signature_size,
                        0 as _,
                        0 as _,
                    )
                };
                debug_assert!(result.is_ok());
            }
            mdtMemberRef => {
                let result = unsafe {
                    metadata.GetMemberRefProps(
                        token.0 as u32,
                        0 as _,
                        None,
                        0 as _,
                        addr_of_mut!(signature),
                        &mut signature_size,
                    )
                };
                assert!(result.is_ok());
            }
            _ => {
                std::unreachable!()
            }
        }
        unsafe { PCCOR_SIGNATURE(signature.offset(1)) }
    }

    pub fn get_signature_argument_count(metadata: &IMetaDataImport2, signature: &mut PCCOR_SIGNATURE) -> u32 {
        cor_sig_uncompress_data(signature)
    }

    pub fn get_method_argument_count(metadata: &IMetaDataImport2, token: CorTokenType) -> u32 {
        let mut signature = Metadata::get_method_signature(metadata, token);
        Metadata::get_signature_argument_count(metadata, &mut signature)
    }

    pub fn get_custom_attributes_with_name(metadata: &IMetaDataImport2, token: CorTokenType, attribute_name: &str) -> Vec<u32> {
        // mdCustomAttribute
        let mut attributes = [0_u32; 512];
        let mut attributes_count = 0;
        let mut attributes_enumerator = std::ptr::null_mut();


        let result = unsafe {
            metadata.EnumCustomAttributes(
                addr_of_mut!(attributes_enumerator),
                token.0 as u32,
                0 as _,
                attributes.as_mut_ptr(),
                attributes.len() as u32,
                &mut attributes_count,
            )
        };

        debug_assert!(
            result.is_ok()
        );
        debug_assert!(
            attributes_count < (attributes.len().saturating_sub(1)) as u32
        );

        unsafe {
            metadata.CloseEnum(attributes_enumerator)
        }


        let mut filtered_attributes: Vec<u32> = Vec::new();
        for i in 0..attributes_count as usize {
            let attribute = attributes[i];
            let class_attribute_class_token = Metadata::get_custom_attribute_class_token(metadata, CorTokenType(attribute as i32));
            // let mut name = [0_u16; MAX_IDENTIFIER_LENGTH];
            let class_attribute_class_name = get_type_name(metadata, CorTokenType(class_attribute_class_token as i32));
            //let mut class_attribute_class_name = windows::HSTRING::from_wide(name[..length]);
            // TODO
            if class_attribute_class_name.as_str() != attribute_name {
                continue;
            }

            filtered_attributes.push(
                attribute
            );
        }

        filtered_attributes
    }

    pub fn get_custom_attribute_type_argument(metadata: &IMetaDataImport2, token: CorTokenType) -> u32 {
        let mut attribute_value = std::ptr::null_mut() as *const c_void;
        let mut attribute_value_size = 0_u32;

        let result = unsafe {
            metadata.GetCustomAttributeProps(
                token.0 as u32,
                0 as _,
                0 as _,
                addr_of_mut!(attribute_value),
                &mut attribute_value_size,
            )
        };

        debug_assert!(result.is_ok());

        let attribute_value = attribute_value as *mut u8;
        let signature = unsafe { PCCOR_SIGNATURE((attribute_value as *mut u8).offset(2)) };


        let type_name = get_string_value_from_blob(
            &signature
        );

        let type_name = HSTRING::from(type_name);
        let type_name = PCWSTR(type_name.as_ptr());

        let mut type_token = 0_u32;

        let result = unsafe {
            metadata.FindTypeDefByName(
                type_name,
                0 as _,
                &mut type_token,
            )
        };

        debug_assert!(result.is_ok());
        type_token
    }

    pub fn get_class_methods(metadata: &IMetaDataImport2, token: CorTokenType) -> Vec<u32> {
        let mut methods = [0_u32; 1024];
        let mut methods_count = 0;
        let mut methods_enumerator = std::ptr::null_mut();

        let result = unsafe {
            metadata.EnumMethods(
                addr_of_mut!(methods_enumerator),
                token.0 as u32,
                methods.as_mut_ptr(),
                methods.len() as u32,
                &mut methods_count,
            )
        };
        debug_assert!(
            result.is_ok()
        );
        debug_assert!(
            methods_count < (methods.len().saturating_sub(1)) as u32
        );
        unsafe {
            metadata.CloseEnum(methods_enumerator)
        }

        methods[..methods_count as usize].to_vec()
    }

    pub fn has_method_first_type_argument(metadata: &IMetaDataImport2, token: CorTokenType) -> bool {
        let mut signature = Metadata::get_method_signature(
            metadata, token,
        );
        let argument_count = cor_sig_uncompress_data(&mut signature);

        if argument_count == 0 {
            return false;
        }

        let return_type = cor_sig_uncompress_element_type(&mut signature);

        debug_assert!(
            return_type == ELEMENT_TYPE_VOID
        );


        let first_argument = cor_sig_uncompress_element_type(&mut signature);

        if first_argument != ELEMENT_TYPE_CLASS {
            return false;
        }

        let first_argument_token = cor_sig_uncompress_token(&mut signature);
        let first_argument_type_name = get_type_name(
            metadata, CorTokenType(first_argument_token as i32),
        );

        if first_argument_type_name != SYSTEM_TYPE {
            return false;
        }

        return true;
    }

    pub fn declaring_interface_for_initializer(metadata: Option<&IMetaDataImport2>, method_token: CorTokenType, out_index: &mut usize) -> Option<Arc<RwLock<dyn BaseClassDeclarationImpl>>> {
        // InterfaceDeclaration

        match metadata {
            None => None,
            Some(metadata) => {

                let method_argument_count = Metadata::get_method_argument_count(metadata, method_token);

                let class_token = Metadata::get_method_containing_class_token(metadata, method_token);

                debug_assert!(
                    CorTokenType(type_from_token(CorTokenType(class_token as i32))) == mdtTypeDef
                );

                let composable_attributes = Metadata::get_custom_attributes_with_name(
                    metadata, CorTokenType(class_token as i32), COMPOSABLE_ATTRIBUTE,
                );

                for attributeToken in composable_attributes.iter() {
                    let attribute_token = CorTokenType(*attributeToken as i32);
                    let factory_token = Metadata::get_custom_attribute_type_argument(
                        metadata, attribute_token,
                    );

                    let factory_methods = Metadata::get_class_methods(
                        metadata, attribute_token,
                    );

                    for (i, factoryMethod) in factory_methods.iter().enumerate() {
                        let factory_method_arguments_count = Metadata::get_method_argument_count(metadata, CorTokenType(*factoryMethod as i32));
                        if factory_method_arguments_count.saturating_sub(2) != method_argument_count {
                            continue;
                        }

                        *out_index = i;
                        return Some(
                            Arc::new(
                                RwLock::new(
                                    InterfaceDeclaration::new(Some(metadata), CorTokenType(factory_token as i32))
                                )
                            )
                        );
                    }
                }

                if method_argument_count == 0 {
                    *out_index = usize::MAX;
                    return None;
                }


                let mut activatable_attributes = Metadata::get_custom_attributes_with_name(
                    metadata, CorTokenType(class_token as i32), ACTIVATABLE_ATTRIBUTE,
                );

                for attributeToken in activatable_attributes.into_iter() {
                    let attribute_constructor_token = Metadata::get_custom_attribute_constructor_token(
                        metadata, CorTokenType(attributeToken as i32),
                    );

                    if !Metadata::has_method_first_type_argument(metadata, CorTokenType(attribute_constructor_token as i32)) {
                        continue;
                    }

                    let factory_token = Metadata::get_custom_attribute_type_argument(metadata, CorTokenType(attributeToken as i32));


                    let factory_methods = Metadata::get_class_methods(
                        metadata, CorTokenType(factory_token as i32),
                    );

                    for (i, factory_method) in factory_methods.iter().enumerate() {
                        let factory_method_arguments_count = Metadata::get_method_argument_count(
                            metadata, CorTokenType(*factory_method as i32),
                        );

                        if factory_method_arguments_count != method_argument_count {
                            continue;
                        }


                        *out_index = i;
                        return Some(
                            Arc::new(
                                RwLock::new(
                                    InterfaceDeclaration::new(Some(metadata), CorTokenType(factory_token as i32))
                                )
                            )
                        );
                    }
                }

                std::unreachable!();
            }
        }
    }

    pub fn declaring_interface_for_static_method(metadata: Option<&IMetaDataImport2>, method_token: CorTokenType, out_index: &mut usize) -> Option<Arc<RwLock<dyn BaseClassDeclarationImpl>>> {
        match metadata {
            None => None,
            Some(metadata) => {
                let method_signature = Metadata::get_method_signature(
                    metadata, method_token,
                );
                let class_token = Metadata::get_method_containing_class_token(
                    metadata, method_token,
                );
                debug_assert!(
                    CorTokenType(type_from_token(CorTokenType(class_token as i32))) == mdtTypeDef
                );

                let static_attributes = Metadata::get_custom_attributes_with_name(
                    metadata, CorTokenType(class_token as i32), STATIC_ATTRIBUTE,
                );


                let mut ret: Option<Arc<RwLock<dyn BaseClassDeclarationImpl>>> = None;
                for attributeToken in static_attributes.iter() {
                    let statics_token = Metadata::get_custom_attribute_type_argument(
                        metadata, CorTokenType(*attributeToken as i32),
                    );

                    let static_methods = Metadata::get_class_methods(
                        metadata, CorTokenType(statics_token as i32),
                    );

                    for (i, staticMethod) in static_methods.iter().enumerate() {
                        let mut static_signature = std::ptr::null_mut();
                        let mut static_signature_size = 0_u32;
                        let result = unsafe {
                            metadata.GetMethodProps(
                                *staticMethod,
                                0 as _,
                                None,
                                0 as _,
                                0 as _,
                                addr_of_mut!(static_signature),
                                &mut static_signature_size,
                                0 as _,
                                0 as _,
                            )
                        };
                        debug_assert!(result.is_ok());
                        //let static_signature: &[u8] = &static_signature[..static_signature_size as usize];

                        // todo validate size is valid;

                        let a = unsafe {
                            std::slice::from_raw_parts(
                                static_signature.offset(1),
                                static_signature_size as usize -1,
                            )
                        };

                        let b = unsafe {
                            std::slice::from_raw_parts(
                                method_signature.0,
                                static_signature_size as usize - 1,
                            )
                        };

                        if a != b {
                            continue;
                        }
                        *out_index = i;
                        ret = Some(Arc::new(RwLock::new(InterfaceDeclaration::new(Some(metadata), CorTokenType(statics_token as i32)))));
                        break;
                    }
                }

                if ret.is_some() {
                    return ret;
                }

                std::unreachable!();
            }
        }
    }

    pub fn find_method_index(metadata: &IMetaDataImport2, class_token: CorTokenType, method_token: CorTokenType) -> usize {
        let mut first_method = 0_u32;

        let mut methods_enumerator = std::ptr::null_mut();

        let result = unsafe {
            metadata.EnumMethods(
                addr_of_mut!(methods_enumerator),
                class_token.0 as u32,
                &mut first_method,
                1,
                0 as _,
            )
        };
        debug_assert!(
            result.is_ok()
        );

        unsafe {
            metadata.CloseEnum(methods_enumerator)
        }

        return (method_token.0 as u32 - first_method) as usize;
    }

    pub fn declaring_interface_for_instance_method(metadata: Option<&IMetaDataImport2>, method_token: CorTokenType, out_index: &mut usize) -> Option<Arc<RwLock<dyn BaseClassDeclarationImpl>>> {
        match metadata {
            None => None,
            Some(metadata) => {
                let class_token = Metadata::get_method_containing_class_token(
                    metadata, method_token,
                );
                debug_assert!(
                    type_from_token(CorTokenType(class_token as i32)) == mdtTypeDef.0
                );

                let mut method_body_tokens = [0_u32; 1024];
                let mut method_decl_tokens = [0_u32; 1024];
                let mut method_impls_count = 0;
                let mut method_impls_enumerator = std::ptr::null_mut();

                let mut result = unsafe {
                    metadata.EnumMethodImpls(
                        addr_of_mut!(method_impls_enumerator),
                        class_token,
                        method_body_tokens.as_mut_ptr(),
                        method_decl_tokens.as_mut_ptr(),
                        method_body_tokens.len() as u32,
                        &mut method_impls_count,
                    )
                };

                println!("{}",method_impls_count);

                debug_assert!(
                    result.is_ok()
                );

                debug_assert!(
                    method_impls_count < (method_body_tokens.len().saturating_sub(1)) as u32
                );

                unsafe {
                    metadata.CloseEnum(method_impls_enumerator)
                }

                let mut ret: Option<Arc<RwLock<dyn BaseClassDeclarationImpl>>> = None;
                for i in 0..method_impls_count as usize {
                    let method_body_token = method_body_tokens[i];
                    debug_assert!(
                        CorTokenType(type_from_token(CorTokenType(method_body_token as i32))) == mdtMethodDef
                    );

                    if method_token != CorTokenType(method_body_token as i32) {
                        continue;
                    }

                    let method_decl_token = method_decl_tokens[i];
                    match CorTokenType(type_from_token(CorTokenType(method_decl_token as i32))) {
                        mdtMethodDef => {
                            let mut declaring_interface_token = 0_u32;
                            let result = unsafe {
                                metadata.GetMethodProps(
                                    method_decl_token,
                                    &mut declaring_interface_token,
                                    None,
                                    0 as _,
                                    0 as _,
                                    0 as _,
                                    0 as _,
                                    0 as _,
                                    0 as _,
                                )
                            };
                            debug_assert!(
                                result.is_ok()
                            );

                            *out_index = Metadata::find_method_index(
                                metadata, CorTokenType(declaring_interface_token as i32), CorTokenType(method_decl_token as i32),
                            );
                            ret = Some(
                                Arc::new(
                                    RwLock::new(
                                        InterfaceDeclaration::new(Some(metadata), CorTokenType(declaring_interface_token as i32))
                                    )
                                )
                            );
                            break;
                        }
                        mdtMemberRef => {
                            let mut parent_token = 0_u32;
                            let result = unsafe {
                                metadata.GetMemberRefProps(
                                    method_decl_token, &mut parent_token, None, 0 as _, 0 as _, 0 as _,
                                )
                            };
                            debug_assert!(
                                result.is_ok()
                            );

                            match CorTokenType(type_from_token(CorTokenType(parent_token as i32))) {
                                mdtTypeRef => {
                                    let mut external_metadata = MaybeUninit::zeroed();
                                    let mut declaring_interface_token = CorTokenType::default();
                                    let is_resolved = unsafe {
                                        resolve_type_ref(
                                            Some(metadata), CorTokenType(parent_token as i32), std::mem::transmute(external_metadata.as_mut_ptr()),&mut declaring_interface_token,
                                        )
                                    };
                                    debug_assert!(
                                        is_resolved
                                    );

                                    let mut external_metadata: IMetaDataImport2 = unsafe { external_metadata.assume_init() };

                                    let mut declaring_method_name = [0_u16; MAX_IDENTIFIER_LENGTH];
                                    let mut declaring_method_name_length = 0_u32;
                                    let mut signature = std::ptr::null_mut();
                                    let mut signature_size = 0;
                                    let result = unsafe {
                                        metadata.GetMemberRefProps(
                                            method_decl_token, 0 as _,
                                            Some(declaring_method_name.as_mut_slice()),
                                            &mut declaring_method_name_length,
                                            addr_of_mut!(signature),
                                            &mut signature_size,
                                        )
                                    };
                                    debug_assert!(
                                        result.is_ok()
                                    );
                                    let mut declaring_method = 0_u32;

                                    let declaring_method_name = String::from_utf16_lossy(&declaring_method_name[..declaring_method_name_length as usize]);
                                    let declaring_method_name = HSTRING::from(declaring_method_name);
                                    let declaring_method_name = PCWSTR(declaring_method_name.as_ptr());
                                    let result = unsafe {
                                        external_metadata.FindMethod(
                                            declaring_interface_token.0 as u32,
                                            declaring_method_name,
                                            signature,
                                            signature_size,
                                            &mut declaring_method,
                                        )
                                    };

                                    debug_assert!(result.is_ok());

                                    *out_index = Metadata::find_method_index(
                                        &mut external_metadata, declaring_interface_token, CorTokenType(declaring_method as i32),
                                    );

                                    ret = Some(
                                        Arc::new(
                                            RwLock::new(
                                                InterfaceDeclaration::new(
                                                    Some(
                                                        &external_metadata
                                                    ), declaring_interface_token)
                                            )
                                        )
                                    );
                                }
                                mdtTypeSpec => {
                                    let mut type_spec_signature = std::ptr::null_mut();
                                    let mut type_spec_signature_size = 0_u32;

                                    let result = unsafe {
                                        metadata.GetTypeSpecFromToken(
                                            parent_token, addr_of_mut!(type_spec_signature), &mut type_spec_signature_size,
                                        )
                                    };

                                    debug_assert!(
                                        result.is_ok()
                                    );

                                    let mut declaring_method_name = [0_u16; MAX_IDENTIFIER_LENGTH];
                                    let mut declaring_method_name_size = 0_u32;
                                    let mut signature = std::ptr::null_mut();
                                    let mut signature_size = 0_u32;

                                    let result = unsafe {
                                        metadata.GetMemberRefProps(
                                            method_decl_token,
                                            0 as _,
                                            Some(declaring_method_name.as_mut_slice()),
                                            &mut declaring_method_name_size,
                                            addr_of_mut!(signature),
                                            &mut signature_size,
                                        )
                                    };

                                    debug_assert!(
                                        result.is_ok()
                                    );

                                    let declaring_method_name = String::from_utf16_lossy(&declaring_method_name[..declaring_method_name_size as usize]);
                                    let declaring_method_name = HSTRING::from(declaring_method_name);
                                    let declaring_method_name = PCWSTR(declaring_method_name.as_ptr());

                                    let mut type_spec_signature = PCCOR_SIGNATURE(type_spec_signature);

                                    let type1 = cor_sig_uncompress_element_type(&mut type_spec_signature);
                                    debug_assert!(
                                        type1 == ELEMENT_TYPE_GENERICINST
                                    );

                                    let type2 = cor_sig_uncompress_element_type(&mut type_spec_signature);
                                    debug_assert!(
                                        type2 == ELEMENT_TYPE_CLASS
                                    );

                                    // TODO: Use signature in matching
                                    let open_generic_class_token = cor_sig_uncompress_token(&mut type_spec_signature);
                                    match CorTokenType(type_from_token(CorTokenType(open_generic_class_token as i32))) {
                                        mdtTypeDef => {
                                            let mut declaring_method = 0_u32;
                                            let result = unsafe {
                                                metadata.FindMethod(
                                                    open_generic_class_token,
                                                    declaring_method_name,
                                                    0 as _,
                                                    0 as _,
                                                    &mut declaring_method,
                                                )
                                            };
                                            debug_assert!(result.is_ok());
                                            *out_index = Metadata::find_method_index(metadata, CorTokenType(open_generic_class_token as i32), CorTokenType::default());

                                            ret = Some(
                                                Arc::new(
                                                    RwLock::new(
                                                        GenericInterfaceInstanceDeclaration::new(
                                                            Some(metadata), CorTokenType(open_generic_class_token as i32), Some(metadata), CorTokenType(parent_token as i32),
                                                        )
                                                    )
                                                )
                                            );
                                        }
                                        mdtTypeRef => {
                                            let mut external_metadata = MaybeUninit::zeroed();
                                            let mut external_class_token = CorTokenType::default();

                                            let is_resolved = unsafe {
                                                resolve_type_ref(
                                                    Some(metadata), CorTokenType(open_generic_class_token as i32), std::mem::transmute(external_metadata.as_mut_ptr()), &mut external_class_token,
                                                )
                                            };
                                            debug_assert!(
                                                is_resolved
                                            );

                                            let mut external_metadata: IMetaDataImport2 = unsafe { external_metadata.assume_init() };

                                            let mut declaring_method = 0_u32;

                                            let result = unsafe {
                                                external_metadata.FindMethod(
                                                    external_class_token.0 as u32,
                                                    declaring_method_name,
                                                    0 as _,
                                                    0 as _,
                                                    &mut declaring_method,
                                                )
                                            };

                                            debug_assert!(
                                                result.is_ok()
                                            );

                                            *out_index = Metadata::find_method_index(&mut external_metadata, external_class_token, CorTokenType(declaring_method as i32));
                                            ret = Some(
                                                Arc::new(
                                                    RwLock::new(
                                                        GenericInterfaceInstanceDeclaration::new(Some(
                                                            &external_metadata
                                                        ), external_class_token, Some(metadata), CorTokenType(parent_token as i32))
                                                    )
                                                )
                                            );
                                        }
                                        _ => {
                                            std::unreachable!()
                                        }
                                    }
                                }
                                _ => {
                                    std::unreachable!()
                                }
                            }
                        }
                        _ => {
                            std::unreachable!();
                        }
                    }
                };
                ret
            }
        }
    }

    pub fn find_declaring_interface_for_method(method: &MethodDeclaration, out_index: &mut usize) -> Option<Arc<RwLock<dyn BaseClassDeclarationImpl>>> {
        debug_assert!(
            *out_index == 0
        );
        let method_token = method.token();

        if method.is_static() {
            return Metadata::declaring_interface_for_static_method(
                method.metadata(), method_token, out_index,
            );
        }

        if method.is_initializer() {
            return Metadata::declaring_interface_for_initializer(
                method.metadata(), method_token, out_index,
            )
        }

        Metadata::declaring_interface_for_instance_method(
            method.metadata(), method_token, out_index,
        )
    }
}