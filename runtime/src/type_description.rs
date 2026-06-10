use crate::class_helpers::split_type_name;
use metadata::declarations::base_class_declaration::BaseClassDeclarationImpl;
use metadata::declarations::class_declaration::ClassDeclaration;
use metadata::declarations::declaration::{Declaration, DeclarationKind};
use metadata::declarations::interface_declaration::generic_interface_declaration::GenericInterfaceDeclaration;
use metadata::declarations::interface_declaration::generic_interface_instance_declaration::GenericInterfaceInstanceDeclaration;
use metadata::declarations::interface_declaration::InterfaceDeclaration;
use metadata::meta_data_reader::MetadataReader;
use metadata::signature::Signature;
use runtime_binding_gen::{
    RuntimeMethodMetadata, RuntimeParameterMetadata, RuntimePropertyMetadata,
};

pub(crate) fn runtime_method_metadata_from_method(
    method: &metadata::declarations::method_declaration::MethodDeclaration,
) -> RuntimeMethodMetadata {
    let return_type = Signature::to_string(method.metadata().unwrap(), &method.return_type());
    let parameters = method
        .parameters()
        .iter()
        .enumerate()
        .map(|(index, parameter)| {
            let type_name = parameter
                .metadata()
                .map(|metadata| Signature::to_string(metadata, &parameter.type_()))
                .unwrap_or_else(|| "Object".to_string());
            let name = if parameter.name().is_empty() {
                format!("arg{}", index)
            } else {
                parameter.name().to_string()
            };
            RuntimeParameterMetadata { name, type_name }
        })
        .collect::<Vec<_>>();

    RuntimeMethodMetadata {
        name: method.name().to_string(),
        return_type,
        parameters,
    }
}

pub(crate) fn runtime_property_metadata_from_property(
    property: &metadata::declarations::property_declaration::PropertyDeclaration,
) -> RuntimePropertyMetadata {
    let prop_type = Signature::to_string(
        property.getter().metadata().unwrap(),
        &property.getter().return_type(),
    );

    RuntimePropertyMetadata {
        name: property.name().to_string(),
        prop_type,
        readable: true,
        writable: property.setter().is_some(),
    }
}

pub(crate) fn base_declaration_descriptor(
    full_name: String,
    namespace: Option<String>,
    class_name: String,
    declaration: &dyn BaseClassDeclarationImpl,
) -> serde_json::Value {
    let methods = declaration
        .methods()
        .iter()
        .filter(|method| method.is_exported())
        .map(runtime_method_metadata_from_method)
        .collect::<Vec<_>>();
    let properties = declaration
        .properties()
        .iter()
        .filter(|property| property.is_exported())
        .map(runtime_property_metadata_from_property)
        .collect::<Vec<_>>();
    let interfaces = declaration
        .implemented_interfaces()
        .iter()
        .map(|interface| interface.full_name().to_string())
        .collect::<Vec<_>>();

    serde_json::json!({
        "typeName": full_name,
        "className": class_name,
        "namespace": namespace,
        "methods": methods,
        "properties": properties,
        "interfaces": interfaces,
    })
}

pub(crate) fn build_runtime_type_descriptor(type_name: &str) -> Option<serde_json::Value> {
    let declaration = MetadataReader::find_by_name(type_name)?;
    let lock = declaration.read();
    let full_name = lock.full_name().to_string();
    let (namespace, class_name) = split_type_name(full_name.as_str());

    match lock.kind() {
        DeclarationKind::Class => {
            let class = lock.as_any().downcast_ref::<ClassDeclaration>()?;
            Some(base_declaration_descriptor(
                full_name, namespace, class_name, class,
            ))
        }
        DeclarationKind::Interface => {
            lock.as_any()
                .downcast_ref::<InterfaceDeclaration>()
                .map(|interface| {
                    base_declaration_descriptor(full_name, namespace, class_name, interface)
                })
        }
        DeclarationKind::GenericInterface => lock
            .as_any()
            .downcast_ref::<GenericInterfaceDeclaration>()
            .map(|interface| {
                base_declaration_descriptor(full_name, namespace, class_name, interface)
            }),
        DeclarationKind::GenericInterfaceInstance => lock
            .as_any()
            .downcast_ref::<GenericInterfaceInstanceDeclaration>()
            .map(|interface| {
                base_declaration_descriptor(full_name, namespace, class_name, interface)
            }),
        _ => None,
    }
}
