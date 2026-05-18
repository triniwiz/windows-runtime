use std::ffi::c_void;
use std::mem::ManuallyDrop;
use std::sync::Arc;
use libffi::middle::*;
use parking_lot::RwLock;
use windows::core::{GUID, HRESULT, Interface, IUnknown, HSTRING, IInspectable};
use windows::Win32::System::WinRT::IActivationFactory;
use windows::Win32::System::WinRT::Metadata::CorTokenType;
use metadata::declarations::base_class_declaration::BaseClassDeclarationImpl;
use metadata::declarations::class_declaration::ClassDeclaration;
use metadata::declarations::declaration::DeclarationKind;
use metadata::declarations::struct_declaration::StructDeclaration;
use metadata::declarations::interface_declaration::generic_interface_declaration::GenericInterfaceDeclaration;
use metadata::declarations::interface_declaration::generic_interface_instance_declaration::GenericInterfaceInstanceDeclaration;
use metadata::declarations::interface_declaration::InterfaceDeclaration;
use metadata::declarations::method_declaration::MethodDeclaration;
use metadata::declarations::parameter_declaration::ParameterDeclaration;
use metadata::declarations::declaration::Declaration;
use metadata::declaring_interface_for_method::Metadata;
use metadata::meta_data_reader::MetadataReader;
use metadata::signature::Signature;
use crate::error::AnyError;
use crate::helpers::{ffi_native_type_from_signature, strip_generic_suffix};
use crate::value::{append_struct_field_bytes, ffi_parse_bool_arg, ffi_parse_buffer_arg_with_length, ffi_parse_f32_arg, ffi_parse_f64_arg, ffi_parse_function_arg, ffi_parse_i16_arg, ffi_parse_i32_arg, ffi_parse_i64_arg, ffi_parse_i8_arg, ffi_parse_isize_arg, ffi_parse_pointer_arg, ffi_parse_query_interface_arg, ffi_parse_string_arg, ffi_parse_struct_arg, ffi_parse_u16_arg, ffi_parse_u32_arg, ffi_parse_u64_arg, ffi_parse_u8_arg, ffi_parse_usize_arg, NativeType, NativeValue};
use crate::DeclarationFFI;

pub struct MethodCall {
    index: usize,
        method_name: String,
    pre_index: usize,
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
    /// Per-call parse-type tracker reused to avoid per-call heap allocation.
    argument_parse_types: Vec<Option<NativeType>>,
    /// Set when construction failed (e.g. QueryInterface returned E_NOINTERFACE for the IID).
    /// call() returns E_FAIL immediately instead of panicking.
    init_error: Option<String>,
}

#[inline]
fn call_failure() -> HRESULT {
    HRESULT(0x8000_4005u32 as i32)
}

impl MethodCall {
    fn new_init_error(interface: IUnknown, is_initializer: bool, is_sealed: bool, iid: GUID, error_msg: String) -> Self {
        Self {
            cif: Cif::new(vec![], Type::i32()),
            func: std::ptr::null_mut(),
            index: 0,
            method_name: String::new(),
            pre_index: 0,
            number_of_parameters: 0,
            number_of_abi_parameters: 0,
            is_initializer,
            is_sealed,
            is_void: true,
            iid,
            interface,
            parameter_types: vec![],
            parse_parameter_types: vec![],
            parameters: vec![],
            declaration: None,
            return_type: String::new(),
            return_value_buf: [0u8; 128],
            argument_buf: Vec::new(),
            argument_parse_types: Vec::new(),
            init_error: Some(error_msg),
        }
    }

    pub fn is_void(&self) -> bool {
        self.is_void
    }

    pub fn return_type(&self) -> &str {
        self.return_type.as_str()
    }

    pub fn init_error_message(&self) -> Option<&str> {
        self.init_error.as_deref()
    }

    pub fn new(
        method: &MethodDeclaration,
        is_sealed: bool,
        interface: IUnknown,
        is_initializer: bool
    ) -> Self {

        let default_iid = IActivationFactory::IID;
        let Some(method_metadata) = method.metadata() else {
            return Self::new_init_error(
                interface,
                is_initializer,
                is_sealed,
                default_iid,
                "MethodCall::new missing metadata for method".to_string(),
            );
        };

        let signature = method.return_type();

        let return_type = Signature::to_string(method_metadata, &signature);


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
                                .downcast_ref::<GenericInterfaceInstanceDeclaration>();
                            iid = ii.map(|value| value.id()).unwrap_or(IActivationFactory::IID);
                        }
                        _ => {
                            let ii = ii_lock
                                .as_declaration()
                                .as_any()
                                .downcast_ref::<InterfaceDeclaration>();
                            iid = ii.map(|value| value.id()).unwrap_or(IActivationFactory::IID);
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

        // QueryInterface can legitimately fail (E_NOINTERFACE) when the IID resolved
        // from metadata is zeroed/missing. Panic here crashes the process via
        // panic_cannot_unwind because we are inside a V8 callback — store the error
        // and surface it through call() as a JS error instead.
        if result.is_err() || interface_ptr.is_null() {
            let hr = if result.is_err() { result.0 } else { 0x8000_4002u32 as i32 }; // E_NOINTERFACE
            let error_msg = format!(
                "QueryInterface failed for IID {:?}: HRESULT 0x{:08X}",
                iid, hr as u32
            );
            return Self::new_init_error(interface, is_initializer, is_sealed, iid, error_msg);
        }

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
            let Some(metadata) = parameter.metadata() else {
                return Self::new_init_error(
                    interface,
                    is_initializer,
                    is_sealed,
                    iid,
                    "MethodCall::new missing parameter metadata".to_string(),
                );
            };

            let signature = Signature::to_string(metadata, &type_);

            let parse_native_type = match NativeType::try_from(signature.as_str()) {
                Ok(value) => value,
                Err(_) => {
                    return Self::new_init_error(
                        interface,
                        is_initializer,
                        is_sealed,
                        iid,
                        format!("Unsupported parameter type signature: {}", signature),
                    );
                }
            };
            parse_parameter_types.push(parse_native_type.clone());
            let abi_native = ffi_native_type_from_signature(signature.as_str());
            // If the parsed parameter is a WinRT `String`, treat its ABI as
            // `NativeType::String` (handle-sized) rather than the generic
            // pointer returned by the signature helper. This ensures the
            // libffi CIF is constructed with the correct usize-sized type.
            if matches!(parse_native_type, NativeType::String) {
                parameter_types.push(NativeType::String);
            } else if matches!(abi_native, NativeType::Buffer) {
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

        let params = match parameter_types
            .iter()
            .cloned()
            .map(libffi::middle::Type::try_from)
            .collect::<std::result::Result<Vec<Type>, AnyError>>()
        {
            Ok(value) => value,
            Err(err) => {
                return Self::new_init_error(
                    interface,
                    is_initializer,
                    is_sealed,
                    iid,
                    format!("Failed to create libffi parameter types: {}", err),
                );
            }
        };

        let cif = Cif::new(
            params,
            Type::i32(),
        );

        // Take ownership of the specific interface pointer returned by QueryInterface
        // so we can inspect its vtable directly to compute the correct function slot.
        let queried_interface = unsafe { IUnknown::from_raw(interface_ptr as *mut c_void) };
        let vtable_struct = queried_interface.vtable();
        let vtable_ptr: *mut *mut c_void = unsafe { std::mem::transmute(vtable_struct) };

        // If the queried interface supports IInspectable, its vtable includes the
        // 6 IInspectable slots; otherwise it's an IUnknown-only vtable with 3 slots.
        let mut inspectable_ptr: *mut c_void = std::ptr::null_mut();
        let supports_iinspectable = unsafe {
            ((*vtable_struct).QueryInterface)(
                queried_interface.as_raw(),
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


        Self {
            cif,
            func,
            index,
            pre_index,
            method_name: method.name().to_string(),
            number_of_parameters,
            number_of_abi_parameters,
            is_initializer,
            is_sealed,
            is_void: method.is_void(),
            iid,
            interface: queried_interface,
            parameter_types,
            parse_parameter_types,
            parameters: method.parameters().to_vec(),
            declaration,
            return_type,
            return_value_buf: [0u8; 128],
            argument_buf: Vec::with_capacity(number_of_abi_parameters),
            argument_parse_types: Vec::with_capacity(number_of_abi_parameters),
            init_error: None,
        }
    }

    /// Returns true when the return type is a WinRT value-struct (GUID, Point, Rect, …)
    /// that is NOT a COM reference type.  These must be written into a caller-allocated
    /// buffer rather than into a pointer-sized slot.
    fn is_value_type_return(&self) -> bool {
        if self.return_type == "Guid" {
            return true;
        }
        if self.return_type.contains('.') {
            let lookup = strip_generic_suffix(self.return_type.as_str());
            return MetadataReader::find_by_name(lookup)
                .map_or(false, |dec| matches!(dec.read().kind(), DeclarationKind::Struct));
        }
        false
    }

    

    pub fn call(
        &mut self,
        scope: &mut v8::PinScope<'_, '_>,
        args: &v8::FunctionCallbackArguments,
    ) -> (HRESULT, *mut c_void) {

        if self.init_error.is_some() {
            return (HRESULT(0x8000_4005u32 as i32), std::ptr::null_mut()); // E_FAIL
        }

        // Snapshot fields before the mutable borrow of argument_buf begins.
        let _number_of_abi_parameters = self.number_of_abi_parameters;
        let is_initializer = self.is_initializer;
        let is_sealed = self.is_sealed;
        let is_void = self.is_void;
        let is_value_type = self.is_value_type_return();

        let is_scalar_return = matches!(self.return_type.as_str(),
            "UInt8" | "Int8" | "UInt16" | "Int16" |
            "UInt32" | "Int32" | "UInt64" | "Int64" |
            "USize" | "ISize" | "Single" | "Double" |
            "Boolean" | "Char16"
        );

        // HSTRING out-params must also land in a stable buffer so the returned
        // pointer remains valid after this call frame is unwound.
        let is_string_return = self.return_type.as_str() == "String";

        self.argument_buf.clear();
        self.argument_parse_types.clear();
        let mut queried_interfaces: Vec<IUnknown> = Vec::new();
        let mut struct_scratch: Vec<Vec<u8>> = Vec::new();

        self.argument_buf.push(NativeValue { pointer: self.interface.as_raw() as *mut c_void });
        self.argument_parse_types.push(None);

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
                    let parameter = &self.parameters[i];
                    let parameter_signature = Signature::to_string(
                        parameter.metadata().unwrap(),
                        &parameter.type_(),
                    );

                    if parameter_signature.contains('.') {
                        let lookup_name = crate::helpers::strip_generic_suffix(parameter_signature.as_str());

                        if let Some(declaration) = MetadataReader::find_by_name(lookup_name) {
                            // Struct parameters (TypeName, Point, Rect, …) are passed as a
                            // pointer to their ABI-layout bytes.  Class constructors passed
                            // where a TypeName is expected are synthesised automatically.
                            if matches!(declaration.read().kind(), DeclarationKind::Struct) {
                                let full_name = declaration.read().full_name().to_string();
                                if full_name == "Windows.UI.Xaml.Interop.TypeName" {
                                    // If the JS argument is a class constructor (instance=None,
                                    // struct_instance=None) synthesise a TypeName{Name,Kind=Metadata}.
                                    let synthesized: Option<*mut c_void> = 'synth: {
                                        if value.is_object() {
                                            let obj = value.to_object(scope).unwrap();
                                            if let Some(field) = obj.get_internal_field(scope, 0) {
                                                let ext = unsafe { field.cast::<v8::External>() };
                                                let dec_ffi = unsafe { &*(ext.value() as *mut DeclarationFFI) };
                                                if dec_ffi.struct_instance.is_none() && dec_ffi.instance.is_none() {
                                                    let class_name = dec_ffi.inner.read().full_name().to_string();
                                                    let hstring = HSTRING::from(class_name.as_str());
                                                    // SAFETY: HSTRING is repr(transparent) over *mut u16.
                                                    // Transmute moves ownership out; the raw handle is
                                                    // stored in the leaked bytes and intentionally leaked
                                                    // alongside them (Navigate reads Name only during the call).
                                                    let raw: usize = unsafe { std::mem::transmute(hstring) };
                                                    let mut bytes: Box<[u8; 16]> = Box::new([0u8; 16]);
                                                    unsafe {
                                                        *(bytes.as_mut_ptr() as *mut usize) = raw;
                                                        *(bytes.as_mut_ptr().add(8) as *mut u32) = 1u32; // TypeKind::Metadata
                                                    }
                                                    break 'synth Some(Box::leak(bytes).as_ptr() as *mut c_void);
                                                }
                                            }
                                        }
                                        None
                                    };
                                    if let Some(ptr) = synthesized {
                                        Ok(NativeValue { pointer: ptr })
                                    } else {
                                        // Already a TypeName struct instance — ffi_parse_pointer_arg
                                        // extracts struct_instance.buf.as_ptr() via try_get_external_handle.
                                        ffi_parse_pointer_arg(scope, value)
                                    }
                                } else {
                                    // Other structs: accept ArrayBuffer, struct instances, or plain JS objects.
                                    if value.is_array_buffer() || value.is_array_buffer_view() {
                                        ffi_parse_struct_arg(scope, value)
                                    } else if value.is_object() {
                                        let obj_v = value.to_object(scope).unwrap();
                                        let has_internal = obj_v.get_internal_field(scope, 0)
                                            .map(|f| !unsafe { f.cast::<v8::External>() }.value().is_null())
                                            .unwrap_or(false);
                                        if has_internal {
                                            ffi_parse_pointer_arg(scope, value)
                                        } else {
                                            // Plain JS object {A:255, R:0, …} — build bytes from named fields
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
                                                    let fv = obj_v.get(scope, key.into())
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
                                }
                            } else {
                                let kind = declaration.read().kind();

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
                                            let iid_name = Signature::to_iid_string(meta, &parameter.type_());
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
                                                .map(|interface| interface.id()),
                                            DeclarationKind::GenericInterface => lock
                                                .as_any()
                                                .downcast_ref::<GenericInterfaceDeclaration>()
                                                .map(|interface| interface.id()),
                                            DeclarationKind::GenericInterfaceInstance => lock
                                                .as_any()
                                                .downcast_ref::<GenericInterfaceInstanceDeclaration>()
                                                .map(|interface| interface.id()),
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
                                            Err(_) => ffi_parse_pointer_arg(scope, value),
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
                        Err(_) => return (call_failure(), std::ptr::null_mut()),
                    };

                    self.argument_buf.push(NativeValue { u32_value: byte_length });
                    self.argument_parse_types.push(Some(native_type.clone()));
                    self.argument_buf.push(buffer_value);
                    self.argument_parse_types.push(Some(native_type.clone()));
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

            self.argument_buf.push(value);
            self.argument_parse_types.push(Some(native_type.clone()));
        }

        let mut result: *mut c_void = std::ptr::null_mut();
        let mut composition_outer: *mut c_void = std::ptr::null_mut();
        let mut composition_inner: *mut c_void = std::ptr::null_mut();

        if is_initializer {
            if !is_sealed {
                unsafe {
                    self.argument_buf.push(NativeValue {
                        pointer: &mut composition_outer as *mut _ as *mut c_void,
                    })
                };
                self.argument_parse_types.push(None);
                unsafe {
                    self.argument_buf.push(NativeValue {
                        pointer: &mut composition_inner as *mut _ as *mut c_void,
                    })
                };
                self.argument_parse_types.push(None);
            }
            unsafe { self.argument_buf.push(NativeValue { pointer: &mut result as *mut _ as *mut c_void }) };
            self.argument_parse_types.push(None);
        } else if !is_void {
            if is_value_type || is_scalar_return || is_string_return {
                let buf_ptr = self.return_value_buf.as_mut_ptr() as *mut c_void;
                self.argument_buf.push(NativeValue { pointer: buf_ptr });
                self.argument_parse_types.push(None);
            } else {
                self.argument_buf.push(NativeValue { pointer: &mut result as *mut _ as *mut c_void });
                self.argument_parse_types.push(None);
            }
        }

        let prep = match crate::ffi::prepare_string_storage(&self.argument_buf, &self.parameter_types, &self.argument_parse_types) {
            Ok(value) => value,
            Err(_) => return (call_failure(), std::ptr::null_mut()),
        };

        let func_to_call = self.func;

        let call_args = crate::ffi::build_call_args(&prep, &self.argument_buf, &self.argument_parse_types);
        let ret = unsafe { self.cif.call(CodePtr::from_ptr(func_to_call), &call_args) };

        if is_initializer && !is_sealed && result.is_null() {
            if !composition_inner.is_null() {
                result = composition_inner;
            } else if !composition_outer.is_null() {
                result = composition_outer;
            }
        }

        if !is_initializer && !is_void && (is_value_type || is_scalar_return || is_string_return) {
            // Point result at the scratch buffer — all three categories write
            // into return_value_buf, so the pointer is stable for the caller.
            result = self.return_value_buf.as_mut_ptr() as *mut c_void;
        }

        (HRESULT(ret), result)
    }

    /// Call an event add-method with a raw COM delegate pointer.
    /// Returns `(HRESULT, token)` where token is the EventRegistrationToken i64 value.
    pub fn call_with_raw_ptr(&mut self, ptr: *mut c_void) -> (HRESULT, i64) {
        let is_void = self.is_void;
        self.argument_buf.clear();
        self.argument_buf.push(NativeValue { pointer: self.interface.as_raw() as *mut c_void });
        self.argument_buf.push(NativeValue { pointer: ptr });
        let mut result: *mut c_void = std::ptr::null_mut();
        if !is_void {
            self.argument_buf.push(NativeValue { pointer: &mut result as *mut _ as *mut c_void });
        }
        let mut call_args: Vec<Arg> = Vec::with_capacity(self.argument_buf.len());
        for (i, v) in self.argument_buf.iter().enumerate() {
            let Some(native_type) = self.parameter_types.get(i) else {
                return (call_failure(), 0);
            };
            call_args.push(unsafe { v.as_arg(native_type) });
        }
        let ret: i32 = unsafe { self.cif.call(CodePtr::from_ptr(self.func), &call_args) };
        // result's bytes were overwritten by WinRT with the EventRegistrationToken (i64).
        let token = result as i64;
        (HRESULT(ret), token)
    }

    /// Call an event remove-method with an EventRegistrationToken value.
    /// The token is passed by value (i64) per the WinRT ABI for remove_* methods.
    pub fn call_with_event_token(&mut self, token: i64) -> HRESULT {
        self.argument_buf.clear();
        self.argument_buf.push(NativeValue { pointer: self.interface.as_raw() as *mut c_void });
        self.argument_buf.push(NativeValue { i64_value: token });
        let mut call_args: Vec<Arg> = Vec::with_capacity(self.argument_buf.len());
        for (i, v) in self.argument_buf.iter().enumerate() {
            let Some(native_type) = self.parameter_types.get(i) else {
                return call_failure();
            };
            call_args.push(unsafe { v.as_arg(native_type) });
        }
        let ret: i32 = unsafe { self.cif.call(CodePtr::from_ptr(self.func), &call_args) };
        HRESULT(ret)
    }
}