#![allow(non_upper_case_globals)]

use windows::s;
use windows::Win32::System::WinRT::Metadata::{ELEMENT_TYPE_VOID, ELEMENT_TYPE_BOOLEAN, ELEMENT_TYPE_CHAR, ELEMENT_TYPE_I1, ELEMENT_TYPE_U1, ELEMENT_TYPE_I2, ELEMENT_TYPE_U2, ELEMENT_TYPE_I4, ELEMENT_TYPE_U4, ELEMENT_TYPE_I8, ELEMENT_TYPE_U8, ELEMENT_TYPE_R4, ELEMENT_TYPE_R8, ELEMENT_TYPE_STRING, IMetaDataImport2, ELEMENT_TYPE_VALUETYPE, ELEMENT_TYPE_CLASS, ELEMENT_TYPE_OBJECT, ELEMENT_TYPE_SZARRAY, ELEMENT_TYPE_VAR, ELEMENT_TYPE_GENERICINST, ELEMENT_TYPE_BYREF, CorTokenType, CorElementType};
use crate::{cor_sig_uncompress_data, cor_sig_uncompress_element_type, cor_sig_uncompress_element_type_raw, cor_sig_uncompress_token};
use crate::prelude::get_type_name;

const Guid: &str = "Guid";

pub struct Signature {}

impl Signature {
    pub fn consume_type(signature: &[u8]) -> &[u8] {
        let start = signature;
        let element_type = cor_sig_uncompress_element_type(signature);
        match CorElementType(element_type) {
            ELEMENT_TYPE_VOID
            | ELEMENT_TYPE_BOOLEAN
            | ELEMENT_TYPE_CHAR
            | ELEMENT_TYPE_I1
            | ELEMENT_TYPE_U1
            | ELEMENT_TYPE_I2
            | ELEMENT_TYPE_U2
            | ELEMENT_TYPE_I4
            | ELEMENT_TYPE_U4
            | ELEMENT_TYPE_I8
            | ELEMENT_TYPE_U8
            | ELEMENT_TYPE_R4
            | ELEMENT_TYPE_R8
            | ELEMENT_TYPE_STRING => start,
            ELEMENT_TYPE_VALUETYPE => {
                cor_sig_uncompress_token(signature);
                start
            }
            ELEMENT_TYPE_CLASS => {
                cor_sig_uncompress_token(signature);
                start
            }
            ELEMENT_TYPE_OBJECT => start,
            ELEMENT_TYPE_SZARRAY => {
                Signature::consume_type(signature);
                start
            }
            ELEMENT_TYPE_VAR => {
                cor_sig_uncompress_data(signature);
                start
            }
            ELEMENT_TYPE_GENERICINST => {
                cor_sig_uncompress_element_type(signature);
                cor_sig_uncompress_token(signature);

                let generic_arguments_count = cor_sig_uncompress_data(signature);
                for _ in 0..generic_arguments_count {
                    Signature::consume_type(signature);
                }
                start
            }
            ELEMENT_TYPE_BYREF => {
                Signature::consume_type(signature);
                start
            }
            _ => {
                unreachable!()
            }
        }
    }

    pub fn to_string(metadata: &IMetaDataImport2, signature: &[u8]) -> String {
        let element_type = cor_sig_uncompress_element_type(signature);
        return match CorElementType(element_type) {
            ELEMENT_TYPE_VOID => "Void".to_string(),
            ELEMENT_TYPE_BOOLEAN => "Boolean".to_string(),
            ELEMENT_TYPE_CHAR => "Char16".to_string(),
            ELEMENT_TYPE_I1 => "Int8".to_string(),
            ELEMENT_TYPE_U1 => "UInt8".to_string(),
            ELEMENT_TYPE_I2 => "Int16".to_string(),
            ELEMENT_TYPE_U2 => "UInt16".to_string(),
            ELEMENT_TYPE_I4 => "Int32".to_string(),
            ELEMENT_TYPE_U4 => "UInt32".to_string(),
            ELEMENT_TYPE_I8 => "Int64".to_string(),
            ELEMENT_TYPE_U8 => "UInt64".to_string(),
            ELEMENT_TYPE_R4 => "Single".to_string(),
            ELEMENT_TYPE_R8 => "Double".to_string(),
            ELEMENT_TYPE_STRING => "String".to_string(),
            ELEMENT_TYPE_VALUETYPE => {
                let token = cor_sig_uncompress_token(signature);
                let class_name = get_type_name(metadata, CorTokenType(token));

                if class_name.eq("System.Guid") {
                    Guid.to_string()
                } else {
                    class_name.to_string()
                }
            }
            ELEMENT_TYPE_CLASS => {
                let token = cor_sig_uncompress_token(signature);
                get_type_name(metadata, CorTokenType(token))
            }
            ELEMENT_TYPE_OBJECT => "Object".to_string(),
            ELEMENT_TYPE_SZARRAY => {
                let result = Signature::to_string(metadata, signature);
                format!("{}[]", result)
            }
            ELEMENT_TYPE_VAR => {
                let index = cor_sig_uncompress_data(signature);
                format!("Var!{}", index)
            }
            ELEMENT_TYPE_GENERICINST => {
                let generic_type = cor_sig_uncompress_element_type(signature);
                assert_eq!(generic_type, ELEMENT_TYPE_CLASS.0);

                let token = cor_sig_uncompress_token(signature);

                let mut result = get_type_name(metadata, CorTokenType(token));

                result += "<";

                let generic_arguments_count = cor_sig_uncompress_data(signature);
                for i in 0..generic_arguments_count {
                    let sig_type = Signature::consume_type(signature);
                    let data = Signature::to_string(metadata, sig_type);
                    result += data.as_ref();
                    if i == generic_arguments_count - 1 {
                        result += ", ";
                    }
                }

                result += ">";

                result
            }
            ELEMENT_TYPE_BYREF => {
                let mut result = "ByRef ".to_string();
                result += Signature::to_string(metadata, signature).as_ref();
                result
            }
            _ => {
                unreachable!()
            }
        };
    }
}