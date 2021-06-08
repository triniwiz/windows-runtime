use std::borrow::Cow;

use core_bindings::{GUID, mdTypeDef, mdTypeSpec};

use crate::{
	metadata::declarations::method_declaration::MethodDeclaration,
	metadata::declarations::event_declaration::EventDeclaration,
	metadata::declarations::declaration::{Declaration, DeclarationKind},
	metadata::declarations::base_class_declaration::BaseClassDeclarationImpl,
	enums::CorTokenType,
	bindings::{enums, imeta_data_import2},
	metadata::declarations::property_declaration::PropertyDeclaration,
	metadata::declarations::type_declaration::TypeDeclaration,
	metadata::generic_instance_id_builder::GenericInstanceIdBuilder,
	metadata::signature::Signature,
	prelude::*,
	metadata::declarations::interface_declaration::InterfaceDeclaration
};

#[derive(Clone, Debug)]
pub struct GenericInterfaceInstanceDeclaration<'a> {
	base: InterfaceDeclaration<'a>,
	closed_metadata: *mut c_void,
	closed_token: mdTypeSpec,
}

impl<'a> GenericInterfaceInstanceDeclaration<'a> {
	pub fn new(open_metadata: *mut c_void, open_token: mdTypeDef, closed_metadata: *mut c_void, closed_token: mdTypeSpec) -> Self {
		debug_assert!(!closed_metadata.is_null());
		debug_assert!(CorTokenType::from(enums::type_from_token(closed_token)) == CorTokenType::mdtTypeSpec);
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

impl<'a> BaseClassDeclarationImpl for GenericInterfaceInstanceDeclaration<'a> {
	fn base(&self) -> &TypeDeclaration {
		self.base.base()
	}

	fn implemented_interfaces(&self) -> &Vec<InterfaceDeclaration> {
		self.base.implemented_interfaces()
	}

	fn methods(&self) -> &Vec<MethodDeclaration> {
		self.base.methods()
	}

	fn properties(&self) -> &Vec<PropertyDeclaration> {
		self.base.properties()
	}

	fn events(&self) -> &Vec<EventDeclaration> {
		self.base.events()
	}
}

impl<'a> Declaration for GenericInterfaceInstanceDeclaration<'a> {
	fn name<'b>(&self) -> Cow<'b, str> {
		self.base.name()
	}

	fn full_name<'b>(&self) -> Cow<'b, str> {
		let mut signature = [0_u8; MAX_IDENTIFIER_LENGTH];
		let signature_ptr = &mut signature.as_ptr();
		let mut signature_size = 0;
		debug_assert!(
			imeta_data_import2::get_type_spec_from_token(
				self.closed_metadata, self.closed_token, signature_ptr, &mut signature_size,
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
