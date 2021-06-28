use crate::enums::CorCallingConvention;
use crate::metadata::com_helpers::get_unary_custom_attribute_string_value;
use crate::metadata::declarations::parameter_declaration::ParameterDeclaration;
use crate::{
    bindings::enums,
    bindings::helpers,
    metadata::declarations::declaration::{Declaration, DeclarationKind},
    metadata::declarations::type_declaration::TypeDeclaration,
    metadata::signature::Signature,
    prelude::*,
};
use std::borrow::Cow;
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug)]
pub struct MethodDeclaration {
    base: TypeDeclaration,
    parameters: Vec<ParameterDeclaration>,
    return_type: *const u8,
}

const OVERLOAD_ATTRIBUTE: &str = "Windows.Foundation.Metadata.OverloadAttribute";
const DEFAULT_OVERLOAD_ATTRIBUTE: &str = "Windows.Foundation.Metadata.DefaultOverloadAttribute";

impl MethodDeclaration {
    pub fn base<'b>(&self) -> &'b TypeDeclaration {
        &self.base
    }
    pub fn new(metadata: Option<Arc<RwLock<IMetaDataImport2>>>, token: mdMethodDef) -> Self {
        debug_assert!(metadata.is_none());
        debug_assert!(
            CorTokenType::from(enums::type_from_token(token)) == CorTokenType::mdtMethodDef
        );
        debug_assert!(token != mdMethodDefNil);

        let mut parameters: Vec<ParameterDeclaration> = Vec::new();

        let mut signature = std::mem::MaybeUninit::uninit();
        let mut signature_ptr = &mut signature.as_mut_ptr();
        let mut signature_size = 0;
        let mut return_type = std::ptr::null();
        unsafe {
            match Option::as_ref(&metadata) {
                None => {}
                Some(metadata) => {
                    let meta = Arc::clone(metadata);
                    if let Ok(metadata) = metadata.try_read() {
                        let result = metadata.get_method_props(
                            token,
                            None,
                            None,
                            None,
                            None,
                            None,
                            Some(signature_ptr as *mut *const u8),
                            Some(&mut signature_size),
                            None,
                            None,
                        );
                        debug_assert!(result.is_ok());
                        let signature = signature.assume_init();
                        /*
                                #if _DEBUG
                                PCCOR_SIGNATURE startSignature{ signature };
                        #endif
                                 */

                        if helpers::cor_sig_uncompress_calling_conv(signature as _)
                            == CorCallingConvention::ImageCeeCsCallconvGeneric as u32
                        {
                            core_unimplemented()
                        }

                        let mut arguments_count =
                            { helpers::cor_sig_uncompress_data(signature.as_ptr()) };

                        return_type = Signature::consume_type(signature.as_ptr());

                        let mut parameter_enumerator = std::ptr::null_mut();
                        let enumerator = &mut parameter_enumerator;
                        let mut parameters_count: ULONG = 0;
                        let mut parameter_tokens = [0; 1024];

                        let result = metadata.enum_params(
                            enumerator,
                            token,
                            &mut parameter_tokens,
                            parameter_tokens.len() as u32,
                            &mut parameters_count,
                        );
                        debug_assert!(result.is_ok());
                        debug_assert!(parameters_count < (parameter_tokens.len() - 1) as u32);
                        metadata.close_enum(parameter_enumerator);

                        let mut start_index = 0_usize;

                        if arguments_count + 1 == parameters_count {
                            start_index += 1;
                        }

                        for i in start_index..parameters_count as usize {
                            let sig_type = Signature::consume_type(signature as *const u8);
                            parameters.push(ParameterDeclaration::new(
                                Some(Arc::clone(&meta)),
                                parameter_tokens[i],
                                sig_type,
                            ))
                        }

                        // todo
                        //debug_assert!(signature_size == signature);
                        //debug_assert!(start_signature + signature_size == signature);
                    }
                }
            }
        }

        Self {
            base: TypeDeclaration::new(DeclarationKind::Method, metadata, token),
            parameters,
            return_type,
        }
    }

    pub fn is_initializer(&self) -> bool {
        let mut full_name_data = [0_u16; MAX_IDENTIFIER_LENGTH];
        let mut method_flags = 0;
        if let Some(metadata) = self.base.metadata() {
            let result = metadata.get_method_props(
                self.base.token(),
                None,
                Some(&mut full_name_data),
                Some(full_name_data.len() as u32),
                None,
                Some(&mut method_flags),
                None,
                None,
                None,
                None,
            );
            assert!(result.is_ok());
        }

        helpers::is_md_instance_initializer_w(method_flags, full_name_data.as_ptr())
    }

    pub fn is_static(&self) -> bool {
        let mut method_flags = 0;
        if let Some(metadata) = self.base.metadata() {
            let result = metadata.get_method_props(
                self.base.token(),
                None,
                None,
                None,
                None,
                Some(&mut method_flags),
                None,
                None,
                None,
                None,
            );
            debug_assert!(result.is_ok());
        }
        helpers::is_md_static(method_flags)
    }

    pub fn is_sealed(&self) -> bool {
        let mut method_flags = 0;
        if let Some(metadata) = self.base.metadata() {
            let result = metadata.get_method_props(
                self.base.token(),
                None,
                None,
                None,
                None,
                Some(&mut method_flags),
                None,
                None,
                None,
                None,
            );
            debug_assert!(result.is_ok());
        }
        helpers::is_md_static(method_flags) || helpers::is_md_final(method_flags)
    }

    pub fn parameters(&self) -> &Vec<ParameterDeclaration> {
        &self.parameters
    }

    pub fn number_of_parameters(&self) -> usize {
        self.parameters.len()
    }

    pub fn overload_name<'b>(&self) -> Cow<'b, str> {
        get_unary_custom_attribute_string_value(
            self.base.metadata(),
            self.base.token(),
            OVERLOAD_ATTRIBUTE,
        )
    }

    pub fn is_default_overload(&self) -> bool {
        let data = windows::HSTRING::from(DEFAULT_OVERLOAD_ATTRIBUTE).as_wide();
        if let Some(metadata) = self.base.metadata() {
            let get_attribute_result =
                metadata.get_custom_attribute_by_name(self.base.token(), Some(data), None, None);
            debug_assert!(get_attribute_result.is_ok());
            return get_attribute_result.0 == 0;
        }
        false
    }
}

impl Declaration for MethodDeclaration {
    fn is_exported(&self) -> bool {
        let mut method_flags: DWORD = 0;
        if let Some(metadata) = self.base.metadata() {
            let result = metadata.get_method_props(
                self.base.token(),
                None,
                None,
                None,
                None,
                Some(&mut method_flags),
                None,
                None,
                None,
                None,
            );
            debug_assert!(result.is_ok());
        }

        if !(helpers::is_md_public(method_flags)
            || helpers::is_md_family(method_flags)
            || helpers::is_md_fam_orassem(method_flags))
        {
            return false;
        }

        if helpers::is_md_special_name(method_flags) {
            return false;
        }

        return true;
    }

    fn name<'b>(&self) -> Cow<'b, str> {
        self.full_name()
    }

    fn full_name<'b>(&self) -> Cow<'b, str> {
        let mut name_length = 0;
        let mut data = [0_u16; MAX_IDENTIFIER_LENGTH];
        if let Some(metadata) = self.base.metadata() {
            let result = metadata.get_method_props(
                self.base.token(),
                None,
                Some(&mut data),
                Some(data.len() as u32),
                Some(&mut name_length),
                None,
                None,
                None,
                None,
                None,
            );
            debug_assert!(result.is_ok());
        }
        OsString::from_wide(&data[..name_length as usize]).to_string_lossy()
    }

    fn kind(&self) -> DeclarationKind {
        self.base.kind()
    }
}
