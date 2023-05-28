use std::borrow::Cow;
use std::ffi::c_void;
use std::fmt::{Display, Formatter};
use std::{fmt, mem};
use std::mem::MaybeUninit;
use std::os::raw::c_ushort;
use std::ptr::{addr_of, addr_of_mut};

use crate::DeclarationFFI;
use libffi::low::*;
use libffi::raw::ffi_call;
use metadata::declarations::base_class_declaration::BaseClassDeclarationImpl;
use metadata::declarations::declaration::{Declaration, DeclarationKind};
use metadata::declarations::delegate_declaration::generic_delegate_declaration::GenericDelegateDeclaration;
use metadata::declarations::interface_declaration::generic_interface_declaration::GenericInterfaceDeclaration;
use metadata::declarations::interface_declaration::generic_interface_instance_declaration::GenericInterfaceInstanceDeclaration;
use metadata::declarations::interface_declaration::InterfaceDeclaration;
use metadata::declarations::method_declaration::MethodDeclaration;
use metadata::declarations::parameter_declaration::ParameterDeclaration;
use metadata::declarations::property_declaration::PropertyDeclaration;
use metadata::declaring_interface_for_method::Metadata;
use metadata::prelude::cor_sig_uncompress_element_type;
use metadata::signature::Signature;
use parking_lot::RwLock;
use std::sync::Arc;
use libffi::middle::{Arg, Cif};
use v8::FunctionCallbackArguments;
use windows::core::{ComInterface, IInspectable, IUnknown, Interface, IntoParam, Type, GUID, HRESULT, HSTRING, IUnknown_Vtbl, IInspectable_Vtbl};
use windows::Data::Json::{IJsonValue, JsonValue};
use windows::Win32::System::Com::IDispatch;
use windows::Win32::System::WinRT::IActivationFactory;
use windows::Win32::System::WinRT::Metadata::{CorElementType, ELEMENT_TYPE_CLASS};
use metadata::{get_method, print_vtable_names};

use anyhow::Error;

pub(crate) const MAX_SAFE_INTEGER: isize = 9007199254740991;
pub(crate) const MIN_SAFE_INTEGER: isize = -9007199254740991;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeType {
    Void,
    Bool,
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
    U64,
    I64,
    USize,
    ISize,
    F32,
    F64,
    Pointer,
    Buffer,
    Function,
    Struct(Box<[NativeType]>),
}

/// A simple error type that lets the creator specify both the error message and
/// the error class name. This type is private; externally it only ever appears
/// wrapped in an `anyhow::Error`. To retrieve the error class name from a wrapped
/// `CustomError`, use the function `get_custom_error_class()`.
#[derive(Debug)]
struct CustomError {
    class: &'static str,
    message: Cow<'static, str>,
}

impl Display for CustomError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CustomError {}

/// If this error was crated with `custom_error()`, return the specified error
/// class name. In all other cases this function returns `None`.
pub fn get_custom_error_class(error: &Error) -> Option<&'static str> {
    error.downcast_ref::<CustomError>().map(|e| e.class)
}

pub type AnyError = anyhow::Error;

pub fn custom_error(
    class: &'static str,
    message: impl Into<Cow<'static, str>>,
) -> Error {
    CustomError {
        class,
        message: message.into(),
    }
        .into()
}

pub fn generic_error(message: impl Into<Cow<'static, str>>) -> Error {
    custom_error("Error", message)
}

pub fn type_error(message: impl Into<Cow<'static, str>>) -> Error {
    custom_error("TypeError", message)
}

impl NativeType {
    pub fn size(&self) -> usize {
        unsafe { match self {
            NativeType::Void => {
                types::void.size
            }
            NativeType::Bool | NativeType::U8 => {
                types::uint8.size
            }
            NativeType::I8 => {
                types::sint8.size
            }
            NativeType::U16 => {
                types::uint16.size
            }
            NativeType::I16 => {
                types::sint16.size
            }
            NativeType::U32 => {
                types::uint32.size
            }
            NativeType::I32 => {
                types::sint32.size
            }
            NativeType::U64 => {
                types::uint64.size
            }
            NativeType::I64 => {
                types::sint64.size
            }
            NativeType::USize => {
                let usize_type = *(libffi::middle::Type::usize().as_raw_ptr());
                usize_type.size
            }
            NativeType::ISize => {
                let isize_type = *(libffi::middle::Type::isize().as_raw_ptr());
                isize_type.size
            }
            NativeType::F32 => {
                types::float.size
            }
            NativeType::F64 => {
                types::double.size
            }
            NativeType::Pointer => {
                types::pointer.size
            }
            NativeType::Buffer => {
                types::pointer.size
            }
            NativeType::Function => {
                types::pointer.size
            }
            NativeType::Struct(ref value) => {
                let mut size = 0_usize;
                for native_type in value.iter() {
                    size = size + 1 + native_type.size();
                }
                size
            }
        } }
    }
}

impl TryFrom<NativeType> for libffi::middle::Type {
    type Error = AnyError;

    fn try_from(native_type: NativeType) -> std::result::Result<Self, Self::Error> {
        Ok(match native_type {
            NativeType::Void => libffi::middle::Type::void(),
            NativeType::U8 | NativeType::Bool => libffi::middle::Type::u8(),
            NativeType::I8 => libffi::middle::Type::i8(),
            NativeType::U16 => libffi::middle::Type::u16(),
            NativeType::I16 => libffi::middle::Type::i16(),
            NativeType::U32 => libffi::middle::Type::u32(),
            NativeType::I32 => libffi::middle::Type::i32(),
            NativeType::U64 => libffi::middle::Type::u64(),
            NativeType::I64 => libffi::middle::Type::i64(),
            NativeType::USize => libffi::middle::Type::usize(),
            NativeType::ISize => libffi::middle::Type::isize(),
            NativeType::F32 => libffi::middle::Type::f32(),
            NativeType::F64 => libffi::middle::Type::f64(),
            NativeType::Pointer | NativeType::Buffer | NativeType::Function => {
                libffi::middle::Type::pointer()
            }
            NativeType::Struct(fields) => {
                libffi::middle::Type::structure(match fields.len() > 0 {
                    true => fields
                        .iter()
                        .map(|field| field.clone().try_into())
                        .collect::<std::result::Result<Vec<_>, _>>()?,
                    false => {
                        return Err(type_error("Struct must have at least one field"));
                    }
                })
            }
        })
    }
}

impl TryFrom<&str> for NativeType {
    type Error = AnyError;

    fn try_from(native_type: &str) -> std::result::Result<Self, Self::Error> {
        if native_type.contains(".") {
            return Ok(NativeType::Pointer);
        }
        Ok(match native_type {
            "Void" => NativeType::Void,
            "Uint8" => NativeType::U8,
            "Bool" => NativeType::Bool,
            "Int8" => NativeType::I8,
            "UInt16" => NativeType::U16,
            "Int16" => NativeType::I16,
            "UInt32" => NativeType::U32,
            "IntI32" => NativeType::I32,
            "UInt64" => NativeType::U64,
            "Int64" => NativeType::I64,
            "USize" => NativeType::USize,
            "ISize" => NativeType::ISize,
            "Single" => NativeType::F32,
            "Double" => NativeType::F64,
            _ => {
                return Err(type_error("Unsupported type"));
            }
        })
    }
}

#[repr(C)]
pub union NativeValue {
    pub void_value: (),
    pub bool_value: bool,
    pub u8_value: u8,
    pub i8_value: i8,
    pub u16_value: u16,
    pub i16_value: i16,
    pub u32_value: u32,
    pub i32_value: i32,
    pub u64_value: u64,
    pub i64_value: i64,
    pub usize_value: usize,
    pub isize_value: isize,
    pub f32_value: f32,
    pub f64_value: f64,
    pub pointer: *mut c_void,
}

impl NativeValue {
    pub unsafe fn as_arg(&self, native_type: &NativeType) -> Arg {
        match native_type {
            NativeType::Void => unreachable!(),
            NativeType::Bool => Arg::new(&self.bool_value),
            NativeType::U8 => Arg::new(&self.u8_value),
            NativeType::I8 => Arg::new(&self.i8_value),
            NativeType::U16 => Arg::new(&self.u16_value),
            NativeType::I16 => Arg::new(&self.i16_value),
            NativeType::U32 => Arg::new(&self.u32_value),
            NativeType::I32 => Arg::new(&self.i32_value),
            NativeType::U64 => Arg::new(&self.u64_value),
            NativeType::I64 => Arg::new(&self.i64_value),
            NativeType::USize => Arg::new(&self.usize_value),
            NativeType::ISize => Arg::new(&self.isize_value),
            NativeType::F32 => Arg::new(&self.f32_value),
            NativeType::F64 => Arg::new(&self.f64_value),
            NativeType::Pointer | NativeType::Buffer | NativeType::Function => {
                Arg::new(&self.pointer)
            }
            NativeType::Struct(_) => Arg::new(&*self.pointer),
        }
    }


    // SAFETY: native_type must correspond to the type of value represented by the union field
    #[inline]
    pub unsafe fn to_v8<'a>(
        &'a self,
        scope: &mut v8::HandleScope<'a>,
        native_type: NativeType,
    ) -> v8::Local<v8::Value> {
       let value = match native_type {
            NativeType::Void => {
                let local_value: v8::Local<v8::Value> = v8::undefined(scope).into();
                local_value
            }
            NativeType::Bool => {
                let local_value: v8::Local<v8::Value> =
                    v8::Boolean::new(scope, self.bool_value).into();
                local_value
            }
            NativeType::U8 => {
                let local_value: v8::Local<v8::Value> =
                    v8::Integer::new_from_unsigned(scope, self.u8_value as u32).into();
                local_value
            }
            NativeType::I8 => {
                let local_value: v8::Local<v8::Value> =
                    v8::Integer::new(scope, self.i8_value as i32).into();
                local_value
            }
            NativeType::U16 => {
                let local_value: v8::Local<v8::Value> =
                    v8::Integer::new_from_unsigned(scope, self.u16_value as u32).into();
                local_value
            }
            NativeType::I16 => {
                let local_value: v8::Local<v8::Value> =
                    v8::Integer::new(scope, self.i16_value as i32).into();
                local_value
            }
            NativeType::U32 => {
                let local_value: v8::Local<v8::Value> =
                    v8::Integer::new_from_unsigned(scope, self.u32_value).into();
                local_value
            }
            NativeType::I32 => {
                let local_value: v8::Local<v8::Value> =
                    v8::Integer::new(scope, self.i32_value).into();
                local_value
            }
            NativeType::U64 => {
                let value = self.u64_value;
                let local_value: v8::Local<v8::Value> =
                    if value > MAX_SAFE_INTEGER as u64 {
                        v8::BigInt::new_from_u64(scope, value).into()
                    } else {
                        v8::Number::new(scope, value as f64).into()
                    };
                local_value
            }
            NativeType::I64 => {
                let value = self.i64_value;
                let local_value: v8::Local<v8::Value> =
                    if value > MAX_SAFE_INTEGER as i64 || value < MIN_SAFE_INTEGER as i64
                    {
                        v8::BigInt::new_from_i64(scope, self.i64_value).into()
                    } else {
                        v8::Number::new(scope, value as f64).into()
                    };
                local_value
            }
            NativeType::USize => {
                let value = self.usize_value;
                let local_value: v8::Local<v8::Value> =
                    if value > MAX_SAFE_INTEGER as usize {
                        v8::BigInt::new_from_u64(scope, value as u64).into()
                    } else {
                        v8::Number::new(scope, value as f64).into()
                    };
                local_value
            }
            NativeType::ISize => {
                let value = self.isize_value;
                let local_value: v8::Local<v8::Value> =
                    if !(MIN_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&value) {
                        v8::BigInt::new_from_i64(scope, self.isize_value as i64).into()
                    } else {
                        v8::Number::new(scope, value as f64).into()
                    };
                local_value
            }
            NativeType::F32 => {
                let local_value: v8::Local<v8::Value> =
                    v8::Number::new(scope, self.f32_value as f64).into();
                local_value
            }
            NativeType::F64 => {
                let local_value: v8::Local<v8::Value> =
                    v8::Number::new(scope, self.f64_value).into();
                local_value
            }
            NativeType::Pointer | NativeType::Buffer | NativeType::Function => {
                let local_value: v8::Local<v8::Value> = if self.pointer.is_null() {
                    v8::null(scope).into()
                } else {
                    v8::External::new(scope, self.pointer).into()
                };
                local_value
            }
            NativeType::Struct(_) => {
                let local_value: v8::Local<v8::Value> = v8::null(scope).into();
                local_value
            }
        };

        let mut scope = v8::EscapableHandleScope::new(scope);

        scope.escape(value)

    }

}


// SAFETY: unsafe trait must have unsafe implementation
unsafe impl Send for NativeValue {}


#[inline]
pub fn ffi_parse_bool_arg(
    arg: v8::Local<v8::Value>,
) -> std::result::Result<NativeValue, AnyError> {
    let bool_value = v8::Local::<v8::Boolean>::try_from(arg)
        .map_err(|_| type_error("Invalid FFI u8 type, expected boolean"))?
        .is_true();
    Ok(NativeValue { bool_value })
}

#[inline]
pub fn ffi_parse_u8_arg(
    arg: v8::Local<v8::Value>,
) -> std::result::Result<NativeValue, AnyError> {
    let u8_value = v8::Local::<v8::Uint32>::try_from(arg)
        .map_err(|_| type_error("Invalid FFI u8 type, expected unsigned integer"))?
        .value() as u8;
    Ok(NativeValue { u8_value })
}

#[inline]
pub fn ffi_parse_i8_arg(
    arg: v8::Local<v8::Value>,
) -> std::result::Result<NativeValue, AnyError> {
    let i8_value = v8::Local::<v8::Int32>::try_from(arg)
        .map_err(|_| type_error("Invalid FFI i8 type, expected integer"))?
        .value() as i8;
    Ok(NativeValue { i8_value })
}

#[inline]
pub fn ffi_parse_u16_arg(
    arg: v8::Local<v8::Value>,
) -> std::result::Result<NativeValue, AnyError> {
    let u16_value = v8::Local::<v8::Uint32>::try_from(arg)
        .map_err(|_| type_error("Invalid FFI u16 type, expected unsigned integer"))?
        .value() as u16;
    Ok(NativeValue { u16_value })
}

#[inline]
pub fn ffi_parse_i16_arg(
    arg: v8::Local<v8::Value>,
) -> std::result::Result<NativeValue, AnyError> {
    let i16_value = v8::Local::<v8::Int32>::try_from(arg)
        .map_err(|_| type_error("Invalid FFI i16 type, expected integer"))?
        .value() as i16;
    Ok(NativeValue { i16_value })
}

#[inline]
pub fn ffi_parse_u32_arg(
    arg: v8::Local<v8::Value>,
) -> std::result::Result<NativeValue, AnyError> {
    let u32_value = v8::Local::<v8::Uint32>::try_from(arg)
        .map_err(|_| type_error("Invalid FFI u32 type, expected unsigned integer"))?
        .value();
    Ok(NativeValue { u32_value })
}

#[inline]
pub fn ffi_parse_i32_arg(
    arg: v8::Local<v8::Value>,
) -> std::result::Result<NativeValue, AnyError> {
    let i32_value = v8::Local::<v8::Int32>::try_from(arg)
        .map_err(|_| type_error("Invalid FFI i32 type, expected integer"))?
        .value();
    Ok(NativeValue { i32_value })
}

#[inline]
pub fn ffi_parse_u64_arg(
    scope: &mut v8::HandleScope,
    arg: v8::Local<v8::Value>,
) -> std::result::Result<NativeValue, AnyError> {
    // Order of checking:
    // 1. BigInt: Uncommon and not supported by Fast API, so optimise slow call for this case.
    // 2. Number: Common, supported by Fast API, so let that be the optimal case.
    let u64_value: u64 = if let Ok(value) = v8::Local::<v8::BigInt>::try_from(arg)
    {
        value.u64_value().0
    } else if let Ok(value) = v8::Local::<v8::Number>::try_from(arg) {
        value.integer_value(scope).unwrap() as u64
    } else {
        return Err(type_error(
            "Invalid FFI u64 type, expected unsigned integer",
        ));
    };
    Ok(NativeValue { u64_value })
}

#[inline]
pub fn ffi_parse_i64_arg(
    scope: &mut v8::HandleScope,
    arg: v8::Local<v8::Value>,
) -> std::result::Result<NativeValue, AnyError> {
    // Order of checking:
    // 1. BigInt: Uncommon and not supported by Fast API, so optimise slow call for this case.
    // 2. Number: Common, supported by Fast API, so let that be the optimal case.
    let i64_value: i64 = if let Ok(value) = v8::Local::<v8::BigInt>::try_from(arg)
    {
        value.i64_value().0
    } else if let Ok(value) = v8::Local::<v8::Number>::try_from(arg) {
        value.integer_value(scope).unwrap()
    } else {
        return Err(type_error("Invalid FFI i64 type, expected integer"));
    };
    Ok(NativeValue { i64_value })
}

#[inline]
pub fn ffi_parse_usize_arg(
    scope: &mut v8::HandleScope,
    arg: v8::Local<v8::Value>,
) -> std::result::Result<NativeValue, AnyError> {
    // Order of checking:
    // 1. BigInt: Uncommon and not supported by Fast API, so optimise slow call for this case.
    // 2. Number: Common, supported by Fast API, so let that be the optimal case.
    let usize_value: usize =
        if let Ok(value) = v8::Local::<v8::BigInt>::try_from(arg) {
            value.u64_value().0 as usize
        } else if let Ok(value) = v8::Local::<v8::Number>::try_from(arg) {
            value.integer_value(scope).unwrap() as usize
        } else {
            return Err(type_error("Invalid FFI usize type, expected integer"));
        };
    Ok(NativeValue { usize_value })
}

#[inline]
pub fn ffi_parse_isize_arg(
    scope: &mut v8::HandleScope,
    arg: v8::Local<v8::Value>,
) -> std::result::Result<NativeValue, AnyError> {
    // Order of checking:
    // 1. BigInt: Uncommon and not supported by Fast API, so optimise slow call for this case.
    // 2. Number: Common, supported by Fast API, so let that be the optimal case.
    let isize_value: isize =
        if let Ok(value) = v8::Local::<v8::BigInt>::try_from(arg) {
            value.i64_value().0 as isize
        } else if let Ok(value) = v8::Local::<v8::Number>::try_from(arg) {
            value.integer_value(scope).unwrap() as isize
        } else {
            return Err(type_error("Invalid FFI isize type, expected integer"));
        };
    Ok(NativeValue { isize_value })
}

#[inline]
pub fn ffi_parse_f32_arg(
    arg: v8::Local<v8::Value>,
) -> std::result::Result<NativeValue, AnyError> {
    let f32_value = v8::Local::<v8::Number>::try_from(arg)
        .map_err(|_| type_error("Invalid FFI f32 type, expected number"))?
        .value() as f32;
    Ok(NativeValue { f32_value })
}

#[inline]
pub fn ffi_parse_f64_arg(
    arg: v8::Local<v8::Value>,
) -> std::result::Result<NativeValue, AnyError> {
    let f64_value = v8::Local::<v8::Number>::try_from(arg)
        .map_err(|_| type_error("Invalid FFI f64 type, expected number"))?
        .value();
    Ok(NativeValue { f64_value })
}

#[inline]
pub fn ffi_parse_pointer_arg(
    _scope: &mut v8::HandleScope,
    arg: v8::Local<v8::Value>,
) -> std::result::Result<NativeValue, AnyError> {
    let pointer = if let Ok(value) = v8::Local::<v8::External>::try_from(arg) {
        value.value()
    } else if arg.is_null() {
        std::ptr::null_mut()
    } else {
        return Err(type_error(
            "Invalid FFI pointer type, expected null, or External",
        ));
    };
    Ok(NativeValue { pointer })
}

#[inline]
pub fn ffi_parse_buffer_arg(
    scope: &mut v8::HandleScope,
    arg: v8::Local<v8::Value>,
) -> std::result::Result<NativeValue, AnyError> {
    // Order of checking:
    // 1. ArrayBuffer: Fairly common and not supported by Fast API, optimise this case.
    // 2. ArrayBufferView: Common and supported by Fast API
    // 5. Null: Very uncommon / can be represented by a 0.

    let pointer = if let Ok(value) = v8::Local::<v8::ArrayBuffer>::try_from(arg) {
        if let Some(non_null) = value.data() {
            non_null.as_ptr()
        } else {
            std::ptr::null_mut()
        }
    } else if let Ok(value) = v8::Local::<v8::ArrayBufferView>::try_from(arg) {
        let byte_offset = value.byte_offset();
        let pointer = value
            .buffer(scope)
            .ok_or_else(|| {
                type_error("Invalid FFI ArrayBufferView, expected data in the buffer")
            })?
            .data();
        if let Some(non_null) = pointer {
            // SAFETY: Pointer is non-null, and V8 guarantees that the byte_offset
            // is within the buffer backing store.
            unsafe { non_null.as_ptr().add(byte_offset) }
        } else {
            std::ptr::null_mut()
        }
    } else if arg.is_null() {
        std::ptr::null_mut()
    } else {
        return Err(type_error(
            "Invalid FFI buffer type, expected null, ArrayBuffer, or ArrayBufferView",
        ));
    };
    Ok(NativeValue { pointer })
}

#[inline]
pub fn ffi_parse_struct_arg(
    scope: &mut v8::HandleScope,
    arg: v8::Local<v8::Value>,
) -> std::result::Result<NativeValue, AnyError> {
    // Order of checking:
    // 1. ArrayBuffer: Fairly common and not supported by Fast API, optimise this case.
    // 2. ArrayBufferView: Common and supported by Fast API

    let pointer = if let Ok(value) = v8::Local::<v8::ArrayBuffer>::try_from(arg) {
        if let Some(non_null) = value.data() {
            non_null.as_ptr()
        } else {
            return Err(type_error(
                "Invalid FFI ArrayBuffer, expected data in buffer",
            ));
        }
    } else if let Ok(value) = v8::Local::<v8::ArrayBufferView>::try_from(arg) {
        let byte_offset = value.byte_offset();
        let pointer = value
            .buffer(scope)
            .ok_or_else(|| {
                type_error("Invalid FFI ArrayBufferView, expected data in the buffer")
            })?
            .data();
        if let Some(non_null) = pointer {
            // SAFETY: Pointer is non-null, and V8 guarantees that the byte_offset
            // is within the buffer backing store.
            unsafe { non_null.as_ptr().add(byte_offset) }
        } else {
            return Err(type_error(
                "Invalid FFI ArrayBufferView, expected data in buffer",
            ));
        }
    } else {
        return Err(type_error(
            "Invalid FFI struct type, expected ArrayBuffer, or ArrayBufferView",
        ));
    };
    Ok(NativeValue { pointer })
}

#[inline]
pub fn ffi_parse_function_arg(
    _scope: &mut v8::HandleScope,
    arg: v8::Local<v8::Value>,
) -> std::result::Result<NativeValue, AnyError> {
    let pointer = if let Ok(value) = v8::Local::<v8::External>::try_from(arg) {
        value.value()
    } else if arg.is_null() {
        std::ptr::null_mut()
    } else {
        return Err(type_error(
            "Invalid FFI function type, expected null, or External",
        ));
    };
    Ok(NativeValue { pointer })
}


pub struct MethodCall {
    index: usize,
    number_of_parameters: usize,
    number_of_abi_parameters: usize,
    is_initializer: bool,
    is_sealed: bool,
    is_void: bool,
    iid: GUID,
    interface: IUnknown,
    cif: Cif,
    parameter_types: Vec<NativeType>,
    parameters: Vec<ParameterDeclaration>,
    return_type: String,
    pub(crate) declaration: Option<Arc<RwLock<dyn BaseClassDeclarationImpl>>>,
}

impl MethodCall {
    pub fn is_void(&self) -> bool {
        self.is_void
    }

    pub fn return_type(&self) -> &str {
        self.return_type.as_str()
    }

    pub fn new(
        method: &MethodDeclaration,
        is_sealed: bool,
        interface: IUnknown,
        is_initializer: bool,
    ) -> MethodCall {
        let number_of_parameters = method.number_of_parameters();

        let mut index = 0 as usize;

        let mut declaration: Option<Arc<RwLock<dyn BaseClassDeclarationImpl>>> = None;

        let iid = match Metadata::find_declaring_interface_for_method(method, &mut index) {
            None => {
                index = 0;
                IActivationFactory::IID
            }
            Some(interface) => {
                let iid;
                {
                    let ii_lock = interface.read();

                    let kind = ii_lock.base().kind();

                    match kind {
                        DeclarationKind::GenericInterfaceInstance => {
                            let ii = ii_lock
                                .as_declaration()
                                .as_any()
                                .downcast_ref::<GenericInterfaceInstanceDeclaration>();
                            let ii = ii.unwrap();
                            iid = ii.id();
                        }
                        _ => {
                            let ii = ii_lock
                                .as_declaration()
                                .as_any()
                                .downcast_ref::<InterfaceDeclaration>();
                            let ii = ii.unwrap();
                            iid = ii.id();
                        }
                    }
                }
                declaration = Some(interface);
                iid
            }
        };

        index = index.saturating_add(6); // account for IInspectable vtable overhead

        //  index = index.saturating_add(mem::size_of::<windows::core::IInspectable_Vtbl>());

        // let mut interface_ptr: *mut c_void = std::ptr::null_mut(); // IActivationFactory

        let ii = unsafe { IInspectable::from_raw(interface.clone().into_raw()) };

        let vtable = interface.vtable();

        let mut interface_ptr: *mut c_void = std::ptr::null_mut();

        let result = unsafe {
            ((*vtable).QueryInterface)(
                interface.as_raw(),
                &iid,
                &mut interface_ptr as *mut _ as *mut *const c_void,
            )
        };

        assert!(result.is_ok());
        assert!(!interface_ptr.is_null());

        let is_composition = !is_sealed;

        let is_void = method.is_void();

        let other_params: usize = if is_initializer {
            if is_sealed {
                2
            } else {
                4
            }
        } else {
            if is_void {
                1
            } else {
                2
            }
        };

        let number_of_abi_parameters = number_of_parameters + other_params;

        let mut parameter_types: Vec<NativeType> = Vec::new();

        parameter_types.reserve(number_of_abi_parameters);

        unsafe {
            parameter_types.push(NativeType::Pointer);
        }

        for parameter in method.parameters().iter() {
            let type_ = parameter.type_();
            let metadata = parameter.metadata().unwrap();

            let signature = Signature::to_string(metadata, &type_);

            match signature.as_str() {
                "Void" => unsafe {
                    parameter_types.push(NativeType::Void);
                },
                "String" => unsafe {
                    parameter_types.push(NativeType::Pointer);
                },
                "Boolean" => unsafe {
                    parameter_types.push(NativeType::Bool);
                },
                "UInt8" => unsafe {
                    parameter_types.push(NativeType::U8);
                },
                "UInt16" => unsafe {
                    parameter_types.push(NativeType::U16);
                },
                "UInt32" => unsafe {
                    parameter_types.push(NativeType::U32);
                },
                "UInt64" => unsafe {
                    parameter_types.push(NativeType::U64);
                },
                "Int8" => unsafe {
                    parameter_types.push(NativeType::I8);
                },
                "Int16" => unsafe {
                    parameter_types.push(NativeType::I16);
                },
                "Int32" => unsafe {
                    parameter_types.push(NativeType::I32);
                },
                "Int64" => unsafe {
                    parameter_types.push(NativeType::I64);
                },
                "Single" => unsafe {
                    parameter_types.push(NativeType::F32);
                },
                "Double" => unsafe {
                    parameter_types.push(NativeType::F64);
                },
                _ => {
                    // objects
                    unsafe {
                        parameter_types.push(NativeType::Pointer);
                    }
                }
            }
        }

        if is_initializer {
            if is_composition {
                unsafe {
                    parameter_types.push(NativeType::Pointer);
                }
                unsafe {
                    parameter_types.push(NativeType::Pointer);
                }
            }
            unsafe {
                parameter_types.push(NativeType::Pointer);
            }
        } else {
            if !is_void {
                unsafe {
                    parameter_types.push(NativeType::Pointer);
                }
            }
        }

        let params =
            parameter_types
                .clone()
                .into_iter()
                .map(libffi::middle::Type::try_from)
                .collect::<std::result::Result<Vec<libffi::middle::Type>, AnyError>>();

        assert!(params.is_ok());

        let mut cif = Cif::new(
            params.unwrap(),
            libffi::middle::Type::i32(),
        );

        // let interface = unsafe { IUnknown::from_raw(interface_ptr as *mut c_void) };

        let signature = method.return_type();
        let return_type = Signature::to_string(method.metadata().unwrap(), &signature);
        Self {
            cif,
            index,
            number_of_parameters,
            number_of_abi_parameters,
            is_initializer,
            is_sealed,
            is_void: method.is_void(),
            iid,
            interface,
            parameter_types,
            parameters: method.parameters().to_vec(),
            declaration,
            return_type,
        }
    }

    pub fn call(
        &mut self,
        scope: &mut v8::HandleScope,
        args: &FunctionCallbackArguments,
    ) -> (HRESULT, *mut c_void) {
        let number_of_abi_parameters = self.number_of_abi_parameters;

        let mut arguments: Vec<NativeValue> = Vec::new();

        arguments.reserve(number_of_abi_parameters);

        unsafe { arguments.push(NativeValue { pointer: self.interface.as_raw() }) };

        let mut string_buf: Vec<HSTRING> = Vec::new();

        for (i, parameter) in self.parameters.iter().enumerate() {
            let type_ = parameter.type_();
            let metadata = parameter.metadata().unwrap();

            let signature = Signature::to_string(metadata, &type_);

            println!("sig {}", signature.as_str());

            let string = args.get(i as i32).to_string(scope).unwrap();

            let string = string.to_rust_string_lossy(scope);

            let mut string = HSTRING::from(string);

            arguments.push(NativeValue { pointer: string.as_ptr() as *mut _ });

            string_buf.push(string);


            /*
            match signature.as_str() {
                "String" => {
                    let string = args.get(i as i32).to_string(scope).unwrap();

                    let string = HSTRING::from(string.to_rust_string_lossy(scope));

                    arguments.push(NativeValue {pointer: addr_of!(string) as *mut c_void});

                    string_buf.push(string);
                }
                "Boolean" => {
                    let value = args.get(i as i32).boolean_value(scope);
                    // arguments.push(
                    //     addr_of!(value) as *mut c_void
                    // )

                    arguments.push(unsafe { mem::transmute(&value) })
                }
                "UInt8" => {
                    let value = args.get(i as i32).uint32_value(scope).unwrap() as u8;

                    arguments.push(unsafe { mem::transmute(&value) })
                }
                "UInt16" => {
                    let value = args.get(i as i32).uint32_value(scope).unwrap() as u16;

                    arguments.push(unsafe { mem::transmute(&value) })
                }
                "UInt32" => {
                    let value = args.get(i as i32).uint32_value(scope).unwrap();

                    arguments.push(unsafe { mem::transmute(&value) })
                }
                "UInt64" => {
                    let value = args.get(i as i32);
                    if value.is_big_int() {
                        let value = value.to_big_int(scope).unwrap().u64_value();

                        arguments.push(unsafe { mem::transmute(&value.0) })
                    } else {
                        let value = args.get(i as i32).uint32_value(scope).unwrap() as u64;

                        arguments.push(unsafe { mem::transmute(&value) })
                    }
                }
                "Int8" => {
                    let value = args.get(i as i32).int32_value(scope).unwrap() as i8;

                    arguments.push(unsafe { mem::transmute(&value) })
                }
                "Int16" => {
                    let value = args.get(i as i32).int32_value(scope).unwrap() as i16;

                    arguments.push(unsafe { mem::transmute(&value) })
                }
                "Int32" => {
                    let value = args.get(i as i32).int32_value(scope).unwrap();

                    arguments.push(unsafe { mem::transmute(&value) })
                }
                "Int64" => {
                    let value = args.get(i as i32);
                    if value.is_big_int() {
                        let value = value.to_big_int(scope).unwrap().i64_value();

                        arguments.push(unsafe { mem::transmute(&value.0) })
                    } else {
                        let value = args.get(i as i32).int32_value(scope).unwrap() as i64;

                        arguments.push(unsafe { mem::transmute(&value) })
                    }
                }
                "Single" => {
                    let value = args.get(i as i32).number_value(scope).unwrap() as f32;

                    arguments.push(unsafe { mem::transmute(&value) })
                }
                "Double" => {
                    let value = args.get(i as i32).number_value(scope).unwrap();

                    arguments.push(unsafe { mem::transmute(&value) })
                }
                _ => {
                    let value = args.get(i as i32);

                    if value.is_object() {
                        let value = value.to_object(scope).unwrap();

                        let dec = value.get_internal_field(scope, 0).unwrap();

                        let dec = unsafe { v8::Local::<v8::External>::cast(dec) };

                        let dec = dec.value() as *mut DeclarationFFI;

                        let dec = unsafe { &*dec };

                        let instance = dec.instance.clone();
                        match instance {
                            None => {
                                arguments.push(std::ptr::null_mut());
                            }
                            Some(mut instance) => {
                                unsafe {
                                    arguments
                                        .push(&mut instance.into_raw() as *mut _ as *mut c_void)
                                };
                            }
                        }
                    }
                }
            }


            */
        }

        let mut result: *mut c_void = std::ptr::null_mut();

        let result_ptr: *mut *mut *mut c_void = &mut addr_of_mut!(result);

        if self.is_initializer {
            // result_ptr as *mut c_void
            unsafe { arguments.push(NativeValue { pointer: addr_of_mut!(result) as *mut _ as *mut c_void }) };
        } else {
            if !self.is_void {
                match self.return_type.as_str() {
                    "Boolean" | "String" => {
                        // arguments.push(&mut result as *mut _ as *mut c_void);
                        // arguments.push(&mut result as *mut _ as *mut c_void);
                        arguments.push(NativeValue { pointer: result });
                    }
                    _ => {
                        // arguments.push(&mut result as *mut _ as *mut c_void);
                        //arguments.push(result_ptr as *mut c_void);

                        arguments.push(NativeValue { pointer: result });
                    }
                }
            }
        }

        // print_vtable_names(&self.interface);

        let mut func = std::ptr::null_mut();

        get_method(&self.interface, self.index, addr_of_mut!(func));

        // let mut vtable = self.interface.vtable();
        //
        // let vtable: *mut *mut c_void = unsafe {mem::transmute(vtable)};
        //
        // let func = unsafe { *vtable.offset((self.index) as isize) };

        let call_args: Vec<Arg> = arguments
            .iter()
            .enumerate()
            // SAFETY: Creating a `Arg` from a `NativeValue` is pretty safe.
            .map(|(i, v)| unsafe { v.as_arg(self.parameter_types.get(i).unwrap()) })
            .collect();


        // let prep_result = unsafe {
        //     prep_cif(
        //         &mut cif,
        //         ffi_abi_FFI_DEFAULT_ABI,
        //         parameter_types.len(),
        //         &mut types::sint32,
        //         parameter_types.as_mut_ptr(),
        //     )
        // };

        //assert!(prep_result.is_ok());

        // todo handle prep_cif error

        /*let ret = unsafe {
            call::<i32>(
                &mut self.cif,
                CodePtr::from_ptr(func),
                call_args.as_mut_ptr(),
            )
        };
        */

        let ret = unsafe {
            self.cif.call(
                CodePtr::from_ptr(func),
                &call_args,
            )
        };

        (HRESULT(ret), result)
    }
}

pub struct PropertyCall {
    index: usize,
    number_of_parameters: usize,
    number_of_abi_parameters: usize,
    is_initializer: bool,
    is_sealed: bool,
    is_void: bool,
    is_setter: bool,
    iid: GUID,
    cif: ffi_cif,
    parent_interface: IUnknown,
    interface: IUnknown,
    parameter_types: Vec<*mut ffi_type>,
    parameters: Vec<ParameterDeclaration>,
    return_type: String,
    pub(crate) declaration: Option<Arc<RwLock<dyn BaseClassDeclarationImpl>>>,
}

impl PropertyCall {
    pub fn is_void(&self) -> bool {
        self.is_void
    }

    pub fn return_type(&self) -> &str {
        self.return_type.as_str()
    }

    pub fn new(
        property: &PropertyDeclaration,
        is_setter: bool,
        interface: IUnknown,
        is_initializer: bool,
    ) -> Self {
        let method = if is_setter {
            property.setter().unwrap()
        } else {
            property.getter()
        };

        let number_of_parameters = method.number_of_parameters();

        let mut index = 0 as usize;

        let mut declaration: Option<Arc<RwLock<dyn BaseClassDeclarationImpl>>> = None;

        let iid = match Metadata::find_declaring_interface_for_method(method, &mut index) {
            None => {
                index = 0;
                IActivationFactory::IID
            }
            Some(interface) => {
                let iid;
                {
                    let ii_lock = interface.read();

                    let kind = ii_lock.base().kind();

                    match kind {
                        DeclarationKind::GenericInterfaceInstance => {
                            let ii = ii_lock
                                .as_declaration()
                                .as_any()
                                .downcast_ref::<GenericInterfaceInstanceDeclaration>();
                            let ii = ii.unwrap();
                            iid = ii.id();
                        }
                        _ => {
                            let ii = ii_lock
                                .as_declaration()
                                .as_any()
                                .downcast_ref::<InterfaceDeclaration>();
                            let ii = ii.unwrap();
                            iid = ii.id();
                        }
                    }
                }
                declaration = Some(interface);
                iid
            }
        };

        index = index.saturating_add(6); // account for IInspectable vtable overhead

        let mut interface_ptr: *const c_void = std::ptr::null_mut(); // IActivationFactory

        let vtable = interface.vtable();

        // let interface_ptr_ptr = addr_of_mut!(interface_ptr);

        let result = unsafe {
            ((*vtable).QueryInterface)(
                interface.as_raw(),
                &iid,
                mem::transmute(&mut interface_ptr),
            )
        };

        assert!(result.is_ok());

        let is_sealed = method.is_sealed();

        let is_composition = !is_sealed;

        let is_void = method.is_void();

        let signature = method.return_type();

        let return_type = Signature::to_string(method.metadata().unwrap(), &signature);


        let other_params: usize = if is_initializer {
            if is_sealed {
                2
            } else {
                4
            }
        } else {
            if is_void {
                1
            } else {
                2
            }
        };

        let number_of_abi_parameters = number_of_parameters + other_params;

        let mut parameter_types: Vec<*mut ffi_type> = Vec::new();

        parameter_types.reserve(number_of_abi_parameters);

        unsafe {
            parameter_types.push(&mut types::pointer);
        }

        for parameter in method.parameters().iter() {
            let type_ = parameter.type_();
            let metadata = parameter.metadata().unwrap();

            let signature = Signature::to_string(metadata, &type_);

            match signature.as_str() {
                "String" => unsafe {
                    parameter_types.push(&mut types::pointer);
                },
                "Boolean" => unsafe {
                    parameter_types.push(&mut types::uint8);
                },
                _ => {
                    // objects
                    unsafe {
                        parameter_types.push(&mut types::pointer);
                    }
                }
            }
        }

        if is_initializer {
            if is_composition {
                unsafe {
                    parameter_types.push(&mut types::pointer);
                }
                unsafe {
                    parameter_types.push(&mut types::pointer);
                }
            }

            unsafe {
                parameter_types.push(&mut types::pointer);
            }
        } else {
            if !is_void {
                if return_type.as_str() == "UInt32" {
                    unsafe {
                        parameter_types.push(&mut types::sint32);
                    }
                } else if return_type.as_str() == "Boolean" {
                    unsafe {
                        parameter_types.push(&mut types::uint8);
                    }
                } else if return_type.as_str() == "String" {
                    unsafe {
                        parameter_types.push(&mut types::pointer);
                    }
                } else if return_type.as_str() == "Object" {
                    unsafe {
                        parameter_types.push(&mut types::pointer);
                    }
                } else {
                    unsafe {
                        parameter_types.push(&mut types::pointer);
                    }
                }
            }
        }

        let parent_interface = interface.clone();
        let interface = unsafe { IUnknown::from_raw(interface_ptr as *mut c_void) };

        let mut cif: ffi_cif = Default::default();

        let prep_result = unsafe {
            prep_cif(
                &mut cif,
                ffi_abi_FFI_DEFAULT_ABI,
                parameter_types.len(),
                &mut types::sint32,
                parameter_types.as_mut_ptr(),
            )
        };

        assert!(prep_result.is_ok());

        // todo handle prep_cif error

        Self {
            index,
            number_of_parameters,
            number_of_abi_parameters,
            is_initializer,
            is_sealed,
            is_void: method.is_void(),
            iid,
            cif,
            parent_interface,
            interface,
            parameter_types,
            parameters: method.parameters().to_vec(),
            declaration,
            return_type,
            is_setter,
        }
    }

    pub fn call(
        &mut self,
        scope: &mut v8::HandleScope,
        args: &v8::FunctionCallbackArguments,
    ) -> (HRESULT, *mut c_void) {
        let number_of_abi_parameters = self.number_of_abi_parameters;

        let mut arguments: Vec<*mut c_void> = Vec::new();

        arguments.reserve(number_of_abi_parameters);

        unsafe { arguments.push(self.interface.as_raw()) };

        let mut string_buf: Vec<HSTRING> = Vec::new();

        for (i, parameter) in self.parameters.iter().enumerate() {
            let type_ = parameter.type_();
            let metadata = parameter.metadata().unwrap();

            let signature = Signature::to_string(metadata, &type_);

            match signature.as_str() {
                "String" => {
                    let string = args.holder().to_string(scope).unwrap();

                    let string = HSTRING::from(string.to_rust_string_lossy(scope));

                    arguments.push(addr_of!(string) as *mut c_void);

                    string_buf.push(string);
                }
                "Boolean" => {
                    let value = args.holder().boolean_value(scope);
                    // arguments.push(
                    //     addr_of!(value) as *mut c_void
                    // )

                    arguments.push(unsafe { mem::transmute(&value) })
                }
                _ => {
                    let value = args.holder();

                    if value.is_object() {
                        let value = value.to_object(scope).unwrap();

                        let dec = value.get_internal_field(scope, 0).unwrap();

                        let dec = unsafe { v8::Local::<v8::External>::cast(dec) };

                        let dec = dec.value() as *mut DeclarationFFI;

                        let dec = unsafe { &*dec };

                        let instance = dec.instance.clone();
                        match instance {
                            None => {
                                arguments.push(std::ptr::null_mut());
                            }
                            Some(mut instance) => {
                                unsafe {
                                    arguments
                                        .push(&mut instance.into_raw() as *mut _ as *mut c_void)
                                };
                            }
                        }
                    }
                }
            }
        }

        let mut result: *mut c_void = std::ptr::null_mut();


        //let result_ptr: *mut *mut *mut c_void = &mut addr_of_mut!(result);

        if self.is_initializer {
            // arguments.push(result_ptr as *mut c_void);
        } else {
            if !self.is_void {
                println!("ret {}", self.return_type.as_str());

                match self.return_type.as_str() {
                    "Boolean" | "String" | "UInt32" => {
                        // unsafe { arguments.push(mem::transmute(&mut result))};
                        //  arguments.push(addr_of_mut!(result) as *mut _ as *mut c_void);
                        //unsafe { arguments.push(mem::transmute(a.as_mut_ptr())); }

                        unsafe { arguments.push(std::mem::transmute(&&result)) };
                    }
                    _ => {
                        //  arguments.push(result_ptr as *mut c_void);
                    }
                }
            }
        }

        // let mut vtable = raw;

        //let mut vtable: *mut *mut *mut c_void = unsafe { mem::transmute(vtable) };

        let mut func = std::ptr::null_mut();

        get_method(&self.interface, self.index, addr_of_mut!(func));

        println!("asdas {:?}", func);

        // let func = unsafe {
        //     vtable.offset(self.index as isize)
        // };

        let ret = unsafe {
            call::<i32>(
                &mut self.cif,
                CodePtr::from_ptr(func),
                arguments.as_mut_ptr(),
            )
        };

        (HRESULT(ret), result)
    }
}
    