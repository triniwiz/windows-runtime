use std::ffi::c_void;
use std::sync::Arc;
use libffi::middle::*;
use parking_lot::RwLock;
use windows::core::{GUID, HRESULT, Interface, IUnknown, IInspectable};
use windows::Win32::System::WinRT::IActivationFactory;
use windows::Win32::System::WinRT::Metadata::CorTokenType;
use metadata::declarations::base_class_declaration::BaseClassDeclarationImpl;
use metadata::declarations::declaration::DeclarationKind;
use metadata::declarations::struct_declaration::StructDeclaration;
use metadata::declarations::interface_declaration::generic_interface_instance_declaration::GenericInterfaceInstanceDeclaration;
use metadata::declarations::declaration::Declaration;
use metadata::declarations::interface_declaration::InterfaceDeclaration;
use metadata::declarations::parameter_declaration::ParameterDeclaration;
use metadata::declarations::property_declaration::PropertyDeclaration;
use metadata::declarations::class_declaration::ClassDeclaration;
use metadata::declaring_interface_for_method::Metadata;
use metadata::declarations::interface_declaration::generic_interface_declaration::GenericInterfaceDeclaration;
use metadata::meta_data_reader::MetadataReader;
use metadata::signature::Signature;
use crate::error::AnyError;
use crate::helpers::{ffi_native_type_from_signature, strip_generic_suffix};
use std::panic::{catch_unwind, AssertUnwindSafe};
use crate::value::{append_struct_field_bytes, ffi_parse_bool_arg, ffi_parse_buffer_arg_with_length, ffi_parse_f32_arg, ffi_parse_f64_arg, ffi_parse_function_arg, ffi_parse_i16_arg, ffi_parse_i32_arg, ffi_parse_i64_arg, ffi_parse_i8_arg, ffi_parse_isize_arg, ffi_parse_pointer_arg, ffi_parse_query_interface_arg, ffi_parse_string_arg, ffi_parse_struct_arg, ffi_parse_u16_arg, ffi_parse_u32_arg, ffi_parse_u64_arg, ffi_parse_u8_arg, ffi_parse_usize_arg, NativeType, NativeValue, read_value_from_ptr, write_v8_value_to_ptr};

fn substitute_type_vars(s: &str, type_args: &[String]) -> String {
    if type_args.is_empty() {
        return s.to_string();
    }
    let mut result = s.to_string();
    for (i, arg) in type_args.iter().enumerate() {
        result = result.replace(&format!("Var!{}", i), arg.as_str());
    }
    result
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
    cif: Cif,
    func: *mut c_void,
    parent_interface: IUnknown,
    interface: IUnknown,
    parameter_types: Vec<NativeType>,
    parse_parameter_types: Vec<NativeType>,
    parameters: Vec<ParameterDeclaration>,
    return_type: String,
    pub(crate) declaration: Option<Arc<RwLock<dyn BaseClassDeclarationImpl>>>,
    type_args: Vec<String>,
    /// Pre-allocated argument buffer reused on every call to avoid per-call heap allocation.
    argument_buf: Vec<NativeValue>,
    /// Per-call parse-type tracker reused to avoid per-call heap allocation.
    argument_parse_types: Vec<Option<NativeType>>,
    /// Scratch buffer used when a WinRT property/method returns a value type
    /// or scalar that must be written into caller-owned storage.
    return_value_buf: [u8; 128],
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

        let pre_index = index;

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
            let abi_native = crate::helpers::struct_native_type_for_sig(signature.as_str())
                .unwrap_or_else(|| ffi_native_type_from_signature(signature.as_str()));
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
        let vtable_struct = interface.vtable();
        let vtable_ptr: *mut *mut c_void = unsafe { std::mem::transmute(vtable_struct) };

        let mut inspectable_ptr: *mut c_void = std::ptr::null_mut();
        let supports_iinspectable = unsafe {
            ((*vtable_struct).QueryInterface)(
                interface.as_raw(),
                &IInspectable::IID,
                &mut inspectable_ptr as *mut _ as *mut *mut c_void,
            )
        };

        let base_offset = if supports_iinspectable.is_ok() && !inspectable_ptr.is_null() {
            unsafe { IUnknown::from_raw(inspectable_ptr as *mut c_void); }
            6
        } else {
            3
        };

        index = index.saturating_add(base_offset);

        let func = unsafe { *vtable_ptr.offset(index as isize) };



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
            type_args: Vec::new(),
            return_value_buf: [0u8; 128],
            argument_buf: Vec::with_capacity(number_of_abi_parameters),
            argument_parse_types: Vec::with_capacity(number_of_abi_parameters),
        })
    }

    /// Constructor for calling properties directly on an interface (not via a class).
    /// Uses the provided `declaring_iid` for QI and applies `type_args` substitution
    /// so that Var!N placeholders in generic interface signatures resolve correctly.
    pub fn new_for_interface(
        property: &PropertyDeclaration,
        is_setter: bool,
        interface: IUnknown,
        is_initializer: bool,
        declaring_iid: GUID,
        type_args: Vec<String>,
    ) -> Option<Self> {
        let method = if is_setter {
            property.setter().unwrap()
        } else {
            property.getter()
        };

        let number_of_parameters = method.number_of_parameters();
        let mut index = 0_usize;

        // Derive vtable index from the method's position in its containing interface.
        if let Some(metadata) = method.metadata() {
            let containing_type = CorTokenType(
                Metadata::get_method_containing_class_token(metadata, method.token()) as i32,
            );
            if containing_type.0 != 0 {
                index = Metadata::find_method_index(metadata, containing_type, method.token());
            }
        }

        let vtable = interface.vtable();

        let mut interface_ptr: *mut c_void = std::ptr::null_mut();
        let result = unsafe {
            ((*vtable).QueryInterface)(
                interface.as_raw(),
                &declaring_iid,
                &mut interface_ptr as *mut _ as *mut *mut c_void,
            )
        };

        if result.is_err() || interface_ptr.is_null() {
            return None;
        }

        let is_sealed = method.is_sealed();
        let is_void = method.is_void();

        let signature = method.return_type();
        let raw_return_type = Signature::to_string(method.metadata()?, &signature);
        let return_type = substitute_type_vars(&raw_return_type, &type_args);

        let other_params: usize = if is_void { 1 } else { 2 };

        let mut parameter_types: Vec<NativeType> = Vec::with_capacity(number_of_parameters + other_params + 2);
        let mut parse_parameter_types: Vec<NativeType> = Vec::with_capacity(number_of_parameters);
        parameter_types.push(NativeType::Pointer);

        for parameter in method.parameters().iter() {
            let type_ = parameter.type_();
            let metadata = parameter.metadata()?;
            let raw_sig = Signature::to_string(metadata, &type_);
            let signature = substitute_type_vars(&raw_sig, &type_args);

            let parse_native_type = NativeType::try_from(signature.as_str()).ok()?;
            parse_parameter_types.push(parse_native_type);
            let abi_native = crate::helpers::struct_native_type_for_sig(signature.as_str())
                .unwrap_or_else(|| crate::helpers::ffi_native_type_from_signature(signature.as_str()));
            if matches!(abi_native, NativeType::Buffer) {
                parameter_types.push(NativeType::U32);
                parameter_types.push(NativeType::Buffer);
            } else {
                parameter_types.push(abi_native);
            }
        }

        if !is_void {
            parameter_types.push(NativeType::Pointer);
        } else {
            // void setter — no return-value pointer slot
        }

        let number_of_abi_parameters = parameter_types.len();

        let params = parameter_types
            .iter()
            .cloned()
            .map(libffi::middle::Type::try_from)
            .collect::<std::result::Result<Vec<Type>, _>>();
        let params = params.ok()?;

        let cif = Cif::new(params, Type::i32());

        let parent_interface = interface.clone();
        let interface = unsafe { IUnknown::from_raw(interface_ptr as *mut c_void) };
        let vtable_struct = interface.vtable();
        let vtable_ptr: *mut *mut c_void = unsafe { std::mem::transmute(vtable_struct) };

        let mut inspectable_ptr: *mut c_void = std::ptr::null_mut();
        let supports_iinspectable = unsafe {
            ((*vtable_struct).QueryInterface)(
                interface.as_raw(),
                &IInspectable::IID,
                &mut inspectable_ptr as *mut _ as *mut *mut c_void,
            )
        };

        let base_offset = if supports_iinspectable.is_ok() && !inspectable_ptr.is_null() {
            unsafe { IUnknown::from_raw(inspectable_ptr as *mut c_void); }
            6
        } else {
            3
        };

        index = index.saturating_add(base_offset);
        let func = unsafe { *vtable_ptr.offset(index as isize) };

        Some(Self {
            index,
            number_of_parameters,
            number_of_abi_parameters,
            is_initializer,
            is_sealed,
            is_void,
            iid: declaring_iid,
            cif,
            func,
            parent_interface,
            interface,
            parameter_types,
            parse_parameter_types,
            parameters: method.parameters().to_vec(),
            declaration: None,
            return_type,
            is_setter,
            type_args,
            return_value_buf: [0u8; 128],
            argument_buf: Vec::with_capacity(number_of_abi_parameters),
            argument_parse_types: Vec::with_capacity(number_of_abi_parameters),
        })
    }

    /// Call a plain method (not a property getter/setter) directly on an interface,
    /// using `declaring_iid` for QI and substituting `type_args` for Var!N placeholders.
    pub fn new_method_for_interface(
        method: &metadata::declarations::method_declaration::MethodDeclaration,
        interface: IUnknown,
        declaring_iid: GUID,
        type_args: Vec<String>,
    ) -> Option<Self> {
        let number_of_parameters = method.number_of_parameters();
        let mut index = 0_usize;

        if let Some(metadata) = method.metadata() {
            let containing_type = windows::Win32::System::WinRT::Metadata::CorTokenType(
                metadata::declaring_interface_for_method::Metadata::get_method_containing_class_token(metadata, method.token()) as i32,
            );
            if containing_type.0 != 0 {
                index = metadata::declaring_interface_for_method::Metadata::find_method_index(metadata, containing_type, method.token());
            }
        }

        let vtable = interface.vtable();
        let mut interface_ptr: *mut c_void = std::ptr::null_mut();
        let result = unsafe {
            ((*vtable).QueryInterface)(
                interface.as_raw(),
                &declaring_iid,
                &mut interface_ptr as *mut _ as *mut *mut c_void,
            )
        };

        if result.is_err() || interface_ptr.is_null() {
            return None;
        }

        let is_sealed = method.is_sealed();
        let is_void = method.is_void();

        let signature = method.return_type();
        let raw_return_type = Signature::to_string(method.metadata()?, &signature);
        let return_type = substitute_type_vars(&raw_return_type, &type_args);

        let other_params: usize = if is_void { 1 } else { 2 };

        let mut parameter_types: Vec<NativeType> = Vec::with_capacity(number_of_parameters + other_params + 2);
        let mut parse_parameter_types: Vec<NativeType> = Vec::with_capacity(number_of_parameters);
        parameter_types.push(NativeType::Pointer);

        for parameter in method.parameters().iter() {
            let type_ = parameter.type_();
            let metadata = parameter.metadata()?;
            let raw_sig = Signature::to_string(metadata, &type_);
            let sig = substitute_type_vars(&raw_sig, &type_args);
            let parse_native_type = NativeType::try_from(sig.as_str()).ok()?;
            parse_parameter_types.push(parse_native_type);
            let abi_native = crate::helpers::struct_native_type_for_sig(sig.as_str())
                .unwrap_or_else(|| crate::helpers::ffi_native_type_from_signature(sig.as_str()));
            if matches!(abi_native, NativeType::Buffer) {
                parameter_types.push(NativeType::U32);
                parameter_types.push(NativeType::Buffer);
            } else {
                parameter_types.push(abi_native);
            }
        }

        if !is_void {
            parameter_types.push(NativeType::Pointer);
        }

        let number_of_abi_parameters = parameter_types.len();

        let params = parameter_types
            .iter()
            .cloned()
            .map(libffi::middle::Type::try_from)
            .collect::<std::result::Result<Vec<Type>, _>>();
        let params = params.ok()?;

        let cif = Cif::new(params, Type::i32());

        let parent_interface = interface.clone();
        let interface = unsafe { IUnknown::from_raw(interface_ptr as *mut c_void) };
        let vtable_struct = interface.vtable();
        let vtable_ptr: *mut *mut c_void = unsafe { std::mem::transmute(vtable_struct) };

        let mut inspectable_ptr: *mut c_void = std::ptr::null_mut();
        let supports_iinspectable = unsafe {
            ((*vtable_struct).QueryInterface)(
                interface.as_raw(),
                &IInspectable::IID,
                &mut inspectable_ptr as *mut _ as *mut *mut c_void,
            )
        };

        let base_offset = if supports_iinspectable.is_ok() && !inspectable_ptr.is_null() {
            unsafe { IUnknown::from_raw(inspectable_ptr as *mut c_void); }
            6
        } else {
            3
        };

        index = index.saturating_add(base_offset);
        let func = unsafe { *vtable_ptr.offset(index as isize) };

        Some(Self {
            index,
            number_of_parameters,
            number_of_abi_parameters,
            is_initializer: false,
            is_sealed,
            is_void,
            iid: declaring_iid,
            cif,
            func,
            parent_interface,
            interface,
            parameter_types,
            parse_parameter_types,
            parameters: method.parameters().to_vec(),
            declaration: None,
            return_type,
            is_setter: false,
            type_args,
            return_value_buf: [0u8; 128],
            argument_buf: Vec::with_capacity(number_of_abi_parameters),
            argument_parse_types: Vec::with_capacity(number_of_abi_parameters),
        })
    }

    pub fn call<'s>(
        &mut self,
        scope: &mut v8::PinScope<'s, '_>,
        args: &v8::FunctionCallbackArguments,
    ) -> (HRESULT, *mut c_void, Vec<v8::Local<'s, v8::Value>>) {
        let mut values = Vec::with_capacity(self.parse_parameter_types.len());
        for index in 0..self.parse_parameter_types.len() {
            values.push(args.get(index as i32));
        }

        self.call_with_values(scope, &values)
    }

    pub fn call_with_values<'s>(
        &mut self,
        scope: &mut v8::PinScope<'s, '_>,
        values: &[v8::Local<v8::Value>],
    ) -> (HRESULT, *mut c_void, Vec<v8::Local<'s, v8::Value>>) {
        let is_void = self.is_void;

        let is_value_type = if self.return_type == "Guid" {
            true
        } else if self.return_type.contains('.') {
            let lookup = strip_generic_suffix(self.return_type.as_str());
            MetadataReader::find_by_name(lookup)
                .map_or(false, |dec| matches!(dec.read().kind(), DeclarationKind::Struct))
        } else {
            false
        };

        let is_scalar_return = matches!(self.return_type.as_str(),
            "UInt8" | "Int8" | "UInt16" | "Int16" |
            "UInt32" | "Int32" | "UInt64" | "Int64" |
            "USize" | "ISize" | "Single" | "Double" |
            "Boolean" | "Char16"
        );

        // HSTRING out-params must also land in a stable buffer; the local
        // `result` variable goes out of scope before the caller can read it.
        let is_string_return = self.return_type.as_str() == "String";

        self.argument_buf.clear();
        self.argument_parse_types.clear();
        let mut queried_interfaces: Vec<IUnknown> = Vec::new();
        let mut struct_scratch: Vec<Vec<u8>> = Vec::new();
        let mut out_slots: Vec<(usize, NativeType, Option<String>)> = Vec::new();

        self.argument_buf.push(NativeValue { pointer: self.interface.as_raw() as *mut c_void });
        self.argument_parse_types.push(None);

        for (i, native_type) in self.parse_parameter_types.iter().enumerate() {
            let value = values.get(i).copied().unwrap_or_else(|| v8::undefined(scope).into());

            let parameter = &self.parameters[i];
            let param_sig_opt = parameter.metadata().map(|m| Signature::to_string(m, &parameter.type_()));
            let is_sig_byref = param_sig_opt.as_ref().map_or(false, |s| s.starts_with("ByRef "));
            if parameter.is_out() || (is_sig_byref && values.get(i).is_none()) {
                let slot_index = self.argument_buf.len();
                let slot_size = match native_type {
                    NativeType::Struct(_) => native_type.size(),
                    NativeType::Pointer | NativeType::Buffer | NativeType::Function | NativeType::String => std::mem::size_of::<usize>(),
                    _ => native_type.size(),
                };
                let mut buf: Vec<u8> = vec![0u8; slot_size];
                let ptr = buf.as_mut_ptr() as *mut c_void;
                struct_scratch.push(buf);
                self.argument_buf.push(NativeValue { pointer: ptr });
                self.argument_parse_types.push(None);
                // Initialize from caller-provided value if present (in/out semantics).
                if let Some(init_val) = values.get(i).copied() {
                    if !init_val.is_undefined() && !init_val.is_null() {
                        match write_v8_value_to_ptr(scope, init_val, ptr, native_type) {
                            Ok(parse_opt) => {
                                if let Some(pt) = parse_opt {
                                    if let Some(slot) = self.argument_parse_types.get_mut(slot_index) {
                                        *slot = Some(pt);
                                    }
                                }
                            }
                            Err(_) => return (call_failure(), std::ptr::null_mut(), Vec::new()),
                        }
                    }
                }

                let sig = param_sig_opt;
                out_slots.push((slot_index, native_type.clone(), sig));
                continue;
            }

            let value = match *native_type {
                NativeType::Void => { return (call_failure(), std::ptr::null_mut(), Vec::new()) }
                NativeType::Bool => ffi_parse_bool_arg(value),
                NativeType::U8 => ffi_parse_u8_arg(value),
                NativeType::I8 => ffi_parse_i8_arg(value),
                NativeType::U16 => ffi_parse_u16_arg(value),
                NativeType::I16 => ffi_parse_i16_arg(value),
                NativeType::U32 => ffi_parse_u32_arg(value),
                NativeType::I32 => ffi_parse_i32_arg(value),
                NativeType::U64 => ffi_parse_u64_arg(scope, value),
                NativeType::I64 => ffi_parse_i64_arg(scope, value),
                NativeType::USize => ffi_parse_usize_arg(scope, value),
                NativeType::ISize => ffi_parse_isize_arg(scope, value),
                NativeType::F32 => ffi_parse_f32_arg(value),
                NativeType::F64 => ffi_parse_f64_arg(value),
                NativeType::Pointer => {
                    let parameter = &self.parameters[i];
                    let parameter_signature = substitute_type_vars(
                        &Signature::to_string(parameter.metadata().unwrap(), &parameter.type_()),
                        &self.type_args,
                    );

                    // IReference<T> parameters: box JS primitives with the correct Create* call
                    // so XAML receives the right typed IPropertyValue (e.g. IReference<Double>).
                    if let Some(inner) = crate::helpers::ireference_inner_type(&parameter_signature) {
                        if let Some(nv) = crate::value::box_as_ireference(scope, value, inner) {
                            Ok(nv)
                        } else {
                            ffi_parse_pointer_arg(scope, value)
                        }
                    } else if parameter_signature.contains('.') {
                        let lookup_name = crate::helpers::strip_generic_suffix(parameter_signature.as_str());

                        if let Some(declaration) = MetadataReader::find_by_name(lookup_name) {
                            if declaration.read().kind() == DeclarationKind::Struct {
                                // ArrayBuffer / ArrayBufferView → raw bytes pointer
                                if value.is_array_buffer() || value.is_array_buffer_view() {
                                    ffi_parse_struct_arg(scope, value)
                                } else if value.is_object() {
                                    let obj = value.to_object(scope).unwrap();
                                    // Struct instance created by `new T(...)` has an internal field
                                    let has_internal = obj.get_internal_field(scope, 0)
                                        .map(|f| !unsafe { f.cast::<v8::External>() }.value().is_null())
                                        .unwrap_or(false);
                                    if has_internal {
                                        // Already a struct instance — extract the bytes pointer
                                        ffi_parse_pointer_arg(scope, value)
                                    } else {
                                        // Plain JS object {A:255, R:0, G:0, B:0} — build bytes from named fields
                                        let fields_info: Vec<(String, NativeType)> = {
                                            let lock = declaration.read();
                                            lock.as_any().downcast_ref::<StructDeclaration>()
                                                .map(|sd| {
                                                    sd.fields().iter().filter_map(|f| {
                                                        let m = f.base().metadata()?;
                                                        let ts = Signature::to_string(m, &f.type_());
                                                        let nt = NativeType::try_from(ts.as_str()).ok()?;
                                                        Some((f.name().to_string(), nt))
                                                    }).collect()
                                                })
                                                .unwrap_or_default()
                                        };
                                        let mut sbuf: Vec<u8> = Vec::new();
                                        for (fname, fnt) in &fields_info {
                                            if let Some(key) = v8::String::new(scope, fname.as_str()) {
                                                let fv = obj.get(scope, key.into())
                                                    .unwrap_or_else(|| v8::undefined(scope).into());
                                                append_struct_field_bytes(&mut sbuf, scope, fv, &fnt);
                                            }
                                        }
                                        let ptr = sbuf.as_mut_ptr() as *mut c_void;
                                        struct_scratch.push(sbuf);
                                        Ok(NativeValue { pointer: ptr })
                                    }
                                } else {
                                    ffi_parse_pointer_arg(scope, value)
                                }
                            } else {
                                let kind = declaration.read().kind();

                                // Delegate types: auto-wrap a JS function as a JsDelegate COM object,
                                // or extract the raw pointer from { handle: External } (NSWinRT.asDelegate result).
                                let is_delegate = matches!(kind,
                                    DeclarationKind::Delegate |
                                    DeclarationKind::GenericDelegate |
                                    DeclarationKind::GenericDelegateInstance
                                );

                                if is_delegate {
                                    let handle_ptr = value.to_object(scope).and_then(|obj| {
                                        let key = v8::String::new(scope, "handle")?;
                                        let hv = obj.get(scope, key.into())?;
                                        v8::Local::<v8::External>::try_from(hv).ok().map(|e| e.value())
                                    });

                                    if let Some(ptr) = handle_ptr {
                                        Ok(NativeValue { pointer: ptr })
                                    } else if let Ok(func) = v8::Local::<v8::Function>::try_from(value) {
                                        let parameter = &self.parameters[i];
                                        let delegate_info = parameter.metadata().and_then(|meta| {
                                            let raw_iid = Signature::to_iid_string(meta, &parameter.type_());
                                            let iid_name = substitute_type_vars(&raw_iid, &self.type_args);
                                            crate::delegate_info_from_type_sig(&iid_name)
                                        });
                                        if let Some((guid, param_types)) = delegate_info {
                                            use std::sync::atomic::AtomicU32;
                                            let data = Box::new(crate::JsDelegateData {
                                                js_func: v8::Global::new(scope, func),
                                                param_types,
                                            });
                                            let delegate = Box::new(crate::JsDelegate {
                                                vtable:    &crate::JS_DELEGATE_VTBL as *const _,
                                                ref_count: AtomicU32::new(1),
                                                guid,
                                                data:      Box::into_raw(data),
                                            });
                                            Ok(NativeValue { pointer: Box::into_raw(delegate) as *mut c_void })
                                        } else {
                                            ffi_parse_pointer_arg(scope, value)
                                        }
                                    } else {
                                        ffi_parse_pointer_arg(scope, value)
                                    }
                                } else {
                                    let iid = {
                                        let lock = declaration.read();
                                        match lock.kind() {
                                            DeclarationKind::Interface => lock
                                                .as_any()
                                                .downcast_ref::<InterfaceDeclaration>()
                                                .map(|iface| iface.id()),
                                            DeclarationKind::GenericInterface => lock
                                                .as_any()
                                                .downcast_ref::<GenericInterfaceDeclaration>()
                                                .map(|iface| iface.id()),
                                            DeclarationKind::GenericInterfaceInstance => lock
                                                .as_any()
                                                .downcast_ref::<GenericInterfaceInstanceDeclaration>()
                                                .map(|iface| iface.id()),
                                            DeclarationKind::Class => lock
                                                .as_any()
                                                .downcast_ref::<ClassDeclaration>()
                                                .and_then(|class| class.default_interface())
                                                .map(|iface| iface.id()),
                                            _ => None,
                                        }
                                    };

                                    if let Some(iid) = iid {
                                        match ffi_parse_query_interface_arg(scope, value, &iid) {
                                            Ok((pointer, Some(interface_guard))) => {
                                                queried_interfaces.push(interface_guard);
                                                Ok(pointer)
                                            }
                                            Ok((pointer, None)) => Ok(pointer),
                                            Err(error) => Err(error),
                                        }
                                    } else {
                                        ffi_parse_pointer_arg(scope, value)
                                    }
                                }
                            }
                        } else {
                            ffi_parse_pointer_arg(scope, value)
                        }
                    } else {
                        ffi_parse_pointer_arg(scope, value)
                    }
                }
                NativeType::Buffer => {
                    let parsed = ffi_parse_buffer_arg_with_length(scope, value);
                    let (buffer_value, byte_length) = match parsed {
                        Ok(value) => value,
                        Err(_) => return (call_failure(), std::ptr::null_mut(), Vec::new()),
                    };

                    self.argument_buf.push(NativeValue { u32_value: byte_length });
                    self.argument_parse_types.push(Some(native_type.clone()));
                    self.argument_buf.push(buffer_value);
                    self.argument_parse_types.push(Some(native_type.clone()));
                    continue;
                }
                NativeType::Function => ffi_parse_function_arg(scope, value),
                NativeType::Struct(_) => ffi_parse_struct_arg(scope, value),
                NativeType::String => ffi_parse_string_arg(scope, value),
            };

            let value = match value {
                Ok(value) => value,
                Err(_) => return (call_failure(), std::ptr::null_mut(), Vec::new()),
            };

            self.argument_buf.push(value);
            self.argument_parse_types.push(Some(native_type.clone()));
        }

        let mut result: *mut c_void = std::ptr::null_mut();

        if !self.is_initializer && !is_void {
            if is_value_type || is_scalar_return || is_string_return {
                let buf_ptr = self.return_value_buf.as_mut_ptr() as *mut c_void;
                self.argument_buf.push(NativeValue { pointer: buf_ptr });
                self.argument_parse_types.push(None);
            } else {
                self.argument_buf.push(NativeValue { pointer: &mut result as *mut _ as *mut c_void });
                self.argument_parse_types.push(None);
            }
        }

        let mut call_args: Vec<Arg> = Vec::with_capacity(self.argument_buf.len());

        for (i, v) in self.argument_buf.iter().enumerate() {
            let Some(abi_native) = self.parameter_types.get(i) else {
                return (call_failure(), std::ptr::null_mut(), Vec::new());
            };

            let effective_native = if matches!(abi_native, NativeType::Pointer) {
                if let Some(Some(parse_pt)) = self.argument_parse_types.get(i) {
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

        let ret = match catch_unwind(AssertUnwindSafe(|| unsafe { self.cif.call(CodePtr::from_ptr(self.func), &call_args) })) {
            Ok(code) => code,
            Err(_) => {
                let msg = format!("WinRT property call panicked during invocation: returning E_FAIL");
                crate::store_last_js_error(msg);
                return (call_failure(), std::ptr::null_mut(), Vec::new());
            }
        };

        // Detect RPC_E_WRONG_THREAD and surface the canonical OS message to
        // JS/tests so embedders can catch it directly.
        let hr = HRESULT(ret);
        const RPC_E_WRONG_THREAD: u32 = 0x8001010E;
        if (hr.0 as u32) == RPC_E_WRONG_THREAD {
            let msg = crate::error::format_hresult_message(hr);
            crate::store_last_js_error(msg.clone());
            if let Some(vmstr) = v8::String::new(scope, &msg) {
                let err = v8::Exception::error(scope, vmstr);
                scope.throw_exception(err);
            }
        }

        if !self.is_initializer && !is_void && (is_value_type || is_scalar_return || is_string_return) {
            result = self.return_value_buf.as_mut_ptr() as *mut c_void;
        }

        // Marshal out-parameters back into V8 values using the recorded slots.
        let mut out_values: Vec<v8::Local<'s, v8::Value>> = Vec::new();
        for (slot_index, parse_native_type, sig_opt) in out_slots.into_iter() {
            let storage_ptr = unsafe { self.argument_buf.get(slot_index).map(|v| v.pointer).unwrap_or(std::ptr::null_mut()) };
            if storage_ptr.is_null() {
                out_values.push(v8::null(scope).into());
                continue;
            }
            unsafe {
                let v = match parse_native_type {
                    NativeType::Pointer | NativeType::Buffer | NativeType::Function => {
                        let inner = std::ptr::read_unaligned(storage_ptr as *const usize) as *mut c_void;
                        if inner.is_null() {
                            v8::null(scope).into()
                        } else if let Some(sig) = sig_opt.as_ref() {
                            if sig.contains('.') {
                                let mut lookup = sig.as_str();
                                if let Some(stripped) = lookup.strip_prefix("ByRef ") {
                                    lookup = stripped;
                                }
                                let lookup = strip_generic_suffix(lookup);
                                if let Some(declaration) = MetadataReader::find_by_name(lookup) {
                                    if matches!(declaration.read().kind(), DeclarationKind::Struct) {
                                        crate::create_struct_object_from_raw(declaration, inner, scope).into()
                                    } else {
                                        let instance = unsafe { IUnknown::from_raw(inner) };
                                        crate::ns_proxy::create_ns_ctor_instance_object(sig.as_str(), None, None, declaration, Some(instance), scope).into()
                                    }
                                } else {
                                    read_value_from_ptr(inner as *const c_void, scope, NativeType::Pointer)
                                }
                            } else {
                                read_value_from_ptr(inner as *const c_void, scope, NativeType::Pointer)
                            }
                        } else {
                            read_value_from_ptr(inner as *const c_void, scope, NativeType::Pointer)
                        }
                    }
                    _ => read_value_from_ptr(storage_ptr as *const c_void, scope, parse_native_type.clone()),
                };
                out_values.push(v);
            }
        }

        (HRESULT(ret), result, out_values)
    }
}