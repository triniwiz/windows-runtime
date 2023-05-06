use std::any::Any;
use std::sync::{Arc};
use parking_lot::RwLock;
use windows::core::{HSTRING, PCWSTR, PWSTR};
use windows::Win32::System::WinRT::Metadata::{CorTokenType, IMAGE_CEE_CS_CALLCONV_GENERIC, IMetaDataImport2, mdtMethodDef};
use crate::{cor_sig_uncompress_calling_conv, cor_sig_uncompress_data};
use crate::declarations::declaration::{Declaration, DeclarationKind};
use crate::declarations::parameter_declaration::ParameterDeclaration;
use crate::declarations::type_declaration::TypeDeclaration;
use crate::signature::Signature;

use crate::prelude::*;

#[derive(Clone, Debug)]
pub struct MethodDeclaration {
    base: TypeDeclaration,
    parameters: Vec<ParameterDeclaration>,
    return_type: Vec<u8>,
    full_name: String,
    overload_name: String,
}

const OVERLOAD_ATTRIBUTE: &str = "Windows.Foundation.Metadata.OverloadAttribute";
const DEFAULT_OVERLOAD_ATTRIBUTE: &str = "Windows.Foundation.Metadata.DefaultOverloadAttribute";

impl MethodDeclaration {
    pub fn base(&self) -> &TypeDeclaration {
        &self.base
    }
    pub fn new(metadata: Option<Arc<RwLock<IMetaDataImport2>>>, token: CorTokenType) -> Self {
        assert!(metadata.is_none());
        assert_eq!(type_from_token(token), mdtMethodDef.0);
        assert_ne!(token.0, 0);

        let mut parameters: Vec<ParameterDeclaration> = Vec::new();

        let mut signature = std::ptr::null_mut();
        let mut signature_ptr = &mut signature;
        let mut signature_size = 0;
        let mut return_type = Vec::new();
        let mut full_name = String::new();
        let mut overload_name = String::new();
        unsafe {
            match Option::as_ref(&metadata) {
                None => {}
                Some(metadata) => {
                    let meta = Arc::clone(&metadata);
                    let metadata = metadata.read();
                    let result = unsafe {
                        metadata.GetMethodProps(
                            token.0 as u32,
                            0 as _,
                            None,
                            0 as _,
                            0 as _,
                            signature_ptr,
                            &mut signature_size,
                            0 as _,
                            0 as _,
                        )
                    };
                    assert!(result.is_ok());
                    /*
                            #if _DEBUG
                            PCCOR_SIGNATURE startSignature{ signature };
                    #endif
                             */

                    let signature = std::slice::from_raw_parts(signature as *const u8, signature_size as usize);


                    if cor_sig_uncompress_calling_conv(signature as _)
                        == IMAGE_CEE_CS_CALLCONV_GENERIC.0 as u32
                    {
                        unimplemented!()
                    }

                    let mut arguments_count =
                        { cor_sig_uncompress_data(signature) };

                    return_type = Signature::consume_type(signature).to_vec();

                    let mut parameter_enumerator = std::ptr::null_mut();
                    let enumerator = &mut parameter_enumerator;
                    // todo
                    let mut parameters_count = 0_u32;
                    let mut parameter_tokens = [0; 1024];

                    let result = metadata.EnumParams(
                        enumerator,
                        token.0 as u32,
                        parameter_tokens.as_mut_ptr(),
                        parameter_tokens.len() as u32,
                        &mut parameters_count,
                    );
                    assert!(result.is_ok());
                    assert!(parameters_count < (parameter_tokens.len().saturating_sub(1)) as u32);
                    metadata.CloseEnum(parameter_enumerator);

                    let mut start_index = 0_usize;

                    if arguments_count + 1 == parameters_count {
                        start_index += 1;
                    }

                    for i in start_index..parameters_count as usize {
                        let sig_type = Signature::consume_type(signature);
                        parameters.push(ParameterDeclaration::new(
                            Some(Arc::clone(&meta)),
                            CorTokenType(parameter_tokens[i] as i32),
                            sig_type.to_vec(),
                        ))
                    }


                    let mut name_length = 0;
                    let mut data = [0_u16; MAX_IDENTIFIER_LENGTH];
                    let result = metadata.GetMethodProps(
                        token.0 as u32,
                        0 as _,
                        Some(&mut data),
                        &mut name_length,
                        0 as _,
                        0 as _,
                        0 as _,
                        0 as _,
                        0 as _,
                    );
                    debug_assert!(result.is_ok());


                    let name = &data[0..name_length as usize];
                    full_name = unsafe { PCWSTR::from_raw(name.as_ptr()).to_string().unwrap_or("".to_string()) };


                    overload_name = get_unary_custom_attribute_string_value(
                        &*metadata,
                        token,
                        OVERLOAD_ATTRIBUTE,
                    );

                    // todo
                    //debug_assert!(signature_size == signature);
                    //debug_assert!(start_signature + signature_size == signature);
                }
            }
        }


        Self {
            base: TypeDeclaration::new(DeclarationKind::Method, metadata, token),
            parameters,
            return_type,
            full_name,
            overload_name,
        }
    }

    pub fn is_initializer(&self) -> bool {
        let mut full_name_data = [0_u16; MAX_IDENTIFIER_LENGTH];
        let mut method_flags = 0;
        if let Some(metadata) = self.base.metadata() {
            let result = unsafe {
                metadata.GetMethodProps(
                    self.base.token().0 as u32,
                    0 as _,
                    Some(&mut full_name_data),
                    0 as _,
                    &mut method_flags,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                )
            };
            assert!(result.is_ok());
        }


        let name = PCWSTR(full_name_data.as_ptr());

        is_md_instance_initializer_w(method_flags as i32, &name)
    }

    pub fn is_static(&self) -> bool {
        let mut method_flags = 0;
        if let Some(metadata) = self.base.metadata() {
            let result = unsafe {
                metadata.GetMethodProps(
                    self.base.token().0 as u32,
                    0 as _,
                    None,
                    0 as _,
                    &mut method_flags,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                )
            };
            assert!(result.is_ok());
        }
        is_md_static(method_flags as i32)
    }

    pub fn is_sealed(&self) -> bool {
        let mut method_flags = 0;
        if let Some(metadata) = self.base.metadata() {
            let result = unsafe {
                metadata.GetMethodProps(
                    self.base.token().0 as u32,
                    0 as _,
                    None,
                    0 as _,
                    &mut method_flags,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                )
            };
            assert!(result.is_ok());
        }
        is_md_static(method_flags as i32) || is_md_final(method_flags as i32)
    }

    pub fn parameters(&self) -> &[ParameterDeclaration] {
        &self.parameters
    }

    pub fn number_of_parameters(&self) -> usize {
        self.parameters.len()
    }

    pub fn overload_name(&self) -> &str {
        self.overload_name.as_str()
    }

    pub fn is_default_overload(&self) -> bool {
        let data = HSTRING::from(DEFAULT_OVERLOAD_ATTRIBUTE);
        let data = PCWSTR(data.as_ptr());
        if let Some(metadata) = self.base.metadata() {
            let get_attribute_result =
                unsafe { metadata.GetCustomAttributeByName(self.base.token().0 as u32, data, 0 as _, 0 as _) };
            debug_assert!(get_attribute_result.is_ok());
            return get_attribute_result.is_ok();
        }
        false
    }
}

impl Declaration for MethodDeclaration {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn is_exported(&self) -> bool {
        let mut method_flags = 0_u32;
        if let Some(metadata) = self.base.metadata() {
            let result = unsafe {
                metadata.GetMethodProps(
                    self.base.token().0 as u32,
                    0 as _,
                    None,
                    0 as _,
                    &mut method_flags,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                )
            };
            debug_assert!(result.is_ok());
        }

        let method_flags = method_flags as i32;

        if !(is_md_public(method_flags)
            || is_md_family(method_flags)
            || is_md_fam_orassem(method_flags))
        {
            return false;
        }

        if is_md_special_name(method_flags) {
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