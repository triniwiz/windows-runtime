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
use metadata::declarations::enum_declaration::EnumDeclaration;
use metadata::declarations::interface_declaration::generic_interface_declaration::GenericInterfaceDeclaration;
use metadata::declarations::interface_declaration::InterfaceDeclaration;
use metadata::declarations::namespace_declaration::NamespaceDeclaration;
use metadata::declarations::struct_declaration::StructDeclaration;
use metadata::meta_data_reader::MetadataReader;
use metadata::prelude::get_type_name;
use metadata::signature::Signature;
use metadata::value::Value;
use windows::Win32::System::WinRT::Metadata::{CorTokenType, IMetaDataImport2};

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

    // Strip generic arguments to get the base type name for matching
    let base = if let Some(idx) = value.find('<') { &value[..idx] } else { value };
    // Strip arity for matching (e.g. "IVector`1" -> "IVector")
    let base_no_arity = if let Some(idx) = base.find('`') { &base[..idx] } else { base };

    if let Some(index) = value.strip_prefix("Var!").and_then(|rest| rest.parse::<usize>().ok()) {
        if let Some(name) = generic_params.get(index) {
            return name.clone();
        }
    }

    // Primitives
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
        // WinRT value structs — projected as inline interface shapes
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

    // IAsyncOperation<T> / IAsyncAction<T> -> Promise<T>
    if base_no_arity == "IAsyncOperation" || base_no_arity.ends_with(".IAsyncOperation") || 
       base_no_arity == "IAsyncAction" || base_no_arity.ends_with(".IAsyncAction") {
        if let Some(inner) = value.find('<').and_then(|s| {
            let inner = &value[s + 1..value.len().saturating_sub(1)];
            if inner.is_empty() { None } else { Some(inner) }
        }) {
            return format!("Promise<{}>", map_type_to_ts_with_generics(inner, generic_params));
        }
        return "Promise<unknown>".to_string();
    }

    // IVector<T> / IReadOnlyList<T> / IIterable<T> -> T[]
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

    // IMap<K,V> / IReadOnlyDictionary<K,V> -> Record<K, V>
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

    // Fully-qualified WinRT reference type usage: return the name with arity stripped.
    if let Some(idx) = value.find('`') {
        return value[..idx].to_string();
    }

    if !value.is_empty() && value.chars().next().unwrap().is_alphabetic() {
        return value.to_string();
    }

    "unknown".to_string()
}

fn method_signature(method: &metadata::declarations::method_declaration::MethodDeclaration, use_arrow: bool) -> String {
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
            let name = if p.name().is_empty() { "arg" } else { p.name() };
            let param_ty = p
                .metadata()
                .map(|m| Signature::to_string(m, &p.type_()))
                .unwrap_or_else(|| "Object".to_string());
            let rendered = if !generic_params.is_empty()
                && (
                    (method_name == "GetMany"
                        && ((total_params == 1 && index == 0) || (total_params >= 2 && index + 1 == total_params)))
                        || (method_name == "ReplaceAll" && index == 0)
                        || (name == "items" && (method_name == "GetMany" || method_name == "ReplaceAll"))
                )
            {
                format!("{}[]", generic_params[0])
            } else if method_name == "GetMany" && total_params >= 2 && index == 0 {
                "number".to_string()
            } else {
                map_type_to_ts_with_generics(param_ty.as_str(), generic_params)
            };
            format!(
                "{}: {}",
                name,
                rendered
            )
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

fn event_type_name(event: &metadata::declarations::event_declaration::EventDeclaration) -> String {
    event
        .type_()
        .map(|delegate| declaration_display_name(delegate.full_name()))
        .unwrap_or_else(|| "unknown".to_string())
}

fn render_interface(name: &str, interface: &InterfaceDeclaration) -> String {
    let mut out = String::new();
    let extends = interface_extends_clause(interface.implemented_interfaces(), &[]);
    out.push_str(&format!("interface {}{} {{\n", name, extends));

    let mut methods = interface.methods().iter().filter(|m| m.is_exported()).collect::<Vec<_>>();
    methods.sort_by_key(|m| m.name());

    for method in methods {
        out.push_str(&format!("  {}{};\n", method.name(), method_signature(method, false)));
    }

    let mut properties = interface.properties().iter().filter(|p| p.is_exported()).collect::<Vec<_>>();
    properties.sort_by_key(|p| p.name());

    for prop in properties {
        let return_ty = Signature::to_string(prop.getter().metadata().unwrap(), &prop.getter().return_type());
        out.push_str(&format!("  {}: {};\n", prop.name(), map_type_to_ts(return_ty.as_str())));
    }

    for event in interface.events().iter().filter(|e| e.is_exported()) {
        out.push_str(&format!("  {}: {};\n", event.name(), event_type_name(event)));
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

    if class_decl.is_instantiable() {
        out.push_str("  constructor(...args: unknown[]);\n");
    }

    let methods = class_decl.methods().iter().filter(|m| m.is_exported()).collect::<Vec<_>>();
    let mut methods = class_decl.methods().iter().filter(|m| m.is_exported()).collect::<Vec<_>>();
    methods.sort_by_key(|m| m.name());

    for method in methods {
        let sig = method_signature(method, false);
        if method.is_static() {
            out.push_str(&format!("  static {}{};\n", method.name(), sig));
        } else {
            out.push_str(&format!("  {}{};\n", method.name(), sig));
        }
    }

    let mut properties = class_decl.properties().iter().filter(|p| p.is_exported()).collect::<Vec<_>>();
    properties.sort_by_key(|p| p.name());

    for prop in properties {
        let return_ty = Signature::to_string(prop.getter().metadata().unwrap(), &prop.getter().return_type());
        if prop.is_static() {
            out.push_str(&format!("  static {}: {};\n", prop.name(), map_type_to_ts(return_ty.as_str())));
        } else {
            out.push_str(&format!("  {}: {};\n", prop.name(), map_type_to_ts(return_ty.as_str())));
        }
    }

    for event in class_decl.events().iter().filter(|e| e.is_exported()) {
        if event.is_static() {
            out.push_str(&format!("  static {}: {};\n", event.name(), event_type_name(event)));
        } else {
            out.push_str(&format!("  {}: {};\n", event.name(), event_type_name(event)));
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
        Value::Boolean(v) => {
            if v { "1".to_string() } else { "0".to_string() }
        }
        _ => "0".to_string(),
    }
}

fn render_enum(name: &str, enum_decl: &EnumDeclaration) -> String {
    let mut out = String::new();
    out.push_str(&format!("enum {} {{\n", name));
    for member in enum_decl.enums() {
        out.push_str(&format!("  {} = {},\n", member.name(), enum_value_to_string(member.value())));
    }
    out.push_str("}\n\n");
    out
}

fn render_struct(name: &str, struct_decl: &StructDeclaration) -> String {
    let mut out = String::new();
    out.push_str(&format!("interface {} {{\n", name));
    for field in struct_decl.fields() {
        let field_ty = Signature::to_string(field.base().metadata().unwrap(), &field.type_());
        out.push_str(&format!("  {}: {};\n", field.name(), map_type_to_ts(field_ty.as_str())));
    }
    out.push_str("}\n\n");
    out
}

fn render_delegate(name: &str, delegate: &DelegateDeclaration) -> String {
    let mut out = String::new();
    let invoke = delegate.invoke_method();
    out.push_str(&format!("type {} = {};\n\n", name, method_signature(invoke, true)));
    out
}

fn render_generic_interface(interface: &GenericInterfaceDeclaration) -> String {
    let mut out = String::new();
    let generic_params = generic_parameter_names(
        interface.full_name(),
        interface.number_of_generic_parameters(),
    );
    let name = declaration_display_name(interface.full_name());
    let generic_suffix = if generic_params.is_empty() {
        String::new()
    } else {
        format!("<{}>", generic_params.join(", "))
    };
    let extends = interface_extends_clause(interface.implemented_interfaces(), &generic_params);
    out.push_str(&format!("interface {}{}{} {{\n", name, generic_suffix, extends));

    let methods = interface.methods().iter().filter(|m| m.is_exported()).collect::<Vec<_>>();
    let mut methods = interface.methods().iter().filter(|m| m.is_exported()).collect::<Vec<_>>();
    methods.sort_by_key(|m| m.name());

    for method in methods {
        out.push_str(&format!(
            "  {}{};\n",
            method.name(),
            method_signature_with_generics(method, &generic_params, false)
        ));
    }

    let mut properties = interface.properties().iter().filter(|p| p.is_exported()).collect::<Vec<_>>();
    properties.sort_by_key(|p| p.name());

    for prop in properties {
        let return_ty = Signature::to_string(prop.getter().metadata().unwrap(), &prop.getter().return_type());
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
    let generic_params = generic_parameter_names(
        delegate.full_name(),
        delegate.number_of_generic_parameters(),
    );
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
                    modules.entry(ns).or_default().push(render_class(name.as_str(), item));
                }
            }
            DeclarationKind::Interface => {
                if let Some(item) = lock.as_any().downcast_ref::<InterfaceDeclaration>() {
                    if !is_in_requested_root(item.full_name(), root) {
                        continue;
                    }
                    let ns = declaration_namespace(item.full_name());
                    let name = declaration_simple_name(item.full_name());
                    modules.entry(ns).or_default().push(render_interface(name.as_str(), item));
                }
            }
            DeclarationKind::GenericInterface => {
                if let Some(item) = lock.as_any().downcast_ref::<GenericInterfaceDeclaration>() {
                    if !is_in_requested_root(item.full_name(), root) {
                        continue;
                    }
                    let ns = declaration_namespace(item.full_name());
                    modules.entry(ns).or_default().push(render_generic_interface(item));
                }
            }
            DeclarationKind::Enum => {
                if let Some(item) = lock.as_any().downcast_ref::<EnumDeclaration>() {
                    if !is_in_requested_root(item.full_name(), root) {
                        continue;
                    }
                    let ns = declaration_namespace(item.full_name());
                    let name = declaration_simple_name(item.full_name());
                    modules.entry(ns).or_default().push(render_enum(name.as_str(), item));
                }
            }
            DeclarationKind::Struct => {
                if let Some(item) = lock.as_any().downcast_ref::<StructDeclaration>() {
                    if !is_in_requested_root(item.full_name(), root) {
                        continue;
                    }
                    let ns = declaration_namespace(item.full_name());
                    let name = declaration_simple_name(item.full_name());
                    modules.entry(ns).or_default().push(render_struct(name.as_str(), item));
                }
            }
            DeclarationKind::Delegate => {
                if let Some(item) = lock.as_any().downcast_ref::<DelegateDeclaration>() {
                    if !is_in_requested_root(item.full_name(), root) {
                        continue;
                    }
                    let ns = declaration_namespace(item.full_name());
                    let name = declaration_simple_name(item.full_name());
                    modules.entry(ns).or_default().push(render_delegate(name.as_str(), item));
                }
            }
            DeclarationKind::GenericDelegate => {
                if let Some(item) = lock.as_any().downcast_ref::<GenericDelegateDeclaration>() {
                    if !is_in_requested_root(item.full_name(), root) {
                        continue;
                    }
                    let ns = declaration_namespace(item.full_name());
                    modules.entry(ns).or_default().push(render_generic_delegate(item));
                }
            }
            _ => {}
        }
    }

    modules
}

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

fn parse_args() -> (Vec<String>, PathBuf, Option<PathBuf>) {
    let mut args = env::args().skip(1);

    let mut roots = vec!["Windows".to_string()];
    let mut out = PathBuf::from("windows-runtime.generated.d.ts");
    let mut input: Option<PathBuf> = None;

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
            "--input" => {
                if let Some(value) = args.next() {
                    input = Some(PathBuf::from(value));
                }
            }
            _ => {}
        }
    }

    if let Some(path) = input.as_ref() {
        let discovered = discover_roots_from_input(path);
        if !discovered.is_empty() {
            roots.extend(discovered);
        }
    }

    roots.sort();
    roots.dedup();

    (roots, out, input)
}

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
    ] {
        anchors.push(format!("{}.{}", root, suffix));
    }

    anchors
}

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

fn enumerate_candidates_from_anchor(root: &str) -> BTreeSet<String> {
    let mut candidates = BTreeSet::new();
    let anchor_names = well_known_anchors_for_root(root);
    if anchor_names.is_empty() {
        return candidates;
    }

    let mut metadata = None;
    for anchor_name in anchor_names {
        metadata = metadata_from_anchor(anchor_name.as_str());
        if metadata.is_some() {
            break;
        }
    }

    let Some(metadata) = metadata else {
        return candidates;
    };

    let mut enumerator = std::ptr::null_mut();

    loop {
        let mut tokens = [0u32; 128];
        let mut fetched = 0;
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

        for token in tokens.into_iter().take(fetched as usize) {
            let name = get_type_name(&metadata, CorTokenType(token as i32));
            if is_in_requested_root(name.as_str(), root) {
                candidates.insert(name);
            }
        }
    }

    if !enumerator.is_null() {
        unsafe { metadata.CloseEnum(enumerator) };
    }

    candidates
}

fn append_rendered_declaration(
    lock: &dyn Declaration,
    modules: &mut BTreeMap<String, Vec<String>>,
) {
    match lock.kind() {
        DeclarationKind::Class => {
            if let Some(item) = lock.as_any().downcast_ref::<ClassDeclaration>() {
                let ns = declaration_namespace(item.full_name());
                let name = declaration_simple_name(item.full_name());
                modules.entry(ns).or_default().push(render_class(name.as_str(), item));
            }
        }
        DeclarationKind::Interface => {
            if let Some(item) = lock.as_any().downcast_ref::<InterfaceDeclaration>() {
                let ns = declaration_namespace(item.full_name());
                let name = declaration_simple_name(item.full_name());
                modules.entry(ns).or_default().push(render_interface(name.as_str(), item));
            }
        }
        DeclarationKind::GenericInterface => {
            if let Some(item) = lock.as_any().downcast_ref::<GenericInterfaceDeclaration>() {
                let ns = declaration_namespace(item.full_name());
                modules.entry(ns).or_default().push(render_generic_interface(item));
            }
        }
        DeclarationKind::Enum => {
            if let Some(item) = lock.as_any().downcast_ref::<EnumDeclaration>() {
                let ns = declaration_namespace(item.full_name());
                let name = declaration_simple_name(item.full_name());
                modules.entry(ns).or_default().push(render_enum(name.as_str(), item));
            }
        }
        DeclarationKind::Struct => {
            if let Some(item) = lock.as_any().downcast_ref::<StructDeclaration>() {
                let ns = declaration_namespace(item.full_name());
                let name = declaration_simple_name(item.full_name());
                modules.entry(ns).or_default().push(render_struct(name.as_str(), item));
            }
        }
        DeclarationKind::Delegate => {
            if let Some(item) = lock.as_any().downcast_ref::<DelegateDeclaration>() {
                let ns = declaration_namespace(item.full_name());
                let name = declaration_simple_name(item.full_name());
                modules.entry(ns).or_default().push(render_delegate(name.as_str(), item));
            }
        }
        DeclarationKind::GenericDelegate => {
            if let Some(item) = lock.as_any().downcast_ref::<GenericDelegateDeclaration>() {
                let ns = declaration_namespace(item.full_name());
                modules.entry(ns).or_default().push(render_generic_delegate(item));
            }
        }
        _ => {}
    }
}

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

fn augment_modules_from_winmd_scan(
    roots: &[String],
    input: Option<&PathBuf>,
    modules: &mut BTreeMap<String, Vec<String>>,
) {
    let mut scan_paths = Vec::new();

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

    for root in roots {
        if root == "Windows" || root.starts_with("Windows.") {
            for path in windows_winmd_paths() {
                scan_paths.push(path);
            }
        }
    }

    scan_paths.sort();
    scan_paths.dedup();

    for root in roots {
        for candidate in enumerate_candidates_from_anchor(root) {
            if let Some(dec) = MetadataReader::find_by_name(candidate.as_str()) {
                let lock = dec.read();
                append_rendered_declaration(&*lock, modules);
            }
        }

        for path in &scan_paths {
            let candidates = scan_winmd_candidates(path, root);
            for candidate in candidates {
                if let Some(dec) = MetadataReader::find_by_name(candidate.as_str()) {
                    let lock = dec.read();
                    append_rendered_declaration(&*lock, modules);
                }
            }
        }
    }
}

fn main() {
    let (roots, out_path, input) = parse_args();

    let mut body = String::new();
    body.push_str("// Auto-generated by typings-generator\n");
    body.push_str("// Experimental: incomplete WinRT projection coverage\n\n");

    // Built-in value struct projections
    body.push_str(concat!(
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
    ));

    if let Some(path) = &input {
        body.push_str(&format!("// Input: {}\n", path.display()));
    }
    body.push_str(&format!("// Roots: {}\n\n", roots.join(", ")));

    let mut modules: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for root in &roots {
        let generated = walk_namespace(root.as_str());
        for (namespace, mut declarations) in generated {
            modules.entry(namespace).or_default().append(&mut declarations);
        }
    }

    augment_modules_from_winmd_scan(&roots, input.as_ref(), &mut modules);

    // Deduplicate declaration text per namespace to keep output stable.
    for declarations in modules.values_mut() {
        declarations.sort();
        declarations.dedup();
    }

    let rendered = render_modules(&modules);
    body.push_str(&rendered);

    if rendered.is_empty() {
        body.push_str("// No declarations were generated for the selected root namespace.\n");
    }

    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            let _ = fs::create_dir_all(parent);
        }
    }

    fs::write(&out_path, body).expect("failed to write generated typings");
    println!("Generated typings: {}", out_path.display());
}
