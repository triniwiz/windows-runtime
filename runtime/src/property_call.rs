use std::ffi::c_void;
use std::sync::Arc;
use libffi::middle::*;
use parking_lot::RwLock;
use windows::core::{GUID, HRESULT, Interface, IUnknown};
use windows::Win32::System::WinRT::IActivationFactory;
use windows::Win32::System::WinRT::Metadata::CorTokenType;
use metadata::declarations::base_class_declaration::BaseClassDeclarationImpl;
use metadata::declarations::declaration::DeclarationKind;
use metadata::declarations::interface_declaration::generic_interface_instance_declaration::GenericInterfaceInstanceDeclaration;
use metadata::declarations::interface_declaration::InterfaceDeclaration;
use metadata::declarations::parameter_declaration::ParameterDeclaration;
use metadata::declarations::property_declaration::PropertyDeclaration;
use metadata::declaring_interface_for_method::Metadata;
use metadata::signature::Signature;
use crate::error::AnyError;
use crate::helpers::ffi_native_type_from_signature;
use crate::value::{ffi_parse_bool_arg, ffi_parse_buffer_arg_with_length, ffi_parse_f32_arg, ffi_parse_f64_arg, ffi_parse_function_arg, ffi_parse_i16_arg, ffi_parse_i32_arg, ffi_parse_i64_arg, ffi_parse_i8_arg, ffi_parse_isize_arg, ffi_parse_pointer_arg, ffi_parse_string_arg, ffi_parse_struct_arg, ffi_parse_u16_arg, ffi_parse_u32_arg, ffi_parse_u64_arg, ffi_parse_u8_arg, ffi_parse_usize_arg, NativeType, NativeValue};

pub struct PropertyCall {
    index: usize,
    number_of_parameters: usize,
    number_of_abi_parameters: usize,
    is_initializer: bool,
    is_sealed: bool,
    is_void: bool,
    is_setter: bool,
    iid: GUID,
    cif: Cif,
    func: *mut c_void,
    parent_interface: IUnknown,
    interface: IUnknown,
    parameter_types: Vec<NativeType>,
    parse_parameter_types: Vec<NativeType>,
    parameters: Vec<ParameterDeclaration>,
    return_type: String,
    pub(crate) declaration: Option<Arc<RwLock<dyn BaseClassDeclarationImpl>>>,
    /// Pre-allocated argument buffer reused on every call to avoid per-call heap allocation.
    argument_buf: Vec<NativeValue>,
}

#[inline]
fn call_failure() -> HRESULT {
    // E_FAIL
    HRESULT(0x8000_4005u32 as i32)
}

impl PropertyCall {
    pub fn is_void(&self) -> bool {
        self.is_void
    }

    pub fn return_type(&self) -> &str {
        self.return_type.as_str()
    }

    pub fn parse_types_debug(&self) -> &[NativeType] {
        &self.parse_parameter_types
    }

    pub fn abi_types_debug(&self) -> &[NativeType] {
        &self.parameter_types
    }

    pub fn new(
        property: &PropertyDeclaration,
        is_setter: bool,
        interface: IUnknown,
        is_initializer: bool,
    ) -> Option<Self> {
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
                if let Some(metadata) = method.metadata() {
                    let containing_type = CorTokenType(
                        Metadata::get_method_containing_class_token(metadata, method.token()) as i32,
                    );

                    if containing_type.0 != 0 {
                        let interface_declaration = Arc::new(RwLock::new(InterfaceDeclaration::new(
                            Some(metadata),
                            containing_type,
                        )));
                        index = Metadata::find_method_index(metadata, containing_type, method.token());
                        let iid = interface_declaration.read().id();
                        declaration = Some(interface_declaration);
                        iid
                    } else {
                        index = 0;
                        IActivationFactory::IID
                    }
                } else {
                    index = 0;
                    IActivationFactory::IID
                }
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
                                .downcast_ref::<GenericInterfaceInstanceDeclaration>()?;
                            iid = ii.id();
                        }
                        _ => {
                            let ii = ii_lock
                                .as_declaration()
                                .as_any()
                                .downcast_ref::<InterfaceDeclaration>()?;
                            iid = ii.id();
                        }
                    }
                }
                declaration = Some(interface);
                iid
            }
        };

        index = index.saturating_add(6); // account for IInspectable vtable overhead

        let vtable = interface.vtable();

        let mut interface_ptr: *mut c_void = std::ptr::null_mut();

        let result = unsafe {
            ((*vtable).QueryInterface)(
                interface.as_raw(),
                &iid,
                &mut interface_ptr as *mut _ as *mut *mut c_void,
            )
        };

        if result.is_err() || interface_ptr.is_null() {
            return None;
        }

        let is_sealed = method.is_sealed();

        let is_composition = !is_sealed;

        let is_void = method.is_void();

        let signature = method.return_type();

        let return_type = Signature::to_string(method.metadata()?, &signature);


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
            let metadata = parameter.metadata()?;

            let signature = Signature::to_string(metadata, &type_);

            let parse_native_type = NativeType::try_from(signature.as_str()).ok()?;
            parse_parameter_types.push(parse_native_type);
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

        let params = params.ok()?;

        let cif = Cif::new(
            params,
            Type::i32(),
        );

        let parent_interface = interface.clone();

        let interface = unsafe { IUnknown::from_raw(interface_ptr as *mut c_void) };
        let vtable: *mut *mut c_void = unsafe { std::mem::transmute(interface.vtable()) };
        let func = unsafe { *vtable.offset(index as isize) };


        Some(Self {
            index,
            number_of_parameters,
            number_of_abi_parameters,
            is_initializer,
            is_sealed,
            is_void: method.is_void(),
            iid,
            cif,
            func,
            parent_interface,
            interface,
            parameter_types,
            parse_parameter_types,
            parameters: method.parameters().to_vec(),
            declaration,
            return_type,
            is_setter,
            argument_buf: Vec::with_capacity(number_of_abi_parameters),
        })
    }

    pub fn call(
        &mut self,
        scope: &mut v8::PinScope<'_, '_>,
        args: &v8::FunctionCallbackArguments,
    ) -> (HRESULT, *mut c_void) {
        let mut values = Vec::with_capacity(self.parse_parameter_types.len());
        for index in 0..self.parse_parameter_types.len() {
            values.push(args.get(index as i32));
        }

        self.call_with_values(scope, &values)
    }

    pub fn call_with_values(
        &mut self,
        scope: &mut v8::PinScope<'_, '_>,
        values: &[v8::Local<v8::Value>],
    ) -> (HRESULT, *mut c_void) {
        let is_void = self.is_void;

        // Reuse the pre-allocated buffer — avoids a heap allocation on every call.
        self.argument_buf.clear();

        self.argument_buf.push(NativeValue { pointer: self.interface.as_raw() as *mut c_void });

        for (i, native_type) in self.parse_parameter_types.iter().enumerate() {
            let Some(value) = values.get(i).copied() else {
                return (call_failure(), std::ptr::null_mut());
            };

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

                    self.argument_buf.push(NativeValue { u32_value: byte_length });
                    self.argument_buf.push(buffer_value);
                    continue;
                }
                NativeType::Function => {
                    ffi_parse_function_arg(scope, value)
                }
                NativeType::Struct(_) => {
                    // todo
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

            self.argument_buf.push(value);
        }

        let mut result: *mut c_void = std::ptr::null_mut();

        if !self.is_initializer && !is_void {
            self.argument_buf.push(NativeValue { pointer: &mut result as *mut _ as *mut c_void });
        }

        let mut call_args: Vec<Arg> = Vec::with_capacity(self.argument_buf.len());
        for (i, v) in self.argument_buf.iter().enumerate() {
            // SAFETY: Creating a `Arg` from a `NativeValue` is safe when the parallel type vector matches.
            let Some(native_type) = self.parameter_types.get(i) else {
                return (call_failure(), std::ptr::null_mut());
            };
            call_args.push(unsafe { v.as_arg(native_type) });
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