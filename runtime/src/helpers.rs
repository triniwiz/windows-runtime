use metadata::declarations::base_class_declaration::BaseClassDeclarationImpl;
use metadata::declarations::class_declaration::ClassDeclaration;
use metadata::declarations::declaration::DeclarationKind;
use metadata::declarations::enum_declaration::EnumDeclaration;
use metadata::declarations::interface_declaration::InterfaceDeclaration;
use metadata::meta_data_reader::MetadataReader;
use metadata::signature::Signature;
use regex::Regex;
use windows::core::HRESULT;

use crate::value::NativeType;

/// E_FAIL — used as the HRESULT for failed COM/WinRT calls originating from JS.
#[inline]
pub fn call_failure() -> HRESULT {
    HRESULT(0x8000_4005u32 as i32)
}

/// Maps a WinRT signature string to the FFI ABI [`NativeType`] used to describe
/// the native parameter slot.
#[inline]
pub fn ffi_native_type_from_signature(signature: &str) -> NativeType {
    match signature {
        "Void" => NativeType::Void,
        "String" => NativeType::Pointer,
        "Boolean" => NativeType::Bool,
        "UInt8" => NativeType::U8,
        "UInt16" => NativeType::U16,
        "UInt32" => NativeType::U32,
        "UInt64" => NativeType::U64,
        "Int8" => NativeType::I8,
        "Int16" => NativeType::I16,
        "Int32" => NativeType::I32,
        "Int64" => NativeType::I64,
        "Single" => NativeType::F32,
        "Double" => NativeType::F64,
        _ => NativeType::Pointer,
    }
}

/// Generic type parameters in projected WinRT signatures (`Var!`/`MVar!`) map
/// to opaque object pointers; everything else is unchanged.
#[inline]
pub fn normalize_parameter_signature(signature: &str) -> &str {
    if signature.starts_with("Var!") || signature.starts_with("MVar!") {
        return "Object";
    }
    signature
}

/// Best-effort mapping of a WinRT signature to the [`NativeType`] used when
/// parsing JS arguments. Falls back to [`NativeType::Pointer`] for anything not
/// directly representable as a primitive.
#[inline]
pub fn parse_native_type_from_signature(signature: &str) -> NativeType {
    if signature.starts_with("Var!") || signature.starts_with("MVar!") {
        return NativeType::Pointer;
    }

    if let Ok(native_type) = NativeType::try_from(signature) {
        if native_type != NativeType::Pointer {
            return native_type;
        }
    }

    if let Some(declaration) = MetadataReader::find_by_name(signature) {
        let lock = declaration.read();
        match lock.kind() {
            DeclarationKind::Enum => {
                if let Some(enum_declaration) = lock.as_any().downcast_ref::<EnumDeclaration>() {
                    let underlying_signature = Signature::as_string(&enum_declaration.type_());
                    if let Ok(enum_native) = NativeType::try_from(underlying_signature.as_str()) {
                        return enum_native;
                    }
                }
                return NativeType::I32;
            }
            DeclarationKind::Class => {
                if let Some(class_declaration) = lock.as_any().downcast_ref::<ClassDeclaration>() {
                    if class_declaration.base_full_name() == "System.Enum" {
                        return NativeType::I32;
                    }
                }
            }
            _ => {}
        }
    }

    NativeType::Pointer
}

/// Recursively counts methods of the inherited interface chain — used to
/// compute the vtable offset of a method declared on a parent interface.
pub fn inherited_interface_method_count(interfaces: &[&InterfaceDeclaration]) -> usize {
    let mut count = 0usize;
    for interface in interfaces {
        count += interface.methods().len();
        count += inherited_interface_method_count(interface.implemented_interfaces().as_slice());
    }
    count
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
