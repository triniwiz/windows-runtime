#![allow(non_upper_case_globals)]

use std::ffi::c_void;
use std::mem::MaybeUninit;
use windows::Win32::System::WinRT::Metadata::{ELEMENT_TYPE_VOID, ELEMENT_TYPE_BOOLEAN, ELEMENT_TYPE_CHAR, ELEMENT_TYPE_I1, ELEMENT_TYPE_U1, ELEMENT_TYPE_I2, ELEMENT_TYPE_U2, ELEMENT_TYPE_I4, ELEMENT_TYPE_U4, ELEMENT_TYPE_I8, ELEMENT_TYPE_U8, ELEMENT_TYPE_R4, ELEMENT_TYPE_R8, ELEMENT_TYPE_STRING, IMetaDataImport2, ELEMENT_TYPE_VALUETYPE, ELEMENT_TYPE_CLASS, ELEMENT_TYPE_OBJECT, ELEMENT_TYPE_SZARRAY, ELEMENT_TYPE_VAR, ELEMENT_TYPE_GENERICINST, ELEMENT_TYPE_BYREF, CorTokenType, CorElementType, mdtTypeDef, mdtTypeRef};
use crate::prelude::*;

const Guid: &str = "Guid";

/// Returns true when `token` is a TypeDef (or a TypeRef that resolves to one) whose base
/// class is `System.Enum` — the invariant for all WinRT enum types.
/// WinRT enums are always backed by a 32-bit integer on the ABI wire.
fn is_enum_type(metadata: &IMetaDataImport2, token: CorTokenType) -> bool {
    let token_kind = CorTokenType(type_from_token(token));
    
    // For TypeRef tokens, resolve to the TypeDef in the external metadata scope first.
    if token_kind == mdtTypeRef {
        // resolve_type_ref opens the metadata file that owns the referenced type and
        // returns both the external IMetaDataImport2 scope and the TypeDef token within it.
        let mut ext_metadata: MaybeUninit<IMetaDataImport2> = MaybeUninit::zeroed();
        let mut ext_token = token;
        if resolve_type_ref(Some(metadata), token, unsafe { ext_metadata.assume_init_mut() }, &mut ext_token) {
            return is_enum_type(unsafe { ext_metadata.assume_init_ref() }, ext_token);
        }
        return false;
    }
    if token_kind != mdtTypeDef {
        return false;
    }

    let name = get_fully_qualified_type_name(metadata, token);
    // Int32 and UInt32 are technically ValueTypes in signatures but not enums.
    // However, WinRT enums are effectively aliases for these on the wire.
    if name == "Int32" || name == "UInt32" {
        return false;
    }

    let mut extends = 0u32;
    let mut len = 0u32;
    let ok = unsafe {
        metadata
            .GetTypeDefProps(token.0 as u32, None, &mut len, 0 as _, &mut extends)
            .is_ok()
    };
    if !ok || extends == 0 {
        return false;
    }
    let extends_kind = CorTokenType(type_from_token(CorTokenType(extends as i32)));
    if extends_kind != mdtTypeDef && extends_kind != mdtTypeRef {
        return false;
    }
    get_fully_qualified_type_name(metadata, CorTokenType(extends as i32)) == SYSTEM_ENUM
}

/// Returns the fully qualified name (namespace + type name) for the given token.
fn get_fully_qualified_type_name(metadata: &IMetaDataImport2, token: CorTokenType) -> String {
    let token_kind = CorTokenType(type_from_token(token));
    if token_kind == mdtTypeRef {
        let mut ext_metadata: MaybeUninit<IMetaDataImport2> = MaybeUninit::zeroed();
        let mut ext_token = token;
        if resolve_type_ref(Some(metadata), token, unsafe { ext_metadata.assume_init_mut() }, &mut ext_token) {
            let metadata_ref = unsafe { ext_metadata.assume_init_ref() };
            return get_fully_qualified_type_name(metadata_ref, ext_token);
        }
        // If TypeRef cannot be resolved, fall back to its own name (which might be short)
        return get_type_name(metadata, token);
    }
    get_type_name(metadata, token)
}

pub struct Signature {}

impl Signature {
    pub fn consume_type(signature: &mut PCCOR_SIGNATURE) -> PCCOR_SIGNATURE {
        let start = signature.clone();

        let element_type = cor_sig_uncompress_element_type(signature);

        match element_type {
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
                // Unknown element type — skip
                start
            }
        }
    }

    fn get_string(metadata: Option<&IMetaDataImport2>, signature: &PCCOR_SIGNATURE) -> String {
        let mut signature = signature.clone();

        let element_type = cor_sig_uncompress_element_type(&mut signature);

        return match element_type {
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
                let token = cor_sig_uncompress_token(&mut signature);
                if let Some(metadata_ref) = metadata {
                    let token_type = CorTokenType(token as i32);

                    // WinRT enums are ELEMENT_TYPE_VALUETYPE but must be passed as
                    // Int32 on the ABI — not as a COM pointer.
                    if is_enum_type(metadata_ref, token_type) {
                        return "Int32".to_string();
                    }

                    let class_name = get_fully_qualified_type_name(metadata_ref, token_type);
                    if class_name.eq("System.Guid") {
                        Guid.to_string()
                    } else {
                        class_name
                    }
                } else {
                    // Fallback when metadata is not available (e.g. for as_string debug output)
                    format!("ValueType(0x{:08X})", token)
                }
            }
            ELEMENT_TYPE_CLASS => {
                let token = cor_sig_uncompress_token(&mut signature);
                if let Some(metadata_ref) = metadata {
                    get_fully_qualified_type_name(metadata_ref, CorTokenType(token as i32))
                } else {
                    // Fallback when metadata is not available
                    "Object".to_string()
                }
            }
            ELEMENT_TYPE_OBJECT => "Object".to_string(),
            ELEMENT_TYPE_SZARRAY => {
                let result = Signature::get_string(metadata, &mut signature);
                format!("{}[]", result)
            }
            ELEMENT_TYPE_VAR => {
                let index = cor_sig_uncompress_data(&mut signature);
                format!("Var!{}", index)
            }
            ELEMENT_TYPE_GENERICINST => {
                let generic_type = cor_sig_uncompress_element_type(&mut signature);

                assert_eq!(generic_type, ELEMENT_TYPE_CLASS);

                let token = cor_sig_uncompress_token(&mut signature);

                let mut result = if let Some(metadata_ref) = metadata {
                    let mut name = get_fully_qualified_type_name(metadata_ref, CorTokenType(token as i32));
                    // Strip generic backtick suffix (e.g. `1, `2) for cleaner output in typings and proxies
                    if let Some(pos) = name.find('`') {
                        name.truncate(pos);
                    }
                    name
                } else {
                    "Object".to_string()
                };

                result += "<";

                let generic_arguments_count = cor_sig_uncompress_data(&mut signature);

                for i in 0..generic_arguments_count {
                    let mut sig_type = Signature::consume_type(&mut signature);
                    let data = Signature::get_string(metadata, &mut sig_type);

                    result += data.as_ref();
                    if i != generic_arguments_count.saturating_sub(1) {
                        result += ", ";
                    }
                }

                result += ">";

                result
            }
            ELEMENT_TYPE_BYREF => {
                let mut result = "ByRef ".to_string();
                result += Signature::get_string(metadata, &mut signature).as_ref();
                result
            }
            _ => {
                "Object".to_string()
            }
        };
    }

    pub fn to_string(metadata: &IMetaDataImport2, signature: &PCCOR_SIGNATURE) -> String {
        Signature::get_string(Some(metadata), signature)
    }

    pub fn as_string(signature: &PCCOR_SIGNATURE) -> String {
        Signature::get_string(None, signature)
    }

    pub fn get_signature_element_type(signature: &PCCOR_SIGNATURE) -> CorElementType {
        let mut signature = signature.clone();

         cor_sig_uncompress_element_type(&mut signature)

    }
}

impl Signature {
    pub fn consume_type(signature: &mut PCCOR_SIGNATURE) -> PCCOR_SIGNATURE {
        let start = signature.clone();

        let element_type = cor_sig_uncompress_element_type(signature);

        match element_type {
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
                // Unknown element type — skip
                start
            }
        }
    }

    fn get_string(metadata: Option<&IMetaDataImport2>, signature: &PCCOR_SIGNATURE) -> String {
        let mut signature = signature.clone();

        let element_type = cor_sig_uncompress_element_type(&mut signature);

        return match element_type {
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
                let token = cor_sig_uncompress_token(&mut signature);
                if let Some(metadata_ref) = metadata {
                    let token_type = CorTokenType(token as i32);

                    // WinRT enums are ELEMENT_TYPE_VALUETYPE but must be passed as
                    // Int32 on the ABI — not as a COM pointer.
                    if is_enum_type(metadata_ref, token_type) {
                        return "Int32".to_string();
                    }

                    let class_name = get_fully_qualified_type_name(metadata_ref, token_type);
                    if class_name.eq("System.Guid") {
                        Guid.to_string()
                    } else {
                        class_name
                    }
                } else {
                    // Fallback when metadata is not available (e.g. for as_string debug output)
                    format!("ValueType(0x{:08X})", token)
                }
            }
            ELEMENT_TYPE_CLASS => {
                let token = cor_sig_uncompress_token(&mut signature);
                if let Some(metadata_ref) = metadata {
                    get_fully_qualified_type_name(metadata_ref, CorTokenType(token as i32))
                } else {
                    // Fallback when metadata is not available
                    "Object".to_string()
                }
            }
            ELEMENT_TYPE_OBJECT => "Object".to_string(),
            ELEMENT_TYPE_SZARRAY => {
                let result = Signature::get_string(metadata, &mut signature);
                format!("{}[]", result)
            }
            ELEMENT_TYPE_VAR => {
                let index = cor_sig_uncompress_data(&mut signature);
                format!("Var!{}", index)
            }
            ELEMENT_TYPE_GENERICINST => {
                let generic_type = cor_sig_uncompress_element_type(&mut signature);

                assert_eq!(generic_type, ELEMENT_TYPE_CLASS);

                let token = cor_sig_uncompress_token(&mut signature);

                let mut result = if let Some(metadata_ref) = metadata {
                    let mut name = get_fully_qualified_type_name(metadata_ref, CorTokenType(token as i32));
                    // Strip generic backtick suffix (e.g. `1, `2) for cleaner output in typings and proxies
                    if let Some(pos) = name.find('`') {
                        name.truncate(pos);
                    }
                    name
                } else {
                    "Object".to_string()
                };

                result += "<";

                let generic_arguments_count = cor_sig_uncompress_data(&mut signature);

                for i in 0..generic_arguments_count {
                    let mut sig_type = Signature::consume_type(&mut signature);
                    let data = Signature::get_string(metadata, &mut sig_type);

                    result += data.as_ref();
                    if i != generic_arguments_count.saturating_sub(1) {
                        result += ", ";
                    }
                }

                result += ">";

                result
            }
            ELEMENT_TYPE_BYREF => {
                let mut result = "ByRef ".to_string();
                result += Signature::get_string(metadata, &mut signature).as_ref();
                result
            }
            _ => {
                "Object".to_string()
            }
        };
    }

    pub fn to_string(metadata: &IMetaDataImport2, signature: &PCCOR_SIGNATURE) -> String {
        Signature::get_string(Some(metadata), signature)
    }

    pub fn as_string(signature: &PCCOR_SIGNATURE) -> String {
        Signature::get_string(None, signature)
    }

    pub fn get_signature_element_type(signature: &PCCOR_SIGNATURE) -> CorElementType {
        let mut signature = signature.clone();

         cor_sig_uncompress_element_type(&mut signature)

    }
}