use std::collections::HashSet;
use metadata::declarations::base_class_declaration::BaseClassDeclarationImpl;
use metadata::declarations::class_declaration::ClassDeclaration;
use metadata::declarations::declaration::Declaration;
use metadata::declarations::event_declaration::EventDeclaration;
use metadata::declarations::method_declaration::MethodDeclaration;
use metadata::declarations::property_declaration::PropertyDeclaration;
use metadata::meta_data_reader::MetadataReader;

pub(crate) fn split_type_name(type_name: &str) -> (Option<String>, String) {
    match type_name.rsplit_once('.') {
        Some((namespace, class_name)) => (Some(namespace.to_string()), class_name.to_string()),
        None => (None, type_name.to_string()),
    }
}

pub(crate) fn extend_class_methods(
    class_declaration: &ClassDeclaration,
    methods: &mut Vec<MethodDeclaration>,
    seen: &mut HashSet<String>,
) {
    for method in class_declaration.methods() {
        let mut method_name = method.overload_name().to_string();
        if method_name.is_empty() {
            method_name = method.name().to_string();
        }
        if seen.insert(method_name) {
            methods.push(method.clone());
        }
    }

    if let Some(default_interface) = class_declaration.default_interface() {
        for method in default_interface.methods() {
            let mut method_name = method.overload_name().to_string();
            if method_name.is_empty() {
                method_name = method.name().to_string();
            }
            if seen.insert(method_name) {
                methods.push(method.clone());
            }
        }
    }

    for interface in class_declaration.implemented_interfaces() {
        for method in interface.methods() {
            let mut method_name = method.overload_name().to_string();
            if method_name.is_empty() {
                method_name = method.name().to_string();
            }
            if seen.insert(method_name) {
                methods.push(method.clone());
            }
        }
    }

    if !class_declaration.base_full_name().is_empty() {
        if let Some(base_declaration) = MetadataReader::find_by_name(class_declaration.base_full_name()) {
            let base_lock = base_declaration.read();
            if let Some(base_class) = base_lock.as_any().downcast_ref::<ClassDeclaration>() {
                extend_class_methods(base_class, methods, seen);
            }
        }
    }
}

pub(crate) fn extend_class_properties(
    class_declaration: &ClassDeclaration,
    properties: &mut Vec<PropertyDeclaration>,
    seen: &mut HashSet<String>,
) {
    for property in class_declaration.properties() {
        if seen.insert(property.name().to_string()) {
            properties.push(property.clone());
        }
    }

    if let Some(default_interface) = class_declaration.default_interface() {
        for property in default_interface.properties() {
            if seen.insert(property.name().to_string()) {
                properties.push(property.clone());
            }
        }
    }

    for interface in class_declaration.implemented_interfaces() {
        for property in interface.properties() {
            if seen.insert(property.name().to_string()) {
                properties.push(property.clone());
            }
        }
    }

    if !class_declaration.base_full_name().is_empty() {
        if let Some(base_declaration) = MetadataReader::find_by_name(class_declaration.base_full_name()) {
            let base_lock = base_declaration.read();
            if let Some(base_class) = base_lock.as_any().downcast_ref::<ClassDeclaration>() {
                extend_class_properties(base_class, properties, seen);
            }
        }
    }
}

pub(crate) fn collect_class_methods(class_declaration: &ClassDeclaration) -> Vec<MethodDeclaration> {
    let mut methods = Vec::new();
    let mut seen = HashSet::new();
    extend_class_methods(class_declaration, &mut methods, &mut seen);
    methods
}

pub(crate) fn collect_class_properties(class_declaration: &ClassDeclaration) -> Vec<PropertyDeclaration> {
    let mut properties = Vec::new();
    let mut seen = HashSet::new();
    extend_class_properties(class_declaration, &mut properties, &mut seen);
    properties
}

/// Look up a property by name across the class, its default interface,
/// implemented interfaces, and base-class chain — returning as soon as one
/// matches. Used on every property write, so it skips the `Vec` allocation
/// and full hierarchy walk that `collect_class_properties` does.
pub(crate) fn find_class_property(class_declaration: &ClassDeclaration, name: &str) -> Option<PropertyDeclaration> {
    if let Some(p) = class_declaration.properties().iter().find(|p| p.name() == name) {
        return Some(p.clone());
    }
    if let Some(di) = class_declaration.default_interface() {
        if let Some(p) = di.properties().iter().find(|p| p.name() == name) {
            return Some(p.clone());
        }
    }
    for iface in class_declaration.implemented_interfaces() {
        if let Some(p) = iface.properties().iter().find(|p| p.name() == name) {
            return Some(p.clone());
        }
    }
    if !class_declaration.base_full_name().is_empty() {
        if let Some(base_declaration) = MetadataReader::find_by_name(class_declaration.base_full_name()) {
            let base_lock = base_declaration.read();
            if let Some(base_class) = base_lock.as_any().downcast_ref::<ClassDeclaration>() {
                return find_class_property(base_class, name);
            }
        }
    }
    None
}

/// Look up a method by name. Matches the overload name when present, falling
/// back to the plain name. Walks the class hierarchy and returns the first
/// match. Same hot-path benefit as `find_class_property`.
pub(crate) fn find_class_method(class_declaration: &ClassDeclaration, name: &str) -> Option<MethodDeclaration> {
    let matches = |m: &MethodDeclaration| {
        let on = m.overload_name();
        (!on.is_empty() && on == name) || m.name() == name
    };
    if let Some(m) = class_declaration.methods().iter().find(|m| matches(m)) {
        return Some(m.clone());
    }
    if let Some(di) = class_declaration.default_interface() {
        if let Some(m) = di.methods().iter().find(|m| matches(m)) {
            return Some(m.clone());
        }
    }
    for iface in class_declaration.implemented_interfaces() {
        if let Some(m) = iface.methods().iter().find(|m| matches(m)) {
            return Some(m.clone());
        }
    }
    if !class_declaration.base_full_name().is_empty() {
        if let Some(base_declaration) = MetadataReader::find_by_name(class_declaration.base_full_name()) {
            let base_lock = base_declaration.read();
            if let Some(base_class) = base_lock.as_any().downcast_ref::<ClassDeclaration>() {
                return find_class_method(base_class, name);
            }
        }
    }
    None
}

pub(crate) fn class_method_matches(class_declaration: &ClassDeclaration, name: &str) -> bool {
    let method_match = |m: &MethodDeclaration| {
        let on = m.overload_name();
        (!on.is_empty() && on == name) || m.name() == name
    };

    if class_declaration.methods().iter().any(method_match) { return true; }

    if let Some(di) = class_declaration.default_interface() {
        if di.methods().iter().any(method_match) { return true; }
    }

    for iface in class_declaration.implemented_interfaces() {
        if iface.methods().iter().any(method_match) { return true; }
    }

    if !class_declaration.base_full_name().is_empty() {
        if let Some(base_decl) = MetadataReader::find_by_name(class_declaration.base_full_name()) {
            let lock = base_decl.read();
            if let Some(base) = lock.as_any().downcast_ref::<ClassDeclaration>() {
                return class_method_matches(base, name);
            }
        }
    }
    false
}

pub(crate) fn class_property_matches(class_declaration: &ClassDeclaration, name: &str) -> bool {
    if class_declaration.properties().iter().any(|p| p.name() == name) { return true; }

    if let Some(di) = class_declaration.default_interface() {
        if di.properties().iter().any(|p| p.name() == name) { return true; }
    }

    for iface in class_declaration.implemented_interfaces() {
        if iface.properties().iter().any(|p| p.name() == name) { return true; }
    }

    if !class_declaration.base_full_name().is_empty() {
        if let Some(base_decl) = MetadataReader::find_by_name(class_declaration.base_full_name()) {
            let lock = base_decl.read();
            if let Some(base) = lock.as_any().downcast_ref::<ClassDeclaration>() {
                return class_property_matches(base, name);
            }
        }
    }
    false
}

pub(crate) fn class_has_member_named(class_declaration: &ClassDeclaration, name: &str) -> bool {
    class_method_matches(class_declaration, name) || class_property_matches(class_declaration, name)
}

pub(crate) fn find_event_methods(
    class_declaration: &ClassDeclaration,
    name: &str,
) -> Option<(MethodDeclaration, MethodDeclaration)> {
    let check = |events: &[EventDeclaration]| -> Option<(MethodDeclaration, MethodDeclaration)> {
        events
            .iter()
            .find(|e| e.name() == name)
            .map(|e| (e.add_method().clone(), e.remove_method().clone()))
    };

    if let Some(m) = check(class_declaration.events()) { return Some(m); }
    if let Some(di) = class_declaration.default_interface() {
        if let Some(m) = check(di.events()) { return Some(m); }
    }
    for iface in class_declaration.implemented_interfaces() {
        if let Some(m) = check(iface.events()) { return Some(m); }
    }
    if !class_declaration.base_full_name().is_empty() {
        if let Some(base_decl) = MetadataReader::find_by_name(class_declaration.base_full_name()) {
            let lock = base_decl.read();
            if let Some(base) = lock.as_any().downcast_ref::<ClassDeclaration>() {
                return find_event_methods(base, name);
            }
        }
    }
    None
}
