use crate::metadata::declarations::type_declaration::TypeDeclaration;
use crate::metadata::declarations::declaration::{Declaration, DeclarationKind};
use std::option::Option::Some;

pub struct NamespaceDeclaration<'a> {
	base: TypeDeclaration<'a>,
	children: Vec<&'a str>,
	full_name: &'a str
}


impl Declaration for NamespaceDeclaration {
	fn name(&self) -> String {
		let mut fully_qualified_name = self.full_name().to_owned();
		if let Some(index) = fully_qualified_name.find(".") {
			fully_qualified_name = fully_qualified_name.chars().skip(index + 1).collect()
		}
		return fully_qualified_name;
	}

	fn full_name<'a>(&self) -> &'a str {
		self.full_name
	}

	fn kind(&self) -> DeclarationKind {
		self.base.kind
	}
}

impl NamespaceDeclaration {
	pub fn new(full_name: &str) -> Self {
		// ASSERT(fullName);

		Self {
			base: TypeDeclaration::new(
				DeclarationKind::Namespace,
				std::ptr::null_mut(),
				std::ptr::null_mut()
			),
			children: Vec::new(),
			full_name
		}
	}
	pub fn children<'a>(&self) -> &[&'a str] {
		self.children.as_slice()
	}
}