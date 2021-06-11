use std::borrow::Cow;


use crate::metadata::com_helpers::get_guid_attribute_value;
use crate::metadata::declarations::base_class_declaration::{BaseClassDeclaration, BaseClassDeclarationImpl};
use crate::metadata::declarations::declaration::{Declaration, DeclarationKind};
use crate::metadata::declarations::event_declaration::EventDeclaration;
use crate::metadata::declarations::method_declaration::MethodDeclaration;
use crate::metadata::declarations::property_declaration::PropertyDeclaration;
use crate::metadata::declarations::type_declaration::TypeDeclaration;
use crate::prelude::*;

pub mod generic_interface_instance_declaration;
pub mod generic_interface_declaration;


use core_bindings::{GUID, mdToken};


#[derive(Clone, Debug)]
pub struct InterfaceDeclaration<'a> {
	base: BaseClassDeclaration<'a>,
}

impl<'a> InterfaceDeclaration<'a> {
	pub fn new(metadata: *mut IMetaDataImport2, token: mdToken) -> Self {
		Self::new_with_kind(
			DeclarationKind::Interface, metadata, token,
		)
	}

	pub fn new_with_kind(kind: DeclarationKind, metadata: *mut IMetaDataImport2, token: mdToken) -> Self {
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

impl<'a> Declaration for InterfaceDeclaration<'a> {
	fn name<'b>(&self) -> Cow<'b, str> {
		self.base.name()
	}

	fn full_name<'b>(&self) -> Cow<'b, str> {
		self.base.full_name()
	}

	fn kind(&self) -> DeclarationKind {
		self.base.kind()
	}
}

impl<'a> BaseClassDeclarationImpl for InterfaceDeclaration<'a> {
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
