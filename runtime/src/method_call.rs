use std::ffi::c_void;
use std::sync::Arc;
use libffi::middle::*;
use parking_lot::RwLock;
use metadata::declarations::declaration::Declaration;
use windows::core::{GUID, HRESULT, Interface, IUnknown};
use windows::Win32::System::WinRT::IActivationFactory;
use metadata::declarations::base_class_declaration::BaseClassDeclarationImpl;
use metadata::declarations::declaration::DeclarationKind;
use metadata::declarations::interface_declaration::generic_interface_declaration::GenericInterfaceDeclaration;
use metadata::declarations::interface_declaration::generic_interface_instance_declaration::GenericInterfaceInstanceDeclaration;
use metadata::declarations::interface_declaration::InterfaceDeclaration;
use metadata::declarations::method_declaration::MethodDeclaration;
use metadata::declarations::parameter_declaration::ParameterDeclaration;
use metadata::declaring_interface_for_method::Metadata;
use metadata::signature::Signature;
use crate::error::AnyError;
use crate::helpers::{
    call_failure, ffi_native_type_from_signature, inherited_interface_method_count,
    normalize_parameter_signature, parse_native_type_from_signature,
};
use crate::value::{ffi_parse_bool_arg, ffi_parse_buffer_arg, ffi_parse_f32_arg, ffi_parse_f64_arg, ffi_parse_function_arg, ffi_parse_i16_arg, ffi_parse_i32_arg, ffi_parse_i64_arg, ffi_parse_i8_arg, ffi_parse_isize_arg, ffi_parse_pointer_arg_with_signature, ffi_parse_string_arg, ffi_parse_struct_arg, ffi_parse_u16_arg, ffi_parse_u32_arg, ffi_parse_u64_arg, ffi_parse_u8_arg, ffi_parse_usize_arg, NativeType, NativeValue};

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
    is_valid: bool,
    /// Scratch buffer used when a WinRT method returns a value type (e.g. GUID, Rect)
    /// that is larger than, or cannot be safely aliased through, a single pointer slot.
    return_value_buf: [u8; 128],
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
        Self::new_with_parent(method, is_sealed, interface, is_initializer, None)
    }

    pub fn new_with_parent(
        method: &MethodDeclaration,
        is_sealed: bool,
        interface: IUnknown,
        is_initializer: bool,
        parent_declaration: Option<Arc<RwLock<dyn Declaration>>>,
    ) -> Self {
        let mut is_valid = true;

        let signature = method.return_type();

        let return_type = Signature::to_string(method.metadata().unwrap(), &signature);


        let number_of_parameters = method.number_of_parameters();

        let mut index = 0 as usize;

        let mut declaration: Option<Arc<RwLock<dyn BaseClassDeclarationImpl>>> = None;
        let mut allow_qi_fallback = false;

        if !is_initializer {
            if let Some(parent_declaration) = parent_declaration.as_ref() {
                let parent = parent_declaration.read();
                match parent.kind() {
                    DeclarationKind::Interface => {
                        if let Some(parent_interface) = parent.as_any().downcast_ref::<InterfaceDeclaration>() {
                            let parent_index = Metadata::find_method_index(
                                method.metadata().unwrap(),
                                parent_interface.base().token(),
                                method.token(),
                            );
                            let inherited_count = inherited_interface_method_count(
                                parent_interface.implemented_interfaces().as_slice(),
                            );
                            declaration = None;
                            return Self::new_bound_to_iid(
                                method,
                                is_sealed,
                                interface,
                                is_initializer,
                                parent_interface.id(),
                                parent_index.saturating_add(6 + inherited_count),
                                declaration,
                            );
                        }
                    }
                    DeclarationKind::GenericInterface => {
                        if let Some(parent_interface) = parent.as_any().downcast_ref::<GenericInterfaceDeclaration>() {
                            let parent_index = Metadata::find_method_index(
                                method.metadata().unwrap(),
                                parent_interface.base().token(),
                                method.token(),
                            );
                            let inherited_count = inherited_interface_method_count(
                                parent_interface.implemented_interfaces().as_slice(),
                            );
                            declaration = None;
                            return Self::new_bound_to_iid(
                                method,
                                is_sealed,
                                interface,
                                is_initializer,
                                parent_interface.id(),
                                parent_index.saturating_add(6 + inherited_count),
                                declaration,
                            );
                        }
                    }
                    DeclarationKind::GenericInterfaceInstance => {
                        if let Some(parent_interface) = parent
                            .as_any()
                            .downcast_ref::<GenericInterfaceInstanceDeclaration>()
                        {
                            let parent_index = Metadata::find_method_index(
                                method.metadata().unwrap(),
                                parent_interface.base().token(),
                                method.token(),
                            );
                            let inherited_count = inherited_interface_method_count(
                                parent_interface.implemented_interfaces().as_slice(),
                            );
                            declaration = None;
                            return Self::new_bound_to_iid(
                                method,
                                is_sealed,
                                interface,
                                is_initializer,
                                parent_interface.id(),
                                parent_index.saturating_add(6 + inherited_count),
                                declaration,
                            );
                        }
                    }
                    _ => {}
                }
            }
        }

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
                        DeclarationKind::GenericInterface => {
                            let ii = ii_lock
                                .as_declaration()
                                .as_any()
                                .downcast_ref::<GenericInterfaceDeclaration>();
                            let ii = ii.unwrap();
                            iid = ii.id();
                            allow_qi_fallback = true;
                        }
                        DeclarationKind::GenericInterfaceInstance => {
                            let ii = ii_lock
                                .as_declaration()
                                .as_any()
                                .downcast_ref::<GenericInterfaceInstanceDeclaration>();
                            let ii = ii.unwrap();
                            iid = ii.id();
                            allow_qi_fallback = true;
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

        if result.is_err() || interface_ptr.is_null() {
            if !(allow_qi_fallback && !is_initializer) {
                is_valid = false;
            }
            interface_ptr = interface.as_raw() as *mut c_void;
        }

        // For constructor calls, composition outer/inner parameters are required only
        // when metadata resolved a specific initializer factory interface.
        let is_composition = is_initializer && declaration.is_some();

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
            let signature = normalize_parameter_signature(signature.as_str()).to_string();

            let parse_native_type = parse_native_type_from_signature(signature.as_str());
            parse_parameter_types.push(parse_native_type.clone());

            let abi_type = if parse_native_type != NativeType::Pointer {
                parse_native_type.clone()
            } else {
                ffi_native_type_from_signature(signature.as_str())
            };
            parameter_types.push(abi_type);
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

        let params = if let Ok(params) = params {
            params
        } else {
            is_valid = false;
            vec![Type::pointer()]
        };

        let cif = Cif::new(
            params,
            Type::i32(),
        );

        let interface = unsafe { IUnknown::from_raw(interface_ptr as *mut c_void) };
        let vtable: *mut *mut c_void = unsafe { std::mem::transmute(interface.vtable()) };
        let func = if is_valid {
            unsafe { *vtable.offset(index as isize) }
        } else {
            std::ptr::null_mut()
        };

        if func.is_null() {
            is_valid = false;
        }

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
            is_valid,
            return_value_buf: [0u8; 128],
        }
    }

    fn new_bound_to_iid(
        method: &MethodDeclaration,
        is_sealed: bool,
        interface: IUnknown,
        is_initializer: bool,
        iid: GUID,
        index: usize,
        declaration: Option<Arc<RwLock<dyn BaseClassDeclarationImpl>>>,
    ) -> Self {
        let mut is_valid = true;

        let signature = method.return_type();
        let return_type = Signature::to_string(method.metadata().unwrap(), &signature);
        let number_of_parameters = method.number_of_parameters();

        let mut interface_ptr: *mut c_void = std::ptr::null_mut();
        let vtable = interface.vtable();

        let result = unsafe {
            ((*vtable).QueryInterface)(
                interface.as_raw(),
                &iid,
                &mut interface_ptr as *mut _ as *mut *mut c_void,
            )
        };

        if result.is_err() || interface_ptr.is_null() {
            if is_initializer {
                is_valid = false;
            }
            interface_ptr = interface.as_raw() as *mut c_void;
        }

        let is_composition = is_initializer && declaration.is_some();
        let is_void = method.is_void();
        let other_params: usize = if is_initializer {
            if is_sealed {
                2
            } else {
                4
            }
        } else if is_void {
            1
        } else {
            2
        };

        let number_of_abi_parameters = number_of_parameters + other_params;

        let mut parameter_types: Vec<NativeType> = Vec::with_capacity(number_of_abi_parameters);
        let mut parse_parameter_types: Vec<NativeType> = Vec::with_capacity(number_of_parameters);
        parameter_types.push(NativeType::Pointer);

        for parameter in method.parameters().iter() {
            let type_ = parameter.type_();
            let metadata = parameter.metadata().unwrap();

            let signature = Signature::to_string(metadata, &type_);
            let signature = normalize_parameter_signature(signature.as_str()).to_string();

            let parse_native_type = parse_native_type_from_signature(signature.as_str());
            parse_parameter_types.push(parse_native_type.clone());

            let abi_type = if parse_native_type != NativeType::Pointer {
                parse_native_type.clone()
            } else {
                ffi_native_type_from_signature(signature.as_str())
            };
            parameter_types.push(abi_type);
        }

        if is_initializer {
            if is_composition {
                parameter_types.push(NativeType::Pointer);
                parameter_types.push(NativeType::Pointer);
            }
            parameter_types.push(NativeType::Pointer);
        } else if !is_void {
            parameter_types.push(NativeType::Pointer);
        }

        let params = parameter_types
            .iter()
            .cloned()
            .map(libffi::middle::Type::try_from)
            .collect::<std::result::Result<Vec<Type>, AnyError>>();

        let params = if let Ok(params) = params {
            params
        } else {
            is_valid = false;
            vec![Type::pointer()]
        };

        let cif = Cif::new(params, Type::i32());

        let interface = unsafe { IUnknown::from_raw(interface_ptr as *mut c_void) };
        let vtable: *mut *mut c_void = unsafe { std::mem::transmute(interface.vtable()) };
        let func = if is_valid {
            unsafe { *vtable.offset(index as isize) }
        } else {
            std::ptr::null_mut()
        };

        if func.is_null() {
            is_valid = false;
        }

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
            is_valid,
            return_value_buf: [0u8; 128],
        }
    }

    /// Returns true when the return type is a WinRT value-struct (GUID, Rect, Point, …)
    /// that is NOT a COM reference type.  These must be written into a caller-allocated
    /// buffer rather than into a pointer-sized slot.
    fn is_value_type_return(&self) -> bool {
        matches!(
            self.return_type.as_str(),
            "Guid"
                | "Rect"
                | "Matrix3x2"
                | "Matrix4x4"
                | "EventRegistrationToken"
                | "Windows.Foundation.EventRegistrationToken"
        )
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
        if !self.is_valid || self.func.is_null() {
            return (call_failure(), std::ptr::null_mut());
        }

        let number_of_abi_parameters = self.number_of_abi_parameters;

        let mut arguments: Vec<NativeValue> = Vec::with_capacity(number_of_abi_parameters);
        let mut pointer_keepalive: Vec<IUnknown> = Vec::new();

        unsafe { arguments.push(NativeValue { pointer: std::mem::transmute_copy(&self.interface) }) };

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
                    let expected_signature = self
                        .parameters
                        .get(i)
                        .and_then(|parameter| {
                            parameter
                                .metadata()
                                .map(|metadata| {
                                    let signature = Signature::to_string(metadata, &parameter.type_());
                                    normalize_parameter_signature(signature.as_str()).to_string()
                                })
                        });

                    match ffi_parse_pointer_arg_with_signature(
                        scope,
                        value,
                        expected_signature.as_deref(),
                    ) {
                        Ok((native_value, keepalive)) => {
                            if let Some(keepalive) = keepalive {
                                pointer_keepalive.push(keepalive);
                            }
                            Ok(native_value)
                        }
                        Err(error) => Err(error),
                    }
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

            arguments.push(value);
        }

        let mut result: *mut c_void = std::ptr::null_mut();
        let mut composition_inner: *mut c_void = std::ptr::null_mut();


        if self.is_initializer {

            if !self.is_sealed {
                // Composable ctor CreateInstance takes:
                // (outer: IInspectable*, inner: IInspectable**)
                // For non-aggregated construction from JS, pass null outer.
                arguments.push(NativeValue {
                    pointer: std::ptr::null_mut(),
                });
                unsafe {
                    arguments.push(NativeValue {
                        pointer: &mut composition_inner as *mut _ as *mut c_void,
                    })
                };
            }

            unsafe { arguments.push(NativeValue { pointer: &mut result as *mut _ as *mut c_void }) };
        } else {
            if !self.is_void {
                if self.is_value_type_return() {
                    // Value structs (GUID=16B, Rect=16B, …) are written directly into the
                    // out-param location — not through a pointer-to-pointer.  Use the
                    // pre-allocated scratch buffer so we don't overflow a pointer-sized slot.
                    let buf_ptr = self.return_value_buf.as_mut_ptr() as *mut c_void;
                    arguments.push(NativeValue { pointer: buf_ptr });
                } else {
                    arguments.push(NativeValue { pointer: &mut result as *mut _ as *mut c_void });
                }
            }
        }

        //let mut func = std::ptr::null_mut();

       // get_method(&self.interface, self.index, addr_of_mut!(func));

        let mut call_args: Vec<Arg> = Vec::with_capacity(arguments.len());
        for (i, v) in arguments.iter().enumerate() {
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

        if self.is_initializer && !self.is_sealed && result.is_null() {
            if !composition_inner.is_null() {
                result = composition_inner;
            }
        }

        if !self.is_initializer && !self.is_void && self.is_value_type_return() {
            // Point result at the scratch buffer so the caller can read the bytes.
            result = self.return_value_buf.as_mut_ptr() as *mut c_void;
        }

        (HRESULT(ret), result)
    }
}
