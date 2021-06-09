use std::borrow::Cow;
use std::any::Any;

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

pub trait Declaration {
	fn is_exported(&self) -> bool {
		return true;
	}

	/// Specifies the simple name (e.g., "String" rather than "System.String") of a given type.
	fn name<'a>(&self) -> Cow<'a, str>;

	/// Specifies the fully-qualified name of a given type.
	/// For generic types, this includes the spelling of generic parameter names.
	fn full_name<'a>(&self) -> Cow<'a, str>;

	fn kind(&self) -> DeclarationKind;

	fn as_any<T: Any>(value: &T) -> &dyn Any {
		value as &dyn Any
	}

}
