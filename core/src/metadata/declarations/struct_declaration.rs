use crate::prelude::*;
use crate::metadata::declarations::declaration::{Declaration, DeclarationKind};
use crate::metadata::declarations::type_declaration::TypeDeclaration;
use crate::bindings::imeta_data_import2;
use crate::metadata::declarations::struct_field_declaration::StructFieldDeclaration;

pub struct StructDeclaration<'a> {
	base: TypeDeclaration<'a>,
	fields: Vec<StructFieldDeclaration<'a>>,
}

impl Declaration for StructDeclaration {
	fn name<'a>(&self) -> &'a str {
		self.base.name()
	}

	fn full_name<'a>(&self) -> &'a str {
		self.base.full_name()
	}

	fn kind(&self) -> DeclarationKind {
		self.base.kind
	}
}

impl StructDeclaration {
	pub fn new(metadata: *mut c_void, token: mdTypeDef,
	) -> Self {
		Self {
			base: TypeDeclaration::new(
				DeclarationKind::Struct, metadata, token,
			),
			fields: StructDeclaration::make_field_declarations(metadata, token),
		}
	}

	pub fn size(&self) -> usize {
		self.fields.len()
	}

	fn make_field_declarations(metadata: *mut c_void, token: mdTypeDef) -> Vec<StructFieldDeclaration> {
		let enumerator = std::ptr::null_mut();
		let mut count = 0;
		let mut tokens: Vec<mdFieldDef> = vec![0; 1024];
		debug_assert!(
			imeta_data_import2::enum_fields(
				metadata, enumerator, token, tokens.as_mut_ptr(), tokens.len(), &mut count,
			).is_ok()
		);

		debug_assert!(count < tokens.len() - 1);

		imeta_data_import2::close_enum(metadata, enumerator);

		let mut result = Vec::new();

		for i in 0..count {
			result.push(
				StructFieldDeclaration::new(metadata, tokens[i])
			)
		}


		return result;
	}
}