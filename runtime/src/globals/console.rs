use std::ffi::c_int;
use std::sync::OnceLock;
use windows::core::PCWSTR;
use crate::DeclarationFFI;
use crate::class_helpers::{collect_class_methods, collect_class_properties};
use metadata::meta_data_reader::MetadataReader;
use metadata::declarations::declaration::{DeclarationKind, Declaration};
use metadata::declarations::class_declaration::ClassDeclaration;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::{Console};
use windows::Win32::System::Console::{CONSOLE_MODE, GetConsoleMode, GetStdHandle, STD_OUTPUT_HANDLE};
use windows::Win32::System::Diagnostics::Debug::OutputDebugStringW;
use windows::Win32::System::EventLog::{RegisterEventSourceW, ReportEventW, DeregisterEventSource, EVENTLOG_ERROR_TYPE, EVENTLOG_WARNING_TYPE, EVENTLOG_INFORMATION_TYPE, REPORT_EVENT_TYPE};

pub fn init_console(scope: &mut v8::ContextScope<v8::HandleScope<v8::Context>>, context: v8::Local<v8::Context>) {
    let console = v8::Object::new(scope);
    let log = v8::Function::new(scope, handle_console_log).unwrap();
    let dir = v8::Function::new(scope, handle_console_dir).unwrap();
    let warn = v8::Function::new(scope, handle_console_warn).unwrap();
    let error = v8::Function::new(scope, handle_console_error).unwrap();

    let name = v8::String::new(scope, "log").unwrap().into();
    console.set(
        scope,
        name,
        log.into(),
    );

    let name = v8::String::new(scope, "dir").unwrap().into();
    console.set(
        scope,
        name,
        dir.into(),
    );

    let name = v8::String::new(scope, "warn").unwrap().into();
    console.set(
        scope,
        name,
        warn.into(),
    );

    let name = v8::String::new(scope, "error").unwrap().into();
    console.set(
        scope,
        name,
        error.into(),
    );

    let global = context.global(scope);
    let value = v8::String::new(
        scope, "console",
    ).unwrap().into();
    global.define_own_property(scope, value, console.into(), v8::PropertyAttribute::READ_ONLY);

    // Emit a small runtime trace so we can confirm console was installed
    // even in packaged AppX runs where terminal output may not be present.
    crate::debug_output("[NativeScript] init_console: console installed\n");
}

fn handle_item_log(scope: &mut v8::PinScope<'_, '_>, item: v8::Local<v8::Value>, output: &mut String, is_last: bool, rich: bool) {
    // Objects: try to detect native proxy instances (internal field)
    // or constructor/type metadata; otherwise perform a guarded
    // property enumeration for `console.dir`.
    if item.is_object() {
        let obj = match v8::Local::<v8::Object>::try_from(item) {
            Ok(o) => o,
            Err(_) => {
                output.push_str(&item.to_rust_string_lossy(scope));
                if !is_last { output.push(' '); }
                return;
            }
        };

        // 1) If the object exposes a `__typeName__` string (set by our
        // Class.extend helpers), prefer printing class-level metadata.
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
                                    let mut method_name = m.overload_name().to_string();
                                    if method_name.is_empty() { method_name = m.name().to_string(); }
                                    output.push_str(&method_name);
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

        // 2) Check for a DeclarationFFI stored directly on the object.
        if let Some(dec_field) = obj.get_internal_field(scope, 0) {
            let dec_ext = unsafe { dec_field.cast::<v8::External>() };
            let dec_ptr = dec_ext.value() as *mut DeclarationFFI;
            if !dec_ptr.is_null() {
                let dec = unsafe { &*dec_ptr };
                let lock = dec.read();
                let kind = lock.kind();
                if matches!(kind, DeclarationKind::Class | DeclarationKind::Interface | DeclarationKind::GenericInterface | DeclarationKind::GenericInterfaceInstance) {
                    if let Some(class_dec) = lock.as_any().downcast_ref::<ClassDeclaration>() {
                        let type_name = class_dec.name();
                        if !rich {
                            output.push_str(type_name);
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
                }
            }
        }

        // 3) If not present on the instance, try the `constructor` object or
        // its `prototype` (some patterns attach the declaration there).
        if let Some(ctor_key) = v8::String::new(scope, "constructor") {
            if let Some(ctor_val) = obj.get(scope, ctor_key.into()) {
                if ctor_val.is_object() {
                    if let Ok(ctor_obj) = v8::Local::<v8::Object>::try_from(ctor_val) {
                        if let Some(dec_field) = ctor_obj.get_internal_field(scope, 0) {
                            let dec_ext = unsafe { dec_field.cast::<v8::External>() };
                            let dec_ptr = dec_ext.value() as *mut DeclarationFFI;
                            if !dec_ptr.is_null() {
                                let dec = unsafe { &*dec_ptr };
                                let lock = dec.read();
                                if let Some(class_dec) = lock.as_any().downcast_ref::<ClassDeclaration>() {
                                    if !rich {
                                        output.push_str(class_dec.name());
                                        if !is_last { output.push(' '); }
                                        return;
                                    }
                                    output.push_str(&format!("{} {{\n", class_dec.name()));
                                    output.push_str("  properties: <native>\n");
                                    output.push_str("  methods: <native>\n");
                                    output.push_str("}\n");
                                    if !is_last { output.push(' '); }
                                    return;
                                }
                            }
                        }

                        // Check constructor.prototype for internal fields.
                        if let Some(proto_val) = ctor_obj.get(scope, v8::String::new(scope, "prototype").unwrap().into()) {
                            if proto_val.is_object() {
                                if let Ok(proto_obj) = v8::Local::<v8::Object>::try_from(proto_val) {
                                    if let Some(dec_field) = proto_obj.get_internal_field(scope, 0) {
                                        let dec_ext = unsafe { dec_field.cast::<v8::External>() };
                                        let dec_ptr = dec_ext.value() as *mut DeclarationFFI;
                                        if !dec_ptr.is_null() {
                                            let dec = unsafe { &*dec_ptr };
                                            let lock = dec.read();
                                            if let Some(class_dec) = lock.as_any().downcast_ref::<ClassDeclaration>() {
                                                if !rich {
                                                    output.push_str(class_dec.name());
                                                    if !is_last { output.push(' '); }
                                                    return;
                                                }
                                                output.push_str(&format!("{} {{\n", class_dec.name()));
                                                output.push_str("  properties: <native>\n");
                                                output.push_str("  methods: <native>\n");
                                                output.push_str("}\n");
                                                if !is_last { output.push(' '); }
                                                return;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // 4) Rich inspection for general JS objects: enumerate own properties
        // and attempt guarded reads. Skip problematic getters.
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
                                } else {
                                    if let Some(sv) = v.to_string(tc) {
                                        output.push_str(&format!("  {}: {}\n", key, sv.to_rust_string_lossy(tc)));
                                    } else {
                                        output.push_str(&format!("  {}: <unavailable>\n", key));
                                    }
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

            // Fallback: try toString() under TryCatch, then JSON.stringify
            v8::tc_scope!(tc, scope);
            if let Some(s) = item.to_string(tc) {
                if !tc.has_caught() {
                    output.push_str(&s.to_rust_string_lossy(tc));
                    if !is_last { output.push(' '); }
                    return;
                }
            }

            if let Some(json) = v8::json::stringify(tc, item) {
                output.push_str(&json.to_rust_string_lossy(tc));
                if !is_last { output.push(' '); }
                return;
            }
        }
    }

    // Arrays and primitives
    if item.is_array() {
        let item = v8::Local::<v8::Array>::try_from(item).unwrap();
        let length = item.length() as usize;
        for i in 0..length {
            let inner_is_last = is_last && i == length.saturating_sub(1);
            if let Some(child) = item.get_index(scope, i as u32) {
                handle_item_log(scope, child, output, inner_is_last, rich);
            }
        }
    } else {
        output.push_str(&item.to_rust_string_lossy(scope));
        if !is_last {
            output.push(' ');
        }
    }
}

/// Detect once whether stdout is attached to a real console. Packaged AppX
/// apps (and most GUI apps) have no console — `WriteConsoleW` fails silently
/// every call, and `print!` writes into a buffer nobody reads. Probing once
/// at startup lets the hot path go straight to `OutputDebugStringW`, which
/// is what the user actually sees in the VS Output window.
fn console_handle() -> Option<HANDLE> {
    static PROBED: OnceLock<Option<isize>> = OnceLock::new();
    let raw = *PROBED.get_or_init(|| unsafe {
        let h = GetStdHandle(STD_OUTPUT_HANDLE).ok()?;
        if h.is_invalid() {
            return None;
        }
        // GetConsoleMode succeeds only if `h` points at a real console
        // screen buffer — perfect detector for "is there actually a console
        // to write to". Returns false for the null handle UWP apps get.
        let mut mode = CONSOLE_MODE::default();
        if GetConsoleMode(h, &mut mode).is_ok() {
            Some(h.0 as isize)
        } else {
            None
        }
    });
    raw.map(|p| HANDLE(p as *mut _))
}

fn write_console(value: &str) {
    // If a real console is attached, write there so terminal users see logs.
    if let Some(handle) = console_handle() {
        let wide: Vec<u16> = value.encode_utf16().collect();
        // WriteConsoleW accepts UTF-16; pass the slice directly.
        let _ = unsafe { Console::WriteConsoleW(handle, &wide, None, None) };
    }

    // Always forward to the runtime debug output/logging path so developers
    // can capture JS console calls via DebugView/Visual Studio or the
    // runtime's log file. Use OutputDebugStringW for Unicode safety.
    let wide_null: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe { OutputDebugStringW(PCWSTR::from_raw(wide_null.as_ptr())) }

    // Also write into the central runtime log (file) via debug_output.
    crate::debug_output(value);

    // Also report important messages to the Windows Event Log so system
    // tooling (Event Viewer, ETW viewers) can surface errors/warnings.
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
    use windows::core::PCWSTR;
    use std::ptr;

    let source = "NativeScript";
    let mut source_w: Vec<u16> = source.encode_utf16().chain(std::iter::once(0)).collect();
    let mut msg_w: Vec<u16> = message.encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        // Register an event source for the local machine. Passing a null
        // server name registers against the local host.
        let h_res = RegisterEventSourceW(PCWSTR::null(), PCWSTR::from_raw(source_w.as_ptr()));
        let h = match h_res {
            Ok(hh) => hh,
            Err(_) => return,
        };
        if h.is_invalid() {
            return;
        }
        let strings = [PCWSTR::from_raw(msg_w.as_ptr())];
        // Call the safe Rust wrapper: parameters are
        // (hEventLog, wType, wCategory, dwEventID, lpUserSid, dwDataSize, lpStrings, lpRawData)
        // where `lpStrings` is `Option<&[PCWSTR]>` and the wrapper computes wNumStrings.
        let _ = ReportEventW(h, event_type, 0, 0, None, 0, Some(&strings), None);
        let _ = DeregisterEventSource(h);
    }
}

pub(crate) fn handle_console_warn(scope: &mut v8::PinScope<'_, '_>, args: v8::FunctionCallbackArguments, _retval: v8::ReturnValue) {
    let mut value = String::from("[WARN] ");
    let length = args.length() as usize;
    for i in 0..length {
        let is_last = i == length.saturating_sub(1);
        handle_item_log(scope, args.get(i as c_int), &mut value, is_last, false);
    }
    value.push('\n');
    write_console(&value);
}

pub(crate) fn handle_console_error(scope: &mut v8::PinScope<'_, '_>, args: v8::FunctionCallbackArguments, _retval: v8::ReturnValue) {
    let mut value = String::from("[ERROR] ");
    let length = args.length() as usize;
    for i in 0..length {
        let is_last = i == length.saturating_sub(1);
        handle_item_log(scope, args.get(i as c_int), &mut value, is_last, false);
    }
    value.push('\n');
    write_console(&value);
}


pub(crate) fn handle_console_log(scope: &mut v8::PinScope<'_, '_>, args: v8::FunctionCallbackArguments, _retval: v8::ReturnValue) {
    let mut value = String::from("[INFO] ");
    let length = args.length() as usize;
    for i in 0..length {
        let is_last = i == length.saturating_sub(1);
        handle_item_log(scope, args.get(i as c_int), &mut value, is_last, false);
    }
    value.push('\n');
    write_console(&value);
}

pub(crate) fn handle_console_dir(scope: &mut v8::PinScope<'_, '_>, args: v8::FunctionCallbackArguments, _retval: v8::ReturnValue) {
    let mut value = String::new();
    let length = args.length() as usize;
    for i in 0..length {
        let is_last = i == length.saturating_sub(1);
        handle_item_log(scope, args.get(i as c_int), &mut value, is_last, true);
    }
    value.push('\n');
    write_console(&value);
}

fn transform_js_object(scope: &mut v8::PinScope<'_, '_>, object: v8::Local<v8::Object>) -> String {
    // Try object.toString() first under TryCatch. If a custom toString
    // exists (not "[object Object]") use it. Otherwise try
    // JSON.stringify and replace circular-structure messages with a
    // short marker.
    v8::tc_scope!(tc, scope);
    if let Some(val) = object.to_string(tc) {
        let s = val.to_rust_string_lossy(tc);
        if !s.contains("[object Object]") {
            return s;
        }
    }
    if tc.has_caught() {
        return String::new();
    }

    if let Some(json) = v8::json::stringify(tc, object.into()) {
        let s = json.to_rust_string_lossy(tc);
        if s.contains("circular structure") {
            return "#CR".to_string();
        }
        return s;
    }

    String::new()
}