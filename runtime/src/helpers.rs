use std::sync::OnceLock;
use regex::Regex;
use crate::value::NativeType;

static RE_GENERIC_COUNT: OnceLock<Regex> = OnceLock::new();
static RE_GENERIC_PARAMS: OnceLock<Regex> = OnceLock::new();

/// Strip the generic instantiation suffix from a WinRT type name.
///
/// `"Windows.Foundation.IAsyncOperation`1<IUICommand>"` → `"Windows.Foundation.IAsyncOperation"`.
///
/// Returns the input unchanged when no backtick / angle-bracket suffix is present.
#[inline]
pub fn strip_generic_suffix(name: &str) -> &str {
    if let Some(angle) = name.find('<') {
        let backtick_pos = name[..angle].rfind('`').unwrap_or(angle);
        return &name[..backtick_pos];
    }
    name
}

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

pub fn get_generic_return_types(name: &str) -> GenericReturnTypes<'_> {
    let re_count = RE_GENERIC_COUNT.get_or_init(|| Regex::new(r"`(\d+)").unwrap());
    let re_params = RE_GENERIC_PARAMS.get_or_init(|| Regex::new(r"<(.*?)>").unwrap());

    let types = re_count
        .captures(name)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<usize>().ok())
        .unwrap_or(0);

    let names = re_params
        .captures(name)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().split(", ").collect::<Vec<_>>())
        .unwrap_or_default();

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

    // EventRegistrationToken passed by value in remove_* methods — treat as i64.
    // "ByRef Windows.Foundation.EventRegistrationToken" (add out-param) stays as Pointer below.
    if signature == "Windows.Foundation.EventRegistrationToken" {
        return NativeType::I64;
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
