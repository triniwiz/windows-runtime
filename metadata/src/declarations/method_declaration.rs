use std::any::Any;
use std::ptr::addr_of_mut;
use std::sync::{Arc};
use parking_lot::RwLock;
use windows::core::{HSTRING, PCWSTR, PWSTR};
use windows::Win32::System::WinRT::Metadata::{CorTokenType, IMAGE_CEE_CS_CALLCONV_GENERIC, IMetaDataImport2, mdtMethodDef};
use crate::declarations::declaration::{Declaration, DeclarationKind};
use crate::declarations::parameter_declaration::ParameterDeclaration;
use crate::declarations::type_declaration::TypeDeclaration;
use crate::signature::Signature;

use crate::prelude::*;

#[derive(Clone, Debug)]
pub struct MethodDeclaration {
    kind: DeclarationKind,
    metadata: Option<IMetaDataImport2>,
    token: CorTokenType,
    parameters: Vec<ParameterDeclaration>,
    return_type: PCCOR_SIGNATURE,
    full_name: String,
    overload_name: String,
    is_void: bool
}

const OVERLOAD_ATTRIBUTE: &str = "Windows.Foundation.Metadata.OverloadAttribute";
const DEFAULT_OVERLOAD_ATTRIBUTE: &str = "Windows.Foundation.Metadata.DefaultOverloadAttribute";

impl MethodDeclaration {
    pub fn new(metadata: Option<&IMetaDataImport2>, token: CorTokenType) -> Self {
        assert!(metadata.is_some());
        assert_eq!(type_from_token(token), mdtMethodDef.0);
        assert_ne!(token.0, 0);

        let mut parameters: Vec<ParameterDeclaration> = Vec::new();

        let mut signature =  std::ptr::null_mut() as *mut u8;
       // let signature_ptr = &mut signature;
        let mut signature_size = 0;
        let mut return_type = PCCOR_SIGNATURE::default();
        let mut full_name = String::new();
        let mut overload_name = String::new();
        let mut is_void = false;
        unsafe {
            match metadata {
                None => {}
                Some(metadata) => {
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
                    assert!(result.is_ok());
                    /*
                            #if _DEBUG
                            PCCOR_SIGNATURE startSignature{ signature };
                    #endif
                             */

                    let mut sig = PCCOR_SIGNATURE(signature);

                    if cor_sig_uncompress_calling_conv(&mut sig)
                        == IMAGE_CEE_CS_CALLCONV_GENERIC.0 as u32
                    {
                        unimplemented!()
                    }

                    let mut arguments_count =
                        { cor_sig_uncompress_data(&mut sig) };

                    return_type = Signature::consume_type(&mut sig);

                    let mut parameter_enumerator = std::ptr::null_mut();
                    // todo
                    let mut parameters_count = 0_u32;
                    let mut parameter_tokens = [0; 1024];

                    let result = metadata.EnumParams(
                        addr_of_mut!(parameter_enumerator),
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
                        let sig_type = Signature::consume_type(&mut sig);
                        parameters.push(ParameterDeclaration::new(
                            Some(metadata),
                            CorTokenType(parameter_tokens[i] as i32),
                            sig_type,
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


                    //let name = &data[0..name_length as usize];
                    full_name = String::from_utf16_lossy(&data[0..name_length.saturating_sub(1) as usize]);
                    //full_name = unsafe { PCWSTR::from_raw(name.as_ptr()).to_string().unwrap_or("".to_string()) };


                    overload_name = get_unary_custom_attribute_string_value(
                        &metadata,
                        token,
                        OVERLOAD_ATTRIBUTE,
                    );

                    is_void = Signature::to_string(metadata, &return_type) == "Void";

                    // todo
                    //debug_assert!(signature_size == signature);
                    //debug_assert!(start_signature + signature_size == signature);
                }
            }
        }

        Self {
            kind:DeclarationKind::Method,
            metadata: metadata.map(|f|f.clone()),
            token,
            parameters,
            return_type,
            full_name,
            overload_name,
            is_void
        }
    }

    pub fn metadata(&self) -> Option<&IMetaDataImport2> {
        self.metadata.as_ref()
    }

    pub fn token(&self) -> CorTokenType {
        self.token
    }

    pub fn is_void(&self) -> bool {
        self.is_void
    }

    pub fn is_initializer(&self) -> bool {
        let mut full_name_data = [0_u16; MAX_IDENTIFIER_LENGTH];
        let mut method_flags = 0;
        if let Some(metadata) = self.metadata.as_ref() {
            let result = unsafe {
                metadata.GetMethodProps(
                    self.token.0 as u32,
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
        if let Some(metadata) = self.metadata.as_ref() {
            let result = unsafe {
                metadata.GetMethodProps(
                    self.token.0 as u32,
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
        if let Some(metadata) = self.metadata.as_ref() {
            let result = unsafe {
                metadata.GetMethodProps(
                    self.token.0 as u32,
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
        if let Some(metadata) = self.metadata.as_ref() {
            let get_attribute_result =
                unsafe { metadata.GetCustomAttributeByName(self.token.0 as u32, data, 0 as _, 0 as _) };
            debug_assert!(get_attribute_result.is_ok());
            return get_attribute_result.is_ok();
        }
        false
    }

    pub fn return_type(&self) -> PCCOR_SIGNATURE {
        self.return_type
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
        if let Some(metadata) = self.metadata.as_ref() {
            let result = unsafe {
                metadata.GetMethodProps(
                    self.token.0 as u32,
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
        self.kind
    }
}