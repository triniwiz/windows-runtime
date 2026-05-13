use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::env;
use std::fs;
use std::path::PathBuf;

use regex::Regex;

use metadata::declarations::base_class_declaration::BaseClassDeclarationImpl;
use metadata::declarations::class_declaration::ClassDeclaration;
use metadata::declarations::declaration::{Declaration, DeclarationKind};
use metadata::declarations::delegate_declaration::{DelegateDeclaration, DelegateDeclarationImpl};
use metadata::declarations::delegate_declaration::generic_delegate_declaration::GenericDelegateDeclaration;
use metadata::declarations::delegate_declaration::generic_delegate_instance_declaration::GenericDelegateInstanceDeclaration;
use metadata::declarations::enum_declaration::EnumDeclaration;
use metadata::declarations::interface_declaration::generic_interface_declaration::GenericInterfaceDeclaration;
use metadata::declarations::interface_declaration::InterfaceDeclaration;
use metadata::declarations::namespace_declaration::NamespaceDeclaration;
use metadata::declarations::struct_declaration::StructDeclaration;
use metadata::meta_data_reader::MetadataReader;
use metadata::prelude::get_type_name;
use metadata::signature::Signature;
use metadata::value::Value;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
use windows::Win32::System::WinRT::Metadata::{CorTokenType, IMetaDataImport2};

// ---------------------------------------------------------------------------
// TypeScript type mapping
// ---------------------------------------------------------------------------

fn map_type_to_ts(value: &str) -> String {
    map_type_to_ts_with_generics(value, &[])
}

fn split_generic_arguments(value: &str) -> Option<Vec<String>> {
    let start = value.find('<')?;
    let end = value.rfind('>')?;
    if end <= start {
        return None;
    }

    let inner = &value[start + 1..end];
    let mut args = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();

    for ch in inner.chars() {
        match ch {
            '<' => {
                depth += 1;
                current.push(ch);
            }
            '>' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if depth == 0 => {
                let arg = current.trim();
                if !arg.is_empty() {
                    args.push(arg.to_string());
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    let tail = current.trim();
    if !tail.is_empty() {
        args.push(tail.to_string());
    }

    Some(args)
}

fn generic_parameter_names(full_name: &str, fallback_count: usize) -> Vec<String> {
    if let Some(args) = split_generic_arguments(full_name) {
        let names = args
            .into_iter()
            .map(|arg| {
                let trimmed = arg.trim();
                if let Some(idx) = trimmed.rfind('.') {
                    trimmed[idx + 1..].to_string()
                } else {
                    trimmed.to_string()
                }
            })
            .filter(|arg| !arg.is_empty())
            .collect::<Vec<_>>();
        if !names.is_empty() {
            return names;
        }
    }

    (0..fallback_count)
        .map(|index| format!("T{}", index + 1))
        .collect()
}

/// JS/TS reserved words that cannot appear as bare identifiers in certain
/// positions (parameter names, destructuring, etc.).  When one of these appears
/// as a parameter name it is suffixed with `_` to form a valid identifier.
/// Method and property names inside interface/class bodies are quoted instead.
const JS_RESERVED: &[&str] = &[
    "break", "case", "catch", "class", "const", "continue", "debugger",
    "default", "delete", "do", "else", "enum", "export", "extends", "false",
    "finally", "for", "function", "if", "implements", "import", "in",
    "instanceof", "interface", "let", "new", "null", "package", "private",
    "protected", "public", "return", "static", "super", "switch", "this",
    "throw", "true", "try", "typeof", "undefined", "var", "void", "while",
    "with", "yield",
    // Additional identifiers that break generated code in practice.
    "arguments", "eval", "constructor", "prototype",
];

/// Make an identifier safe for use as a parameter / variable name.
/// Reserved words get a trailing `_`; everything else is unchanged.
fn sanitize_param(name: &str) -> String {
    if name.is_empty() {
        return "arg".to_string();
    }
    if JS_RESERVED.contains(&name) {
        format!("{}_", name)
    } else {
        name.to_string()
    }
}

/// Make an identifier safe for use as a member name inside a TypeScript
/// interface or class body.  Reserved words are quoted with bracket notation.
fn sanitize_member(name: &str) -> String {
    if JS_RESERVED.contains(&name) {
        format!("[\"{name}\"]")
    } else {
        name.to_string()
    }
}

fn declaration_display_name(full_name: &str) -> String {
    let name = match full_name.rfind('.') {
        Some(index) => full_name[index + 1..].to_string(),
        None => full_name.to_string(),
    };

    if let Some(index) = name.find('`') {
        name[..index].to_string()
    } else {
        name
    }
}

fn map_type_to_ts_with_generics(value: &str, generic_params: &[String]) -> String {
    let value = value.trim();

    if let Some(inner) = value.strip_prefix("ByRef ") {
        return map_type_to_ts_with_generics(inner, generic_params);
    }

    if let Some(inner) = value.strip_suffix("[]") {
        return format!("{}[]", map_type_to_ts_with_generics(inner, generic_params));
    }

    let base = if let Some(idx) = value.find('<') { &value[..idx] } else { value };
    let base_no_arity = if let Some(idx) = base.find('`') { &base[..idx] } else { base };

    if let Some(index) = value.strip_prefix("Var!").and_then(|rest| rest.parse::<usize>().ok()) {
        if let Some(name) = generic_params.get(index) {
            return name.clone();
        }
    }

    match base_no_arity {
        "Void" => return "void".to_string(),
        "Boolean" => return "boolean".to_string(),
        "DateTime" => return "Date".to_string(),
        "TimeSpan" => return "number".to_string(),
        "Guid" => return "Guid".to_string(),
        "String" | "Char16" => return "string".to_string(),
        "Int8" | "Int16" | "Int32" | "IntI32" => return "number".to_string(),
        "UInt8" | "Uint8" | "UInt16" | "UInt32" => return "number".to_string(),
        "Single" | "Float" | "Double" => return "number".to_string(),
        "Int64" | "UInt64" | "ISize" | "USize" => return "number | bigint".to_string(),
        "Point"        => return "{ X: number; Y: number }".to_string(),
        "Size"         => return "{ Width: number; Height: number }".to_string(),
        "Rect"         => return "{ X: number; Y: number; Width: number; Height: number }".to_string(),
        "Color"        => return "{ A: number; R: number; G: number; B: number }".to_string(),
        "Thickness"    => return "{ Left: number; Top: number; Right: number; Bottom: number }".to_string(),
        "CornerRadius" => return "{ TopLeft: number; TopRight: number; BottomRight: number; BottomLeft: number }".to_string(),
        "GridLength"   => return "{ Value: number; GridUnitType: number }".to_string(),
        "Duration"     => return "{ DurationType: number; TimeSpan: number }".to_string(),
        "Matrix"       => return "{ M11: number; M12: number; M21: number; M22: number; OffsetX: number; OffsetY: number }".to_string(),
        _ => {}
    }

    if base_no_arity == "IAsyncOperation" || base_no_arity.ends_with(".IAsyncOperation") ||
       base_no_arity == "IAsyncAction" || base_no_arity.ends_with(".IAsyncAction") {
        if let Some(inner) = value.find('<').and_then(|s| {
            let inner = &value[s + 1..value.len().saturating_sub(1)];
            if inner.is_empty() { None } else { Some(inner) }
        }) {
            let iface = if base_no_arity.contains('.') {
                base_no_arity.to_string()
            } else {
                format!("Windows.Foundation.{}", base_no_arity)
            };
            return format!("{}<{}>", iface, map_type_to_ts_with_generics(inner, generic_params));
        }
        return if base_no_arity.contains('.') {
            base_no_arity.to_string()
        } else {
            format!("Windows.Foundation.{}", base_no_arity)
        };
    }

    if base_no_arity == "IVector" || base_no_arity.ends_with(".IVector") ||
       base_no_arity == "IReadOnlyList" || base_no_arity.ends_with(".IReadOnlyList") ||
       base_no_arity == "IIterable" || base_no_arity.ends_with(".IIterable") {
        if let Some(inner) = value.find('<').and_then(|s| {
            let inner = &value[s + 1..value.len().saturating_sub(1)];
            if inner.is_empty() { None } else { Some(inner) }
        }) {
            return format!("{}[]", map_type_to_ts_with_generics(inner, generic_params));
        }
        return "unknown[]".to_string();
    }

    if base_no_arity == "IMap" || base_no_arity.ends_with(".IMap") ||
       base_no_arity == "IReadOnlyDictionary" || base_no_arity.ends_with(".IReadOnlyDictionary") {
        if let Some(inner) = value.find('<').map(|s| &value[s + 1..value.len().saturating_sub(1)]) {
            let mut depth = 0usize;
            let mut split = None;
            for (i, c) in inner.char_indices() {
                match c {
                    '<' => depth += 1,
                    '>' => depth = depth.saturating_sub(1),
                    ',' if depth == 0 => { split = Some(i); break; }
                    _ => {}
                }
            }
            if let Some(split) = split {
                let k = map_type_to_ts_with_generics(inner[..split].trim(), generic_params);
                let v = map_type_to_ts_with_generics(inner[split + 1..].trim(), generic_params);
                return format!("Record<{}, {}>", k, v);
            }
        }
        return "Record<unknown, unknown>".to_string();
    }

    if value.contains('<') {
        let name = if let Some(idx) = base.find('`') { &base[..idx] } else { base };
        if let Some(args) = split_generic_arguments(value) {
            let rendered = args
                .iter()
                .map(|arg| map_type_to_ts_with_generics(arg, generic_params))
                .collect::<Vec<_>>()
                .join(", ");
            return format!("{}<{}>", name, rendered);
        }
    }

    if let Some(idx) = value.find('`') {
        return value[..idx].to_string();
    }

    if !value.is_empty() && value.chars().next().unwrap().is_alphabetic() {
        return value.to_string();
    }

    "unknown".to_string()
}

// ---------------------------------------------------------------------------
// Signature formatting
// ---------------------------------------------------------------------------

fn method_signature(
    method: &metadata::declarations::method_declaration::MethodDeclaration,
    use_arrow: bool,
) -> String {
    method_signature_with_generics(method, &[], use_arrow)
}

fn method_signature_with_generics(
    method: &metadata::declarations::method_declaration::MethodDeclaration,
    generic_params: &[String],
    use_arrow: bool,
) -> String {
    let method_name = method.name();
    let total_params = method.parameters().len();
    let params = method
        .parameters()
        .iter()
        .enumerate()
        .map(|(index, p)| {
            let raw_name = if p.name().is_empty() { "arg" } else { p.name() };
            let name = sanitize_param(raw_name);
            let param_ty = p
                .metadata()
                .map(|m| Signature::to_string(m, &p.type_()))
                .unwrap_or_else(|| "Object".to_string());
            let rendered = if !generic_params.is_empty()
                && (
                    (method_name == "GetMany"
                        && ((total_params == 1 && index == 0)
                            || (total_params >= 2 && index + 1 == total_params)))
                        || (method_name == "ReplaceAll" && index == 0)
                        || (raw_name == "items"
                            && (method_name == "GetMany" || method_name == "ReplaceAll"))
                )
            {
                format!("{}[]", generic_params[0])
            } else if method_name == "GetMany" && total_params >= 2 && index == 0 {
                "number".to_string()
            } else {
                map_type_to_ts_with_generics(param_ty.as_str(), generic_params)
            };
            format!("{}: {}", name, rendered)
        })
        .collect::<Vec<_>>()
        .join(", ");

    let ret_ty = method
        .metadata()
        .map(|m| Signature::to_string(m, &method.return_type()))
        .unwrap_or_else(|| "Void".to_string());

    let sep = if use_arrow { " => " } else { ": " };
    format!(
        "({params}){sep}{}",
        map_type_to_ts_with_generics(ret_ty.as_str(), generic_params)
    )
}

// ---------------------------------------------------------------------------
// Method / interface collection helpers
// ---------------------------------------------------------------------------

fn collect_all_interface_methods(
    interface: &InterfaceDeclaration,
    methods: &mut Vec<metadata::declarations::method_declaration::MethodDeclaration>,
    seen: &mut BTreeSet<String>,
) {
    for method in interface.methods().iter().filter(|m| m.is_exported()) {
        let sig_key = format!("{}:{}", method.name(), method.parameters().len());
        if seen.insert(sig_key) {
            methods.push(method.clone());
        }
    }
    for base in interface.implemented_interfaces() {
        collect_all_interface_methods(base, methods, seen);
    }
}

fn collect_all_class_methods(
    class_decl: &ClassDeclaration,
    methods: &mut Vec<metadata::declarations::method_declaration::MethodDeclaration>,
    seen: &mut BTreeSet<String>,
) {
    for method in class_decl.methods().iter().filter(|m| m.is_exported()) {
        let sig_key = format!("{}:{}", method.name(), method.parameters().len());
        if seen.insert(sig_key) {
            methods.push(method.clone());
        }
    }
    for interface in class_decl.implemented_interfaces() {
        collect_all_interface_methods(interface, methods, seen);
    }
    let base_name = class_decl.base_full_name();
    if !base_name.is_empty() && base_name != "System.Object" {
        if let Some(base_dec) = MetadataReader::find_by_name(base_name) {
            let lock = base_dec.read();
            if let Some(base_class) = lock.as_any().downcast_ref::<ClassDeclaration>() {
                collect_all_class_methods(base_class, methods, seen);
            }
        }
    }
}

fn interface_extends_clause(
    implemented: Vec<&InterfaceDeclaration>,
    generic_params: &[String],
) -> String {
    let mut bases = implemented
        .iter()
        .map(|iface| map_type_to_ts_with_generics(iface.full_name(), generic_params))
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>();

    bases.sort();
    bases.dedup();

    if bases.is_empty() {
        String::new()
    } else {
        format!(" extends {}", bases.join(", "))
    }
}

fn event_type_name(
    event: &metadata::declarations::event_declaration::EventDeclaration,
) -> String {
    // If the event's delegate is available, try to render it. Prefer
    // namespace-qualified aliases so references resolve from any module.
    if let Some(dimpl) = event.delegate_impl() {
        // Concrete non-generic delegate (named alias)
        if let Some(delegate) = dimpl.as_declaration().as_any().downcast_ref::<DelegateDeclaration>() {
            let full = delegate.full_name();
            let ns = declaration_namespace(full);
            let name = declaration_display_name(full);
            return if ns.is_empty() { name } else { format!("{}.{}", ns, name) };
        }

        // Generic closed-instance delegate (e.g. TypedEventHandler<TSender, TArgs>)
        if let Some(generic_instance) = dimpl.as_declaration().as_any().downcast_ref::<GenericDelegateInstanceDeclaration>() {
            let full = generic_instance.full_name();
            let base = if let Some(idx) = full.find('<') { &full[..idx] } else { full };
            let ns = declaration_namespace(base);
            let base_name = declaration_display_name(base);
            if let Some(args) = split_generic_arguments(full) {
                let mapped = args
                    .iter()
                    .map(|a| map_type_to_ts_with_generics(a.trim(), &[]))
                    .collect::<Vec<_>>()
                    .join(", ");
                if ns.is_empty() {
                    return format!("{}<{}>", base_name, mapped);
                } else {
                    return format!("{}.{}<{}>", ns, base_name, mapped);
                }
            }
            return if ns.is_empty() { base_name } else { format!("{}.{}", ns, base_name) };
        }
    }

    // Fallback: inspect the `add` method parameter to infer the delegate shape
    // and produce an inline signature when we can't render the delegate alias.
    if let Some(md) = event.add_method().metadata() {
        let params = event.add_method().parameters();
        if let Some(param) = params.iter().find(|p| !p.is_out()) {
            let param_ty = Signature::to_string(md, &param.type_());
            let base = if let Some(idx) = param_ty.find('<') { &param_ty[..idx] } else { param_ty.as_str() };
            let base_no_arity = if let Some(idx) = base.find('`') { &base[..idx] } else { base };
            let simple = declaration_display_name(base_no_arity);

            match simple.as_str() {
                "TypedEventHandler" => {
                    if let Some(args) = split_generic_arguments(&param_ty) {
                        if args.len() == 2 {
                            let sender_ts = map_type_to_ts_with_generics(args[0].trim(), &[]);
                            let args_ts = map_type_to_ts_with_generics(args[1].trim(), &[]);
                            return format!("(sender: {}, args: {}) => void", sender_ts, args_ts);
                        }
                    }
                    return "unknown".to_string();
                }
                "EventHandler" => {
                    if let Some(args) = split_generic_arguments(&param_ty) {
                        if args.len() == 1 {
                            let args_ts = map_type_to_ts_with_generics(args[0].trim(), &[]);
                            return format!("(sender: Object, args: {}) => void", args_ts);
                        }
                    }
                    return "(sender: Object, args: any) => void".to_string();
                }
                "DispatcherQueueHandler" => return "() => void".to_string(),
                "RoutedEventHandler" => {
                    if let Some(args) = split_generic_arguments(&param_ty) {
                        if args.len() == 1 {
                            let args_ts = map_type_to_ts_with_generics(args[0].trim(), &[]);
                            return format!("(sender: Object, args: {}) => void", args_ts);
                        }
                    }
                    return "(sender: Object, args: any) => void".to_string();
                }
                _ => return map_type_to_ts_with_generics(param_ty.as_str(), &[]),
            }
        }
    }

    "unknown".to_string()
}

// ---------------------------------------------------------------------------
// Declaration renderers
// ---------------------------------------------------------------------------

fn render_interface(name: &str, interface: &InterfaceDeclaration) -> String {
    let mut out = String::new();
    let extends = interface_extends_clause(interface.implemented_interfaces(), &[]);
    out.push_str(&format!("interface {}{} {{\n", name, extends));

    let mut methods = Vec::new();
    let mut seen = BTreeSet::new();
    collect_all_interface_methods(interface, &mut methods, &mut seen);
    methods.sort_by(|a, b| a.name().cmp(b.name()));

    for method in methods {
        out.push_str(&format!("  {}{};\n",
            sanitize_member(method.name()), method_signature(&method, false)));
    }

    let mut properties = interface
        .properties()
        .iter()
        .filter(|p| p.is_exported())
        .collect::<Vec<_>>();
    properties.sort_by(|a, b| a.name().cmp(b.name()));

    for prop in properties {
        let Some(md) = prop.getter().metadata() else { continue };
        let return_ty = Signature::to_string(md, &prop.getter().return_type());
        out.push_str(&format!(
            "  {}: {};\n",
            sanitize_member(prop.name()),
            map_type_to_ts(return_ty.as_str())
        ));
    }

    for event in interface.events().iter().filter(|e| e.is_exported()) {
        out.push_str(&format!("  {}: {};\n",
            sanitize_member(event.name()), event_type_name(event)));
    }

    out.push_str("}\n\n");
    out
}

fn render_class(name: &str, class_decl: &ClassDeclaration) -> String {
    let mut out = String::new();
    let base = class_decl.base_full_name();
    let extends = if base.is_empty() || base == "System.Object" {
        String::new()
    } else {
        format!(" extends {}", map_type_to_ts(base))
    };
    let mut interfaces = class_decl
        .implemented_interfaces()
        .iter()
        .map(|iface| map_type_to_ts(iface.full_name()))
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>();
    interfaces.sort();
    interfaces.dedup();
    let implements = if interfaces.is_empty() {
        String::new()
    } else {
        format!(" implements {}", interfaces.join(", "))
    };

    out.push_str(&format!("class {}{}{} {{\n", name, extends, implements));

    for ctor in class_decl.initializers() {
        let params = ctor
            .parameters()
            .iter()
            .map(|p| {
                let raw_name = if p.name().is_empty() { "arg" } else { p.name() };
                let name = sanitize_param(raw_name);
                let param_ty = p
                    .metadata()
                    .map(|m| Signature::to_string(m, &p.type_()))
                    .unwrap_or_else(|| "unknown".to_string());
                format!("{}: {}", name, map_type_to_ts(&param_ty))
            })
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("  constructor({params});\n"));
    }

    let mut methods = Vec::new();
    let mut seen = BTreeSet::new();
    collect_all_class_methods(class_decl, &mut methods, &mut seen);
    methods.sort_by(|a, b| a.name().cmp(b.name()));

    for method in methods {
        let sig  = method_signature(&method, false);
        let mname = sanitize_member(method.name());
        if method.is_static() {
            out.push_str(&format!("  static {mname}{sig};\n"));
        } else {
            out.push_str(&format!("  {mname}{sig};\n"));
        }
    }

    let mut properties = class_decl
        .properties()
        .iter()
        .filter(|p| p.is_exported())
        .collect::<Vec<_>>();
    properties.sort_by(|a, b| a.name().cmp(b.name()));

    for prop in properties {
        let Some(md) = prop.getter().metadata() else { continue };
        let return_ty = Signature::to_string(md, &prop.getter().return_type());
        let pname = sanitize_member(prop.name());
        let ts_ty = map_type_to_ts(return_ty.as_str());
        if prop.is_static() {
            out.push_str(&format!("  static {pname}: {ts_ty};\n"));
        } else {
            out.push_str(&format!("  {pname}: {ts_ty};\n"));
        }
    }

    for event in class_decl.events().iter().filter(|e| e.is_exported()) {
        let ename = sanitize_member(event.name());
        let ety   = event_type_name(event);
        if event.is_static() {
            out.push_str(&format!("  static {ename}: {ety};\n"));
        } else {
            out.push_str(&format!("  {ename}: {ety};\n"));
        }
    }

    out.push_str("}\n\n");
    out
}

fn enum_value_to_string(value: Value) -> String {
    match value {
        Value::Int8(v) => v.to_string(),
        Value::Uint8(v) => v.to_string(),
        Value::Int16(v) => v.to_string(),
        Value::Uint16(v) => v.to_string(),
        Value::Int32(v) => v.to_string(),
        Value::Uint32(v) => v.to_string(),
        Value::Int64(v) => v.to_string(),
        Value::Uint64(v) => format!("{}", v),
        Value::Single(v) => v.to_string(),
        Value::Double(v) => v.to_string(),
        Value::Boolean(v) => if v { "1".to_string() } else { "0".to_string() },
        _ => "0".to_string(),
    }
}

fn render_enum(name: &str, enum_decl: &EnumDeclaration) -> String {
    let mut out = String::new();
    out.push_str(&format!("enum {} {{\n", name));
    for member in enum_decl.enums() {
        out.push_str(&format!(
            "  {} = {},\n",
            member.name(),
            enum_value_to_string(member.value())
        ));
    }
    out.push_str("}\n\n");
    out
}

fn render_struct(name: &str, struct_decl: &StructDeclaration) -> String {
    let mut out = String::new();
    out.push_str(&format!("interface {} {{\n", name));
    for field in struct_decl.fields() {
        let Some(md) = field.base().metadata() else { continue };
        let field_ty = Signature::to_string(md, &field.type_());
        out.push_str(&format!(
            "  {}: {};\n",
            field.name(),
            map_type_to_ts(field_ty.as_str())
        ));
    }
    out.push_str("}\n\n");
    out
}

fn render_delegate(name: &str, delegate: &DelegateDeclaration) -> String {
    let mut out = String::new();
    let invoke = delegate.invoke_method();
    out.push_str(&format!(
        "type {} = {};\n\n",
        name,
        method_signature(invoke, true)
    ));
    out
}

fn render_generic_interface(interface: &GenericInterfaceDeclaration) -> String {
    let mut out = String::new();
    let generic_params =
        generic_parameter_names(interface.full_name(), interface.number_of_generic_parameters());
    let name = declaration_display_name(interface.full_name());
    let generic_suffix = if generic_params.is_empty() {
        String::new()
    } else {
        format!("<{}>", generic_params.join(", "))
    };
    let extends = interface_extends_clause(interface.implemented_interfaces(), &generic_params);
    out.push_str(&format!("interface {}{}{} {{\n", name, generic_suffix, extends));

    let mut methods = interface
        .methods()
        .iter()
        .filter(|m| m.is_exported())
        .collect::<Vec<_>>();
    methods.sort_by(|a, b| a.name().cmp(b.name()));

    for method in methods {
        out.push_str(&format!(
            "  {}{};\n",
            method.name(),
            method_signature_with_generics(method, &generic_params, false)
        ));
    }

    let mut properties = interface
        .properties()
        .iter()
        .filter(|p| p.is_exported())
        .collect::<Vec<_>>();
    properties.sort_by(|a, b| a.name().cmp(b.name()));

    for prop in properties {
        let Some(md) = prop.getter().metadata() else { continue };
        let return_ty = Signature::to_string(md, &prop.getter().return_type());
        out.push_str(&format!(
            "  {}: {};\n",
            prop.name(),
            map_type_to_ts_with_generics(return_ty.as_str(), &generic_params)
        ));
    }

    for event in interface.events().iter().filter(|e| e.is_exported()) {
        out.push_str(&format!("  {}: {};\n", event.name(), event_type_name(event)));
    }

    out.push_str("}\n\n");
    out
}

fn render_generic_delegate(delegate: &GenericDelegateDeclaration) -> String {
    let mut out = String::new();
    let generic_params =
        generic_parameter_names(delegate.full_name(), delegate.number_of_generic_parameters());
    let name = declaration_display_name(delegate.full_name());
    let generic_suffix = if generic_params.is_empty() {
        String::new()
    } else {
        format!("<{}>", generic_params.join(", "))
    };
    let invoke = delegate.invoke_method();
    out.push_str(&format!(
        "type {}{} = {};\n\n",
        name,
        generic_suffix,
        method_signature_with_generics(invoke, &generic_params, true)
    ));
    out
}

// ---------------------------------------------------------------------------
// Namespace / declaration utilities
// ---------------------------------------------------------------------------

fn declaration_namespace(full_name: &str) -> String {
    match full_name.rfind('.') {
        Some(index) => full_name[..index].to_string(),
        None => String::new(),
    }
}

fn declaration_simple_name(full_name: &str) -> String {
    match full_name.rfind('.') {
        Some(index) => full_name[index + 1..].to_string(),
        None => full_name.to_string(),
    }
}

fn join_namespace_child(current: &str, child: &str) -> String {
    if current.is_empty() {
        return child.to_string();
    }
    if child.starts_with(current) {
        return child.to_string();
    }
    format!("{}.{}", current, child)
}

fn is_in_requested_root(full_name: &str, root: &str) -> bool {
    if root.is_empty() {
        return true;
    }
    full_name == root || full_name.starts_with(&format!("{}.", root))
}

// ---------------------------------------------------------------------------
// Namespace BFS walker (handles sub-namespace hierarchy)
// ---------------------------------------------------------------------------

fn walk_namespace(root: &str) -> BTreeMap<String, Vec<String>> {
    let mut modules: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut queue = VecDeque::new();
    let mut visited = BTreeSet::new();

    if root.is_empty() {
        queue.push_back(String::new());
    } else {
        queue.push_back(root.to_string());
        queue.push_back(String::new());
    }

    while let Some(current) = queue.pop_front() {
        if !visited.insert(current.clone()) {
            continue;
        }

        let Some(dec) = MetadataReader::find_by_name(&current) else {
            continue;
        };

        let lock = dec.read();

        match lock.kind() {
            DeclarationKind::Namespace => {
                if let Some(ns) = lock.as_any().downcast_ref::<NamespaceDeclaration>() {
                    for child in ns.children() {
                        let fq = join_namespace_child(current.as_str(), child.as_str());
                        if is_in_requested_root(fq.as_str(), root) {
                            queue.push_back(fq);
                        }
                    }
                }
            }
            DeclarationKind::Class => {
                if let Some(item) = lock.as_any().downcast_ref::<ClassDeclaration>() {
                    if !is_in_requested_root(item.full_name(), root) {
                        continue;
                    }
                    let ns = declaration_namespace(item.full_name());
                    let name = declaration_simple_name(item.full_name());
                    modules
                        .entry(ns)
                        .or_default()
                        .push(render_class(name.as_str(), item));
                    // Exclusive interfaces (marked [ExclusiveTo]) are not enumerated by
                    // RoResolveNamespace, so the BFS never visits them.  Enqueue them here
                    // so they get declared and the `implements` references resolve.
                    for iface in item.implemented_interfaces() {
                        let iface_name = iface.full_name().to_string();
                        if is_in_requested_root(&iface_name, root) && !visited.contains(&iface_name) {
                            queue.push_back(iface_name);
                        }
                    }
                }
            }
            DeclarationKind::Interface => {
                if let Some(item) = lock.as_any().downcast_ref::<InterfaceDeclaration>() {
                    if !is_in_requested_root(item.full_name(), root) {
                        continue;
                    }
                    let ns = declaration_namespace(item.full_name());
                    let name = declaration_simple_name(item.full_name());
                    modules
                        .entry(ns)
                        .or_default()
                        .push(render_interface(name.as_str(), item));
                    // Also queue base interfaces (may include exclusive ones).
                    for iface in item.implemented_interfaces() {
                        let iface_name = iface.full_name().to_string();
                        if is_in_requested_root(&iface_name, root) && !visited.contains(&iface_name) {
                            queue.push_back(iface_name);
                        }
                    }
                }
            }
            DeclarationKind::GenericInterface => {
                if let Some(item) = lock.as_any().downcast_ref::<GenericInterfaceDeclaration>() {
                    if !is_in_requested_root(item.full_name(), root) {
                        continue;
                    }
                    let ns = declaration_namespace(item.full_name());
                    modules
                        .entry(ns)
                        .or_default()
                        .push(render_generic_interface(item));
                }
            }
            DeclarationKind::Enum => {
                if let Some(item) = lock.as_any().downcast_ref::<EnumDeclaration>() {
                    if !is_in_requested_root(item.full_name(), root) {
                        continue;
                    }
                    let ns = declaration_namespace(item.full_name());
                    let name = declaration_simple_name(item.full_name());
                    modules
                        .entry(ns)
                        .or_default()
                        .push(render_enum(name.as_str(), item));
                }
            }
            DeclarationKind::Struct => {
                if let Some(item) = lock.as_any().downcast_ref::<StructDeclaration>() {
                    if !is_in_requested_root(item.full_name(), root) {
                        continue;
                    }
                    let ns = declaration_namespace(item.full_name());
                    let name = declaration_simple_name(item.full_name());
                    modules
                        .entry(ns)
                        .or_default()
                        .push(render_struct(name.as_str(), item));
                }
            }
            DeclarationKind::Delegate => {
                if let Some(item) = lock.as_any().downcast_ref::<DelegateDeclaration>() {
                    if !is_in_requested_root(item.full_name(), root) {
                        continue;
                    }
                    let ns = declaration_namespace(item.full_name());
                    let name = declaration_simple_name(item.full_name());
                    modules
                        .entry(ns)
                        .or_default()
                        .push(render_delegate(name.as_str(), item));
                }
            }
            DeclarationKind::GenericDelegate => {
                if let Some(item) = lock.as_any().downcast_ref::<GenericDelegateDeclaration>() {
                    if !is_in_requested_root(item.full_name(), root) {
                        continue;
                    }
                    let ns = declaration_namespace(item.full_name());
                    modules
                        .entry(ns)
                        .or_default()
                        .push(render_generic_delegate(item));
                }
            }
            _ => {}
        }
    }

    modules
}

// ---------------------------------------------------------------------------
// Metadata-API based type enumeration (replaces fragile regex scan)
// ---------------------------------------------------------------------------

/// Enumerates all TypeDef names in an already-open metadata scope that fall
/// under `root`.  Uses IMetaDataImport2::EnumTypeDefs which is authoritative
/// and handles all edge cases the regex binary scan missed (e.g. IClosable).
fn enumerate_from_metadata(metadata: &IMetaDataImport2, root: &str) -> BTreeSet<String> {
    let mut candidates = BTreeSet::new();
    let mut enumerator = std::ptr::null_mut();

    loop {
        let mut tokens = [0u32; 256];
        let mut fetched = 0u32;
        let result = unsafe {
            metadata.EnumTypeDefs(
                &mut enumerator,
                tokens.as_mut_ptr(),
                tokens.len() as u32,
                &mut fetched,
            )
        };
        if result.is_err() || fetched == 0 {
            break;
        }
        for &token in tokens[..fetched as usize].iter() {
            let name = get_type_name(metadata, CorTokenType(token as i32));
            if !name.is_empty() && is_in_requested_root(&name, root) {
                candidates.insert(name);
            }
        }
    }

    if !enumerator.is_null() {
        unsafe { metadata.CloseEnum(enumerator) };
    }

    candidates
}

/// Returns the set of well-known anchor type names for a given namespace root.
/// Used to locate an IMetaDataImport2 scope without having a direct file path.
fn well_known_anchors_for_root(root: &str) -> Vec<String> {
    let mut anchors = vec![root.to_string()];

    for suffix in [
        "Deferral",
        "Uri",
        "PropertyValue",
        "IIterable`1",
        "IIterator`1",
        "IVector`1",
        "IVectorView`1",
        "IMap`2",
        "IMapView`2",
        "IAsyncAction",
        "IAsyncOperation`1",
        "IReference`1",
        "EventHandler`1",
        "TypedEventHandler`2",
        "PropertySet",
        "StringMap",
        "ValueSet",
        "IClosable",
        "IStringable",
    ] {
        anchors.push(format!("{}.{}", root, suffix));
    }

    anchors
}

/// Attempts to obtain an IMetaDataImport2 scope by resolving a known anchor
/// type via the system WinRT metadata resolver.
fn metadata_from_anchor(anchor_name: &str) -> Option<IMetaDataImport2> {
    let anchor = MetadataReader::find_by_name(anchor_name)?;
    let lock = anchor.read();

    match lock.kind() {
        DeclarationKind::GenericInterface => lock
            .as_any()
            .downcast_ref::<GenericInterfaceDeclaration>()
            .and_then(|item| item.base().metadata().cloned()),
        DeclarationKind::Interface => lock
            .as_any()
            .downcast_ref::<InterfaceDeclaration>()
            .and_then(|item| item.base().metadata().cloned()),
        DeclarationKind::Class => lock
            .as_any()
            .downcast_ref::<ClassDeclaration>()
            .and_then(|item| item.base().metadata().cloned()),
        DeclarationKind::Delegate => lock
            .as_any()
            .downcast_ref::<DelegateDeclaration>()
            .and_then(|item| item.base().metadata().cloned()),
        DeclarationKind::GenericDelegate => lock
            .as_any()
            .downcast_ref::<GenericDelegateDeclaration>()
            .and_then(|item| item.base().metadata().cloned()),
        _ => None,
    }
}

/// Collects all type names matching `root` from a single file.
///
/// Strategy (in priority order):
/// 1. Open via `IMetaDataDispenserEx::OpenScope` — works for any CLI file.
/// 2. Anchor lookup using the file's namespace stem — for registered WinRT types.
/// 3. Regex scan of raw bytes — legacy fallback.
fn collect_candidates_from_file(path: &PathBuf, root: &str) -> BTreeSet<String> {
    // Strategy 1: Direct file open via IMetaDataDispenserEx.
    if let Some(metadata) = metadata::open_metadata_scope_from_file(path) {
        return enumerate_from_metadata(&metadata, root);
    }

    // Strategy 2: Anchor-based lookup.  Works for registered WinRT types without
    // needing CoCreateInstance(CorMetaDataDispenser).  Use the file stem (e.g.,
    // "Windows.Foundation") to build a richer anchor list than the root alone.
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();

    let mut all_anchors = well_known_anchors_for_root(&stem);
    // Also try root-qualified anchors so cross-namespace lookups succeed.
    all_anchors.extend(well_known_anchors_for_root(root));

    for anchor in &all_anchors {
        if let Some(metadata) = metadata_from_anchor(anchor) {
            return enumerate_from_metadata(&metadata, root);
        }
    }

    // Strategy 3: Regex scan — last resort for files that resist both approaches.
    scan_winmd_candidates(path, root)
}

/// Regex-based binary scan (legacy fallback).  Reads the file bytes and looks
/// for strings matching the namespace root pattern in both UTF-8 and UTF-16-LE.
fn scan_winmd_candidates(path: &PathBuf, root: &str) -> BTreeSet<String> {
    let mut candidates = BTreeSet::new();

    let Ok(bytes) = fs::read(path) else {
        return candidates;
    };

    let text = String::from_utf8_lossy(&bytes);
    let pattern = format!(
        r"{}(?:\.[A-Za-z_][A-Za-z0-9_`]*)+",
        regex::escape(root)
    );

    let Ok(re) = Regex::new(pattern.as_str()) else {
        return candidates;
    };

    for found in re.find_iter(&text) {
        candidates.insert(found.as_str().to_string());
    }

    let utf16 = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>();
    let utf16_text = String::from_utf16_lossy(&utf16);

    for found in re.find_iter(&utf16_text) {
        candidates.insert(found.as_str().to_string());
    }

    candidates
}

// ---------------------------------------------------------------------------
// Declaration renderer dispatcher
// ---------------------------------------------------------------------------

fn append_rendered_declaration(
    lock: &dyn Declaration,
    modules: &mut BTreeMap<String, Vec<String>>,
) {
    match lock.kind() {
        DeclarationKind::Class => {
            if let Some(item) = lock.as_any().downcast_ref::<ClassDeclaration>() {
                let ns = declaration_namespace(item.full_name());
                let name = declaration_simple_name(item.full_name());
                modules
                    .entry(ns)
                    .or_default()
                    .push(render_class(name.as_str(), item));
            }
        }
        DeclarationKind::Interface => {
            if let Some(item) = lock.as_any().downcast_ref::<InterfaceDeclaration>() {
                let ns = declaration_namespace(item.full_name());
                let name = declaration_simple_name(item.full_name());
                modules
                    .entry(ns)
                    .or_default()
                    .push(render_interface(name.as_str(), item));
            }
        }
        DeclarationKind::GenericInterface => {
            if let Some(item) = lock.as_any().downcast_ref::<GenericInterfaceDeclaration>() {
                let ns = declaration_namespace(item.full_name());
                modules
                    .entry(ns)
                    .or_default()
                    .push(render_generic_interface(item));
            }
        }
        DeclarationKind::Enum => {
            if let Some(item) = lock.as_any().downcast_ref::<EnumDeclaration>() {
                let ns = declaration_namespace(item.full_name());
                let name = declaration_simple_name(item.full_name());
                modules
                    .entry(ns)
                    .or_default()
                    .push(render_enum(name.as_str(), item));
            }
        }
        DeclarationKind::Struct => {
            if let Some(item) = lock.as_any().downcast_ref::<StructDeclaration>() {
                let ns = declaration_namespace(item.full_name());
                let name = declaration_simple_name(item.full_name());
                modules
                    .entry(ns)
                    .or_default()
                    .push(render_struct(name.as_str(), item));
            }
        }
        DeclarationKind::Delegate => {
            if let Some(item) = lock.as_any().downcast_ref::<DelegateDeclaration>() {
                let ns = declaration_namespace(item.full_name());
                let name = declaration_simple_name(item.full_name());
                modules
                    .entry(ns)
                    .or_default()
                    .push(render_delegate(name.as_str(), item));
            }
        }
        DeclarationKind::GenericDelegate => {
            if let Some(item) = lock.as_any().downcast_ref::<GenericDelegateDeclaration>() {
                let ns = declaration_namespace(item.full_name());
                modules
                    .entry(ns)
                    .or_default()
                    .push(render_generic_delegate(item));
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// File / library discovery
// ---------------------------------------------------------------------------

/// Returns all .winmd paths from the system WinMetadata directory.
fn windows_winmd_paths() -> Vec<PathBuf> {
    let dir = PathBuf::from(r"C:\Windows\System32\WinMetadata");
    if !dir.exists() {
        return Vec::new();
    }
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("winmd"))
                .unwrap_or(false)
        })
        .collect()
}

/// Discovers roots from a single input path (C# source, csproj, dll, or winmd).
fn discover_roots_from_input(path: &PathBuf) -> Vec<String> {
    let mut roots = Vec::new();

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if ext == "cs" {
        if let Ok(contents) = fs::read_to_string(path) {
            for line in contents.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("namespace ") {
                    let ns = rest.trim().trim_end_matches('{').trim().to_string();
                    if !ns.is_empty() {
                        roots.push(ns);
                    }
                }
                if let Some(rest) = trimmed.strip_prefix("using ") {
                    let ns = rest.trim().trim_end_matches(';').trim().to_string();
                    if ns.starts_with("Windows") {
                        roots.push(ns);
                    }
                }
            }
        }
    } else if ext == "csproj" {
        if let Ok(contents) = fs::read_to_string(path) {
            if let Some(start) = contents.find("<RootNamespace>") {
                let tag = "<RootNamespace>";
                if let Some(end) = contents[start + tag.len()..].find("</RootNamespace>") {
                    let value = &contents[start + tag.len()..start + tag.len() + end];
                    let value = value.trim().to_string();
                    if !value.is_empty() {
                        roots.push(value);
                    }
                }
            }
        }
    } else if ext == "dll" || ext == "winmd" {
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            if !stem.is_empty() {
                roots.push(stem.to_string());
            }
        }
    }

    roots
}

/// Expands a lib path into concrete scannable file paths.
///
/// - A `.dll` or `.winmd` file is returned as-is.
/// - A `.nupkg` is a ZIP archive: DLLs in `lib/net*/` sub-directories are extracted
///   to a temp location and returned.
/// - A directory is scanned recursively for `.dll` and `.winmd` files.
fn expand_lib_path(path: &PathBuf) -> Vec<PathBuf> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if ext == "dll" || ext == "winmd" {
        return vec![path.clone()];
    }

    if ext == "nupkg" {
        return expand_nupkg(path);
    }

    if path.is_dir() {
        return scan_dir_for_libs(path);
    }

    Vec::new()
}

/// Extracts DLLs from a NuGet package (.nupkg is a ZIP) into a temp directory
/// and returns their paths.
fn expand_nupkg(nupkg: &PathBuf) -> Vec<PathBuf> {
    use std::io::{Read, Write};

    let Ok(file) = fs::File::open(nupkg) else {
        eprintln!("warning: could not open NuGet package {}", nupkg.display());
        return Vec::new();
    };

    let mut archive = match zip::ZipArchive::new(file) {
        Ok(a) => a,
        Err(e) => {
            eprintln!(
                "warning: {} is not a valid NuGet package: {}",
                nupkg.display(),
                e
            );
            return Vec::new();
        }
    };

    // Extract to a temp directory named after the package.
    let stem = nupkg.file_stem().and_then(|s| s.to_str()).unwrap_or("pkg");
    let tmp_dir = std::env::temp_dir().join(format!("nswrt_nupkg_{}", stem));
    let _ = fs::create_dir_all(&tmp_dir);

    let mut extracted = Vec::new();

    for i in 0..archive.len() {
        let mut entry = match archive.by_index(i) {
            Ok(e) => e,
            Err(_) => continue,
        };

        let name = entry.name().to_lowercase();
        // Only extract DLLs from the lib/ tree (covers netX, netstandard, etc.)
        if !name.starts_with("lib/") {
            continue;
        }
        let entry_ext = std::path::Path::new(entry.name())
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if entry_ext != "dll" && entry_ext != "winmd" {
            continue;
        }

        // Flatten to the tmp_dir (just the filename).
        let file_name = std::path::Path::new(entry.name())
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if file_name.is_empty() {
            continue;
        }

        let out_path = tmp_dir.join(&file_name);
        if let Ok(mut out_file) = fs::File::create(&out_path) {
            let mut buf = Vec::new();
            if entry.read_to_end(&mut buf).is_ok() {
                let _ = out_file.write_all(&buf);
                extracted.push(out_path);
            }
        }
    }

    extracted
}

/// Recursively scans a directory for .dll and .winmd files.
fn scan_dir_for_libs(dir: &PathBuf) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut result = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            result.extend(scan_dir_for_libs(&path));
        } else {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if ext == "dll" || ext == "winmd" {
                result.push(path);
            }
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Module renderer
// ---------------------------------------------------------------------------

fn render_modules(modules: &BTreeMap<String, Vec<String>>) -> String {
    let mut out = String::new();

    for (namespace, declarations) in modules {
        if declarations.is_empty() {
            continue;
        }

        if namespace.is_empty() {
            for item in declarations {
                out.push_str(item);
            }
            continue;
        }

        out.push_str(&format!("declare namespace {} {{\n", namespace));
        for item in declarations {
            for line in item.lines() {
                if line.is_empty() {
                    out.push('\n');
                } else {
                    out.push_str("  ");
                    out.push_str(line);
                    out.push('\n');
                }
            }
        }
        out.push_str("}\n\n");
    }

    out
}

// ---------------------------------------------------------------------------
// Scope extraction helpers
// ---------------------------------------------------------------------------

/// Returns a cloned `IMetaDataImport2` scope from any declaration that carries one.
/// Used to enumerate all types in the same WinMD file when a sibling was found.
fn extract_metadata_scope(lock: &dyn Declaration) -> Option<IMetaDataImport2> {
    match lock.kind() {
        DeclarationKind::Class => lock
            .as_any()
            .downcast_ref::<ClassDeclaration>()
            .and_then(|item| item.base().metadata().cloned()),
        DeclarationKind::Interface => lock
            .as_any()
            .downcast_ref::<InterfaceDeclaration>()
            .and_then(|item| item.base().metadata().cloned()),
        DeclarationKind::Struct => lock
            .as_any()
            .downcast_ref::<StructDeclaration>()
            .and_then(|item| item.metadata().cloned()),
        DeclarationKind::Enum => lock
            .as_any()
            .downcast_ref::<EnumDeclaration>()
            .and_then(|item| item.metadata().cloned()),
        DeclarationKind::Delegate => lock
            .as_any()
            .downcast_ref::<DelegateDeclaration>()
            .and_then(|item| item.base().metadata().cloned()),
        DeclarationKind::GenericDelegate => lock
            .as_any()
            .downcast_ref::<GenericDelegateDeclaration>()
            .and_then(|item| item.base().metadata().cloned()),
        DeclarationKind::GenericInterface => lock
            .as_any()
            .downcast_ref::<GenericInterfaceDeclaration>()
            .and_then(|item| item.base().metadata().cloned()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Augmentation: fills in types that the namespace BFS did not find
// ---------------------------------------------------------------------------

/// Collects additional scan paths based on roots and explicit lib paths.
fn collect_scan_paths(roots: &[String], input: Option<&PathBuf>, lib_paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut scan_paths = Vec::new();

    // Explicit --input file (winmd or dll treated as a source of types).
    if let Some(path) = input {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext == "winmd" || ext == "dll" {
            scan_paths.push(path.clone());
        }
    }

    // Explicit --lib paths.
    scan_paths.extend(lib_paths.iter().cloned());

    // System WinMetadata directory for any Windows.* roots.
    for root in roots {
        if root == "Windows" || root.starts_with("Windows.") {
            for path in windows_winmd_paths() {
                scan_paths.push(path);
            }
        }
    }

    scan_paths.sort();
    scan_paths.dedup();
    scan_paths
}

/// Augments `modules` with types discovered by scanning metadata files.
/// Replaces the legacy enumerate-then-regex approach with a unified
/// `collect_candidates_from_file` strategy that prefers the metadata API.
fn augment_modules_from_files(
    roots: &[String],
    input: Option<&PathBuf>,
    lib_paths: &[PathBuf],
    modules: &mut BTreeMap<String, Vec<String>>,
) {
    let scan_paths = collect_scan_paths(roots, input, lib_paths);

    for root in roots {
        let mut pending: VecDeque<String> = VecDeque::new();
        let mut visited: BTreeSet<String> = BTreeSet::new();
        // Candidates discovered by the initial file scan (not from scope re-enumeration).
        // Only these trigger a scope-level sibling enumeration so we don't re-scan the
        // same WinMD file dozens of times once its structs/enums are in the queue.
        let mut from_file_scan: BTreeSet<String> = BTreeSet::new();

        for path in &scan_paths {
            for candidate in collect_candidates_from_file(path, root) {
                from_file_scan.insert(candidate.clone());
                pending.push_back(candidate);
            }
        }

        while let Some(candidate) = pending.pop_front() {
            if !visited.insert(candidate.clone()) {
                continue;
            }
            let Some(dec) = MetadataReader::find_by_name(&candidate) else { continue };
            let lock = dec.read();

            // When a candidate came from the file scan (not a sibling we added below),
            // enumerate ALL TypeDefs in its metadata scope.  This catches structs, enums,
            // and other types that the regex file scan cannot find as contiguous strings
            // (because WinMD stores TypeName and TypeNamespace separately).
            if from_file_scan.contains(&candidate) {
                if let Some(scope) = extract_metadata_scope(&*lock) {
                    for sibling in enumerate_from_metadata(&scope, root) {
                        if !visited.contains(&sibling) {
                            pending.push_back(sibling);
                        }
                    }
                }
            }

            // Enqueue exclusive interfaces (not returned by RoResolveNamespace).
            match lock.kind() {
                DeclarationKind::Class => {
                    if let Some(item) = lock.as_any().downcast_ref::<ClassDeclaration>() {
                        for iface in item.implemented_interfaces() {
                            let n = iface.full_name().to_string();
                            if is_in_requested_root(&n, root) && !visited.contains(&n) {
                                pending.push_back(n);
                            }
                        }
                    }
                }
                DeclarationKind::Interface => {
                    if let Some(item) = lock.as_any().downcast_ref::<InterfaceDeclaration>() {
                        for iface in item.implemented_interfaces() {
                            let n = iface.full_name().to_string();
                            if is_in_requested_root(&n, root) && !visited.contains(&n) {
                                pending.push_back(n);
                            }
                        }
                    }
                }
                _ => {}
            }

            append_rendered_declaration(&*lock, modules);
        }
    }
}

// ---------------------------------------------------------------------------
// CLI argument parsing
// ---------------------------------------------------------------------------

struct GeneratorConfig {
    roots: Vec<String>,
    out: PathBuf,
    /// When set, write one `.d.ts` file per top-level namespace into this directory
    /// instead of a single combined file.
    out_dir: Option<PathBuf>,
    input: Option<PathBuf>,
    /// Explicit extra library paths (--lib / --libs).
    lib_paths: Vec<PathBuf>,
}

fn parse_args() -> GeneratorConfig {
    let mut args = env::args().skip(1);

    let mut roots = vec!["Windows".to_string()];
    let mut out = PathBuf::from("windows-runtime.generated.d.ts");
    let mut out_dir: Option<PathBuf> = None;
    let mut input: Option<PathBuf> = None;
    let mut lib_paths: Vec<PathBuf> = Vec::new();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" => {
                if let Some(value) = args.next() {
                    roots = vec![value];
                }
            }
            "--roots" => {
                if let Some(value) = args.next() {
                    roots = value
                        .split(',')
                        .map(|v| v.trim().to_string())
                        .filter(|v| !v.is_empty())
                        .collect();
                }
            }
            "--out" => {
                if let Some(value) = args.next() {
                    out = PathBuf::from(value);
                }
            }
            "--out-dir" => {
                if let Some(value) = args.next() {
                    out_dir = Some(PathBuf::from(value));
                }
            }
            "--input" => {
                if let Some(value) = args.next() {
                    input = Some(PathBuf::from(value));
                }
            }
            // --lib <path>  (single path: .dll, .winmd, .nupkg, or directory)
            "--lib" => {
                if let Some(value) = args.next() {
                    let path = PathBuf::from(&value);
                    let expanded = expand_lib_path(&path);
                    if expanded.is_empty() {
                        // Treat as a directory or single file not matched above.
                        lib_paths.push(path);
                    } else {
                        lib_paths.extend(expanded);
                    }
                }
            }
            // --libs <comma-separated paths>
            "--libs" => {
                if let Some(value) = args.next() {
                    for part in value.split(',') {
                        let path = PathBuf::from(part.trim());
                        let expanded = expand_lib_path(&path);
                        if expanded.is_empty() {
                            lib_paths.push(path);
                        } else {
                            lib_paths.extend(expanded);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // Discover extra roots from the input file.
    if let Some(path) = input.as_ref() {
        let discovered = discover_roots_from_input(path);
        if !discovered.is_empty() {
            roots.extend(discovered);
        }
    }

    // Discover roots from lib paths (e.g., MyLib.dll → add "MyLib" root).
    for path in &lib_paths {
        let discovered = discover_roots_from_input(path);
        roots.extend(discovered);
    }

    roots.sort();
    roots.dedup();

    GeneratorConfig { roots, out, out_dir, input, lib_paths }
}

// ---------------------------------------------------------------------------
// Output helpers
// ---------------------------------------------------------------------------

const PREAMBLE: &str = concat!(
    "// Auto-generated by typings-generator\n",
    "// Experimental: incomplete WinRT projection coverage\n\n",
    "/** Projected WinRT GUID value struct. toString()/valueOf() return the standard\n",
    " *  {XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX} string form. */\n",
    "interface Guid {\n",
    "  /** High 32 bits */\n",
    "  data1: number;\n",
    "  /** Bits 32-47 */\n",
    "  data2: number;\n",
    "  /** Bits 48-63 */\n",
    "  data3: number;\n",
    "  /** Low 64 bits as 8 individual bytes */\n",
    "  data4: number[];\n",
    "  toString(): string;\n",
    "  valueOf(): string;\n",
    "}\n\n",
);

/// Returns the "file group" key for a namespace: the second-level prefix, e.g.
/// `Windows.Foundation` for `Windows.Foundation.Collections` or for `Windows.Foundation`.
/// Returns the full name for namespaces with fewer than two segments.
fn namespace_file_group(namespace: &str) -> String {
    if namespace.is_empty() {
        return String::new();
    }
    let parts: Vec<&str> = namespace.splitn(3, '.').collect();
    if parts.len() >= 2 {
        format!("{}.{}", parts[0], parts[1])
    } else {
        namespace.to_string()
    }
}

fn build_file_header(config: &GeneratorConfig) -> String {
    let mut h = String::new();
    if let Some(path) = &config.input {
        h.push_str(&format!("// Input: {}\n", path.display()));
    }
    for path in &config.lib_paths {
        h.push_str(&format!("// Lib: {}\n", path.display()));
    }
    h.push_str(&format!("// Roots: {}\n\n", config.roots.join(", ")));
    h
}

fn write_single_output(
    out: &PathBuf,
    modules: &BTreeMap<String, Vec<String>>,
    config: &GeneratorConfig,
) {
    let mut body = String::from(PREAMBLE);
    body.push_str(&build_file_header(config));

    let rendered = render_modules(modules);
    body.push_str(&rendered);
    if rendered.is_empty() {
        body.push_str("// No declarations were generated for the selected root namespace.\n");
    }

    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            let _ = fs::create_dir_all(parent);
        }
    }
    fs::write(out, body).expect("failed to write generated typings");
    println!("Generated typings: {}", out.display());
}

fn write_split_output(
    out_dir: &PathBuf,
    modules: &BTreeMap<String, Vec<String>>,
    config: &GeneratorConfig,
) {
    let _ = fs::create_dir_all(out_dir);
    let header = build_file_header(config);

    // Group namespaces by their second-level prefix (file group).
    let mut groups: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    for (ns, decls) in modules {
        let group = namespace_file_group(ns);
        groups
            .entry(group)
            .or_default()
            .insert(ns.clone(), decls.clone());
    }

    let mut file_count = 0usize;
    for (group, group_modules) in &groups {
        let file_name = if group.is_empty() {
            "globals.d.ts".to_string()
        } else {
            format!("{}.d.ts", group)
        };
        let out_path = out_dir.join(&file_name);

        let mut body = String::from(PREAMBLE);
        body.push_str(&header);

        let rendered = render_modules(group_modules);
        body.push_str(&rendered);
        if rendered.is_empty() {
            continue;
        }

        fs::write(&out_path, body).expect("failed to write split typings file");
        file_count += 1;
    }

    println!(
        "Generated {} typings files in: {}",
        file_count,
        out_dir.display()
    );
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    // Initialize COM MTA so that CoCreateInstance(CLSID_CorMetaDataDispenser) works
    // in OpenMetadataScope (used by open_metadata_scope_from_file in Phase 2).
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok() };

    let config = parse_args();

    // Phase 1: BFS namespace walk — discovers types reachable via RoResolveNamespace.
    let mut modules: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for root in &config.roots {
        let generated = walk_namespace(root.as_str());
        for (namespace, mut declarations) in generated {
            modules.entry(namespace).or_default().append(&mut declarations);
        }
    }

    // Phase 2: Metadata file scan — picks up types (like IClosable) that are not
    // namespaces themselves and therefore never appear in the BFS queue.
    augment_modules_from_files(
        &config.roots,
        config.input.as_ref(),
        &config.lib_paths,
        &mut modules,
    );

    // Deduplicate per namespace.
    for declarations in modules.values_mut() {
        declarations.sort();
        declarations.dedup();
    }

    if let Some(out_dir) = &config.out_dir {
        write_split_output(out_dir, &modules, &config);
    } else {
        write_single_output(&config.out, &modules, &config);
    }
}
