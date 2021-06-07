use crate::prelude::*;
use crate::metadata::declarations::base_class_declaration::{BaseClassDeclaration, BaseClassDeclarationImpl, BaseClassDeclarationImp};
use crate::metadata::declarations::event_declaration::EventDeclaration;
use crate::metadata::declarations::method_declaration::MethodDeclaration;
use crate::metadata::declarations::property_declaration::PropertyDeclaration;
use crate::metadata::declarations::type_declaration::TypeDeclaration;
use crate::metadata::declarations::declaration::{DeclarationKind, Declaration};
use crate::bindings::{helpers, imeta_data_import2, enums};
use crate::metadata::com_helpers::get_guid_attribute_value;
use std::borrow::Cow;
use crate::metadata::generic_instance_id_builder::GenericInstanceIdBuilder;
use crate::metadata::signature::Signature;

pub struct InterfaceDeclaration<'a> {
	base: BaseClassDeclaration<'a>,
}

impl InterfaceDeclaration {
	pub fn new(metadata: *mut c_void, token: mdToken) -> Self {
		Self::new_with_kind(
			DeclarationKind::Interface, metadata, token,
		)
	}

	pub fn new_with_kind(kind: DeclarationKind, metadata: *mut c_void, token: mdToken) -> Self {
		Self {
			base: BaseClassDeclaration::new(
				kind, metadata, token,
			)
		}
	}

	pub fn id(&self) -> GUID {
		let base = self.base.base();
		get_guid_attribute_value(base.metadata, base.token)
	}
}

impl<'a> Declaration for InterfaceDeclaration {
	fn name<'a>(&self) -> Cow<'a, str> {
		self.base.name()
	}

	fn full_name<'a>(&self) -> Cow<'a, str> {
		self.base.full_name()
	}

	fn kind(&self) -> DeclarationKind {
		self.base.kind()
	}
}

impl<'a> BaseClassDeclarationImpl for InterfaceDeclaration {
	fn base(&self) -> &TypeDeclaration<'a> {
		self.base.base()
	}

	fn implemented_interfaces(&self) -> &Vec<InterfaceDeclaration<'a>> {
		self.base.implemented_interfaces()
	}

	fn methods(&self) -> &Vec<MethodDeclaration<'a>> {
		self.base.methods()
	}

	fn properties(&self) -> &Vec<PropertyDeclaration<'a>> {
		self.base.properties()
	}

	fn events(&self) -> &Vec<EventDeclaration> {
		self.base.events()
	}
}

pub struct GenericInterfaceDeclaration<'a> {
	base: InterfaceDeclaration<'a>,
}

impl GenericInterfaceDeclaration {
	pub fn new(metadata: *mut c_void, token: mdToken) -> Self {
		Self {
			base: InterfaceDeclaration::new_with_kind(DeclarationKind::GenericInterface, metadata, token)
		}
	}

	pub fn number_of_generic_parameters(&self) -> usize {
		let mut count = 0;
		let enumerator = std::ptr::null_mut();
		let base = self.base.base.base();
		debug_assert!(
			imeta_data_import2::enum_generic_params(
				base.metadata, enumerator, base.token, None, None, None,
			).is_ok()
		);
		debug_assert!(
			imeta_data_import2::count_enum(base.metadata, enumerator, &mut count).is_ok()
		);

		return count;
	}

	pub fn id(&self) -> GUID {
		self.base.id()
	}
}

impl<'a> BaseClassDeclarationImpl for GenericInterfaceDeclaration<'a> {
	fn base(&self) -> &TypeDeclaration<'a> {
		self.base.base()
	}

	fn implemented_interfaces(&self) -> &Vec<InterfaceDeclaration<'a>> {
		self.base.implemented_interfaces()
	}

	fn methods(&self) -> &Vec<MethodDeclaration<'a>> {
		self.base.methods()
	}

	fn properties(&self) -> &Vec<PropertyDeclaration<'a>> {
		self.base.properties()
	}

	fn events(&self) -> &Vec<EventDeclaration> {
		self.base.events()
	}
}

impl<'a> Declaration for GenericInterfaceDeclaration {
	fn name<'a>(&self) -> Cow<'a, str> {
		self.base.name()
	}

	fn full_name<'a>(&self) -> Cow<'a, str> {
		self.base.full_name()
	}

	fn kind(&self) -> DeclarationKind {
		self.base.kind()
	}
}


pub struct GenericInterfaceInstanceDeclaration<'a> {
	base: InterfaceDeclaration<'a>,
	closed_metadata: *mut c_void,
	closed_token: mdTypeSpec,
}

impl GenericInterfaceInstanceDeclaration {
	pub fn new(open_metadata: *mut c_void, open_token: mdTypeDef, closed_metadata: *mut c_void, closed_token: mdTypeSpec) -> Self {
		debug_assert!(!closed_metadata.is_null());
		debug_assert!(enums::type_from_token(closed_token) == CorTokenType::mdtTypeSpec);
		debug_assert!(closed_token != mdTypeSpecNil);

		Self {
			base: InterfaceDeclaration::new_with_kind(
				DeclarationKind::GenericInterfaceInstance, open_metadata, open_token,
			),
			closed_metadata,
			closed_token,
		}
	}
	pub fn id(&self) -> GUID {
		return GenericInstanceIdBuilder::generate_id(self);
	}
}

impl<'a> BaseClassDeclarationImpl for GenericInterfaceInstanceDeclaration {
	fn base(&self) -> &TypeDeclaration<'a> {
		self.base.base()
	}

	fn implemented_interfaces(&self) -> &Vec<InterfaceDeclaration<'a>> {
		self.base.implemented_interfaces()
	}

	fn methods(&self) -> &Vec<MethodDeclaration<'a>> {
		self.base.methods()
	}

	fn properties(&self) -> &Vec<PropertyDeclaration<'a>> {
		self.base.properties()
	}

	fn events(&self) -> &Vec<EventDeclaration<'a>> {
		self.base.events()
	}
}

impl<'a> Declaration for GenericInterfaceInstanceDeclaration {
	fn name<'a>(&self) -> Cow<'a, str> {
		self.base.name()
	}

	fn full_name<'a>(&self) -> Cow<'a, str> {
		let mut signature = [0_u8; MAX_IDENTIFIER_LENGTH];
		let mut signature_size = 0;
		debug_assert!(
			imeta_data_import2::get_type_spec_from_token(
				self.closed_metadata, self.closed_token, signature.as_mut_ptr(), &mut signature_size,
			).is_ok()
		);
		Signature::to_string(
			self.closed_metadata, signature.as_ptr(),
		).into()
	}

	fn kind(&self) -> DeclarationKind {
		self.base.kind()
	}
}