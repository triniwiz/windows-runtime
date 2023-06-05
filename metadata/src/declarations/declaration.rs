use std::any::Any;
use std::fmt::{Display, Formatter, write};

#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum DeclarationKind {
    Namespace,
    Class,
    Interface,
    GenericInterface,
    GenericInterfaceInstance,
    Enum,
    EnumMember,
    Struct,
    StructField,
    Delegate,
    GenericDelegate,
    GenericDelegateInstance,
    Event,
    Property,
    Method,
    Parameter,
}

impl Display for DeclarationKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
       let value = match self {
           DeclarationKind::Namespace => "Namespace",
           DeclarationKind::Class => "Class",
           DeclarationKind::Interface => "Interface",
           DeclarationKind::GenericInterface => "GenericInterface",
           DeclarationKind::GenericInterfaceInstance => "GenericInterfaceInstance",
           DeclarationKind::Enum => "Enum",
           DeclarationKind::EnumMember => "EnumMember",
           DeclarationKind::Struct => "Struct",
           DeclarationKind::StructField => "StructField",
           DeclarationKind::Delegate => "Delegate",
           DeclarationKind::GenericDelegate => "GenericDelegate",
           DeclarationKind::GenericDelegateInstance => "GenericDelegateInstance",
           DeclarationKind::Event => "Event",
           DeclarationKind::Property => "Property",
           DeclarationKind::Method => "Method",
           DeclarationKind::Parameter => "Parameter"
       };
        write!(f,"{}", value)
    }
}

pub trait Declaration {

    fn as_any(&self) -> &dyn Any;

    fn as_any_mut(&mut self) -> &mut dyn Any;

    fn is_exported(&self) -> bool {
        true
    }

    /// Specifies the simple name (e.g., "String" rather than "System.String") of a given type.
    fn name(&self) -> &str;

    /// Specifies the fully-qualified name of a given type.
    /// For generic types, this includes the spelling of generic parameter names.
    fn full_name(&self) -> &str;

    fn kind(&self) -> DeclarationKind;
}
