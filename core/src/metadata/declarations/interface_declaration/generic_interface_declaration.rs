use std::borrow::Cow;

use core_bindings::{GUID, mdToken};

use crate::bindings::imeta_data_import2;
use crate::metadata::declarations::base_class_declaration::BaseClassDeclarationImpl;
use crate::metadata::declarations::declaration::{Declaration, DeclarationKind};
use crate::metadata::declarations::event_declaration::EventDeclaration;
use crate::metadata::declarations::method_declaration::MethodDeclaration;
use crate::metadata::declarations::property_declaration::PropertyDeclaration;
use crate::metadata::declarations::type_declaration::TypeDeclaration;
use crate::prelude::c_void;
use crate::metadata::declarations::interface_declaration::InterfaceDeclaration;

#[derive(Clone, Debug)]
pub struct GenericInterfaceDeclaration<'a> {
	base: InterfaceDeclaration<'a>,
}

impl<'a> GenericInterfaceDeclaration<'a> {
	pub fn new(metadata: *mut c_void, token: mdToken) -> Self {
		Self {
			base: InterfaceDeclaration::new_with_kind(DeclarationKind::GenericInterface, metadata, token)
		}
	}

	pub fn number_of_generic_parameters(&self) -> usize {
		let mut count = 0;
		let mut enumerator = std::mem::MaybeUninit::uninit();
		let enumerator_ptr = &mut enumerator;
		let base = self.base.base.base();
		debug_assert!(
			imeta_data_import2::enum_generic_params(
				base.metadata, enumerator_ptr, base.token, None, None, None,
			).is_ok()
		);
		debug_assert!(
			imeta_data_import2::count_enum(base.metadata, enumerator, &mut count).is_ok()
		);

		return count as usize;
	}

	pub fn id(&self) -> GUID {
		self.base.id()
	}
}

impl<'a> BaseClassDeclarationImpl for GenericInterfaceDeclaration<'a> {
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

impl<'a> Declaration for GenericInterfaceDeclaration<'a> {
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
