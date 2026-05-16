use std::ffi::c_void;
use libffi::middle::*;
use windows::core::{GUID, HRESULT, Interface, IUnknown};
use metadata::declarations::base_class_declaration::BaseClassDeclarationImpl;
use metadata::declarations::interface_declaration::generic_interface_declaration::GenericInterfaceDeclaration;
use metadata::declarations::method_declaration::MethodDeclaration;
use metadata::declarations::parameter_declaration::ParameterDeclaration;
use metadata::declaring_interface_for_method::Metadata;
use metadata::signature::Signature;
use crate::error::AnyError;
use crate::helpers::ffi_native_type_from_signature;
use crate::value::{ffi_parse_bool_arg, ffi_parse_buffer_arg_with_length, ffi_parse_f32_arg, ffi_parse_f64_arg, ffi_parse_function_arg, ffi_parse_i16_arg, ffi_parse_i32_arg, ffi_parse_i64_arg, ffi_parse_i8_arg, ffi_parse_isize_arg, ffi_parse_pointer_arg, ffi_parse_string_arg, ffi_parse_struct_arg, ffi_parse_u16_arg, ffi_parse_u32_arg, ffi_parse_u64_arg, ffi_parse_u8_arg, ffi_parse_usize_arg, NativeType, NativeValue};

pub struct GenericMethodCall {
    index: usize,
    number_of_parameters: usize,
    number_of_abi_parameters: usize,
    is_initializer: bool,
    is_sealed: bool,
    is_void: bool,
    iid: GUID,
    interface: IUnknown,
    cif: Cif,
    func: *mut c_void,
    parameter_types: Vec<NativeType>,
    parse_parameter_types: Vec<NativeType>,
    parameters: Vec<ParameterDeclaration>,
    return_type: String,
    /// Scratch buffer for value-type returns (same role as in MethodCall).
    return_value_buf: [u8; 128],
}

#[inline]
fn call_failure() -> HRESULT {
    // E_FAIL
    HRESULT(0x8000_4005u32 as i32)
}

impl GenericMethodCall {
    pub fn is_void(&self) -> bool {
        self.is_void
    }

    pub fn return_type(&self) -> &str {
        self.return_type.as_str()
    }

    pub fn new(
        class: &GenericInterfaceDeclaration,
        method: &MethodDeclaration,
        is_sealed: bool,
        interface: IUnknown,
        is_initializer: bool,
        return_type: String,
    ) -> Self {
        let number_of_parameters = method.number_of_parameters();

        let mut index = Metadata::find_method_index(method.metadata().unwrap(), class.base().token(),method.token());

        let iid = class.id();

        index = index.saturating_add(6); // account for IInspectable vtable overhead

        let mut interface_ptr: *mut c_void = std::ptr::null_mut();

        let result = unsafe {
            ((*interface.vtable()).QueryInterface)(
                interface.as_raw(),
                &iid,
                &mut interface_ptr as *mut _ as *mut *mut c_void,
            )
        };

        // Generic interface QI is best-effort: fall back to the original interface
        // when the object doesn't implement the expected instantiation.
        let qi_ok = result.is_ok() && !interface_ptr.is_null();

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

        let mut parameter_types: Vec<NativeType> = Vec::with_capacity(number_of_parameters + other_params + 4);
        let mut parse_parameter_types: Vec<NativeType> = Vec::with_capacity(number_of_parameters);
        parameter_types.push(NativeType::Pointer);

        for parameter in method.parameters().iter() {
            let type_ = parameter.type_();
            let metadata = parameter.metadata().unwrap();

            let signature = Signature::to_string(metadata, &type_);

            let parse_native_type = NativeType::try_from(signature.as_str());
            assert!(parse_native_type.is_ok());
            parse_parameter_types.push(parse_native_type.unwrap());
            let abi_native = ffi_native_type_from_signature(signature.as_str());
            if matches!(abi_native, NativeType::Buffer) {
                parameter_types.push(NativeType::U32);
                parameter_types.push(NativeType::Buffer);
            } else {
                parameter_types.push(abi_native);
            }
        }

        if is_initializer {
            if is_composition {
                parameter_types.push(NativeType::Pointer);
                parameter_types.push(NativeType::Pointer);
            }
            parameter_types.push(NativeType::Pointer);
        } else {
            if !is_void {
                parameter_types.push(NativeType::Pointer);
            }
        }

        let number_of_abi_parameters = parameter_types.len();

        let params =
            parameter_types
                .iter()
                .cloned()
                .map(libffi::middle::Type::try_from)
                .collect::<std::result::Result<Vec<Type>, AnyError>>();

        assert!(params.is_ok());

        let cif = Cif::new(
            params.unwrap(),
            Type::i32(),
        );

        let effective_interface = if qi_ok {
            unsafe { IUnknown::from_raw(interface_ptr) }
        } else {
            interface.clone()
        };
        let vtable: *mut *mut c_void = unsafe { std::mem::transmute(effective_interface.vtable()) };
        let func = unsafe { *vtable.offset(index as isize) };

        Self {
            cif,
            func,
            index,
            number_of_parameters,
            number_of_abi_parameters,
            is_initializer,
            is_sealed,
            is_void: method.is_void(),
            iid,
            interface: effective_interface,
            parameter_types,
            parse_parameter_types,
            parameters: method.parameters().to_vec(),
            return_type,
            return_value_buf: [0u8; 128],
        }
    }

    pub fn call(
        &mut self,
        scope: &mut v8::PinScope<'_, '_>,
        args: &v8::FunctionCallbackArguments,
    ) -> (HRESULT, *mut c_void) {
        let number_of_abi_parameters = self.number_of_abi_parameters;

        let mut arguments: Vec<NativeValue> = Vec::with_capacity(number_of_abi_parameters);
        // Track parse-level types for each ABI argument slot.
        let mut argument_parse_types: Vec<Option<NativeType>> = Vec::with_capacity(number_of_abi_parameters);

        arguments.push(NativeValue { pointer: self.interface.as_raw() as *mut c_void });
        argument_parse_types.push(None);

        for (i, native_type) in self.parse_parameter_types.iter().enumerate() {
            let value = args.get(i as i32);

            let value = match *native_type {
                NativeType::Void => {
                    return (call_failure(), std::ptr::null_mut())
                }
                NativeType::Bool => {
                    ffi_parse_bool_arg(value)
                }
                NativeType::U8 => {
                    ffi_parse_u8_arg(value)
                }
                NativeType::I8 => {
                    ffi_parse_i8_arg(value)
                }
                NativeType::U16 => {
                    ffi_parse_u16_arg(value)
                }
                NativeType::I16 => {
                    ffi_parse_i16_arg(value)
                }
                NativeType::U32 => {
                    ffi_parse_u32_arg(value)
                }
                NativeType::I32 => {
                    ffi_parse_i32_arg(value)
                }
                NativeType::U64 => {
                    ffi_parse_u64_arg(scope, value)
                }
                NativeType::I64 => {
                    ffi_parse_i64_arg(scope, value)
                }
                NativeType::USize => {
                    ffi_parse_usize_arg(scope, value)
                }
                NativeType::ISize => {
                    ffi_parse_isize_arg(scope, value)
                }
                NativeType::F32 => {
                    ffi_parse_f32_arg(value)
                }
                NativeType::F64 => {
                    ffi_parse_f64_arg(value)
                }
                NativeType::Pointer => {
                    ffi_parse_pointer_arg(scope, value)
                }
                NativeType::Buffer => {
                    let parsed = ffi_parse_buffer_arg_with_length(scope, value);
                    let (buffer_value, byte_length) = match parsed {
                        Ok(value) => value,
                        Err(_) => return (call_failure(), std::ptr::null_mut()),
                    };

                    arguments.push(NativeValue { u32_value: byte_length });
                    argument_parse_types.push(Some(native_type.clone()));
                    arguments.push(buffer_value);
                    argument_parse_types.push(Some(native_type.clone()));
                    continue;
                }
                NativeType::Function => {
                    ffi_parse_function_arg(scope, value)
                }
                NativeType::Struct(_) => {
                    ffi_parse_struct_arg(scope, value)
                }
                NativeType::String => {
                    ffi_parse_string_arg(scope, value)
                }
            };

            let value = match value {
                Ok(value) => value,
                Err(_) => return (call_failure(), std::ptr::null_mut()),
            };

            arguments.push(value);
            argument_parse_types.push(Some(native_type.clone()));
        }

        let mut result: *mut c_void = std::ptr::null_mut();


        if self.is_initializer {
            unsafe { arguments.push(NativeValue { pointer: &mut result as *mut _ as *mut c_void }) };
            argument_parse_types.push(None);
        } else {
            if !self.is_void {
                arguments.push(NativeValue { pointer: &mut result as *mut _ as *mut c_void });
                argument_parse_types.push(None);
            }
        }

        let mut call_args: Vec<Arg> = Vec::with_capacity(arguments.len());
        for (i, v) in arguments.iter().enumerate() {
            // SAFETY: Creating a `Arg` from a `NativeValue` is safe when the parallel type vector matches.
            let Some(abi_native) = self.parameter_types.get(i) else {
                return (call_failure(), std::ptr::null_mut());
            };

            let effective_native = if matches!(abi_native, NativeType::Pointer) {
                if let Some(Some(parse_pt)) = argument_parse_types.get(i) {
                    if matches!(parse_pt, NativeType::String) {
                        NativeType::String
                    } else {
                        abi_native.clone()
                    }
                } else {
                    abi_native.clone()
                }
            } else {
                abi_native.clone()
            };

            call_args.push(unsafe { v.as_arg(&effective_native) });
        }


        let ret = unsafe {
            self.cif.call(
                CodePtr::from_ptr(self.func),
                &call_args,
            )
        };

        (HRESULT(ret), result)
    }
}