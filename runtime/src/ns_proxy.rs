use std::collections::HashSet;
use std::ffi::c_void;
use std::sync::Arc;
use parking_lot::RwLock;
use v8::{FunctionTemplate, Local};
use windows::core::{HSTRING, IInspectable, IUnknown, Interface};
use windows::Win32::System::Console::GetConsoleWindow;
use windows::Win32::System::WinRT::IActivationFactory;
use windows::Win32::UI::Shell::IInitializeWithWindow;
use metadata::declarations::base_class_declaration::BaseClassDeclarationImpl;
use metadata::declarations::class_declaration::ClassDeclaration;
use metadata::declarations::declaration::{Declaration, DeclarationKind};
use metadata::declarations::delegate_declaration::{DelegateDeclaration, DelegateDeclarationImpl};
use metadata::declarations::delegate_declaration::generic_delegate_declaration::GenericDelegateDeclaration;
use metadata::declarations::delegate_declaration::generic_delegate_instance_declaration::GenericDelegateInstanceDeclaration;
use metadata::declarations::enum_declaration::EnumDeclaration;
use metadata::declarations::interface_declaration::InterfaceDeclaration;
use metadata::declarations::interface_declaration::generic_interface_declaration::GenericInterfaceDeclaration;
use metadata::declarations::interface_declaration::generic_interface_instance_declaration::GenericInterfaceInstanceDeclaration;
use metadata::declarations::method_declaration::MethodDeclaration;
use metadata::declarations::namespace_declaration::NamespaceDeclaration;
use metadata::declarations::property_declaration::PropertyDeclaration;
use metadata::declarations::struct_declaration::StructDeclaration;
use metadata::meta_data_reader::MetadataReader;
use metadata::signature::Signature;
use crate::value::{
    ffi_parse_bool_arg, ffi_parse_buffer_arg, ffi_parse_f32_arg, ffi_parse_f64_arg,
    ffi_parse_function_arg, ffi_parse_i16_arg, ffi_parse_i32_arg, ffi_parse_i64_arg,
    ffi_parse_i8_arg, ffi_parse_isize_arg, ffi_parse_pointer_arg, ffi_parse_string_arg,
    ffi_parse_struct_arg, ffi_parse_u16_arg, ffi_parse_u32_arg, ffi_parse_u64_arg,
    ffi_parse_u8_arg, ffi_parse_usize_arg, set_ret_val, NativeType, NativeValue,
    MAX_SAFE_INTEGER, MIN_SAFE_INTEGER,
};
use crate::class_helpers::{
    class_has_member_named, collect_class_methods, collect_class_properties, find_event_methods,
};
use metadata::value::Value;
use crate::method_call::MethodCall;
use crate::property_call::PropertyCall;
use crate::generic_method_call::GenericMethodCall;
use crate::{
    debug_output, throw_js_error, class_activation_factory, resolve_class_factory_from_parent,
    DeclarationFFI, JsDelegate, JsDelegateData, JS_DELEGATE_VTBL, js_delegate_params_from_declaration,
};
use crate::error;

// ── Stub callbacks ───────────────────────────────────────────────────────────

pub(crate) fn handle_ns_func(
    _scope: &mut v8::PinScope<'_, '_>,
    _args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
}

// ── Indexed property stubs ───────────────────────────────────────────────────

pub(crate) fn handle_indexed_property_setter(
    _scope: &mut v8::PinScope<'_, '_>,
    _index: u32,
    _value: v8::Local<v8::Value>,
    _args: v8::PropertyCallbackArguments,
    mut _rv: v8::ReturnValue<()>,
) -> v8::Intercepted {
    v8::Intercepted::kNo
}

pub(crate) fn handle_indexed_property_getter(
    _scope: &mut v8::PinScope<'_, '_>,
    _index: u32,
    _args: v8::PropertyCallbackArguments,
    mut _rv: v8::ReturnValue<v8::Value>,
) -> v8::Intercepted {
    v8::Intercepted::kNo
}

// ── Namespace-object property handlers ──────────────────────────────────────

pub(crate) fn handle_named_property_query(
    _scope: &mut v8::PinScope<'_, '_>,
    _key: v8::Local<v8::Name>,
    _args: v8::PropertyCallbackArguments,
    mut rv: v8::ReturnValue<v8::Integer>,
) -> v8::Intercepted {
    rv.set_int32(0);
    v8::Intercepted::kNo
}

pub(crate) fn handle_named_property_getter(
    scope: &mut v8::PinScope<'_, '_>,
    key: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut rv: v8::ReturnValue<v8::Value>,
) -> v8::Intercepted {
    let this = args.holder();
    let dec = this.get_internal_field(scope, 0).unwrap();
    let dec = unsafe { dec.cast::<v8::External>() };
    let dec = dec.value() as *mut DeclarationFFI;
    let dec = unsafe { &*dec };
    let lock = dec.read();
    let store = this.get_internal_field(scope, 1).unwrap();
    let store = unsafe { store.cast::<v8::Map>() };
    let kind = lock.kind();

    if key.is_string() {
        if let Some(cache) = store.get(scope, key.into()) {
            if !cache.is_null_or_undefined() {
                rv.set(cache);
                return v8::Intercepted::kYes;
            }
        }

        let name = key.to_string(scope).unwrap().to_rust_string_lossy(scope);
        match kind {
            DeclarationKind::Namespace => {
                let parent = dec.inner.clone();
                let dec = lock.as_any().downcast_ref::<NamespaceDeclaration>();
                if let Some(dec) = dec {
                    let full_name = format!("{}.{}", dec.full_name(), name.as_str());

                    if let Some(dec) = MetadataReader::find_by_name(full_name.as_str()) {
                        let declaration = Arc::clone(&dec);
                        let lock = dec.read();

                        match lock.kind() {
                            DeclarationKind::Struct => {
                                let struct_dec = lock.as_any().downcast_ref::<StructDeclaration>().unwrap();
                                let name = struct_dec.name().to_string();
                                drop(lock);

                                let ret = create_ns_struct_ctor_object(name.as_str(), Arc::clone(&dec), scope);
                                let ret: Local<v8::Value> = ret.into();
                                store.set(scope, key.into(), ret);
                                rv.set(ret);
                            }
                            DeclarationKind::Class => {
                                let ret: Local<v8::Value> = create_ns_ctor_object(lock.name(), Some(parent), declaration, scope).into();
                                store.set(scope, key.into(), ret);
                                rv.set(ret);
                            }
                            DeclarationKind::Interface
                            | DeclarationKind::GenericInterface
                            | DeclarationKind::GenericInterfaceInstance
                            | DeclarationKind::Delegate
                            | DeclarationKind::GenericDelegate
                            | DeclarationKind::GenericDelegateInstance
                            | DeclarationKind::Event => {
                                let ret: Local<v8::Value> = create_ns_ctor_object(lock.name(), Some(parent), declaration, scope).into();
                                store.set(scope, key.into(), ret);
                                rv.set(ret);
                            }
                            _ => {
                                let ret: Local<v8::Value> = create_ns_object(name.as_str(), declaration, scope).into();
                                store.set(scope, key.into(), ret);
                                rv.set(ret);
                            }
                        }
                        return v8::Intercepted::kYes;
                    }

                    return v8::Intercepted::kNo;
                }
            }
            DeclarationKind::Class => {
                let clazz_dec = lock.as_any().downcast_ref::<ClassDeclaration>();

                if let Some(clazz_dec) = clazz_dec {
                    for method in collect_class_methods(clazz_dec) {
                        let mut method_name = method.overload_name();
                        if method_name.is_empty() {
                            method_name = method.name();
                        }

                        if method_name == name {
                            let declaration = Arc::new(RwLock::new(method.clone()));
                            let declaration = Box::into_raw(Box::new(DeclarationFFI::new_with_instance(declaration, dec.instance.clone())));
                            let ext = v8::External::new(scope, declaration as _);

                            let builder = v8::Function::builder(|scope: &mut v8::PinScope<'_, '_>,
                                                                 args: v8::FunctionCallbackArguments,
                                                                 _retval: v8::ReturnValue| {
                                let dec = unsafe { args.data().cast::<v8::External>() };
                                let dec = dec.value() as *mut DeclarationFFI;
                                let dec = unsafe { &*dec };
                                let lock = dec.read();
                                let method = lock.as_any().downcast_ref::<MethodDeclaration>().unwrap();
                                let instance = dec.instance.clone().unwrap();
                                let mut method = MethodCall::new(method, method.is_sealed(), instance, false);
                                let (_ret, _result) = method.call(scope, &args);
                            })
                            .data(ext.into())
                            .build(scope);

                            let func = builder.unwrap();
                            let func: Local<v8::Value> = func.into();
                            store.set(scope, key.into(), func);
                            rv.set(func);
                            return v8::Intercepted::kYes;
                        }
                    }
                }
            }
            DeclarationKind::Interface
            | DeclarationKind::GenericInterface
            | DeclarationKind::GenericInterfaceInstance
            | DeclarationKind::Delegate
            | DeclarationKind::GenericDelegate
            | DeclarationKind::GenericDelegateInstance
            | DeclarationKind::Event => {
                if let Some(impl_key) = v8::String::new(scope, "__implementation__") {
                    if let Some(implementation) = store.get(scope, impl_key.into()) {
                        if let Some(implementation) = implementation.to_object(scope) {
                            if let Some(value) = implementation.get(scope, key.into()) {
                                if !value.is_null_or_undefined() {
                                    store.set(scope, key.into(), value);
                                    rv.set(value);
                                    return v8::Intercepted::kYes;
                                }
                            }
                        }
                    }
                }
            }
            DeclarationKind::Enum => {
                let enum_dec = lock.as_any().downcast_ref::<EnumDeclaration>();
                if let Some(enum_dec) = enum_dec {
                    if let Some(value) = enum_dec.enum_for_name(name.as_str()) {
                        match value.value() {
                            Value::Int32(value) => {
                                rv.set_int32(value);
                                let ret: Local<v8::Value> = v8::Integer::new(scope, value).into();
                                store.set(scope, key.into(), ret);
                                return v8::Intercepted::kYes;
                            }
                            Value::Uint32(value) => {
                                rv.set_uint32(value);
                                let ret: Local<v8::Value> = v8::Integer::new_from_unsigned(scope, value).into();
                                store.set(scope, key.into(), ret);
                                return v8::Intercepted::kYes;
                            }
                            _ => {}
                        }
                    }
                    return v8::Intercepted::kNo;
                }
            }
            _ => {}
        }
    }
    v8::Intercepted::kNo
}

pub(crate) fn handle_named_property_setter(
    scope: &mut v8::PinScope<'_, '_>,
    key: Local<v8::Name>,
    value: Local<v8::Value>,
    args: v8::PropertyCallbackArguments,
    mut _rv: v8::ReturnValue<()>,
) -> v8::Intercepted {
    let this = args.holder();
    let Some(dec_field) = this.get_internal_field(scope, 0) else { return v8::Intercepted::kNo };
    let dec = unsafe { dec_field.cast::<v8::External>() }.value() as *mut DeclarationFFI;
    let dec = unsafe { &*dec };
    let lock = dec.read();
    let kind = lock.kind();

    let Some(store_field) = this.get_internal_field(scope, 1) else { return v8::Intercepted::kNo };
    let store = unsafe { store_field.cast::<v8::Map>() };

    let name = key.to_rust_string_lossy(scope);

    let is_reserved = match kind {
        DeclarationKind::Namespace => lock
            .as_any()
            .downcast_ref::<NamespaceDeclaration>()
            .map(|d| d.children().contains(&name))
            .unwrap_or(false),
        DeclarationKind::Enum => lock
            .as_any()
            .downcast_ref::<EnumDeclaration>()
            .map(|d| d.enum_for_name(&name).is_some())
            .unwrap_or(false),
        DeclarationKind::Class => lock
            .as_any()
            .downcast_ref::<ClassDeclaration>()
            .map(|d| class_has_member_named(d, &name))
            .unwrap_or(false),
        DeclarationKind::Interface => lock
            .as_any()
            .downcast_ref::<InterfaceDeclaration>()
            .map(|d| {
                d.methods().iter().any(|m| m.name() == name)
                    || d.properties().iter().any(|p| p.name() == name)
            })
            .unwrap_or(false),
        DeclarationKind::Struct => lock
            .as_any()
            .downcast_ref::<StructDeclaration>()
            .map(|d| d.fields().iter().any(|f| f.name() == name))
            .unwrap_or(false),
        _ => false,
    };

    // Wire WinRT event handlers via add/remove ABI methods for class instances.
    if kind == DeclarationKind::Class && !is_reserved {
        if let Some(class) = lock.as_any().downcast_ref::<ClassDeclaration>() {
            if let Some((add_method, remove_method)) = find_event_methods(class, &name) {
                let instance = dec.instance.clone();
                drop(lock);

                let token_key = format!("__tok_{}__", name);
                if let Some(tok_key_str) = v8::String::new(scope, &token_key) {
                    if let Some(tok_val) = store.get(scope, tok_key_str.into()) {
                        if let Ok(tok_ext) = v8::Local::<v8::External>::try_from(tok_val) {
                            let token = tok_ext.value() as i64;
                            if let Some(instance) = instance.clone() {
                                let mut mc = MethodCall::new(
                                    &remove_method, remove_method.is_sealed(), instance, false,
                                );
                                mc.call_with_event_token(token);
                            }
                            let undef = v8::undefined(scope);
                            store.set(scope, tok_key_str.into(), undef.into());
                        }
                    }
                }

                if value.is_object() {
                    if let Some(obj) = value.to_object(scope) {
                        if let Some(handle_key) = v8::String::new(scope, "handle") {
                            if let Some(handle_val) = obj.get(scope, handle_key.into()) {
                                if let Ok(ext) = v8::Local::<v8::External>::try_from(handle_val) {
                                    let delegate_ptr = ext.value();
                                    if let Some(instance) = instance {
                                        let mut mc = MethodCall::new(
                                            &add_method, add_method.is_sealed(), instance, false,
                                        );
                                        let (_, token) = mc.call_with_raw_ptr(delegate_ptr);
                                        if let Some(tok_key_str) = v8::String::new(scope, &token_key) {
                                            let tok_ptr = token as *mut c_void;
                                            store.set(scope, tok_key_str.into(), v8::External::new(scope, tok_ptr).into());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                store.set(scope, key.into(), value);
                return v8::Intercepted::kYes;
            }
        }
    }

    if !is_reserved {
        store.set(scope, key.into(), value);
        v8::Intercepted::kYes
    } else {
        v8::Intercepted::kNo
    }
}

// ── Instance-object property handlers (named-accessor pattern) ───────────────

/// Method dispatch callback stored on instance object method slots.
/// The data External points to a `DeclarationFFI` wrapping a `MethodDeclaration`.
fn instance_method_dispatch(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let dec = unsafe { args.data().cast::<v8::External>() };
    let dec = dec.value() as *mut DeclarationFFI;
    let dec = unsafe { &*dec };
    let lock = dec.read();
    let method = lock.as_any().downcast_ref::<MethodDeclaration>().unwrap();
    let mut method = MethodCall::new(method, method.is_sealed(), dec.instance.clone().unwrap(), false);
    let (ret, result) = method.call(scope, &args);

    if ret.is_err() {
        let msg = v8::String::new(scope, &ret.message().to_string()).unwrap();
        let err = v8::Exception::error(scope, msg);
        scope.throw_exception(err);
        return;
    }

    if method.is_void() {
        retval.set_undefined();
        return;
    }

    let return_sig = method.return_type().to_string();
    if return_sig.contains('.') {
        let instance = unsafe { IUnknown::from_raw(result) };
        let lookup = crate::helpers::strip_generic_suffix(return_sig.as_str());
        if let Some(declaration) = MetadataReader::find_by_name(lookup) {
            let ret: Local<v8::Value> = create_ns_ctor_instance_object(
                return_sig.as_str(), None, dec.parent.clone(), declaration, Some(instance), scope,
            ).into();
            retval.set(ret);
            return;
        }
    }

    if let Ok(return_type) = NativeType::try_from(return_sig.as_str()) {
        unsafe { set_ret_val(result, scope, retval, return_type); }
    }
}

/// Named property getter for WinRT instance objects (ClassDeclaration wrappers).
/// Handles property reads and returns bound method functions.
/// Data External points to a `DeclarationFFI` wrapping the ClassDeclaration.
pub(crate) fn handle_instance_property_getter(
    scope: &mut v8::PinScope<'_, '_>,
    key: Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut rv: v8::ReturnValue<v8::Value>,
) -> v8::Intercepted {
    if !key.is_string() {
        return v8::Intercepted::kNo;
    }

    let name = key.to_rust_string_lossy(scope);
    if name == "__probe__" {
        let value = v8::String::new(scope, "instance-handler-active").unwrap();
        rv.set(value.into());
        return v8::Intercepted::kYes;
    }

    let dec = unsafe { args.data().cast::<v8::External>() };
    let dec = dec.value() as *mut DeclarationFFI;
    let dec = unsafe { &*dec };
    let lock = dec.read();

    let Some(clazz) = lock.as_any().downcast_ref::<ClassDeclaration>() else {
        return v8::Intercepted::kNo;
    };

    for property in collect_class_properties(clazz) {
        if property.name() != name {
            continue;
        }

        let Some(mut property_call) = PropertyCall::new(&property, false, dec.instance.clone().unwrap(), false) else {
            continue;
        };
        let (ret, result) = property_call.call_with_values(scope, &[]);

        if ret.is_err() {
            let detail = format!("Property get '{}' failed: {} (0x{:08X})", name, ret.message(), ret.0 as u32);
            debug_output(&format!("[NativeScript] {}\n", detail));
            let message = v8::String::new(scope, &detail).unwrap();
            let error = v8::Exception::error(scope, message);
            scope.throw_exception(error);
            return v8::Intercepted::kYes;
        }

        if property_call.is_void() {
            rv.set_undefined();
            return v8::Intercepted::kYes;
        }

        let return_sig = property_call.return_type().to_string();
        if return_sig.contains('.') {
            let instance = unsafe { IUnknown::from_raw(result) };
            let lookup = crate::helpers::strip_generic_suffix(return_sig.as_str());
            if let Some(declaration) = MetadataReader::find_by_name(lookup) {
                let ret: Local<v8::Value> = create_ns_ctor_instance_object(
                    return_sig.as_str(), None, None, declaration, Some(instance), scope,
                ).into();
                rv.set(ret);
                return v8::Intercepted::kYes;
            }
        }

        if let Ok(return_type) = NativeType::try_from(return_sig.as_str()) {
            unsafe { set_ret_val(result, scope, rv, return_type); }
            return v8::Intercepted::kYes;
        }

        return v8::Intercepted::kNo;
    }

    for method in collect_class_methods(clazz) {
        let mut method_name = method.overload_name();
        if method_name.is_empty() {
            method_name = method.name();
        }

        if method_name != name {
            continue;
        }

        let method_dec = Arc::new(RwLock::new(method.clone()));
        let method_ffi = DeclarationFFI::new_with_instance(method_dec, dec.instance.clone());
        let method_ffi = Box::into_raw(Box::new(method_ffi));
        let ext = v8::External::new(scope, method_ffi as _);

        let builder = v8::Function::builder(instance_method_dispatch)
            .data(ext.into())
            .build(scope)
            .unwrap();

        rv.set(builder.into());
        return v8::Intercepted::kYes;
    }

    v8::Intercepted::kNo
}

/// Named property setter for WinRT instance objects.
/// Handles WinRT property writes (setter ABI call).
/// Data External points to a `DeclarationFFI` wrapping the ClassDeclaration.
pub(crate) fn handle_instance_property_setter(
    scope: &mut v8::PinScope<'_, '_>,
    key: Local<v8::Name>,
    value: Local<v8::Value>,
    args: v8::PropertyCallbackArguments,
    mut _rv: v8::ReturnValue<()>,
) -> v8::Intercepted {
    if !key.is_string() {
        return v8::Intercepted::kNo;
    }

    let name = key.to_rust_string_lossy(scope);
    let dec = unsafe { args.data().cast::<v8::External>() };
    let dec = dec.value() as *mut DeclarationFFI;
    let dec = unsafe { &mut *dec };
    let lock = dec.read();

    let Some(clazz) = lock.as_any().downcast_ref::<ClassDeclaration>() else {
        return v8::Intercepted::kNo;
    };

    debug_output(&format!("[NativeScript] instance setter on {} name='{}' value_kind=obj:{} null:{} undef:{}\n",
        clazz.full_name(), name, value.is_object(), value.is_null(), value.is_undefined()));

    // Try WinRT properties first.
    for property in collect_class_properties(clazz) {
        if property.name() != name {
            continue;
        }

        if property.setter().is_none() {
            return v8::Intercepted::kNo;
        }

        let Some(mut property_call) = PropertyCall::new(&property, true, dec.instance.clone().unwrap(), false) else {
            return v8::Intercepted::kNo;
        };
        debug_output(&format!(
            "[NativeScript] set '{}' param_types={:?} abi_types={:?}\n",
            name, property_call.parse_types_debug(), property_call.abi_types_debug()
        ));
        let (ret, _) = property_call.call_with_values(scope, &[value]);
        if ret.is_err() {
            let detail = format!("Property set '{}' failed: {} (0x{:08X})", name, ret.message(), ret.0 as u32);
            debug_output(&format!("[NativeScript] {}\n", detail));
            let message = v8::String::new(scope, &detail).unwrap();
            let error = v8::Exception::error(scope, message);
            scope.throw_exception(error);
        }
        return v8::Intercepted::kYes;
    }

    // Try WinRT events: `instance.EventName = new DelegateType({ Invoke: fn })`.
    if let Some((add_method, remove_method)) = find_event_methods(clazz, &name) {
        let instance = dec.instance.clone();
        drop(lock);

        debug_output(&format!("[NativeScript] event set '{}' add='{}'\n", name, add_method.name()));

        // Remove the previous handler if one was registered under this name.
        if let Some(&old_token) = dec.event_tokens.get(&name) {
            if let Some(inst) = instance.clone() {
                let mut mc = MethodCall::new(&remove_method, remove_method.is_sealed(), inst, false);
                mc.call_with_event_token(old_token);
            }
            dec.event_tokens.remove(&name);
        }

        // Register the new handler: the value must be `{ handle: External }` from a delegate constructor.
        if value.is_object() {
            if let Some(obj) = value.to_object(scope) {
                if let Some(handle_key) = v8::String::new(scope, "handle") {
                    if let Some(handle_val) = obj.get(scope, handle_key.into()) {
                        if let Ok(ext) = v8::Local::<v8::External>::try_from(handle_val) {
                            let delegate_ptr = ext.value();
                            if let Some(inst) = instance {
                                let mut mc = MethodCall::new(&add_method, add_method.is_sealed(), inst, false);
                                let (ret, token) = mc.call_with_raw_ptr(delegate_ptr);
                                debug_output(&format!("[NativeScript] add_Click hr=0x{:08X} token={}\n", ret.0 as u32, token));
                                if ret.is_ok() {
                                    dec.event_tokens.insert(name, token);
                                }
                            }
                        } else {
                            debug_output(&format!("[NativeScript] event set '{}': value has no External handle (value is_null={})\n", name, value.is_null()));
                        }
                    }
                }
            }
        } else {
            debug_output(&format!("[NativeScript] event set '{}': value is not object (is_null={} is_undefined={})\n", name, value.is_null(), value.is_undefined()));
        }

        return v8::Intercepted::kYes;
    }

    v8::Intercepted::kNo
}

// ── GUID helper ──────────────────────────────────────────────────────────────

pub(crate) unsafe fn guid_ptr_to_js_object<'a>(
    ptr: *mut c_void,
    scope: &mut v8::PinScope<'a, '_>,
) -> v8::Local<'a, v8::Object> {
    use windows::core::GUID;
    let g = &*(ptr as *const GUID);

    let guid_str = format!(
        "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        g.data1, g.data2, g.data3,
        g.data4[0], g.data4[1],
        g.data4[2], g.data4[3], g.data4[4], g.data4[5], g.data4[6], g.data4[7]
    );

    let obj = v8::Object::new(scope);

    let key_data1 = v8::String::new(scope, "data1").unwrap();
    let val_data1 = v8::Integer::new_from_unsigned(scope, g.data1);
    obj.set(scope, key_data1.into(), val_data1.into());

    let key_data2 = v8::String::new(scope, "data2").unwrap();
    let val_data2 = v8::Integer::new_from_unsigned(scope, g.data2 as u32);
    obj.set(scope, key_data2.into(), val_data2.into());

    let key_data3 = v8::String::new(scope, "data3").unwrap();
    let val_data3 = v8::Integer::new_from_unsigned(scope, g.data3 as u32);
    obj.set(scope, key_data3.into(), val_data3.into());

    let arr = v8::Array::new(scope, 8);
    for (i, &byte) in g.data4.iter().enumerate() {
        let byte_val = v8::Integer::new_from_unsigned(scope, byte as u32);
        arr.set_index(scope, i as u32, byte_val.into());
    }
    let key_data4 = v8::String::new(scope, "data4").unwrap();
    obj.set(scope, key_data4.into(), arr.into());

    let guid_v8 = v8::String::new(scope, &guid_str).unwrap();
    let to_string_fn = v8::FunctionTemplate::builder(
        |_scope: &mut v8::PinScope<'_, '_>,
         args: v8::FunctionCallbackArguments,
         mut retval: v8::ReturnValue| {
            let s = unsafe { args.data().cast::<v8::String>() };
            retval.set(s.into());
        },
    )
    .data(guid_v8.into())
    .build(scope)
    .get_function(scope)
    .unwrap();

    let key_to_string = v8::String::new(scope, "toString").unwrap();
    obj.set(scope, key_to_string.into(), to_string_fn.into());
    let key_value_of = v8::String::new(scope, "valueOf").unwrap();
    obj.set(scope, key_value_of.into(), to_string_fn.into());

    obj
}

// ── Raw result → JS value conversion ────────────────────────────────────────

pub(crate) unsafe fn raw_result_to_local<'s>(
    result: *mut c_void,
    signature: &str,
    parent_decl: Option<Arc<RwLock<dyn Declaration>>>,
    scope: &mut v8::PinScope<'s, '_>,
) -> Option<Local<'s, v8::Value>> {
    debug_output(&format!("[NativeScript] raw_result_to_local: sig={} result={:p}\n", signature, result));
    let raw = result as usize;
    match signature {
        "Void" => None,
        "Guid" => Some(guid_ptr_to_js_object(result, scope).into()),
        _ if !signature.contains('.') => {
            let native_type = NativeType::try_from(signature).ok()?;
            let v: Local<v8::Value> = match native_type {
                NativeType::Void => return None,
                NativeType::Bool  => v8::Boolean::new(scope, (raw as u8) != 0).into(),
                NativeType::U8    => v8::Number::new(scope, raw as u8 as f64).into(),
                NativeType::I8    => v8::Number::new(scope, (raw as u8 as i8) as f64).into(),
                NativeType::U16   => v8::Number::new(scope, raw as u16 as f64).into(),
                NativeType::I16   => v8::Number::new(scope, (raw as u16 as i16) as f64).into(),
                NativeType::U32   => v8::Number::new(scope, raw as u32 as f64).into(),
                NativeType::I32   => v8::Number::new(scope, (raw as u32 as i32) as f64).into(),
                NativeType::U64 => {
                    let v = raw as u64;
                    if v > MAX_SAFE_INTEGER as u64 { v8::BigInt::new_from_u64(scope, v).into() }
                    else { v8::Number::new(scope, v as f64).into() }
                }
                NativeType::I64 => {
                    let v = raw as u64 as i64;
                    if v > MAX_SAFE_INTEGER as i64 || v < MIN_SAFE_INTEGER as i64 {
                        v8::BigInt::new_from_i64(scope, v).into()
                    } else {
                        v8::Number::new(scope, v as f64).into()
                    }
                }
                NativeType::USize => {
                    if raw > MAX_SAFE_INTEGER as usize { v8::BigInt::new_from_u64(scope, raw as u64).into() }
                    else { v8::Number::new(scope, raw as f64).into() }
                }
                NativeType::ISize => {
                    let v = raw as isize;
                    if !(MIN_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&v) {
                        v8::BigInt::new_from_i64(scope, v as i64).into()
                    } else {
                        v8::Number::new(scope, v as f64).into()
                    }
                }
                NativeType::F32 => v8::Number::new(scope, f32::from_bits(raw as u32) as f64).into(),
                NativeType::F64 => v8::Number::new(scope, f64::from_bits(raw as u64)).into(),
                NativeType::String => {
                    let hstring: HSTRING = std::mem::transmute(result);
                    let s = hstring.to_string();
                    v8::String::new(scope, s.as_str())?.into()
                }
                NativeType::Pointer => {
                    if result.is_null() { return None; }
                    let unknown = IUnknown::from_raw(result);
                    if let Ok(inspectable) = unknown.cast::<IInspectable>() {
                        if let Ok(class_name) = inspectable.GetRuntimeClassName() {
                            let name_str = class_name.to_string();
                            if let Some(decl) = MetadataReader::find_by_name(&name_str) {
                                let instance = unknown.clone();
                                return Some(create_ns_ctor_instance_object(
                                    &name_str, None, parent_decl, decl, Some(instance), scope,
                                ).into());
                            }
                        }
                    }
                    v8::External::new(scope, result).into()
                }
                _ => return None,
            };
            Some(v)
        }
        _ => {
            if result.is_null() { return None; }
            let com_instance = IUnknown::from_raw(result);
            let decl = MetadataReader::find_by_name(signature)?;
            Some(create_ns_ctor_instance_object(
                signature, None, parent_decl, decl, Some(com_instance), scope,
            ).into())
        }
    }
}

// ── Namespace proxy object ───────────────────────────────────────────────────

pub(crate) fn create_ns_object<'a>(
    name: &str,
    declaration: Arc<RwLock<dyn Declaration>>,
    scope: &mut v8::PinScope<'a, '_>,
) -> Local<'a, v8::Value> {
    let name = v8::String::new(scope, name).unwrap();
    let tmpl = FunctionTemplate::new(scope, handle_ns_func);
    tmpl.set_class_name(name);
    let object_tmpl = tmpl.instance_template(scope);
    object_tmpl.set_named_property_handler(
        v8::NamedPropertyHandlerConfiguration::new()
            .query(handle_named_property_query)
            .getter(handle_named_property_getter)
            .setter(handle_named_property_setter)
    );
    object_tmpl.set_internal_field_count(2);

    let object = object_tmpl.new_instance(scope).unwrap();
    let declaration = Box::new(DeclarationFFI::new(declaration));
    let ext = v8::External::new(scope, Box::into_raw(declaration) as _);
    object.set_internal_field(0, ext.into());

    let object_store = v8::Map::new(scope);
    object.set_internal_field(1, object_store.into());

    object.into()
}

// ── WinRT class/interface instance object ────────────────────────────────────

pub(crate) fn create_ns_ctor_instance_object<'a>(
    name: &str,
    factory: Option<IUnknown>,
    parent: Option<Arc<RwLock<dyn Declaration>>>,
    declaration: Arc<RwLock<dyn Declaration>>,
    instance: Option<IUnknown>,
    scope: &mut v8::PinScope<'a, '_>,
) -> Local<'a, v8::Value> {
    let class_name = v8::String::new(scope, name).unwrap();

    let tmpl = FunctionTemplate::new(scope, handle_ns_func);
    let object_tmpl = tmpl.instance_template(scope);

    object_tmpl.set_internal_field_count(1);

    let declaration_ffi = Box::into_raw(Box::new(
        DeclarationFFI::new_with_instance(declaration.clone(), instance.clone()),
    ));
    let ext = v8::External::new(scope, declaration_ffi as _);

    // Use named top-level functions instead of inline closures (named accessor pattern).
    object_tmpl.set_named_property_handler(
        v8::NamedPropertyHandlerConfiguration::new()
            .getter(handle_instance_property_getter)
            .setter(handle_instance_property_setter)
            .data(ext.into())
    );

    tmpl.set_class_name(class_name);

    let proto = tmpl.prototype_template(scope);

    {
        let lock = declaration.read();
        let kind = lock.kind();

        match kind {
            DeclarationKind::Class => {
                let clazz = lock.as_any().downcast_ref::<ClassDeclaration>().unwrap();
                let class_methods = collect_class_methods(clazz);
                let class_properties = collect_class_properties(clazz);
                let mut seen_member_names: HashSet<String> = HashSet::new();

                let to_string_func = FunctionTemplate::builder(|_scope: &mut v8::PinScope<'_, '_>,
                                                                args: v8::FunctionCallbackArguments,
                                                                mut retval: v8::ReturnValue| {
                    retval.set(args.data());
                })
                .data(class_name.into())
                .build(scope);

                let to_string = v8::String::new(scope, "toString").unwrap();
                object_tmpl.set(to_string.into(), to_string_func.into());

                for method in class_methods.iter() {
                    let method_name = if method.overload_name().is_empty() {
                        method.name().to_string()
                    } else {
                        method.overload_name().to_string()
                    };
                    let is_static = method.is_static();
                    let key = format!("{}:{}", method_name, if is_static { "static" } else { "instance" });
                    if !seen_member_names.insert(key) { continue; }

                    let name = v8::String::new(scope, method_name.as_str());
                    let declaration = DeclarationFFI::new_with_instance(
                        Arc::new(RwLock::new(method.clone())),
                        if is_static { factory.clone() } else { instance.clone() },
                    );
                    let declaration = Box::into_raw(Box::new(declaration));
                    let ext = v8::External::new(scope, declaration as _);

                    extern "C" fn callback(callback: *const v8::FunctionCallbackInfo) {
                        let info = unsafe { &*callback };
                        v8::callback_scope!(unsafe scope, info);
                        let args = unsafe { v8::FunctionCallbackArguments::from_function_callback_info(info) };
                        let mut retval = v8::ReturnValue::from_function_callback_info(info);

                        let dec = unsafe { args.data().cast::<v8::External>() };
                        let dec = dec.value() as *mut DeclarationFFI;
                        let dec = unsafe { &*dec };
                        let lock = dec.read();
                        let method = lock.as_any().downcast_ref::<MethodDeclaration>().unwrap();
                        let mut method = MethodCall::new(
                            method, method.is_sealed(), dec.instance.clone().unwrap(), false,
                        );
                        let (ret, result) = method.call(scope, &args);

                        if ret.is_err() {
                            let msg = v8::String::new(scope, &ret.message().to_string()).unwrap();
                            let err = v8::Exception::error(scope, msg.into());
                            scope.throw_exception(err);
                            return;
                        } else if !method.is_void() {
                            let return_sig = method.return_type().to_string();
                            if return_sig == "Guid" {
                                let obj = unsafe { guid_ptr_to_js_object(result, scope) };
                                retval.set(obj.into());
                            } else {
                                match NativeType::try_from(return_sig.as_str()) {
                                    Ok(return_type) => {
                                        if return_sig.contains('.') {
                                            let instance = unsafe { IUnknown::from_raw(result) };
                                            let lookup = crate::helpers::strip_generic_suffix(return_sig.as_str());
                                            let declaration = MetadataReader::find_by_name(lookup)
                                                .unwrap_or_else(|| dec.inner.clone());
                                            let ret: Local<v8::Value> = create_ns_ctor_instance_object(
                                                return_sig.as_str(), None, dec.parent.clone(), declaration, Some(instance), scope,
                                            ).into();
                                            retval.set(ret.into());
                                            return;
                                        }
                                        unsafe { set_ret_val(result, scope, retval, return_type); }
                                    }
                                    Err(_) => {}
                                }
                            }
                        } else {
                            retval.set_undefined();
                        }
                    }

                    let func = FunctionTemplate::builder_raw(callback)
                        .data(ext.into())
                        .build(scope);

                    if is_static {
                        tmpl.set_with_attr(name.unwrap().into(), func.into(), v8::PropertyAttribute::DONT_DELETE);
                    } else {
                        object_tmpl.set_with_attr(name.unwrap().into(), func.into(), v8::PropertyAttribute::DONT_DELETE);
                    }
                }

                for property in class_properties.iter() {
                    let property_name = property.name().to_string();
                    let is_static = property.is_static();
                    let key = format!("{}:{}", property_name, if is_static { "static" } else { "instance" });
                    if !seen_member_names.insert(key) { continue; }

                    let name = v8::String::new(scope, property_name.as_str());
                    let declaration = DeclarationFFI::new_with_instance(
                        Arc::new(RwLock::new(property.clone())),
                        if is_static { factory.clone() } else { instance.clone() },
                    );

                    let getter_declaration = declaration.clone();
                    let getter_declaration = Box::into_raw(Box::new(getter_declaration));
                    let getter_declaration_ext = v8::External::new(scope, getter_declaration as _);

                    let getter = FunctionTemplate::builder(|scope: &mut v8::PinScope<'_, '_>,
                                                            args: v8::FunctionCallbackArguments,
                                                            mut retval: v8::ReturnValue| {
                        let dec = unsafe { args.data().cast::<v8::External>() };
                        let dec = dec.value() as *mut DeclarationFFI;
                        let dec = unsafe { &*dec };
                        let lock = dec.read();
                        let method = lock.as_any().downcast_ref::<PropertyDeclaration>().unwrap();
                        let Some(mut method) = PropertyCall::new(method, false, dec.instance.clone().unwrap(), false) else { return; };
                        let (ret, result) = method.call(scope, &args);
                        if ret.is_err() {
                            let msg = v8::String::new(scope, &ret.message().to_string()).unwrap();
                            let err = v8::Exception::error(scope, msg.into());
                            scope.throw_exception(err);
                            return;
                        } else if !method.is_void() {
                            let return_sig = method.return_type().to_string();
                            if return_sig.contains('.') {
                                let instance = unsafe { IUnknown::from_raw(result) };
                                let lookup = crate::helpers::strip_generic_suffix(return_sig.as_str());
                                if let Some(declaration) = MetadataReader::find_by_name(lookup) {
                                    let ret: Local<v8::Value> = create_ns_ctor_instance_object(return_sig.as_str(), None, None, declaration, Some(instance), scope).into();
                                    retval.set(ret.into());
                                    return;
                                }
                            }
                            match NativeType::try_from(return_sig.as_str()) {
                                Ok(return_type) => { unsafe { set_ret_val(result, scope, retval, return_type); } }
                                Err(_) => {}
                            }
                        } else {
                            retval.set_undefined();
                        }
                    })
                    .data(getter_declaration_ext.into())
                    .build(scope);

                    let mut setter: Option<Local<FunctionTemplate>> = None;
                    if property.setter().is_some() {
                        let setter_declaration = declaration;
                        let setter_declaration = Box::into_raw(Box::new(setter_declaration));
                        let setter_declaration_ext = v8::External::new(scope, setter_declaration as _);
                        setter = Some(FunctionTemplate::builder(|scope: &mut v8::PinScope<'_, '_>,
                                                                 args: v8::FunctionCallbackArguments,
                                                                 _retval: v8::ReturnValue| {
                            let dec = unsafe { args.data().cast::<v8::External>() };
                            let dec = dec.value() as *mut DeclarationFFI;
                            let dec = unsafe { &*dec };
                            let lock = dec.read();
                            let prop = lock.as_any().downcast_ref::<PropertyDeclaration>().unwrap();
                            let Some(mut method) = PropertyCall::new(prop, true, dec.instance.clone().unwrap(), false) else { return; };
                            let (ret, _) = method.call(scope, &args);
                            if ret.is_err() {
                                let msg = v8::String::new(scope, &ret.message().to_string()).unwrap();
                                let err = v8::Exception::error(scope, msg);
                                scope.throw_exception(err);
                            }
                        })
                        .data(setter_declaration_ext.into())
                        .build(scope));
                    }

                    if property.is_static() {
                        let name = name.unwrap();
                        tmpl.set_accessor_property(name.into(), Some(getter), setter, v8::PropertyAttribute::DONT_DELETE);
                    } else {
                        let name = name.unwrap();
                        object_tmpl.set_accessor_property(name.into(), Some(getter), setter, v8::PropertyAttribute::NONE);
                    }
                }
            }
            DeclarationKind::Interface
            | DeclarationKind::GenericInterface
            | DeclarationKind::GenericInterfaceInstance => {
                let clazz: &dyn BaseClassDeclarationImpl = match kind {
                    DeclarationKind::Interface => lock.as_any().downcast_ref::<InterfaceDeclaration>().unwrap(),
                    DeclarationKind::GenericInterface => lock.as_any().downcast_ref::<GenericInterfaceDeclaration>().unwrap(),
                    DeclarationKind::GenericInterfaceInstance => lock.as_any().downcast_ref::<GenericInterfaceInstanceDeclaration>().unwrap(),
                    _ => unreachable!(),
                };

                let to_string_func = FunctionTemplate::builder(|_scope: &mut v8::PinScope<'_, '_>,
                                                                args: v8::FunctionCallbackArguments,
                                                                mut retval: v8::ReturnValue| {
                    retval.set(args.data());
                })
                .data(class_name.into())
                .build(scope);

                let to_string = v8::String::new(scope, "toString").unwrap();
                proto.set(to_string.into(), to_string_func.into());

                if let Some(clazz) = parent {
                    let clazz = clazz.read();
                    let kind = clazz.kind();

                    match kind {
                        DeclarationKind::Class => {
                            let clazz = clazz.as_any().downcast_ref::<ClassDeclaration>().unwrap();

                            for method in clazz.methods().iter() {
                                let name = v8::String::new(scope, method.name());
                                let is_static = method.is_static();
                                let declaration = DeclarationFFI::new_with_instance(
                                    Arc::new(RwLock::new(method.clone())),
                                    if is_static { factory.clone() } else { instance.clone() },
                                );
                                let declaration = Box::into_raw(Box::new(declaration));
                                let ext = v8::External::new(scope, declaration as _);

                                let func = v8::FunctionTemplate::builder(|scope: &mut v8::PinScope<'_, '_>,
                                                                          args: v8::FunctionCallbackArguments,
                                                                          mut retval: v8::ReturnValue| {
                                    let dec = unsafe { args.data().cast::<v8::External>() };
                                    let dec = dec.value() as *mut DeclarationFFI;
                                    let dec = unsafe { &*dec };
                                    let lock = dec.read();
                                    let method = lock.as_any().downcast_ref::<MethodDeclaration>().unwrap();
                                    let mut method = MethodCall::new(method, method.is_sealed(), dec.instance.clone().unwrap(), false);
                                    let (ret, result) = method.call(scope, &args);
                                    if ret.is_err() {
                                        let msg = v8::String::new(scope, &ret.message().to_string()).unwrap();
                                        let err = v8::Exception::error(scope, msg.into());
                                        scope.throw_exception(err);
                                        return;
                                    } else if !method.is_void() {
                                        match NativeType::try_from(method.return_type()) {
                                            Ok(return_type) => { unsafe { set_ret_val(result, scope, retval, return_type); } }
                                            Err(_) => {}
                                        }
                                    } else {
                                        retval.set_undefined();
                                    }
                                })
                                .data(ext.into())
                                .build(scope);

                                if is_static {
                                    tmpl.set(name.unwrap().into(), func.into());
                                } else {
                                    proto.set(name.unwrap().into(), func.into());
                                }
                            }

                            for property in clazz.properties().iter() {
                                let name = v8::String::new(scope, property.name());
                                let is_static = property.is_static();
                                let declaration = DeclarationFFI::new_with_instance(
                                    Arc::new(RwLock::new(property.clone())),
                                    if is_static { factory.clone() } else { instance.clone() },
                                );

                                let getter_declaration = declaration.clone();
                                let getter_declaration = Box::into_raw(Box::new(getter_declaration));
                                let getter_declaration_ext = v8::External::new(scope, getter_declaration as _);

                                let getter = FunctionTemplate::builder(|scope: &mut v8::PinScope<'_, '_>,
                                                                        args: v8::FunctionCallbackArguments,
                                                                        mut retval: v8::ReturnValue| {
                                    let dec = unsafe { args.data().cast::<v8::External>() };
                                    let dec = dec.value() as *mut DeclarationFFI;
                                    let dec = unsafe { &*dec };
                                    let lock = dec.read();
                                    let method = lock.as_any().downcast_ref::<PropertyDeclaration>().unwrap();
                                    let mut method = MethodCall::new(method.getter(), false, dec.instance.clone().unwrap(), false);
                                    let (ret, result) = method.call(scope, &args);
                                    if ret.is_err() {
                                        let msg = v8::String::new(scope, &ret.message().to_string()).unwrap();
                                        let err = v8::Exception::error(scope, msg.into());
                                        scope.throw_exception(err);
                                        return;
                                    } else if !method.is_void() {
                                        match NativeType::try_from(method.return_type()) {
                                            Ok(return_type) => { unsafe { set_ret_val(result, scope, retval, return_type); } }
                                            Err(_) => {}
                                        }
                                    } else {
                                        retval.set_undefined();
                                    }
                                })
                                .data(getter_declaration_ext.into())
                                .build(scope);

                                let mut setter: Option<Local<FunctionTemplate>> = None;
                                if property.setter().is_some() {
                                    let setter_declaration = declaration;
                                    let setter_declaration = Box::into_raw(Box::new(setter_declaration));
                                    let setter_declaration_ext = v8::External::new(scope, setter_declaration as _);
                                    setter = Some(FunctionTemplate::builder(|_scope: &mut v8::PinScope<'_, '_>,
                                                                             _args: v8::FunctionCallbackArguments,
                                                                             _retval: v8::ReturnValue| {})
                                        .data(setter_declaration_ext.into())
                                        .build(scope));
                                }

                                if property.is_static() {
                                    let name = name.unwrap();
                                    tmpl.set_accessor_property(name.into(), Some(getter), setter, v8::PropertyAttribute::DONT_DELETE);
                                } else {
                                    let name = name.unwrap();
                                    proto.set_accessor_property(name.into(), Some(getter), setter, v8::PropertyAttribute::READ_ONLY | v8::PropertyAttribute::DONT_DELETE);
                                }
                            }
                        }
                        DeclarationKind::Interface
                        | DeclarationKind::GenericInterface
                        | DeclarationKind::GenericInterfaceInstance => {
                            let iface_kind = kind;
                            let clazz: &dyn BaseClassDeclarationImpl = match iface_kind {
                                DeclarationKind::Interface => clazz.as_any().downcast_ref::<InterfaceDeclaration>().unwrap(),
                                DeclarationKind::GenericInterface => clazz.as_any().downcast_ref::<GenericInterfaceDeclaration>().unwrap(),
                                DeclarationKind::GenericInterfaceInstance => clazz.as_any().downcast_ref::<GenericInterfaceInstanceDeclaration>().unwrap(),
                                _ => unreachable!(),
                            };

                            for method in clazz.methods().iter() {
                                let name = v8::String::new(scope, method.name());
                                let is_static = method.is_static();
                                let declaration = DeclarationFFI::new_with_instance(
                                    Arc::new(RwLock::new(method.clone())),
                                    if is_static { factory.clone() } else { instance.clone() },
                                );
                                let declaration = Box::into_raw(Box::new(declaration));
                                let ext = v8::External::new(scope, declaration as _);

                                let func = v8::FunctionTemplate::builder(|scope: &mut v8::PinScope<'_, '_>,
                                                                          args: v8::FunctionCallbackArguments,
                                                                          mut retval: v8::ReturnValue| {
                                    let dec = unsafe { args.data().cast::<v8::External>() };
                                    let dec = dec.value() as *mut DeclarationFFI;
                                    let dec = unsafe { &*dec };
                                    let lock = dec.read();
                                    let method = lock.as_any().downcast_ref::<MethodDeclaration>().unwrap();
                                    let mut method = MethodCall::new(method, method.is_sealed(), dec.instance.clone().unwrap(), false);
                                    let (ret, result) = method.call(scope, &args);
                                    if ret.is_err() {
                                        let msg = v8::String::new(scope, &ret.message().to_string()).unwrap();
                                        let err = v8::Exception::error(scope, msg.into());
                                        scope.throw_exception(err);
                                        return;
                                    } else if !method.is_void() {
                                        match NativeType::try_from(method.return_type()) {
                                            Ok(return_type) => { unsafe { set_ret_val(result, scope, retval, return_type); } }
                                            Err(_) => {}
                                        }
                                    } else {
                                        retval.set_undefined();
                                    }
                                })
                                .data(ext.into())
                                .build(scope);

                                if is_static {
                                    tmpl.set(name.unwrap().into(), func.into());
                                } else {
                                    proto.set(name.unwrap().into(), func.into());
                                }
                            }

                            for property in clazz.properties().iter() {
                                let name = v8::String::new(scope, property.name());
                                let is_static = property.is_static();
                                let declaration = DeclarationFFI::new_with_instance(
                                    Arc::new(RwLock::new(property.clone())),
                                    if is_static { factory.clone() } else { instance.clone() },
                                );

                                let getter_declaration = declaration.clone();
                                let getter_declaration = Box::into_raw(Box::new(getter_declaration));
                                let getter_declaration_ext = v8::External::new(scope, getter_declaration as _);

                                let getter = FunctionTemplate::builder(|scope: &mut v8::PinScope<'_, '_>,
                                                                        args: v8::FunctionCallbackArguments,
                                                                        mut retval: v8::ReturnValue| {
                                    let dec = unsafe { args.data().cast::<v8::External>() };
                                    let dec = dec.value() as *mut DeclarationFFI;
                                    let dec = unsafe { &*dec };
                                    let lock = dec.read();
                                    let method = lock.as_any().downcast_ref::<PropertyDeclaration>().unwrap();
                                    let Some(mut method) = PropertyCall::new(method, false, dec.instance.clone().unwrap(), false) else { return; };
                                    let (ret, result) = method.call(scope, &args);
                                    if ret.is_err() {
                                        let msg = v8::String::new(scope, &ret.message().to_string()).unwrap();
                                        let err = v8::Exception::error(scope, msg.into());
                                        scope.throw_exception(err);
                                        return;
                                    } else if !method.is_void() {
                                        match NativeType::try_from(method.return_type()) {
                                            Ok(return_type) => { unsafe { set_ret_val(result, scope, retval, return_type); } }
                                            Err(_) => {}
                                        }
                                    } else {
                                        retval.set_undefined();
                                    }
                                })
                                .data(getter_declaration_ext.into())
                                .build(scope);

                                let mut setter: Option<Local<FunctionTemplate>> = None;
                                if property.setter().is_some() {
                                    let setter_declaration = declaration;
                                    let setter_declaration = Box::into_raw(Box::new(setter_declaration));
                                    let setter_declaration_ext = v8::External::new(scope, setter_declaration as _);
                                    setter = Some(FunctionTemplate::builder(|_scope: &mut v8::PinScope<'_, '_>,
                                                                             _args: v8::FunctionCallbackArguments,
                                                                             _retval: v8::ReturnValue| {})
                                        .data(setter_declaration_ext.into())
                                        .build(scope));
                                }

                                if property.is_static() {
                                    let name = name.unwrap();
                                    tmpl.set_accessor_property(name.into(), Some(getter), setter, v8::PropertyAttribute::DONT_DELETE);
                                } else {
                                    let name = name.unwrap();
                                    proto.set_accessor_property(name.into(), Some(getter), setter, v8::PropertyAttribute::READ_ONLY | v8::PropertyAttribute::DONT_DELETE);
                                }
                            }
                        }
                        DeclarationKind::Delegate
                        | DeclarationKind::GenericDelegate
                        | DeclarationKind::GenericDelegateInstance => {
                            let method = match kind {
                                DeclarationKind::Delegate => clazz
                                    .as_any()
                                    .downcast_ref::<DelegateDeclaration>()
                                    .map(|delegate| delegate.invoke_method().clone()),
                                DeclarationKind::GenericDelegate => clazz
                                    .as_any()
                                    .downcast_ref::<GenericDelegateDeclaration>()
                                    .map(|delegate| delegate.invoke_method().clone()),
                                DeclarationKind::GenericDelegateInstance => clazz
                                    .as_any()
                                    .downcast_ref::<GenericDelegateInstanceDeclaration>()
                                    .map(|delegate| delegate.invoke_method().clone()),
                                _ => None,
                            };

                            if let Some(method) = method {
                                let name = v8::String::new(scope, method.name());
                                let declaration = DeclarationFFI::new_with_instance(
                                    Arc::new(RwLock::new(method)),
                                    instance.clone(),
                                );
                                let declaration = Box::into_raw(Box::new(declaration));
                                let ext = v8::External::new(scope, declaration as _);

                                let func = v8::FunctionTemplate::builder(|scope: &mut v8::PinScope<'_, '_>,
                                                                          args: v8::FunctionCallbackArguments,
                                                                          mut retval: v8::ReturnValue| {
                                    let dec = unsafe { args.data().cast::<v8::External>() };
                                    let dec = dec.value() as *mut DeclarationFFI;
                                    let dec = unsafe { &*dec };
                                    let lock = dec.read();
                                    let method = lock.as_any().downcast_ref::<MethodDeclaration>().unwrap();
                                    let mut method = MethodCall::new(method, method.is_sealed(), dec.instance.clone().unwrap(), false);
                                    let (ret, result) = method.call(scope, &args);
                                    if ret.is_err() {
                                        let msg = v8::String::new(scope, &ret.message().to_string()).unwrap();
                                        let err = v8::Exception::error(scope, msg.into());
                                        scope.throw_exception(err);
                                        return;
                                    } else if !method.is_void() {
                                        if let Ok(return_type) = NativeType::try_from(method.return_type()) {
                                            unsafe { set_ret_val(result, scope, retval, return_type); }
                                        }
                                    } else {
                                        retval.set_undefined();
                                    }
                                })
                                .data(ext.into())
                                .build(scope);

                                if let Some(name) = name {
                                    proto.set(name.into(), func.into());
                                }
                            }
                        }
                        _ => {}
                    }
                }

                for method in clazz.methods().iter() {
                    let name = v8::String::new(scope, method.name());
                    let is_static = method.is_static();
                    let declaration = DeclarationFFI::new_with_instance(
                        Arc::new(RwLock::new(method.clone())),
                        if is_static { factory.clone() } else { instance.clone() },
                    );
                    let declaration = Box::into_raw(Box::new(declaration));
                    let ext = v8::External::new(scope, declaration as _);

                    let func = v8::FunctionTemplate::builder(|scope: &mut v8::PinScope<'_, '_>,
                                                              args: v8::FunctionCallbackArguments,
                                                              mut retval: v8::ReturnValue| {
                        let dec = unsafe { args.data().cast::<v8::External>() };
                        let dec = dec.value() as *mut DeclarationFFI;
                        let dec = unsafe { &*dec };
                        let lock = dec.read();
                        let method = lock.as_any().downcast_ref::<MethodDeclaration>().unwrap();
                        let mut method = MethodCall::new(method, method.is_sealed(), dec.instance.clone().unwrap(), false);
                        let (ret, result) = method.call(scope, &args);
                        if ret.is_err() {
                            let msg = v8::String::new(scope, &ret.message().to_string()).unwrap();
                            let err = v8::Exception::error(scope, msg.into());
                            scope.throw_exception(err);
                            return;
                        } else if !method.is_void() {
                            match NativeType::try_from(method.return_type()) {
                                Ok(return_type) => { unsafe { set_ret_val(result, scope, retval, return_type); } }
                                Err(_) => {}
                            }
                        } else {
                            retval.set_undefined();
                        }
                    })
                    .data(ext.into())
                    .build(scope);

                    if is_static {
                        tmpl.set(name.unwrap().into(), func.into());
                    } else {
                        proto.set(name.unwrap().into(), func.into());
                    }
                }

                for property in clazz.properties().iter() {
                    let name = v8::String::new(scope, property.name());
                    let is_static = property.is_static();
                    let declaration = DeclarationFFI::new_with_instance(
                        Arc::new(RwLock::new(property.clone())),
                        if is_static { factory.clone() } else { instance.clone() },
                    );

                    let getter_declaration = declaration.clone();
                    let getter_declaration = Box::into_raw(Box::new(getter_declaration));
                    let getter_declaration_ext = v8::External::new(scope, getter_declaration as _);

                    let getter = FunctionTemplate::builder(|scope: &mut v8::PinScope<'_, '_>,
                                                            args: v8::FunctionCallbackArguments,
                                                            mut retval: v8::ReturnValue| {
                        let dec = unsafe { args.data().cast::<v8::External>() };
                        let dec = dec.value() as *mut DeclarationFFI;
                        let dec = unsafe { &*dec };
                        let lock = dec.read();
                        let method = lock.as_any().downcast_ref::<PropertyDeclaration>().unwrap();
                        let Some(mut method) = PropertyCall::new(method, false, dec.instance.clone().unwrap(), false) else { return; };
                        let (ret, result) = method.call(scope, &args);
                        if ret.is_err() {
                            let msg = v8::String::new(scope, &ret.message().to_string()).unwrap();
                            let err = v8::Exception::error(scope, msg.into());
                            scope.throw_exception(err);
                            return;
                        } else if !method.is_void() {
                            match NativeType::try_from(method.return_type()) {
                                Ok(return_type) => { unsafe { set_ret_val(result, scope, retval, return_type); } }
                                Err(_) => {}
                            }
                        } else {
                            retval.set_undefined();
                        }
                    })
                    .data(getter_declaration_ext.into())
                    .build(scope);

                    let mut setter: Option<Local<FunctionTemplate>> = None;
                    if property.setter().is_some() {
                        let setter_declaration = declaration;
                        let setter_declaration = Box::into_raw(Box::new(setter_declaration));
                        let setter_declaration_ext = v8::External::new(scope, setter_declaration as _);
                        setter = Some(FunctionTemplate::builder(|scope: &mut v8::PinScope<'_, '_>,
                                                                 args: v8::FunctionCallbackArguments,
                                                                 _retval: v8::ReturnValue| {
                            let dec = unsafe { args.data().cast::<v8::External>() };
                            let dec = dec.value() as *mut DeclarationFFI;
                            let dec = unsafe { &*dec };
                            let lock = dec.read();
                            let prop = lock.as_any().downcast_ref::<PropertyDeclaration>().unwrap();
                            let setter = prop.setter().unwrap();
                            let mut method = MethodCall::new(setter, false, dec.instance.clone().unwrap(), false);
                            let (ret, _) = method.call(scope, &args);
                            if ret.is_err() {
                                let msg = v8::String::new(scope, &ret.message().to_string()).unwrap();
                                let err = v8::Exception::error(scope, msg);
                                scope.throw_exception(err);
                            }
                        })
                        .data(setter_declaration_ext.into())
                        .build(scope));
                    }

                    if property.is_static() {
                        let name = name.unwrap();
                        tmpl.set_accessor_property(name.into(), Some(getter), setter, v8::PropertyAttribute::DONT_DELETE);
                    } else {
                        let name = name.unwrap();
                        proto.set_accessor_property(name.into(), Some(getter), setter, v8::PropertyAttribute::READ_ONLY | v8::PropertyAttribute::DONT_DELETE);
                    }
                }
            }
            DeclarationKind::GenericInterface => {
                let clazz = lock.as_any().downcast_ref::<GenericInterfaceDeclaration>().unwrap();
                let return_types = crate::helpers::get_generic_return_types(name);

                for method in clazz.methods() {
                    let signature = method.return_type();
                    let return_type = Signature::to_string(method.metadata().unwrap(), &signature);
                    let return_type_index = usize::from_str_radix(&*return_type.as_str().replace("Var!", ""), 10).unwrap();
                    let return_type = *return_types.names().get(return_type_index).unwrap();

                    let name = v8::String::new(scope, method.name());
                    let is_static = method.is_static();
                    let parent = declaration.clone();
                    let mut declaration = DeclarationFFI::new_with_instance(
                        Arc::new(RwLock::new(method.clone())),
                        if is_static { factory.clone() } else { instance.clone() },
                    );
                    declaration.parent = Some(parent);
                    let declaration = Box::into_raw(Box::new(declaration));
                    let return_type = v8::String::new(scope, return_type).unwrap();
                    let ext = v8::External::new(scope, declaration as _);
                    let data = v8::Array::new_with_elements(scope, &[ext.into(), return_type.into()]);

                    let func = FunctionTemplate::builder(|scope: &mut v8::PinScope<'_, '_>,
                                                          args: v8::FunctionCallbackArguments,
                                                          mut retval: v8::ReturnValue| {
                        let data = v8::Local::<v8::Array>::try_from(args.data()).unwrap();
                        let return_type = data.get_index(scope, 1).unwrap().to_rust_string_lossy(scope);
                        let dec = unsafe { data.get_index(scope, 0).unwrap().cast::<v8::External>() };
                        let dec = dec.value() as *mut DeclarationFFI;
                        let dec = unsafe { &*dec };
                        let lock = dec.read();
                        let method = lock.as_any().downcast_ref::<MethodDeclaration>().unwrap();
                        let parent = dec.parent.as_ref().unwrap();
                        let parent = parent.read();
                        let parent = parent.as_any().downcast_ref::<GenericInterfaceDeclaration>().unwrap();
                        let mut method = GenericMethodCall::new(
                            parent, method, method.is_sealed(), dec.instance.clone().unwrap(), false, return_type,
                        );
                        let (ret, result) = method.call(scope, &args);
                        if ret.is_err() {
                            let msg = v8::String::new(scope, &ret.message().to_string()).unwrap();
                            let err = v8::Exception::error(scope, msg.into());
                            scope.throw_exception(err);
                            return;
                        } else if !method.is_void() {
                            let return_sig = method.return_type();
                            match NativeType::try_from(return_sig) {
                                Ok(return_type) => {
                                    if return_sig.contains('.') {
                                        let instance = unsafe { IUnknown::from_raw(*(result as *mut *mut c_void)) };
                                        let lookup = crate::helpers::strip_generic_suffix(return_sig);
                                        let declaration = MetadataReader::find_by_name(lookup)
                                            .unwrap_or_else(|| dec.inner.clone());
                                        let ret: Local<v8::Value> = create_ns_ctor_instance_object(
                                            return_sig, None, dec.parent.clone(), declaration, Some(instance), scope,
                                        ).into();
                                        retval.set(ret.into());
                                        return;
                                    }
                                    unsafe { set_ret_val(result, scope, retval, return_type); }
                                }
                                Err(_) => {}
                            }
                        } else {
                            retval.set_undefined();
                        }
                    })
                    .data(data.into())
                    .build(scope);

                    if is_static {
                        tmpl.set_with_attr(name.unwrap().into(), func.into(), v8::PropertyAttribute::DONT_DELETE);
                    } else {
                        proto.set_with_attr(name.unwrap().into(), func.into(), v8::PropertyAttribute::DONT_DELETE);
                    }
                }
            }
            _ => {}
        }
    }

    debug_output("[NativeScript] create_ns_ctor_instance_object: calling new_instance\n");
    let object = match object_tmpl.new_instance(scope) {
        Some(o) => o,
        None => {
            debug_output("[NativeScript] create_ns_ctor_instance_object: new_instance returned None!\n");
            let msg = v8::String::new(scope, "Failed to create instance object").unwrap();
            let err = v8::Exception::error(scope, msg.into());
            scope.throw_exception(err);
            return v8::null(scope).into();
        }
    };
    debug_output("[NativeScript] create_ns_ctor_instance_object: new_instance OK\n");

    object.set_internal_field(0, ext.into());

    if let Some(handle_key) = v8::String::new(scope, "handle") {
        let handle_value: Local<v8::Value> = if let Some(instance) = instance.as_ref() {
            v8::External::new(scope, instance.as_raw() as *mut c_void).into()
        } else {
            v8::null(scope).into()
        };
        object.set(scope, handle_key.into(), handle_value);
    }

    object.into()
}

// ── Class constructor object ─────────────────────────────────────────────────

pub(crate) fn create_ns_ctor_object<'a>(
    name: &str,
    parent: Option<Arc<RwLock<dyn Declaration>>>,
    declaration: Arc<RwLock<dyn Declaration>>,
    scope: &mut v8::PinScope<'a, '_>,
) -> Local<'a, v8::Value> {
    let name = v8::String::new(scope, name).unwrap();

    let mut ext = DeclarationFFI::new(Arc::clone(&declaration));
    ext.parent = parent;
    let ext = Box::into_raw(Box::new(ext));
    let ext = v8::External::new(scope, ext as _);

    let tmpl = v8::FunctionTemplate::builder(|scope: &mut v8::PinScope<'_, '_>,
                                              args: v8::FunctionCallbackArguments,
                                              mut retval: v8::ReturnValue| {
        let length = args.length();
        let dec = unsafe { args.data().cast::<v8::External>() };
        let dec = dec.value() as *mut DeclarationFFI;
        let dec = unsafe { &*dec };
        let lock = dec.read();
        let kind = lock.kind();
        let ext = args.data();

        match kind {
            DeclarationKind::Class => {
                let clazz = lock.as_any().downcast_ref::<ClassDeclaration>().unwrap();
                debug_output(&format!("[NativeScript] ctor-callback: new {}\n", clazz.full_name()));

                let clazz_factory = match class_activation_factory(clazz.full_name()) {
                    Ok(factory) => factory,
                    Err(error) => {
                        throw_js_error(scope, &format!(
                            "Failed to activate WinRT class {}: {}", clazz.full_name(), error.message()
                        ));
                        return;
                    }
                };

                if length == 0 {
                    match clazz_factory.cast::<IActivationFactory>() {
                        Ok(activation_factory) => {
                            match unsafe { activation_factory.ActivateInstance() } {
                                Ok(instance) => {
                                    let result = match instance.cast::<IUnknown>() {
                                        Ok(value) => value,
                                        Err(error) => {
                                            throw_js_error(scope, &format!(
                                                "Failed to cast activated instance for {}: {}",
                                                clazz.full_name(), error.message()
                                            ));
                                            return;
                                        }
                                    };

                                    if let Ok(init) = result.cast::<IInitializeWithWindow>() {
                                        let hwnd = unsafe { GetConsoleWindow() };
                                        if !hwnd.is_invalid() {
                                            let _ = unsafe { init.Initialize(hwnd) };
                                        }
                                    }

                                    let instance = create_ns_ctor_instance_object(
                                        clazz.name(), Some(clazz_factory.clone()), None,
                                        dec.inner.clone(), Some(result), scope,
                                    );
                                    retval.set(instance);
                                    return;
                                }
                                Err(error) => {
                                    throw_js_error(scope, &format!(
                                        "ActivateInstance failed for WinRT class {}: {}",
                                        clazz.full_name(), error.message()
                                    ));
                                    return;
                                }
                            }
                        }
                        Err(_) => {}
                    }
                }

                unsafe {
                    let is_sealed = clazz.is_sealed();
                    for ctor in clazz.initializers() {
                        let number_of_parameters = ctor.number_of_parameters();
                        if number_of_parameters != length as usize { continue; }
                        let mut method = MethodCall::new(ctor, is_sealed, clazz_factory.clone(), true);
                        let (ret, result) = method.call(scope, &args);

                        if ret.is_ok() {
                            let result = IUnknown::from_raw(result);
                            let vtable = result.vtable();
                            let mut ret: *mut c_void = std::ptr::null_mut();
                            let res = unsafe {
                                ((*vtable).QueryInterface)(
                                    result.as_raw(), &IUnknown::IID, std::mem::transmute(&mut ret),
                                )
                            };

                            if res.is_err() || ret.is_null() {
                                let message = res.message().to_string();
                                let message = v8::String::new(scope, message.as_str()).unwrap();
                                let error = v8::Exception::error(scope, message.into());
                                scope.throw_exception(error);
                                return;
                            }

                            let result = IUnknown::from_raw(ret);

                            if let Ok(init) = result.cast::<IInitializeWithWindow>() {
                                let hwnd = unsafe { GetConsoleWindow() };
                                if !hwnd.is_invalid() {
                                    let _ = unsafe { init.Initialize(hwnd) };
                                }
                            }

                            let instance = create_ns_ctor_instance_object(
                                clazz.name(), Some(clazz_factory), None, dec.inner.clone(), Some(result), scope,
                            );
                            retval.set(instance);
                            return;
                        } else {
                            let message = ret.message().to_string();
                            let message = v8::String::new(scope, message.as_str()).unwrap();
                            let error = v8::Exception::error(scope, message.into());
                            scope.throw_exception(error);
                        }
                    }
                }
            }
            DeclarationKind::Struct => {}
            DeclarationKind::Delegate
            | DeclarationKind::GenericDelegate
            | DeclarationKind::GenericDelegateInstance => {
                if length >= 1 {
                    let arg0 = args.get(0);
                    let maybe_func: Option<v8::Local<v8::Function>> = if arg0.is_function() {
                        v8::Local::<v8::Function>::try_from(arg0).ok()
                    } else if arg0.is_object() {
                        if let Some(obj) = arg0.to_object(scope) {
                            let mut found = None;
                            for name in &["Invoke", "invoke"] {
                                if let Some(key) = v8::String::new(scope, name) {
                                    if let Some(val) = obj.get(scope, key.into()) {
                                        if let Ok(f) = v8::Local::<v8::Function>::try_from(val) {
                                            found = Some(f);
                                            break;
                                        }
                                    }
                                }
                            }
                            found
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    if let Some(func) = maybe_func {
                        if let Some((guid, param_types)) = js_delegate_params_from_declaration(&*lock, kind) {
                            debug_output(&format!("[NativeScript] delegate ctor: created JsDelegate guid={:?} params={}\n", guid, param_types.len()));
                            let global_func = v8::Global::new(scope, func);
                            let data = Box::new(JsDelegateData { js_func: global_func, param_types });
                            let delegate = Box::new(JsDelegate {
                                vtable: &JS_DELEGATE_VTBL as *const _,
                                ref_count: std::sync::atomic::AtomicU32::new(1),
                                guid,
                                data: Box::into_raw(data),
                            });
                            let raw = Box::into_raw(delegate) as *mut c_void;
                            let result_obj = v8::Object::new(scope);
                            if let Some(key) = v8::String::new(scope, "handle") {
                                result_obj.set(scope, key.into(), v8::External::new(scope, raw).into());
                            }
                            retval.set(result_obj.into());
                            return;
                        }
                    }
                }
            }
            _ => {}
        }

        let object_tmpl = v8::ObjectTemplate::new(scope);
        object_tmpl.set_named_property_handler(
            v8::NamedPropertyHandlerConfiguration::new()
                .query(handle_named_property_query)
                .getter(handle_named_property_getter)
                .setter(handle_named_property_setter)
        );
        object_tmpl.set_indexed_property_handler(
            v8::IndexedPropertyHandlerConfiguration::new()
                .setter(handle_indexed_property_setter)
                .getter(handle_indexed_property_getter)
        );
        object_tmpl.set_internal_field_count(2);
        let object = object_tmpl.new_instance(scope).unwrap();
        object.set_internal_field(0, ext.into());

        let object_store = v8::Map::new(scope);

        if matches!(
            kind,
            DeclarationKind::Interface
                | DeclarationKind::GenericInterface
                | DeclarationKind::GenericInterfaceInstance
                | DeclarationKind::Delegate
                | DeclarationKind::GenericDelegate
                | DeclarationKind::GenericDelegateInstance
                | DeclarationKind::Event
        ) && length >= 1
        {
            let implementation = args.get(0);
            if implementation.is_object() || implementation.is_function() {
                if let Some(impl_key) = v8::String::new(scope, "__implementation__") {
                    object_store.set(scope, impl_key.into(), implementation);
                }
            }
        }

        object.set_internal_field(1, object_store.into());
        retval.set(object.into());
    })
    .data(ext.into())
    .build(scope);
    tmpl.set_class_name(name);

    {
        let lock = declaration.read();

        if lock.kind() != DeclarationKind::Class {
            let func = tmpl.get_function(scope).unwrap();
            return func.into();
        }

        let clazz = lock.as_any().downcast_ref::<ClassDeclaration>().unwrap();
        debug_output(&format!("[NativeScript] create_ns_ctor_object: building methods for {}\n", clazz.full_name()));

        for method in clazz.methods().iter() {
            let name = v8::String::new(scope, method.name());
            let is_static = method.is_static();
            if !is_static { continue; }

            let parent = Arc::clone(&declaration);
            let mut declaration = DeclarationFFI::new_with_instance(
                Arc::new(RwLock::new(method.clone())),
                None,
            );
            declaration.parent = Some(parent);
            let declaration = Box::into_raw(Box::new(declaration));
            let ext = v8::External::new(scope, declaration as _);

            let func = v8::FunctionTemplate::builder(|scope: &mut v8::PinScope<'_, '_>,
                                                      args: v8::FunctionCallbackArguments,
                                                      mut retval: v8::ReturnValue| {
                let dec = unsafe { args.data().cast::<v8::External>() };
                let dec = dec.value() as *mut DeclarationFFI;
                let dec = unsafe { &*dec };
                let lock = dec.read();
                let method = lock.as_any().downcast_ref::<MethodDeclaration>().unwrap();
                let return_type = method.return_type();
                let signature = Signature::to_string(method.metadata().unwrap(), &return_type);

                let factory = match resolve_class_factory_from_parent(dec) {
                    Ok(factory) => factory,
                    Err(error) => {
                        throw_js_error(scope, &format!(
                            "Failed to resolve WinRT static method factory for {}: {}",
                            method.name(), error.message()
                        ));
                        return;
                    }
                };

                let mut method = MethodCall::new(method, method.is_sealed(), factory, false);
                let (ret, result) = method.call(scope, &args);

                if ret.is_ok() {
                    unsafe {
                        match signature.as_str() {
                            "Boolean" => { retval.set_bool(*(result as *mut bool)) }
                            "Guid" => {
                                let obj = guid_ptr_to_js_object(result, scope);
                                retval.set(obj.into());
                            }
                            _ if !signature.contains('.') => {
                                match NativeType::try_from(signature.as_str()) {
                                    Ok(return_type) => { set_ret_val(result, scope, retval, return_type); }
                                    Err(_) => { retval.set_undefined(); }
                                }
                            }
                            _ => {
                                let instance = IUnknown::from_raw(result);
                                let Some(declaration) = MetadataReader::find_by_name(signature.as_str()) else {
                                    let message = format!(
                                        "Unable to resolve return declaration for WinRT type '{}'", signature
                                    );
                                    let message = v8::String::new(scope, message.as_str()).unwrap();
                                    let error = v8::Exception::error(scope, message.into());
                                    scope.throw_exception(error);
                                    return;
                                };
                                let ret: Local<v8::Value> = create_ns_ctor_instance_object(
                                    signature.as_str(), dec.instance.clone(), dec.parent.clone(),
                                    declaration, Some(instance), scope,
                                ).into();
                                retval.set(ret.into());
                            }
                        }
                    }
                } else {
                    let message = ret.message().to_string();
                    let message = v8::String::new(scope, message.as_str()).unwrap();
                    let error = v8::Exception::error(scope, message.into());
                    scope.throw_exception(error);
                }
            })
            .data(ext.into())
            .build(scope);

            tmpl.set(name.unwrap().into(), func.into());
        }

        for property in clazz.properties().iter() {
            if !property.is_static() { continue; }

            let Some(prop_name) = v8::String::new(scope, property.name()) else { continue };

            let parent = Arc::clone(&declaration);
            let mut prop_dec = DeclarationFFI::new_with_instance(
                Arc::new(RwLock::new(property.clone())),
                None,
            );
            prop_dec.parent = Some(parent);
            let prop_ext = Box::into_raw(Box::new(prop_dec));
            let prop_ext = v8::External::new(scope, prop_ext as _);

            let getter = v8::FunctionTemplate::builder(|scope: &mut v8::PinScope<'_, '_>,
                                                        args: v8::FunctionCallbackArguments,
                                                        mut retval: v8::ReturnValue| {
                let dec = unsafe { args.data().cast::<v8::External>() };
                let dec = dec.value() as *mut DeclarationFFI;
                let dec = unsafe { &*dec };
                let lock = dec.read();
                let Some(property) = lock.as_any().downcast_ref::<PropertyDeclaration>() else { return };

                let signature = {
                    let sig = property.getter().return_type();
                    match property.getter().metadata() {
                        Some(md) => Signature::to_string(md, &sig),
                        None => return,
                    }
                };

                let factory = match resolve_class_factory_from_parent(dec) {
                    Ok(f) => f,
                    Err(e) => {
                        throw_js_error(scope, &format!("Failed to resolve static property factory: {}", e.message()));
                        return;
                    }
                };

                let Some(mut prop_call) = PropertyCall::new(property, false, factory, false) else { return };
                let (hresult, result) = prop_call.call_with_values(scope, &[]);

                if hresult.is_ok() {
                    unsafe {
                        match signature.as_str() {
                            "Boolean" => { retval.set_bool(*(result as *mut bool)); }
                            "Guid" => {
                                let obj = guid_ptr_to_js_object(result, scope);
                                retval.set(obj.into());
                            }
                            _ if !signature.contains('.') => {
                                match NativeType::try_from(signature.as_str()) {
                                    Ok(return_type) => { set_ret_val(result, scope, retval, return_type); }
                                    Err(_) => { retval.set_undefined(); }
                                }
                            }
                            _ => {
                                let instance = IUnknown::from_raw(result);
                                let Some(ret_decl) = MetadataReader::find_by_name(signature.as_str()) else { return };
                                let ret: Local<v8::Value> = create_ns_ctor_instance_object(
                                    signature.as_str(), dec.instance.clone(), dec.parent.clone(),
                                    ret_decl, Some(instance), scope,
                                ).into();
                                retval.set(ret.into());
                            }
                        }
                    }
                }
            })
            .data(prop_ext.into())
            .build(scope);

            tmpl.set_accessor_property(
                prop_name.into(), Some(getter), None, v8::PropertyAttribute::DONT_DELETE,
            );
        }
    }

    let func = tmpl.get_function(scope).unwrap();

    {
        let lock = declaration.read();
        if let Some(full_name) = match lock.kind() {
            DeclarationKind::Class => lock
                .as_any()
                .downcast_ref::<ClassDeclaration>()
                .map(|clazz| clazz.full_name().to_string()),
            _ => None,
        } {
            let key = v8::String::new(scope, "__typeName__").unwrap();
            let value = v8::String::new(scope, full_name.as_str()).unwrap();
            func.set(scope, key.into(), value.into());
        }
    }

    func.into()
}

// ── Struct constructor object ────────────────────────────────────────────────

pub(crate) fn create_ns_struct_ctor_object<'a>(
    name: &str,
    declaration: Arc<RwLock<dyn Declaration>>,
    scope: &mut v8::PinScope<'a, '_>,
) -> Local<'a, v8::Value> {
    let name = v8::String::new(scope, name).unwrap();

    let ext = DeclarationFFI::new(Arc::clone(&declaration));
    let ext = Box::into_raw(Box::new(ext));
    let ext = v8::External::new(scope, ext as _);

    let tmpl = FunctionTemplate::builder(|scope: &mut v8::PinScope<'_, '_>,
                                          args: v8::FunctionCallbackArguments,
                                          mut retval: v8::ReturnValue| {
        let dec = unsafe { args.data().cast::<v8::External>() };
        let dec = dec.value() as *mut DeclarationFFI;
        let dec = unsafe { &mut *dec };
        let lock = dec.write();
        let ext = args.data();

        let object_tmpl = v8::ObjectTemplate::new(scope);
        object_tmpl.set_internal_field_count(1);

        let mut field_args: Vec<NativeValue> = Vec::new();
        let mut field_types: Vec<NativeType> = Vec::new();

        let struct_dec = lock.as_any().downcast_ref::<StructDeclaration>().unwrap();
        let object = args.get(0).to_object(scope).unwrap();

        for field in struct_dec.fields() {
            let field_type = Signature::to_string(field.base().metadata().unwrap(), &field.type_());
            let native_type = NativeType::try_from(field_type.as_str()).unwrap();
            field_types.push(native_type.clone());

            let name = v8::String::new(scope, field.name()).unwrap();
            let field_value = object.get(scope, name.into());

            match field_value {
                None => {
                    let message = format!("Missing key {}", field.name());
                    let message = v8::String::new(scope, message.as_str()).unwrap();
                    let error = v8::Exception::error(scope, message.into());
                    scope.throw_exception(error);
                }
                Some(field) => {
                    let value = match native_type {
                        NativeType::Void => Err(error::type_error("Void is not a valid WinRT struct field type")),
                        NativeType::Bool    => ffi_parse_bool_arg(field),
                        NativeType::U8      => ffi_parse_u8_arg(field),
                        NativeType::I8      => ffi_parse_i8_arg(field),
                        NativeType::U16     => ffi_parse_u16_arg(field),
                        NativeType::I16     => ffi_parse_i16_arg(field),
                        NativeType::U32     => ffi_parse_u32_arg(field),
                        NativeType::I32     => ffi_parse_i32_arg(field),
                        NativeType::U64     => ffi_parse_u64_arg(scope, field),
                        NativeType::I64     => ffi_parse_i64_arg(scope, field),
                        NativeType::USize   => ffi_parse_usize_arg(scope, field),
                        NativeType::ISize   => ffi_parse_isize_arg(scope, field),
                        NativeType::F32     => ffi_parse_f32_arg(field),
                        NativeType::F64     => ffi_parse_f64_arg(field),
                        NativeType::Pointer => ffi_parse_pointer_arg(scope, field),
                        NativeType::Buffer  => ffi_parse_buffer_arg(scope, field),
                        NativeType::Function => ffi_parse_function_arg(scope, field),
                        NativeType::Struct(_) => ffi_parse_struct_arg(scope, field),
                        NativeType::String  => ffi_parse_string_arg(scope, field),
                    };
                    match value {
                        Ok(value) => { field_args.push(value); }
                        Err(err) => {
                            let message = err.to_string();
                            let message = v8::String::new(scope, message.as_str()).unwrap();
                            let error = v8::Exception::error(scope, message.into());
                            scope.throw_exception(error);
                        }
                    }
                }
            }
        }

        let mut struct_size = 0_usize;
        let params = field_types.clone().into_iter().map(|item| {
            struct_size = struct_size + item.size();
            libffi::middle::Type::try_from(item)
        }).collect::<Result<Vec<libffi::middle::Type>, error::AnyError>>();

        assert!(params.is_ok());

        let mut struct_buf: Vec<u8> = unsafe { vec![0_u8; struct_size] };
        struct_buf.shrink_to_fit();

        let mut position = 0_isize;
        for (field_type, field_value) in field_types.iter().zip(field_args.iter()) {
            let size = field_type.size();
            unsafe {
                let buffer = struct_buf.as_mut_ptr();
                let buffer = buffer.offset(position);
                let value: *mut u8 = std::mem::transmute(field_value.as_arg(field_type));
                let slice = std::slice::from_raw_parts_mut(buffer, size);
                std::ptr::copy(value, slice.as_mut_ptr(), size);
            }
            position = position + size as isize;
        }

        let name = lock.name().to_string();
        drop(lock);
        dec.struct_instance = Some((struct_buf, field_types));

        let _name = v8::String::new(scope, name.as_str()).unwrap();

        let getter = |scope: &mut v8::PinScope<'_, '_>,
                      key: Local<v8::Name>,
                      args: v8::PropertyCallbackArguments,
                      mut rv: v8::ReturnValue<v8::Value>| -> v8::Intercepted {
            let key = key.to_rust_string_lossy(scope);
            let this = args.data();
            let dec = unsafe { this.cast::<v8::External>() };
            let dec = dec.value() as *mut DeclarationFFI;
            let dec = unsafe { &*dec };
            let lock = dec.read();

            if key == "toString" {
                let name = lock.name();
                let name = v8::String::new(scope, name).unwrap();
                let func = v8::Function::builder(|_scope: &mut v8::PinScope<'_, '_>,
                                                  args: v8::FunctionCallbackArguments,
                                                  mut retval: v8::ReturnValue| {
                    retval.set(args.data());
                }).data(name.into()).build(scope);
                rv.set(func.unwrap().into());
                return v8::Intercepted::kYes;
            }

            let struct_dec = lock.as_any().downcast_ref::<StructDeclaration>().unwrap();
            let mut offset = 0;
            let instance = dec.struct_instance.as_ref();
            let mut position = 0;
            for field in struct_dec.fields() {
                if field.name() == key.as_str() {
                    if let Some((buffer, types)) = instance {
                        let mut current_field_position = 0;
                        for field_type in types.iter() {
                            let size = field_type.size();
                            if position == current_field_position {
                                unsafe {
                                    let buffer = buffer.as_ptr();
                                    let buffer = buffer.offset(offset);
                                    let slice = std::slice::from_raw_parts(buffer, size);
                                    match field_type {
                                        NativeType::Void => {}
                                        NativeType::Bool => {
                                            let ret: &u8 = std::mem::transmute(slice.as_ptr() as *const u8);
                                            rv.set_bool(*ret == 1);
                                        }
                                        NativeType::U8 => {
                                            let ret: &u8 = std::mem::transmute(slice.as_ptr() as *const u8);
                                            rv.set_uint32(*ret as u32);
                                        }
                                        NativeType::I8 => {
                                            let ret: &i8 = std::mem::transmute(slice.as_ptr() as *const i8);
                                            rv.set_int32(*ret as i32);
                                        }
                                        NativeType::U16 => {
                                            let ret: &u16 = std::mem::transmute(slice.as_ptr() as *const u16);
                                            rv.set_uint32(*ret as u32);
                                        }
                                        NativeType::I16 => {
                                            let ret: &i16 = std::mem::transmute(slice.as_ptr() as *const i16);
                                            rv.set_int32(*ret as i32);
                                        }
                                        NativeType::U32 => {
                                            let ret: &u32 = std::mem::transmute(slice.as_ptr() as *const u32);
                                            rv.set_uint32(*ret);
                                        }
                                        NativeType::I32 => {
                                            let ret: &i32 = std::mem::transmute(slice.as_ptr() as *const i32);
                                            rv.set_int32(*ret);
                                        }
                                        NativeType::U64 => {
                                            let ret: u64 = *std::mem::transmute::<*const u64, &u64>(slice.as_ptr() as *const u64);
                                            let local_value: v8::Local<v8::Value> =
                                                if ret > MAX_SAFE_INTEGER as u64 {
                                                    v8::BigInt::new_from_u64(scope, ret).into()
                                                } else {
                                                    v8::Number::new(scope, ret as f64).into()
                                                };
                                            rv.set(local_value);
                                        }
                                        NativeType::I64 => {
                                            let ret: i64 = *std::mem::transmute::<*const i64, &i64>(slice.as_ptr() as *const i64);
                                            let local_value: v8::Local<v8::Value> =
                                                if ret > MAX_SAFE_INTEGER as i64 || ret < MIN_SAFE_INTEGER as i64 {
                                                    v8::BigInt::new_from_i64(scope, ret).into()
                                                } else {
                                                    v8::Number::new(scope, ret as f64).into()
                                                };
                                            rv.set(local_value);
                                        }
                                        NativeType::USize => {}
                                        NativeType::ISize => {}
                                        NativeType::F32 => {
                                            let ret: f32 = if cfg!(target_endian = "big") {
                                                f32::from_be_bytes(<[u8; 4]>::try_from(slice).unwrap())
                                            } else {
                                                f32::from_le_bytes(<[u8; 4]>::try_from(slice).unwrap())
                                            };
                                            rv.set(v8::Number::new(scope, ret as f64).into());
                                        }
                                        NativeType::F64 => {
                                            let ret: &f64 = std::mem::transmute(slice.as_ptr() as *const f64);
                                            rv.set(v8::Number::new(scope, *ret).into());
                                        }
                                        NativeType::Pointer => {}
                                        NativeType::Buffer => {}
                                        NativeType::Function => {}
                                        NativeType::Struct(_) => {}
                                        NativeType::String => {}
                                    }
                                }
                            }
                            current_field_position = current_field_position + 1;
                            offset = offset + size as isize;
                        }
                    }
                    break;
                }
                position = position + 1;
            }
            v8::Intercepted::kYes
        };

        let setter = |scope: &mut v8::PinScope<'_, '_>,
                      key: Local<v8::Name>,
                      value: Local<v8::Value>,
                      args: v8::PropertyCallbackArguments,
                      _rv: v8::ReturnValue<()>| -> v8::Intercepted {
            let key = key.to_rust_string_lossy(scope);
            let this = args.data();
            let dec = unsafe { this.cast::<v8::External>() };
            let dec = dec.value() as *mut DeclarationFFI;
            let instance = unsafe { (&mut *dec).struct_instance.as_mut() };
            let dec = unsafe { &mut *dec };
            let lock = dec.write();
            let struct_dec = lock.as_any().downcast_ref::<StructDeclaration>().unwrap();
            let mut offset = 0;
            let mut position = 0;
            for field in struct_dec.fields() {
                if field.name() == key.as_str() {
                    if let Some((buffer, types)) = instance {
                        let field = value;
                        let mut current_field_position = 0;
                        for field_type in types.iter() {
                            let size = field_type.size();
                            if position == current_field_position {
                                let value = match field_type {
                                    NativeType::Void => Err(error::type_error("Void is not a valid WinRT struct field type")),
                                    NativeType::Bool    => ffi_parse_bool_arg(field),
                                    NativeType::U8      => ffi_parse_u8_arg(field),
                                    NativeType::I8      => ffi_parse_i8_arg(field),
                                    NativeType::U16     => ffi_parse_u16_arg(field),
                                    NativeType::I16     => ffi_parse_i16_arg(field),
                                    NativeType::U32     => ffi_parse_u32_arg(field),
                                    NativeType::I32     => ffi_parse_i32_arg(field),
                                    NativeType::U64     => ffi_parse_u64_arg(scope, field),
                                    NativeType::I64     => ffi_parse_i64_arg(scope, field),
                                    NativeType::USize   => ffi_parse_usize_arg(scope, field),
                                    NativeType::ISize   => ffi_parse_isize_arg(scope, field),
                                    NativeType::F32     => ffi_parse_f32_arg(field),
                                    NativeType::F64     => ffi_parse_f64_arg(field),
                                    NativeType::Pointer => ffi_parse_pointer_arg(scope, field),
                                    NativeType::Buffer  => ffi_parse_buffer_arg(scope, field),
                                    NativeType::Function => ffi_parse_function_arg(scope, field),
                                    NativeType::Struct(_) => ffi_parse_struct_arg(scope, field),
                                    NativeType::String  => ffi_parse_string_arg(scope, field),
                                };
                                match value {
                                    Ok(value) => {
                                        unsafe {
                                            let buffer = buffer.as_mut_ptr();
                                            let buffer = buffer.offset(offset);
                                            let value: *mut u8 = std::mem::transmute(value.as_arg(field_type));
                                            let slice = std::slice::from_raw_parts_mut(buffer, size);
                                            std::ptr::copy(value, slice.as_mut_ptr(), size);
                                        }
                                    }
                                    Err(err) => {
                                        let message = err.to_string();
                                        let message = v8::String::new(scope, message.as_str()).unwrap();
                                        let error = v8::Exception::error(scope, message.into());
                                        scope.throw_exception(error);
                                    }
                                }
                            }
                            current_field_position = current_field_position + 1;
                            offset = offset + size as isize;
                        }
                    }
                    break;
                }
                position = position + 1;
            }
            v8::Intercepted::kYes
        };

        object_tmpl.set_named_property_handler(
            v8::NamedPropertyHandlerConfiguration::new()
                .getter(getter)
                .setter(setter)
                .data(ext)
        );

        let object = object_tmpl.new_instance(scope).unwrap();
        object.set_internal_field(0, unsafe { ext.cast::<v8::Data>() });
        retval.set(object.into());
    })
    .data(ext.into())
    .build(scope);
    tmpl.set_class_name(name);

    let func = tmpl.get_function(scope).unwrap();
    func.into()
}

// ── Global namespace initialisation ─────────────────────────────────────────

pub(crate) fn init_meta(
    scope: &mut v8::ContextScope<v8::HandleScope<v8::Context>>,
    context: Local<v8::Context>,
) {
    use metadata::declarations::namespace_declaration::NamespaceDeclaration;
    let global = context.global(scope);
    let global_metadata = MetadataReader::find_by_name("").unwrap();
    let data = global_metadata.read();
    let ns = data.as_any().downcast_ref::<NamespaceDeclaration>();
    if let Some(global_namespaces) = ns {
        let full_name = global_namespaces.full_name();
        for ns in global_namespaces.children() {
            let full_name = if full_name.is_empty() {
                ns.to_string()
            } else {
                format!("{}.{}", full_name, ns)
            };

            let name: Local<v8::Name> = v8::String::new(scope, ns.as_str()).unwrap().into();
            if let Some(name_space) = MetadataReader::find_by_name(full_name.as_str()) {
                let object = create_ns_object(ns, name_space, scope);
                global.define_own_property(
                    scope, name, object,
                    v8::PropertyAttribute::READ_ONLY | v8::PropertyAttribute::DONT_DELETE | v8::PropertyAttribute::NONE,
                );
            }
        }
    }
}
