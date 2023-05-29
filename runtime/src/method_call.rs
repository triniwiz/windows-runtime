use std::ffi::c_void;
use std::ptr::addr_of_mut;
use std::sync::Arc;
use libffi::middle::*;
use parking_lot::RwLock;
use windows::core::{ComInterface, GUID, HRESULT, Interface, IUnknown};
use windows::Win32::System::WinRT::IActivationFactory;
use metadata::declarations::base_class_declaration::BaseClassDeclarationImpl;
use metadata::declarations::declaration::DeclarationKind;
use metadata::declarations::interface_declaration::generic_interface_instance_declaration::GenericInterfaceInstanceDeclaration;
use metadata::declarations::interface_declaration::InterfaceDeclaration;
use metadata::declarations::method_declaration::MethodDeclaration;
use metadata::declarations::parameter_declaration::ParameterDeclaration;
use metadata::declaring_interface_for_method::Metadata;
use metadata::signature::Signature;
use crate::error::AnyError;
use crate::value::{ffi_parse_bool_arg, ffi_parse_buffer_arg, ffi_parse_f32_arg, ffi_parse_f64_arg, ffi_parse_function_arg, ffi_parse_i16_arg, ffi_parse_i32_arg, ffi_parse_i64_arg, ffi_parse_i8_arg, ffi_parse_isize_arg, ffi_parse_pointer_arg, ffi_parse_string_arg, ffi_parse_struct_arg, ffi_parse_u16_arg, ffi_parse_u32_arg, ffi_parse_u64_arg, ffi_parse_u8_arg, ffi_parse_usize_arg, NativeType, NativeValue};

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

    pub fn new( method: &MethodDeclaration,
                is_sealed: bool,
                interface: IUnknown,
                is_initializer: bool) -> Self {
        let signature = method.return_type();

        let return_type = Signature::to_string(method.metadata().unwrap(), &signature);

        Self::new_with_return_type(
            method, is_sealed, interface, is_initializer, return_type, None
        )
    }

    pub fn new_with_return_type(
        method: &MethodDeclaration,
        is_sealed: bool,
        interface: IUnknown,
        is_initializer: bool,
        return_type: String,
        iid: Option<GUID> // use for generic
    ) -> Self {

        let number_of_parameters = method.number_of_parameters();

        let mut index = 0 as usize;

        let mut declaration: Option<Arc<RwLock<dyn BaseClassDeclarationImpl>>> = None;


        let iid = iid.unwrap_or(match Metadata::find_declaring_interface_for_method(method, &mut index) {
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
        });

        index = index.saturating_add(6); // account for IInspectable vtable overhead

        // let mut interface_ptr: *mut c_void = std::ptr::null_mut(); // IActivationFactory

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
                parameter_types.push(NativeType::Pointer);
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

        let interface = unsafe { IUnknown::from_raw(interface_ptr as *mut c_void) };

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
        args: &v8::FunctionCallbackArguments,
    ) -> (HRESULT, *mut c_void) {

        let number_of_abi_parameters = self.number_of_abi_parameters;

        let mut arguments: Vec<NativeValue> = Vec::new();

        arguments.reserve(number_of_abi_parameters);

        unsafe { arguments.push(NativeValue { pointer: std::mem::transmute_copy(&self.interface) }) };

        for (i, parameter) in self.parameters.iter().enumerate() {
            let type_ = parameter.type_();
            let metadata = parameter.metadata().unwrap();

            let signature = Signature::to_string(metadata, &type_);

            let value = args.get(i as i32);

            let native_type = NativeType::try_from(signature.as_str());

            // todo error
            assert!(native_type.is_ok());

            let native_type = native_type.unwrap();

            let value = match native_type {
                NativeType::Void => {
                    // todo
                    unreachable!()
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
                    ffi_parse_i64_arg(scope,value)
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
                    ffi_parse_buffer_arg(scope, value)
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

            // todo error
            assert!(value.is_ok());

            let value = value.unwrap();

            arguments.push(value);
        }

        let mut result: *mut c_void = std::ptr::null_mut();


        if self.is_initializer {
            unsafe { arguments.push(NativeValue { pointer: &mut result as *mut _ as *mut c_void }) };
        } else {
            if !self.is_void {
                arguments.push(NativeValue { pointer: &mut result as *mut _ as *mut c_void });
            }
        }

        //let mut func = std::ptr::null_mut();

       // get_method(&self.interface, self.index, addr_of_mut!(func));

         let mut vtable = self.interface.vtable();

         let vtable: *mut *mut c_void = unsafe {std::mem::transmute(vtable)};

         let func = unsafe { *vtable.offset((self.index) as isize) };

        let call_args: Vec<Arg> = arguments
            .iter()
            .enumerate()
            // SAFETY: Creating a `Arg` from a `NativeValue` is pretty safe.
            .map(|(i, v)| {
                unsafe { v.as_arg(self.parameter_types.get(i).unwrap()) }
            })
            .collect();


        let ret = unsafe {
            self.cif.call(
                CodePtr::from_ptr(func),
                &call_args,
            )
        };


        (HRESULT(ret), result)
    }
}