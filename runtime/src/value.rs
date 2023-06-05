use std::borrow::Cow;
use std::ffi::c_void;
use std::fmt::{Display, Formatter};
use std::{fmt, mem};
use std::mem::{ManuallyDrop, MaybeUninit};
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
use byteorder::ByteOrder;
use libffi::middle::{Arg, Cif};
use v8::FunctionCallbackArguments;
use windows::core::{ComInterface, IInspectable, IUnknown, Interface, IntoParam, Type, GUID, HRESULT, HSTRING, IUnknown_Vtbl, IInspectable_Vtbl};
use windows::Win32::System::Com::IDispatch;
use windows::Win32::System::WinRT::IActivationFactory;
use windows::Win32::System::WinRT::Metadata::{CorElementType, ELEMENT_TYPE_CLASS};
use metadata::{get_method, print_vtable_names};
use crate::error::*;
use chrono::Local;

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
    String
}


impl NativeType {
    pub fn size(&self) -> usize {
        unsafe {
            match self {
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
                NativeType::Pointer | NativeType::String => {
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
            }
        }
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
            NativeType::Pointer | NativeType::Buffer | NativeType::Function | NativeType::String => {
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
            "Boolean" => NativeType::Bool,
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
            "String" | "Char16" => NativeType::String,
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
    pub string: ManuallyDrop<HSTRING>
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
            NativeType::String => {
                let addr = &*self.string;
                Arg::new::<*mut c_void>(mem::transmute(addr))
            }
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
            NativeType::String => {
                let local_value: v8::Local<v8::Value> =
                    v8::String::new_from_two_byte(scope, self.string.as_wide(), v8::NewStringType::Normal).unwrap().into();
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
pub fn ffi_parse_string_arg(
    scope: &mut v8::HandleScope,
    arg: v8::Local<v8::Value>,
) -> std::result::Result<NativeValue, AnyError> {
    let string_value = v8::Local::<v8::String>::try_from(arg)
        .map_err(|_| type_error("Invalid FFI String type, expected String"))?;

    let string = string_value.to_rust_string_lossy(scope);

    Ok(NativeValue { string: ManuallyDrop::new(HSTRING::from(string)) })
}


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
    scope: &mut v8::HandleScope,
    arg: v8::Local<v8::Value>,
) -> std::result::Result<NativeValue, AnyError> {
    if arg.is_object() {
        let arg = arg.to_object(scope).unwrap();
        let dec = arg.get_internal_field(scope, 0).unwrap();
        let dec = unsafe { v8::Local::<v8::External>::cast(dec) };
        let dec = dec.value() as *mut DeclarationFFI;
        let dec = unsafe { &*dec };
        return Ok(NativeValue {pointer: dec.instance.as_ref()
            .map(|instance| unsafe {mem::transmute_copy(instance)})
            .unwrap_or(std::ptr::null_mut())
        })
    }
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



#[inline]
pub unsafe fn set_ret_val(value:*mut c_void, scope: &mut v8::HandleScope, mut rv: v8::ReturnValue, native_type: NativeType){
    match native_type {
        NativeType::Void => {
            unimplemented!()
        }
        NativeType::Bool => {
            // just check if null :D
            rv.set_bool(!value.is_null());
        }
        NativeType::U8 => {
            let ret: &u8 = mem::transmute(value as *const u8);
            rv.set_uint32(
                *ret as u32
            );
        }
        NativeType::I8 => {
            let ret: &i8 = mem::transmute(value as *const i8);
            rv.set_int32(
                *ret as i32
            );
        }
        NativeType::U16 => {
            rv.set_uint32(
                value as u32
            );
        }
        NativeType::I16 => {
            rv.set_int32(
                value as i32
            );
        }
        NativeType::U32 => {
            rv.set_uint32(
                value as u32
            );
        }
        NativeType::I32 => {
            rv.set_int32(
                value as i32
            );
        }
        NativeType::U64 => {
            let ret: u64 = *mem::transmute::<*const u64, &u64>(value as *const u64);

            let local_value: v8::Local<v8::Value> =
                if ret > MAX_SAFE_INTEGER as u64 {
                    v8::BigInt::new_from_u64(scope, ret).into()
                } else {
                    v8::Number::new(scope, ret as f64).into()
                };

            rv.set(local_value);
        }
        NativeType::I64 => {
            let ret: i64 = *mem::transmute::<*const i64, &i64>(value as *const i64);
            let local_value: v8::Local<v8::Value> =
                if ret > MAX_SAFE_INTEGER as i64 || ret < MIN_SAFE_INTEGER as i64
                {
                    v8::BigInt::new_from_i64(scope, ret).into()
                } else {
                    v8::Number::new(scope, ret as f64).into()
                };
            rv.set(local_value);
        }
        NativeType::USize => {}
        NativeType::ISize => {}
        NativeType::F32 => {
            let slice = unsafe {std::slice::from_raw_parts(value as *const u8, 4)};
            let ret: f32 = if cfg!(target_endian = "big") {
                byteorder::BigEndian::read_f32(slice)
            } else {
                byteorder::LittleEndian::read_f32(slice)
            };

            rv.set(
                v8::Number::new(scope, ret as f64).into()
            );
        }
        NativeType::F64 => {
            let slice = unsafe {std::slice::from_raw_parts((&value) as *const _ as *const u8, 8)};

            let ret: f64 = if cfg!(target_endian = "big") {
                byteorder::BigEndian::read_f64(slice)
            } else {
                byteorder::LittleEndian::read_f64(slice)
            };
            rv.set_double(ret);
        }
        NativeType::Pointer => {
            // trying something
            let size = mem::size_of_val(&*value);

            // enum value ??
            if size == 1 {
                let result = value as *const _ as i8;

                rv.set_int32(result as i32);
            }else {

            }

        }
        NativeType::Buffer => {}
        NativeType::Function => {}
        NativeType::Struct(_) => {}
        NativeType::String => {
            let string: HSTRING = unsafe { mem::transmute(value) };
            let string = string.to_string();
            let string = v8::String::new(scope, string.as_str()).unwrap();
            rv.set(string.into());
        }
    }
}
    