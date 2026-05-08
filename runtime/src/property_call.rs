use std::ffi::c_void;
use std::sync::Arc;
use libffi::middle::*;
use parking_lot::RwLock;
use windows::core::{GUID, HRESULT, Interface, IUnknown};
use windows::Win32::System::WinRT::IActivationFactory;
use metadata::declarations::base_class_declaration::BaseClassDeclarationImpl;
use metadata::declarations::class_declaration::ClassDeclaration;
use metadata::declarations::declaration::{Declaration, DeclarationKind};
use metadata::declarations::enum_declaration::EnumDeclaration;
use metadata::declarations::interface_declaration::generic_interface_declaration::GenericInterfaceDeclaration;
use metadata::declarations::interface_declaration::generic_interface_instance_declaration::GenericInterfaceInstanceDeclaration;
use metadata::declarations::interface_declaration::InterfaceDeclaration;
use metadata::declarations::parameter_declaration::ParameterDeclaration;
use metadata::declarations::property_declaration::PropertyDeclaration;
use metadata::declaring_interface_for_method::Metadata;
use metadata::meta_data_reader::MetadataReader;
use metadata::signature::Signature;
use crate::error::AnyError;
use crate::value::{ffi_parse_bool_arg, ffi_parse_buffer_arg, ffi_parse_f32_arg, ffi_parse_f64_arg, ffi_parse_function_arg, ffi_parse_i16_arg, ffi_parse_i32_arg, ffi_parse_i64_arg, ffi_parse_i8_arg, ffi_parse_isize_arg, ffi_parse_pointer_arg_with_signature, ffi_parse_string_arg, ffi_parse_struct_arg, ffi_parse_u16_arg, ffi_parse_u32_arg, ffi_parse_u64_arg, ffi_parse_u8_arg, ffi_parse_usize_arg, NativeType, NativeValue};

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
    is_valid: bool,
}

#[inline]
fn ffi_native_type_from_signature(signature: &str) -> NativeType {
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

#[inline]
fn call_failure() -> HRESULT {
    // E_FAIL
    HRESULT(0x8000_4005u32 as i32)
}

#[inline]
fn normalize_parameter_signature(signature: &str) -> &str {
    if signature.starts_with("Var!") || signature.starts_with("MVar!") {
        return "Object";
    }

    signature
}

fn inherited_interface_method_count(interfaces: &[&InterfaceDeclaration]) -> usize {
    let mut count = 0;
    for inherited in interfaces {
        count += inherited.methods().len();
        count += inherited_interface_method_count(inherited.implemented_interfaces().as_slice());
    }
    count
}

#[inline]
fn describe_js_value(scope: &mut v8::PinScope<'_, '_>, value: v8::Local<v8::Value>) -> String {
    if value.is_undefined() {
        return "undefined".to_string();
    }
    if value.is_null() {
        return "null".to_string();
    }
    if value.is_boolean() {
        return "boolean".to_string();
    }
    if value.is_int32() {
        return "int32".to_string();
    }
    if value.is_uint32() {
        return "uint32".to_string();
    }
    if value.is_number() {
        return "number".to_string();
    }
    if value.is_big_int() {
        return "bigint".to_string();
    }
    if value.is_string() {
        return "string".to_string();
    }
    if value.is_function() {
        return "function".to_string();
    }
    if value.is_array() {
        return "array".to_string();
    }
    if value.is_object() {
        if let Some(obj) = value.to_object(scope) {
            if obj.internal_field_count() > 0 {
                return format!("object(internal_fields={})", obj.internal_field_count());
            }
        }
        return "object".to_string();
    }

    "unknown".to_string()
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
        Self::new_with_parent(property, is_setter, interface, is_initializer, None)
    }

    pub fn new_with_parent(
        property: &PropertyDeclaration,
        is_setter: bool,
        interface: IUnknown,
        is_initializer: bool,
        parent_declaration: Option<Arc<RwLock<dyn Declaration>>>,
    ) -> Self {
        let mut is_valid = true;
        let method = if is_setter {
            property.setter().unwrap()
        } else {
            property.getter()
        };

        let number_of_parameters = method.number_of_parameters();

        let mut index = 0 as usize;
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
                            return Self::new_bound_to_iid(
                                property,
                                is_setter,
                                interface,
                                is_initializer,
                                parent_interface.id(),
                                parent_index.saturating_add(6 + inherited_count),
                                None,
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
                            return Self::new_bound_to_iid(
                                property,
                                is_setter,
                                interface,
                                is_initializer,
                                parent_interface.id(),
                                parent_index.saturating_add(6 + inherited_count),
                                None,
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
                            return Self::new_bound_to_iid(
                                property,
                                is_setter,
                                interface,
                                is_initializer,
                                parent_interface.id(),
                                parent_index.saturating_add(6 + inherited_count),
                                None,
                            );
                        }
                    }
                    _ => {}
                }
            }
        }

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

        let mut interface_ptr: *const c_void = std::ptr::null_mut(); // IActivationFactory

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
            eprintln!(
                "[runtime][property_call] QueryInterface failed iid={:?} hr={} null_ptr={}",
                iid,
                result.0,
                interface_ptr.is_null()
            );
            if allow_qi_fallback && !is_initializer {
                eprintln!(
                    "[runtime][property_call] continuing with original interface pointer for generic binding"
                );
            } else {
                is_valid = false;
            }
            interface_ptr = interface.as_raw() as *mut c_void;
        }

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

        let mut parameter_types: Vec<NativeType> = Vec::with_capacity(number_of_abi_parameters);
        let mut parse_parameter_types: Vec<NativeType> = Vec::with_capacity(number_of_parameters);
        parameter_types.push(NativeType::Pointer);

        for parameter in method.parameters().iter() {
            let type_ = parameter.type_();
            let metadata = parameter.metadata().unwrap();

            let signature = Signature::to_string(metadata, &type_);
            let signature = normalize_parameter_signature(signature.as_str()).to_string();

            let parse_native_type = parse_native_type_from_signature(signature.as_str());
            if parse_native_type == NativeType::Pointer
                && should_warn_pointer_fallback(signature.as_str())
            {
                eprintln!(
                    "[runtime][property_call] parse type failed signature={} (defaulting parse type to pointer)",
                    signature
                );
            }
            parse_parameter_types.push(parse_native_type.clone());

            let abi_type = if parse_native_type != NativeType::Pointer {
                parse_native_type.clone()
            } else {
                ffi_native_type_from_signature(signature.as_str())
            };

            eprintln!(
                "[runtime][property_call] bind property={} setter={} param_sig={} parse_type={:?} abi_type={:?}",
                property.name(),
                is_setter,
                signature,
                parse_native_type,
                abi_type
            );

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
            eprintln!("[runtime][property_call] ffi parameter conversion failed");
            is_valid = false;
            vec![Type::pointer()]
        };

        let cif = Cif::new(
            params,
            Type::i32(),
        );

        let parent_interface = interface.clone();

        let interface = unsafe { IUnknown::from_raw(interface_ptr as *mut c_void) };
        let vtable: *mut *mut c_void = unsafe { std::mem::transmute(interface.vtable()) };
        let func = if is_valid {
            unsafe { *vtable.offset(index as isize) }
        } else {
            std::ptr::null_mut()
        };

        if func.is_null() {
            is_valid = false;
            eprintln!(
                "[runtime][property_call] unresolved function pointer index={} setter={}",
                index,
                is_setter
            );
        }

        eprintln!(
            "[runtime][property_call] ready property={} setter={} valid={} vtable_index={} params={} abi_params={} return_type={} iid={:?}",
            property.name(),
            is_setter,
            is_valid,
            index,
            number_of_parameters,
            number_of_abi_parameters,
            return_type,
            iid
        );


        Self {
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
            is_valid,
        }
    }

    fn new_bound_to_iid(
        property: &PropertyDeclaration,
        is_setter: bool,
        interface: IUnknown,
        is_initializer: bool,
        iid: GUID,
        index: usize,
        declaration: Option<Arc<RwLock<dyn BaseClassDeclarationImpl>>>,
    ) -> Self {
        let mut is_valid = true;
        let method = if is_setter {
            property.setter().unwrap()
        } else {
            property.getter()
        };

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
            eprintln!(
                "[runtime][property_call] QueryInterface failed iid={:?} hr={} null_ptr={}",
                iid,
                result.0,
                interface_ptr.is_null()
            );
            if !is_initializer {
                eprintln!(
                    "[runtime][property_call] continuing with existing interface pointer for parent-bound call"
                );
            } else {
                is_valid = false;
            }
            interface_ptr = interface.as_raw() as *mut c_void;
        }

        let is_sealed = method.is_sealed();
        let is_void = method.is_void();
        let signature = method.return_type();
        let return_type = Signature::to_string(method.metadata().unwrap(), &signature);
        let other_params: usize = if is_initializer {
            if is_sealed { 2 } else { 4 }
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

        if !is_initializer && !is_void {
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
        let parent_interface = interface.clone();
        let interface = unsafe { IUnknown::from_raw(interface_ptr as *mut c_void) };
        let vtable: *mut *mut c_void = unsafe { std::mem::transmute(interface.vtable()) };
        let func = if is_valid {
            unsafe { *vtable.offset(index as isize) }
        } else {
            std::ptr::null_mut()
        };

        let mut final_valid = is_valid;
        if func.is_null() {
            final_valid = false;
            eprintln!(
                "[runtime][property_call] unresolved function pointer index={} setter={}",
                index,
                is_setter
            );
        }

        eprintln!(
            "[runtime][property_call] ready property={} setter={} valid={} vtable_index={} params={} abi_params={} return_type={} iid={:?}",
            property.name(),
            is_setter,
            final_valid,
            index,
            number_of_parameters,
            number_of_abi_parameters,
            return_type,
            iid
        );

        Self {
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
            is_valid: final_valid,
        }
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
            eprintln!(
                "[runtime][property_call] refusing call on invalid binding index={} setter={} return_type={}",
                self.index,
                self.is_setter,
                self.return_type
            );
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

            let abi_type = self
                .parameter_types
                .get(i + 1)
                .cloned()
                .unwrap_or(NativeType::Pointer);

            let js_kind = describe_js_value(scope, value);
            eprintln!(
                "[runtime][property_call] arg property_call_index={} parse_type={:?} abi_type={:?} expected_sig={} js_kind={}",
                i,
                native_type,
                abi_type,
                expected_signature.as_deref().unwrap_or("<unknown>"),
                js_kind
            );

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
                        Err(error) => {
                            eprintln!(
                                "[runtime][property_call] pointer arg parse failed index={} signature={} err={}",
                                i,
                                expected_signature
                                    .as_deref()
                                    .unwrap_or("<unknown>"),
                                error
                            );
                            Err(error)
                        }
                    }
                }
                NativeType::Buffer => {
                    ffi_parse_buffer_arg(scope, value)
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
                Err(error) => {
                    eprintln!(
                        "[runtime][property_call] arg parse failed index={} setter={} return_type={} err={}",
                        i,
                        self.is_setter,
                        self.return_type,
                        error
                    );
                    return (call_failure(), std::ptr::null_mut());
                }
            };

            arguments.push(value);
        }

        let mut result: *mut c_void = std::ptr::null_mut();


        if self.is_initializer {
            // arguments.push(result_ptr as *mut c_void);
        } else {
            if !self.is_void {
                arguments.push(NativeValue { pointer: &mut result as *mut _ as *mut c_void });
            }
        }

        // let mut func = std::ptr::null_mut();
        //
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

        (HRESULT(ret), result)
    }
}

#[inline]
fn parse_native_type_from_signature(signature: &str) -> NativeType {
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

#[inline]
fn should_warn_pointer_fallback(signature: &str) -> bool {
    if signature.starts_with("Var!") || signature.starts_with("MVar!") {
        return false;
    }

    if signature == "Object" {
        return false;
    }

    if let Some(declaration) = MetadataReader::find_by_name(signature) {
        let lock = declaration.read();
        return !matches!(
            lock.kind(),
            DeclarationKind::Class
                | DeclarationKind::Interface
                | DeclarationKind::GenericInterface
                | DeclarationKind::GenericInterfaceInstance
                | DeclarationKind::Delegate
                | DeclarationKind::GenericDelegate
                | DeclarationKind::GenericDelegateInstance
                | DeclarationKind::Struct
                | DeclarationKind::Event
        );
    }

    true
}