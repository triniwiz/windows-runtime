use regex::Regex;
use crate::value::NativeType;

pub struct GenericReturnTypes<'s> {
    names: Vec<&'s str>,
    types: usize,
}

impl GenericReturnTypes<'_> {
    pub fn names(&self) -> &[&str] {
        self.names.as_slice()
    }

    pub fn types(&self) -> usize {
        self.types
    }
}

pub fn get_generic_return_types(name: &str) -> GenericReturnTypes {
    let types = match Regex::new(r"`(\d+)") {
        Ok(types) => {
            if let Some(captures) = types.captures(name) {
                captures.get(1).unwrap().as_str().parse::<usize>().unwrap()
            } else {
                0
            }
        }
        Err(_) => 0,
    };

    let names = match Regex::new(r"<(.*?)>") {
        Ok(names) => {
            if let Some(captures) = names.captures(name) {
                captures.get(1).unwrap().as_str().split(", ").collect::<Vec<_>>()
            } else {
                vec![]
            }
        }
        Err(_) => vec![],
    };

    GenericReturnTypes { names, types }
}

/// Shared mapping from WinRT signature string to FFI `NativeType`.
/// Used by `MethodCall` and `PropertyCall` during construction.
#[inline]
pub(crate) fn ffi_native_type_from_signature(signature: &str) -> NativeType {
    let signature = signature.trim();
    let by_ref_inner = signature.strip_prefix("ByRef ").unwrap_or(signature);

    if let Some(element) = by_ref_inner.strip_suffix("[]") {
        return match element {
            "UInt8" | "Uint8" | "Int8" | "Byte" | "SByte" => NativeType::Buffer,
            _ => NativeType::Pointer,
        };
    }

    if by_ref_inner.starts_with("Var!")
        || by_ref_inner.contains('.')
        || by_ref_inner == "Object"
        || by_ref_inner == "Guid"
    {
        return NativeType::Pointer;
    }

    match by_ref_inner {
        "Void"    => NativeType::Void,
        "String"  => NativeType::Pointer,
        "Char16"  => NativeType::U16,
        "Boolean" => NativeType::Bool,
        "UInt8" | "Uint8" | "Byte" => NativeType::U8,
        "Int8" | "SByte" => NativeType::I8,
        "UInt16"  => NativeType::U16,
        "UInt32"  => NativeType::U32,
        "UInt64"  => NativeType::U64,
        "Int16"   => NativeType::I16,
        "Int32"   => NativeType::I32,
        "Int64"   => NativeType::I64,
        "Single"  => NativeType::F32,
        "Double"  => NativeType::F64,
        _         => NativeType::Pointer,
    }
}
