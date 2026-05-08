use std::ffi::c_void;
use std::sync::Arc;
use libffi::middle::*;
use parking_lot::RwLock;
use windows::core::{GUID, HRESULT, Interface, IUnknown};
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
use crate::helpers::ffi_native_type_from_signature;
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
    func: *mut c_void,
    parameter_types: Vec<NativeType>,
    parse_parameter_types: Vec<NativeType>,
    parameters: Vec<ParameterDeclaration>,
    return_type: String,
    pub(crate) declaration: Option<Arc<RwLock<dyn BaseClassDeclarationImpl>>>,
    /// Scratch buffer used when a WinRT method returns a value type (e.g. GUID, Rect)
    /// that is larger than, or cannot be safely aliased through, a single pointer slot.
    return_value_buf: [u8; 128],
    /// Pre-allocated argument buffer reused on every call to avoid per-call heap allocation.
    argument_buf: Vec<NativeValue>,
}

#[inline]
fn call_failure() -> HRESULT {
    // E_FAIL
    HRESULT(0x8000_4005u32 as i32)
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
        is_initializer: bool
    ) -> Self {

        let signature = method.return_type();

        let return_type = Signature::to_string(method.metadata().unwrap(), &signature);


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

        // let mut interface_ptr: *mut c_void = std::ptr::null_mut(); // IActivationFactory

        let vtable = interface.vtable();

        let mut interface_ptr: *mut c_void = std::ptr::null_mut();

        let result = unsafe {
            ((*vtable).QueryInterface)(
                interface.as_raw(),
                &iid,
                &mut interface_ptr as *mut _ as *mut *mut c_void,
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

        let mut parameter_types: Vec<NativeType> = Vec::with_capacity(number_of_abi_parameters);
        let mut parse_parameter_types: Vec<NativeType> = Vec::with_capacity(number_of_parameters);
        parameter_types.push(NativeType::Pointer);

        for parameter in method.parameters().iter() {
            let type_ = parameter.type_();
            let metadata = parameter.metadata().unwrap();

            let signature = Signature::to_string(metadata, &type_);

            let parse_native_type = NativeType::try_from(signature.as_str());
            assert!(parse_native_type.is_ok());
            parse_parameter_types.push(parse_native_type.unwrap());
            parameter_types.push(ffi_native_type_from_signature(signature.as_str()));
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

        let interface = unsafe { IUnknown::from_raw(interface_ptr as *mut c_void) };
        let vtable: *mut *mut c_void = unsafe { std::mem::transmute(interface.vtable()) };
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
            interface,
            parameter_types,
            parse_parameter_types,
            parameters: method.parameters().to_vec(),
            declaration,
            return_type,
            return_value_buf: [0u8; 128],
            argument_buf: Vec::with_capacity(number_of_abi_parameters),
        }
    }

    /// Returns true when the return type is a WinRT value-struct (GUID, Rect, Point, …)
    /// that is NOT a COM reference type.  These must be written into a caller-allocated
    /// buffer rather than into a pointer-sized slot.
    fn is_value_type_return(&self) -> bool {
        matches!(
            self.return_type.as_str(),
            "Guid" | "Rect" | "Matrix3x2" | "Matrix4x4"
        )
    }

    pub fn call(
        &mut self,
        scope: &mut v8::PinScope<'_, '_>,
        args: &v8::FunctionCallbackArguments,
    ) -> (HRESULT, *mut c_void) {

        // Snapshot fields before the mutable borrow of argument_buf begins.
        let number_of_abi_parameters = self.number_of_abi_parameters;
        let is_initializer = self.is_initializer;
        let is_sealed = self.is_sealed;
        let is_void = self.is_void;
        let is_value_type = matches!(
            self.return_type.as_str(),
            "Guid" | "Rect" | "Matrix3x2" | "Matrix4x4"
        );

        // Reuse the pre-allocated buffer — avoids a heap allocation on every call.
        self.argument_buf.clear();

        unsafe { self.argument_buf.push(NativeValue { pointer: std::mem::transmute_copy(&self.interface) }) };

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

            let value = match value {
                Ok(value) => value,
                Err(_) => return (call_failure(), std::ptr::null_mut()),
            };

            self.argument_buf.push(value);
        }

        let mut result: *mut c_void = std::ptr::null_mut();
        let mut composition_outer: *mut c_void = std::ptr::null_mut();
        let mut composition_inner: *mut c_void = std::ptr::null_mut();

        if is_initializer {
            if !is_sealed {
                // WinRT composition constructors receive separate outer/inner pointers.
                unsafe {
                    self.argument_buf.push(NativeValue {
                        pointer: &mut composition_outer as *mut _ as *mut c_void,
                    })
                };
                unsafe {
                    self.argument_buf.push(NativeValue {
                        pointer: &mut composition_inner as *mut _ as *mut c_void,
                    })
                };
            }
            unsafe { self.argument_buf.push(NativeValue { pointer: &mut result as *mut _ as *mut c_void }) };
        } else if !is_void {
            if is_value_type {
                // Value structs (GUID=16B, Rect=16B, …) are written directly into the
                // out-param location — not through a pointer-to-pointer.  Use the
                // pre-allocated scratch buffer so we don't overflow a pointer-sized slot.
                let buf_ptr = self.return_value_buf.as_mut_ptr() as *mut c_void;
                self.argument_buf.push(NativeValue { pointer: buf_ptr });
            } else {
                self.argument_buf.push(NativeValue { pointer: &mut result as *mut _ as *mut c_void });
            }
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

        if is_initializer && !is_sealed && result.is_null() {
            if !composition_inner.is_null() {
                result = composition_inner;
            } else if !composition_outer.is_null() {
                result = composition_outer;
            }
        }

        if !is_initializer && !is_void && is_value_type {
            // Point result at the scratch buffer so the caller can read the bytes.
            result = self.return_value_buf.as_mut_ptr() as *mut c_void;
        }

        (HRESULT(ret), result)
    }
}