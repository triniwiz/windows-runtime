use std::ffi::c_int;
use std::sync::OnceLock;
use windows::core::PCWSTR;
use crate::DeclarationFFI;
use crate::class_helpers::{collect_class_methods, collect_class_properties};
use metadata::meta_data_reader::MetadataReader;
use metadata::declarations::declaration::{DeclarationKind, Declaration};
use metadata::declarations::class_declaration::ClassDeclaration;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Console::{self, CONSOLE_MODE, GetConsoleMode, GetStdHandle, STD_OUTPUT_HANDLE};
use windows::Win32::System::EventLog::{RegisterEventSourceW, ReportEventW, EVENTLOG_ERROR_TYPE, EVENTLOG_WARNING_TYPE, EVENTLOG_INFORMATION_TYPE, REPORT_EVENT_TYPE};

pub fn init_console(scope: &mut v8::ContextScope<v8::HandleScope<v8::Context>>, context: v8::Local<v8::Context>) {
    let console = v8::Object::new(scope);

    macro_rules! bind {
        ($name:expr, $cb:expr) => {{
            let f = v8::Function::new(scope, $cb).unwrap();
            let key: v8::Local<v8::Value> = v8::String::new(scope, $name).unwrap().into();
            console.set(scope, key, f.into());
        }};
    }

    bind!("log",    handle_console_log);
    bind!("info",   handle_console_log);   // alias — same output, different semantics
    bind!("dir",    handle_console_dir);
    bind!("warn",   handle_console_warn);
    bind!("error",  handle_console_error);
    bind!("trace",  handle_console_trace);
    bind!("assert", handle_console_assert);

    let global = context.global(scope);
    let key = v8::String::new(scope, "console").unwrap();
    global.define_own_property(scope, key.into(), console.into(), v8::PropertyAttribute::READ_ONLY);
}

// ── Core string-builder ──────────────────────────────────────────────────────

/// Converts one JS value to its string representation and appends to `output`.

fn handle_item_log(
    scope: &mut v8::PinScope<'_, '_>,
    item: v8::Local<v8::Value>,
    output: &mut String,
    is_last: bool,
    rich: bool,
) {
    // ── Typed arrays first ─────────────────────────────────────────────────
    // ArrayBufferView (Uint8Array, Float32Array, DataView, …) are objects in
    // V8 but their slot 0 is a V8-owned backing store pointer, NOT a user
    // External.  Calling cast::<External>() on it panics with BadType.
    // toString() on a typed array yields the comma-separated values.
    if item.is_array_buffer_view() {
        output.push_str(&item.to_rust_string_lossy(scope));
        if !is_last { output.push(' '); }
        return;
    }

    if item.is_array() {
        if let Ok(arr) = v8::Local::<v8::Array>::try_from(item) {
            let len = arr.length() as usize;
            output.push('[');
            for i in 0..len {
                if i > 0 { output.push_str(", "); }
                if let Some(child) = arr.get_index(scope, i as u32) {
                    handle_item_log(scope, child, output, true, false);
                }
            }
            output.push(']');
            if !is_last { output.push(' '); }
        }
        return;
    }

    // ── Functions ──────────────────────────────────────────────────────────
    if item.is_function() {
        output.push_str(&item.to_rust_string_lossy(scope));
        if !is_last { output.push(' '); }
        return;
    }

    // ── Objects ────────────────────────────────────────────────────────────
    if item.is_object() {
        let obj = match v8::Local::<v8::Object>::try_from(item) {
            Ok(o) => o,
            Err(_) => {
                output.push_str(&item.to_rust_string_lossy(scope));
                if !is_last { output.push(' '); }
                return;
            }
        };

        // 1) __typeName__ set by Class.extend helpers → prefer metadata name
        if let Some(type_key) = v8::String::new(scope, "__typeName__") {
            if let Some(type_val) = obj.get(scope, type_key.into()) {
                if type_val.is_string() {
                    let full_name = type_val.to_rust_string_lossy(scope);
                    if let Some(declaration) = MetadataReader::find_by_name(full_name.as_ref()) {
                        let lock = declaration.read();
                        if let Some(class_dec) = lock.as_any().downcast_ref::<ClassDeclaration>() {
                            if !rich {
                                output.push_str(class_dec.name());
                                if !is_last { output.push(' '); }
                                return;
                            }
                            output.push_str(&format!("{} (constructor) {{\n", class_dec.name()));
                            let props = collect_class_properties(class_dec);
                            for p in props.iter().filter(|p| p.is_static()) {
                                output.push_str(&format!("  {}: <static>\n", p.name()));
                            }
                            let methods = collect_class_methods(class_dec);
                            let proto_methods: Vec<_> = methods.iter().filter(|m| !m.is_static()).collect();
                            if !proto_methods.is_empty() {
                                output.push_str("  prototype methods: [");
                                for (i, m) in proto_methods.iter().enumerate() {
                                    if i > 0 { output.push_str(", "); }
                                    let mut mn = m.overload_name().to_string();
                                    if mn.is_empty() { mn = m.name().to_string(); }
                                    output.push_str(&mn);
                                }
                                output.push_str("]\n");
                            }
                            output.push_str("}\n");
                            if !is_last { output.push(' '); }
                            return;
                        }
                    }
                }
            }
        }

        // 2) WinRT native proxy: check internal field 0 for a DeclarationFFI
        //    External.  The cast is guarded via TryFrom so non-External slots
        //    (V8 built-in objects, SMIs, …) silently fall through rather than
        //    panicking with BadType.
        if let Some(type_name) = winrt_type_name_from_object(scope, obj) {
            if !rich {
                output.push_str(&type_name);
                if !is_last { output.push(' '); }
                return;
            }
            output.push_str(&format!("{} {{\n", type_name));
            output.push_str("  properties: <native>\n");
            output.push_str("  methods: <native>\n");
            output.push_str("}\n");
            if !is_last { output.push(' '); }
            return;
        }

        // 3) Rich inspection for console.dir
        if rich {
            if let Some(prop_names) = obj.get_own_property_names(scope, v8::GetPropertyNamesArgs::default()) {
                output.push_str("{\n");
                let length = prop_names.length() as usize;
                for i in 0..length {
                    if let Some(name_val) = prop_names.get_index(scope, i as u32) {
                        if let Ok(name_str) = v8::Local::<v8::String>::try_from(name_val) {
                            let key = name_str.to_rust_string_lossy(scope);
                            v8::tc_scope!(tc, scope);
                            let prop_val = obj.get(tc, name_str.into());
                            if tc.has_caught() {
                                output.push_str(&format!("  {}: <getter threw>\n", key));
                                continue;
                            }
                            if let Some(v) = prop_val {
                                if v.is_function() {
                                    output.push_str(&format!("  {}: ()\n", key));
                                } else if v.is_array() {
                                    if let Ok(arr) = v8::Local::<v8::Array>::try_from(v) {
                                        if let Some(json) = v8::json::stringify(tc, arr.into()) {
                                            output.push_str(&format!("  {}: {}\n", key, json.to_rust_string_lossy(tc)));
                                        } else {
                                            output.push_str(&format!("  {}: [Array]\n", key));
                                        }
                                    } else {
                                        output.push_str(&format!("  {}: [Array]\n", key));
                                    }
                                } else if v.is_object() {
                                    if let Ok(obj_val) = v8::Local::<v8::Object>::try_from(v) {
                                        let s = transform_js_object(tc, obj_val);
                                        output.push_str(&format!("  {}: {}\n", key, s));
                                    } else {
                                        output.push_str(&format!("  {}: <object>\n", key));
                                    }
                                } else if let Some(sv) = v.to_string(tc) {
                                    output.push_str(&format!("  {}: {}\n", key, sv.to_rust_string_lossy(tc)));
                                } else {
                                    output.push_str(&format!("  {}: <unavailable>\n", key));
                                }
                            } else {
                                output.push_str(&format!("  {}: <unavailable>\n", key));
                            }
                        }
                    }
                }
                output.push_str("}\n");
                if !is_last { output.push(' '); }
                return;
            }
        }


        v8::tc_scope!(tc, scope);
        if let Some(s) = item.to_string(tc) {
            if !tc.has_caught() {
                let s = s.to_rust_string_lossy(tc);
                if !s.contains("[object Object]") {
                    output.push_str(&s);
                    if !is_last { output.push(' '); }
                    return;
                }
            }
        }
        if let Some(json) = v8::json::stringify(tc, item) {
            let s = json.to_rust_string_lossy(tc);
            output.push_str(if s.contains("circular structure") { "#CR" } else { &s });
        } else {
            output.push_str("[object Object]");
        }
        if !is_last { output.push(' '); }
        return;
    }

    // ── Primitives (string, number, boolean, null, undefined, symbol, BigInt)
    output.push_str(&item.to_rust_string_lossy(scope));
    if !is_last { output.push(' '); }
}

/// Extract the WinRT type name from a native proxy object by inspecting the
/// DeclarationFFI External in internal field 0.  Checks the instance, then its
/// constructor, then constructor.prototype.  Returns None for plain JS objects.
///
/// All casts use TryFrom rather than unchecked cast so V8 built-in objects
/// whose slot 0 holds a non-External value (SMI, backing-store pointer, …)
/// never cause a BadType panic.
fn winrt_type_name_from_object(
    scope: &mut v8::PinScope<'_, '_>,
    obj: v8::Local<v8::Object>,
) -> Option<String> {
    if let Some(name) = winrt_name_from_slot(scope, obj) {
        return Some(name);
    }
    let ctor_key = v8::String::new(scope, "constructor")?;
    let ctor_val = obj.get(scope, ctor_key.into())?;
    if !ctor_val.is_object() { return None; }
    let ctor_obj = v8::Local::<v8::Object>::try_from(ctor_val).ok()?;
    if let Some(name) = winrt_name_from_slot(scope, ctor_obj) {
        return Some(name);
    }
    let proto_key = v8::String::new(scope, "prototype")?;
    let proto_val = ctor_obj.get(scope, proto_key.into())?;
    if !proto_val.is_object() { return None; }
    let proto_obj = v8::Local::<v8::Object>::try_from(proto_val).ok()?;
    winrt_name_from_slot(scope, proto_obj)
}

fn winrt_name_from_slot(
    scope: &mut v8::PinScope<'_, '_>,
    obj: v8::Local<v8::Object>,
) -> Option<String> {
    let field = obj.get_internal_field(scope, 0)?;
    // TryFrom returns Err for non-External slots (e.g. V8 SMIs) — safe fallthrough.
    let ext = v8::Local::<v8::External>::try_from(field).ok()?;
    let dec_ptr = ext.value() as *mut DeclarationFFI;
    if dec_ptr.is_null() { return None; }
    let dec = unsafe { &*dec_ptr };
    let lock = dec.read();
    if !matches!(
        lock.kind(),
        DeclarationKind::Class
            | DeclarationKind::Interface
            | DeclarationKind::GenericInterface
            | DeclarationKind::GenericInterfaceInstance
    ) {
        return None;
    }
    Some(lock.name().to_string())
}

// ── Output sink ─────────────────────────────────────────────────────────────

/// Detect once whether stdout is attached to a real console.
fn console_handle() -> Option<HANDLE> {
    static PROBED: OnceLock<Option<isize>> = OnceLock::new();
    let raw = *PROBED.get_or_init(|| unsafe {
        let h = GetStdHandle(STD_OUTPUT_HANDLE).ok()?;
        if h.is_invalid() { return None; }
        let mut mode = CONSOLE_MODE::default();
        if GetConsoleMode(h, &mut mode).is_ok() { Some(h.0 as isize) } else { None }
    });
    raw.map(|p| HANDLE(p as *mut _))
}

fn write_console(value: &str) {
    if let Some(handle) = console_handle() {
        let wide: Vec<u16> = value.encode_utf16().collect();
        let _ = unsafe { Console::WriteConsoleW(handle, &wide, None, None) };
    }
    crate::debug_output(value);
    let event_type: REPORT_EVENT_TYPE = if value.starts_with("[ERROR]") {
        EVENTLOG_ERROR_TYPE
    } else if value.starts_with("[WARN]") {
        EVENTLOG_WARNING_TYPE
    } else {
        EVENTLOG_INFORMATION_TYPE
    };
    report_event(value, event_type);
}

pub(crate) fn report_event(message: &str, event_type: REPORT_EVENT_TYPE) {
    static EVENT_SOURCE: OnceLock<isize> = OnceLock::new();
    let h_raw = *EVENT_SOURCE.get_or_init(|| unsafe {
        let source_w: Vec<u16> = "NativeScript\0".encode_utf16().collect();
        RegisterEventSourceW(PCWSTR::null(), PCWSTR::from_raw(source_w.as_ptr()))
            .map(|h| h.0 as isize)
            .unwrap_or(0)
    });
    if h_raw == 0 { return; }
    let h = HANDLE(h_raw as *mut _);
    let msg_w: Vec<u16> = message.encode_utf16().chain(std::iter::once(0)).collect();
    let strings = [PCWSTR::from_raw(msg_w.as_ptr())];
    unsafe { let _ = ReportEventW(h, event_type, 0, 0, None, 0, Some(&strings), None); }
}

// ── Console handlers ─────────────────────────────────────────────────────────

pub(crate) fn handle_console_log(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    _retval: v8::ReturnValue,
) {
    let mut value = String::from("[INFO] ");
    let length = args.length() as usize;
    for i in 0..length {
        handle_item_log(scope, args.get(i as c_int), &mut value, i == length.saturating_sub(1), false);
    }
    value.push('\n');
    write_console(&value);
}

pub(crate) fn handle_console_warn(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    _retval: v8::ReturnValue,
) {
    let mut value = String::from("[WARN] ");
    let length = args.length() as usize;
    for i in 0..length {
        handle_item_log(scope, args.get(i as c_int), &mut value, i == length.saturating_sub(1), false);
    }
    value.push('\n');
    write_console(&value);
}

pub(crate) fn handle_console_error(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    _retval: v8::ReturnValue,
) {
    let mut value = String::from("[ERROR] ");
    let length = args.length() as usize;
    let mut printed_stack = false;

    for i in 0..length {
        let is_last = i == length.saturating_sub(1);
        let arg = args.get(i as c_int);

        let used_stack = 'stack: {
            if !arg.is_object() { break 'stack false; }
            let Ok(obj) = v8::Local::<v8::Object>::try_from(arg) else { break 'stack false; };
            let Some(key) = v8::String::new(scope, "stack") else { break 'stack false; };
            v8::tc_scope!(tc, scope);
            let Some(stack_val) = obj.get(tc, key.into()) else { break 'stack false; };
            if tc.has_caught() || !stack_val.is_string() { break 'stack false; }
            value.push_str(&stack_val.to_rust_string_lossy(tc));
            if !is_last { value.push(' '); }
            true
        };

        if used_stack {
            printed_stack = true;
        } else {
            handle_item_log(scope, arg, &mut value, is_last, false);
        }
    }

    if !printed_stack {
        if let Some(stack) = v8::StackTrace::current_stack_trace(scope, 3) {
            if stack.get_frame_count() > 0 {
                if let Some(frame) = stack.get_frame(scope, 0) {
                    let script = frame.get_script_name(scope)
                        .map(|s| s.to_rust_string_lossy(scope))
                        .unwrap_or_default();
                    let line = frame.get_line_number();
                    if !script.is_empty() {
                        value.push_str(&format!("\n    at {}:{}", script, line));
                    }
                }
            }
        }
    }

    value.push('\n');
    write_console(&value);
}

pub(crate) fn handle_console_dir(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    _retval: v8::ReturnValue,
) {
    let mut value = String::new();
    let length = args.length() as usize;
    for i in 0..length {
        handle_item_log(scope, args.get(i as c_int), &mut value, i == length.saturating_sub(1), true);
    }
    value.push('\n');
    write_console(&value);
}

pub(crate) fn handle_console_trace(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    _retval: v8::ReturnValue,
) {
    let mut value = String::from("[TRACE] ");
    let length = args.length() as usize;
    if length == 0 {
        value.push_str("Trace");
    } else {
        for i in 0..length {
            handle_item_log(scope, args.get(i as c_int), &mut value, i == length.saturating_sub(1), false);
        }
    }

    if let Some(stack) = v8::StackTrace::current_stack_trace(scope, 10) {
        value.push('\n');
        let frame_count = stack.get_frame_count();
        for i in 0..frame_count {
            if let Some(frame) = stack.get_frame(scope, i) {
                let func = frame.get_function_name(scope)
                    .map(|s| s.to_rust_string_lossy(scope))
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "<anonymous>".to_string());
                let script = frame.get_script_name(scope)
                    .map(|s| s.to_rust_string_lossy(scope))
                    .unwrap_or_else(|| "VM".to_string());
                let line = frame.get_line_number();
                let col = frame.get_column();
                value.push_str(&format!("    at {} ({}:{}:{})\n", func, script, line, col));
            }
        }
    } else {
        value.push('\n');
    }
    write_console(&value);
}

pub(crate) fn handle_console_assert(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    _retval: v8::ReturnValue,
) {
    let passes = args.length() > 0 && args.get(0).boolean_value(scope);
    if passes { return; }

    let mut value = String::from("[ERROR] Assertion failed");
    if args.length() > 1 {
        value.push_str(": ");
        let length = args.length() as usize;
        for i in 1..length {
            handle_item_log(scope, args.get(i as c_int), &mut value, i == length.saturating_sub(1), false);
        }
    } else {
        value.push_str(": console.assert");
    }
    value.push('\n');
    write_console(&value);
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn transform_js_object(scope: &mut v8::PinScope<'_, '_>, object: v8::Local<v8::Object>) -> String {
    v8::tc_scope!(tc, scope);
    if let Some(val) = object.to_string(tc) {
        let s = val.to_rust_string_lossy(tc);
        if !s.contains("[object Object]") {
            return s;
        }
    }
    if tc.has_caught() { return String::new(); }
    if let Some(json) = v8::json::stringify(tc, object.into()) {
        let s = json.to_rust_string_lossy(tc);
        if s.contains("circular structure") { return "#CR".to_string(); }
        return s;
    }
    String::new()
}
