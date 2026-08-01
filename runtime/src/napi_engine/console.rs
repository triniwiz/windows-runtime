//! Node-API implementation of the console formatter and handlers (log/info/warn/error/dir/
//! trace/table), producing output identical to the rusty_v8 console in `globals/console.rs`:
//! same prefixes, same object summaries, same table rendering via the shared `render_table`.
//! Engine differences, by necessity:
//! - Stack traces: Node-API has no stack-trace API, so `console.trace` / `console.error`
//!   location lines come from a fresh `Error().stack` (host-engine formatting) instead of
//!   `v8::StackTrace` frames.
//! - The WinRT-proxy detection reads the `DeclarationFFI` via `napi_unwrap` (activates once
//!   ns_proxy wraps instances) instead of internal field 0.

use napi::{sys, CallContext, Env, JsObject, JsString, JsUnknown, NapiRaw, NapiValue, ValueType};

use crate::globals::console::{render_table, write_console};
use crate::napi_engine::value::{clear_pending_exception, dup, js_to_rust_string};
use metadata::declarations::class_declaration::ClassDeclaration;
use metadata::declarations::declaration::{Declaration, DeclarationKind};
use metadata::meta_data_reader::MetadataReader;

fn is_array(env: &Env, v: &JsUnknown) -> bool {
    let mut is = false;
    unsafe { sys::napi_is_array(env.raw(), v.raw(), &mut is) == sys::Status::napi_ok && is }
}

fn is_arraybuffer_view(env: &Env, v: &JsUnknown) -> bool {
    let mut is = false;
    unsafe {
        (sys::napi_is_typedarray(env.raw(), v.raw(), &mut is) == sys::Status::napi_ok && is) || {
            let mut is_dv = false;
            sys::napi_is_dataview(env.raw(), v.raw(), &mut is_dv) == sys::Status::napi_ok && is_dv
        }
    }
}

fn array_len(env: &Env, v: &JsUnknown) -> u32 {
    let mut len = 0u32;
    unsafe {
        let _ = sys::napi_get_array_length(env.raw(), v.raw(), &mut len);
    }
    len
}

fn get_index(env: &Env, obj: &JsUnknown, i: u32) -> Option<JsUnknown> {
    unsafe {
        let mut out: sys::napi_value = std::ptr::null_mut();
        if sys::napi_get_element(env.raw(), obj.raw(), i, &mut out) == sys::Status::napi_ok {
            Some(JsUnknown::from_raw_unchecked(env.raw(), out))
        } else {
            clear_pending_exception(env);
            None
        }
    }
}

/// Own, string-keyed property names — mirrors the v8 code's
/// `get_own_property_names(GetPropertyNamesArgs::default())` + string filter (integer keys
/// arrive as numbers there and are skipped by the `v8::String::try_from` check).
fn own_string_keys(env: &Env, obj: &JsUnknown) -> Vec<String> {
    let mut keys = Vec::new();
    unsafe {
        let mut names: sys::napi_value = std::ptr::null_mut();
        if sys::napi_get_all_property_names(
            env.raw(),
            obj.raw(),
            sys::KeyCollectionMode::own_only,
            sys::KeyFilter::enumerable | sys::KeyFilter::skip_symbols,
            sys::KeyConversion::keep_numbers,
            &mut names,
        ) != sys::Status::napi_ok
        {
            clear_pending_exception(env);
            return keys;
        }
        let names = JsUnknown::from_raw_unchecked(env.raw(), names);
        let len = array_len(env, &names);
        for i in 0..len {
            if let Some(k) = get_index(env, &names, i) {
                if matches!(k.get_type(), Ok(ValueType::String)) {
                    let s: JsString = k.cast();
                    if let Ok(u) = s.into_utf8() {
                        if let Ok(s) = u.as_str() {
                            keys.push(s.to_owned());
                        }
                    }
                }
            }
        }
    }
    keys
}

/// Property get that treats a throwing getter as an error marker (mirrors the tc_scope guard).
fn get_prop(env: &Env, obj: &JsUnknown, key: &str) -> Result<Option<JsUnknown>, ()> {
    let o: JsObject = unsafe { obj.cast() };
    match o.get_named_property::<JsUnknown>(key) {
        Ok(v) => Ok(Some(v)),
        Err(_) => {
            clear_pending_exception(env);
            Err(())
        }
    }
}

/// JSON.stringify via the host's JSON global (there is no napi JSON API, so this calls through
/// JS rather than a native binding).
fn json_stringify(env: &Env, value: &JsUnknown) -> Option<String> {
    let global = env.get_global().ok()?;
    let json: JsObject = global.get_named_property("JSON").ok()?;
    let stringify: napi::JsFunction = json.get_named_property("stringify").ok()?;
    match stringify.call(None, &[dup(env, value)]) {
        Ok(ret) => {
            if matches!(ret.get_type(), Ok(ValueType::String)) {
                let s: JsString = unsafe { ret.cast() };
                s.into_utf8().ok()?.as_str().ok().map(|s| s.to_owned())
            } else {
                None
            }
        }
        Err(_) => {
            clear_pending_exception(env);
            None
        }
    }
}

/// The current JS stack from a fresh Error (frames after the "Error" header line).
fn current_stack(env: &Env) -> Option<String> {
    let global = env.get_global().ok()?;
    let error_ctor: napi::JsFunction = global.get_named_property("Error").ok()?;
    let err = error_ctor.new_instance::<JsUnknown>(&[]).ok()?;
    let stack: JsUnknown = err.get_named_property("stack").ok()?;
    if !matches!(stack.get_type(), Ok(ValueType::String)) {
        return None;
    }
    let s: JsString = unsafe { stack.cast() };
    let full = s.into_utf8().ok()?.as_str().ok()?.to_owned();
    // Drop the "Error" header and the frames inside this native binding.
    let frames: Vec<&str> = full
        .lines()
        .skip(1)
        .filter(|l| !l.contains("node:") || l.contains(".js"))
        .collect();
    if frames.is_empty() {
        None
    } else {
        Some(frames.join("\n"))
    }
}

/// WinRT type name from a native proxy: unwraps the `DeclarationFFI` via `napi_unwrap`, checking
/// the instance itself, then its constructor, then the constructor's prototype.
fn winrt_type_name_from_object(env: &Env, obj: &JsUnknown) -> Option<String> {
    if let Some(name) = winrt_name_from_wrap(env, obj) {
        return Some(name);
    }
    let ctor = get_prop(env, obj, "constructor").ok()??;
    if !matches!(
        ctor.get_type(),
        Ok(ValueType::Object) | Ok(ValueType::Function)
    ) {
        return None;
    }
    if let Some(name) = winrt_name_from_wrap(env, &ctor) {
        return Some(name);
    }
    let proto = get_prop(env, &ctor, "prototype").ok()??;
    if !matches!(proto.get_type(), Ok(ValueType::Object)) {
        return None;
    }
    winrt_name_from_wrap(env, &proto)
}

fn winrt_name_from_wrap(env: &Env, obj: &JsUnknown) -> Option<String> {
    if !matches!(
        obj.get_type(),
        Ok(ValueType::Object) | Ok(ValueType::Function)
    ) {
        return None;
    }
    let o: JsObject = unsafe { obj.cast() };
    let dec = env.unwrap::<crate::DeclarationFFI>(&o).ok()?;
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

fn transform_js_object(env: &Env, object: &JsUnknown) -> String {
    let s = js_to_rust_string(env, object);
    if !s.contains("[object Object]") && !s.is_empty() {
        return s;
    }
    if let Some(json) = json_stringify(env, object) {
        if json.contains("circular structure") {
            return "#CR".to_string();
        }
        return json;
    }
    String::new()
}

/// Short description for native proxies / externals: recognizes DotNet bridge objects and WinRT
/// proxies by shape, returning `None` for anything else.
fn short_js_value_description(env: &Env, val: &JsUnknown) -> Option<String> {
    match val.get_type().ok()? {
        ValueType::Null | ValueType::Undefined => return Some("null".to_string()),
        ValueType::External => {
            let p = crate::napi_engine::value::ptr_from_external(env, val)
                .map(|p| p as usize)
                .unwrap_or(0);
            return Some(format!("External@0x{:x}", p));
        }
        ValueType::Object => {}
        _ => return None,
    }
    let s = js_to_rust_string(env, val);
    if s.contains("DotNetObject") {
        let parts: Vec<&str> = s.split_whitespace().collect();
        for p in parts.iter() {
            if p.contains('.') {
                let type_name =
                    p.trim_matches(|c: char| !c.is_alphanumeric() && c != '.' && c != '_');
                let mut id_suffix = String::new();
                if let Some(pos) = parts.iter().position(|x| *x == *p) {
                    if parts.len() > pos + 1 {
                        let next = parts[pos + 1];
                        if next.starts_with('#') {
                            let id = next.trim_matches(|c: char| !c.is_numeric());
                            if !id.is_empty() {
                                id_suffix = format!("#{}", id);
                            }
                        }
                    }
                }
                return Some(format!("DotNet.{}{}", type_name, id_suffix));
            }
        }
    }
    if let Some(name) = winrt_type_name_from_object(env, val) {
        return Some(name);
    }
    None
}

/// Formats one console argument into `output`. Arrays, functions, WinRT proxies, and plain
/// objects each get their own representation; `rich` selects the more verbose `console.dir` view.
pub fn format_item(env: &Env, item: &JsUnknown, output: &mut String, is_last: bool, rich: bool) {
    let vt = item.get_type().unwrap_or(ValueType::Undefined);

    if is_arraybuffer_view(env, item) {
        output.push_str(&js_to_rust_string(env, item));
        if !is_last {
            output.push(' ');
        }
        return;
    }

    if is_array(env, item) {
        let len = array_len(env, item);
        output.push('[');
        for i in 0..len {
            if i > 0 {
                output.push_str(", ");
            }
            if let Some(child) = get_index(env, item, i) {
                format_item(env, &child, output, true, false);
            }
        }
        output.push(']');
        if !is_last {
            output.push(' ');
        }
        return;
    }

    if vt == ValueType::Function {
        output.push_str(&js_to_rust_string(env, item));
        if !is_last {
            output.push(' ');
        }
        return;
    }

    if vt == ValueType::Object {
        // Prefer explicit __typeName__ metadata when present.
        if let Ok(Some(type_val)) = get_prop(env, item, "__typeName__") {
            if matches!(type_val.get_type(), Ok(ValueType::String)) {
                let full_name = js_to_rust_string(env, &type_val);
                if let Some(declaration) = MetadataReader::find_by_name(full_name.as_ref()) {
                    let lock = declaration.read();
                    if let Some(class_dec) = lock.as_any().downcast_ref::<ClassDeclaration>() {
                        if !rich {
                            output.push_str(class_dec.name());
                            if !is_last {
                                output.push(' ');
                            }
                            return;
                        }
                        output.push_str(&format!("{} (constructor) {{\n", class_dec.name()));
                        let props = crate::class_helpers::collect_class_properties(class_dec);
                        for p in props.iter().filter(|p| p.is_static()) {
                            output.push_str(&format!("  {}: <static>\n", p.name()));
                        }
                        let methods = crate::class_helpers::collect_class_methods(class_dec);
                        let proto_methods: Vec<_> =
                            methods.iter().filter(|m| !m.is_static()).collect();
                        if !proto_methods.is_empty() {
                            output.push_str("  prototype methods: [");
                            for (i, m) in proto_methods.iter().enumerate() {
                                if i > 0 {
                                    output.push_str(", ");
                                }
                                let mut mn = m.overload_name().to_string();
                                if mn.is_empty() {
                                    mn = m.name().to_string();
                                }
                                output.push_str(&mn);
                            }
                            output.push_str("]\n");
                        }
                        output.push_str("}\n");
                        if !is_last {
                            output.push(' ');
                        }
                        return;
                    }
                }
            }
        }

        // WinRT native proxy.
        if let Some(type_name) = winrt_type_name_from_object(env, item) {
            if !rich {
                output.push_str(&type_name);
                if !is_last {
                    output.push(' ');
                }
                return;
            }
            output.push_str(&format!("{} {{\n", type_name));
            output.push_str("  properties: <native>\n");
            output.push_str("  methods: <native>\n");
            output.push_str("}\n");
            if !is_last {
                output.push(' ');
            }
            return;
        }

        // Rich inspection for console.dir.
        if rich {
            let keys = own_string_keys(env, item);
            output.push_str("{\n");
            for key in keys {
                match get_prop(env, item, &key) {
                    Err(_) => output.push_str(&format!("  {}: <getter threw>\n", key)),
                    Ok(None) => output.push_str(&format!("  {}: <unavailable>\n", key)),
                    Ok(Some(v)) => {
                        let kvt = v.get_type().unwrap_or(ValueType::Undefined);
                        if kvt == ValueType::Function {
                            output.push_str(&format!("  {}: ()\n", key));
                        } else if is_array(env, &v) {
                            match json_stringify(env, &v) {
                                Some(json) => {
                                    output.push_str(&format!("  {}: {}\n", key, json))
                                }
                                None => output.push_str(&format!("  {}: [Array]\n", key)),
                            }
                        } else if kvt == ValueType::Object {
                            let s = transform_js_object(env, &v);
                            output.push_str(&format!("  {}: {}\n", key, s));
                        } else {
                            let s = js_to_rust_string(env, &v);
                            output.push_str(&format!("  {}: {}\n", key, s));
                        }
                    }
                }
            }
            output.push_str("}\n");
            if !is_last {
                output.push(' ');
            }
            return;
        }

        // Shallow summary for plain JS objects.
        {
            let keys = own_string_keys(env, item);
            let mut parts: Vec<String> = Vec::new();
            for key in keys {
                match get_prop(env, item, &key) {
                    Err(_) => parts.push(format!("{}: <getter threw>", key)),
                    Ok(None) => parts.push(format!("{}: <unavailable>", key)),
                    Ok(Some(v)) => {
                        if let Some(desc) = short_js_value_description(env, &v) {
                            parts.push(format!("{}: {}", key, desc));
                            continue;
                        }
                        let kvt = v.get_type().unwrap_or(ValueType::Undefined);
                        if kvt == ValueType::Function {
                            parts.push(format!("{}: ()", key));
                        } else if kvt == ValueType::Object {
                            let s = transform_js_object(env, &v);
                            parts.push(format!("{}: {}", key, s));
                        } else {
                            parts.push(format!("{}: {}", key, js_to_rust_string(env, &v)));
                        }
                    }
                }
            }
            if !parts.is_empty() {
                output.push_str(&format!("{{ {} }}", parts.join(", ")));
                if !is_last {
                    output.push(' ');
                }
                return;
            }
        }

        // Generic stringification fallback.
        let s = js_to_rust_string(env, item);
        if !s.contains("[object Object]") && !s.is_empty() {
            output.push_str(&s);
            if !is_last {
                output.push(' ');
            }
            return;
        }
        if let Some(json) = json_stringify(env, item) {
            output.push_str(if json.contains("circular structure") {
                "#CR"
            } else {
                &json
            });
        } else {
            output.push_str("[object Object]");
        }
        if !is_last {
            output.push(' ');
        }
        return;
    }

    output.push_str(&js_to_rust_string(env, item));
    if !is_last {
        output.push(' ');
    }
}

fn format_args(env: &Env, ctx: &CallContext, start: usize, output: &mut String) {
    let length = ctx.length;
    for i in start..length {
        if let Ok(v) = ctx.get::<JsUnknown>(i) {
            format_item(env, &v, output, i == length.saturating_sub(1), false);
        }
    }
}

/// Install log/info/warn/error/dir/trace/table onto `console` (timers/assert are installed by
/// `install_console_timers`).
pub fn install_console_formatters(env: &Env, console: &mut JsObject) -> napi::Result<()> {
    let log = env.create_function_from_closure("log", |ctx: CallContext| {
        let env = &ctx.env;
        let mut value = String::from("[INFO] ");
        format_args(env, &ctx, 0, &mut value);
        value.push('\n');
        write_console(&value);
        Ok(())
    })?;
    console.set_named_property("log", log)?;
    let info = env.create_function_from_closure("info", |ctx: CallContext| {
        let env = &ctx.env;
        let mut value = String::from("[INFO] ");
        format_args(env, &ctx, 0, &mut value);
        value.push('\n');
        write_console(&value);
        Ok(())
    })?;
    console.set_named_property("info", info)?;

    let warn = env.create_function_from_closure("warn", |ctx: CallContext| {
        let env = &ctx.env;
        let mut value = String::from("[WARN] ");
        format_args(env, &ctx, 0, &mut value);
        value.push('\n');
        write_console(&value);
        Ok(())
    })?;
    console.set_named_property("warn", warn)?;

    let error = env.create_function_from_closure("error", |ctx: CallContext| {
        let env = &ctx.env;
        let mut value = String::from("[ERROR] ");
        let length = ctx.length;
        let mut printed_stack = false;
        for i in 0..length {
            let is_last = i == length.saturating_sub(1);
            let Ok(arg) = ctx.get::<JsUnknown>(i) else {
                continue;
            };
            // Prefer an Error's own stack (remapped via __ns_remapStack when present).
            let used_stack = (|| -> Option<()> {
                if !matches!(arg.get_type(), Ok(ValueType::Object)) {
                    return None;
                }
                let stack_val = get_prop(env, &arg, "stack").ok()??;
                if !matches!(stack_val.get_type(), Ok(ValueType::String)) {
                    return None;
                }
                let stack_str = js_to_rust_string(env, &stack_val);
                let remapped = (|| -> Option<String> {
                    let global = env.get_global().ok()?;
                    let remap: JsUnknown = global.get_named_property("__ns_remapStack").ok()?;
                    if !matches!(remap.get_type(), Ok(ValueType::Function)) {
                        return None;
                    }
                    let f: napi::JsFunction = unsafe { remap.cast() };
                    let arg_js = env.create_string(&stack_str).ok()?;
                    let ret = f
                        .call(None, &[arg_js])
                        .map_err(|_| clear_pending_exception(env))
                        .ok()?;
                    if matches!(ret.get_type(), Ok(ValueType::String)) {
                        Some(js_to_rust_string(env, &ret))
                    } else {
                        None
                    }
                })();
                value.push_str(&remapped.unwrap_or(stack_str));
                if !is_last {
                    value.push(' ');
                }
                Some(())
            })()
            .is_some();
            if used_stack {
                printed_stack = true;
            } else {
                format_item(env, &arg, &mut value, is_last, false);
            }
        }
        if !printed_stack {
            if let Some(stack) = current_stack(env) {
                if let Some(first) = stack.lines().next() {
                    value.push_str(&format!("\n{}", first));
                }
            }
        }
        value.push('\n');
        write_console(&value);
        Ok(())
    })?;
    console.set_named_property("error", error)?;

    let dir = env.create_function_from_closure("dir", |ctx: CallContext| {
        let env = &ctx.env;
        let mut value = String::new();
        let length = ctx.length;
        for i in 0..length {
            if let Ok(v) = ctx.get::<JsUnknown>(i) {
                format_item(env, &v, &mut value, i == length.saturating_sub(1), true);
            }
        }
        value.push('\n');
        write_console(&value);
        Ok(())
    })?;
    console.set_named_property("dir", dir)?;

    let trace = env.create_function_from_closure("trace", |ctx: CallContext| {
        let env = &ctx.env;
        let mut value = String::from("[TRACE] ");
        if ctx.length == 0 {
            value.push_str("Trace");
        } else {
            format_args(env, &ctx, 0, &mut value);
        }
        value.push('\n');
        if let Some(stack) = current_stack(env) {
            value.push_str(&stack);
            value.push('\n');
        }
        write_console(&value);
        Ok(())
    })?;
    console.set_named_property("trace", trace)?;

    let table = env.create_function_from_closure("table", |ctx: CallContext| {
        let env = &ctx.env;
        if ctx.length == 0 {
            write_console("[INFO] (no data)\n");
            return Ok(());
        }
        let data = ctx.get::<JsUnknown>(0)?;
        let filter: Option<Vec<String>> = if ctx.length > 1 {
            let ca = ctx.get::<JsUnknown>(1)?;
            if is_array(env, &ca) {
                let mut cols = Vec::new();
                for i in 0..array_len(env, &ca) {
                    if let Some(v) = get_index(env, &ca, i) {
                        cols.push(js_to_rust_string(env, &v));
                    }
                }
                Some(cols)
            } else {
                None
            }
        } else {
            None
        };

        let mut out = String::from("[INFO] \n");
        if is_array(env, &data) {
            out.push_str(&table_from_array(env, &data, filter.as_deref()));
        } else if matches!(data.get_type(), Ok(ValueType::Object)) {
            out.push_str(&table_from_object(env, &data));
        } else {
            format_item(env, &data, &mut out, true, false);
            out.push('\n');
        }
        write_console(&out);
        Ok(())
    })?;
    console.set_named_property("table", table)?;
    Ok(())
}

/// Table text for a value, dispatching like the `console.table` handler (array → rows,
/// object → key/value, primitive → formatted item). Public for cross-crate tests.
pub fn format_table(env: &Env, data: &JsUnknown, filter: Option<&[String]>) -> String {
    if is_array(env, data) {
        table_from_array(env, data, filter)
    } else if matches!(data.get_type(), Ok(ValueType::Object)) {
        table_from_object(env, data)
    } else {
        let mut out = String::new();
        format_item(env, data, &mut out, true, false);
        out.push('\n');
        out
    }
}

/// Builds a `console.table` layout for an array: a first pass discovers columns from each row's
/// own keys (or a single `Values` column for non-object rows), then a second pass fills the cells.
pub(crate) fn table_from_array(env: &Env, arr: &JsUnknown, filter: Option<&[String]>) -> String {
    let row_count = array_len(env, arr);
    if row_count == 0 {
        return "(empty)\n".to_string();
    }

    let mut cols: Vec<String> = vec!["(index)".to_string()];
    for i in 0..row_count {
        if let Some(row_val) = get_index(env, arr, i) {
            let is_obj = matches!(row_val.get_type(), Ok(ValueType::Object));
            if is_obj && !is_array(env, &row_val) {
                for col in own_string_keys(env, &row_val) {
                    if let Some(f) = filter {
                        if !f.iter().any(|fc| fc == &col) {
                            continue;
                        }
                    }
                    if !cols.contains(&col) {
                        cols.push(col);
                    }
                }
            } else if cols.len() < 2 && !cols.contains(&"Values".to_string()) {
                cols.push("Values".to_string());
            }
        }
    }

    let mut rows: Vec<Vec<String>> = Vec::with_capacity(row_count as usize);
    for i in 0..row_count {
        let mut row = vec![i.to_string()];
        if let Some(row_val) = get_index(env, arr, i) {
            let is_obj = matches!(row_val.get_type(), Ok(ValueType::Object));
            if is_obj && !is_array(env, &row_val) {
                for col in cols.iter().skip(1) {
                    let cell = match get_prop(env, &row_val, col) {
                        Ok(Some(v)) => {
                            let mut s = String::new();
                            format_item(env, &v, &mut s, true, false);
                            s
                        }
                        _ => String::new(),
                    };
                    row.push(cell);
                }
            } else {
                let mut s = String::new();
                format_item(env, &row_val, &mut s, true, false);
                row.push(s);
                while row.len() < cols.len() {
                    row.push(String::new());
                }
            }
        }
        while row.len() < cols.len() {
            row.push(String::new());
        }
        rows.push(row);
    }

    render_table(&cols, &rows)
}

/// Builds a `console.table` layout for a plain object: one row per key, under `(index)`/`Values`
/// columns.
pub(crate) fn table_from_object(env: &Env, obj: &JsUnknown) -> String {
    let cols = vec!["(index)".to_string(), "Values".to_string()];
    let mut rows: Vec<Vec<String>> = Vec::new();
    for key in own_string_keys(env, obj) {
        let cell = match get_prop(env, obj, &key) {
            Ok(Some(v)) => {
                let mut s = String::new();
                format_item(env, &v, &mut s, true, false);
                s
            }
            _ => String::new(),
        };
        rows.push(vec![key, cell]);
    }
    render_table(&cols, &rows)
}
