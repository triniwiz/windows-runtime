use crate::prelude::*;
use crate::metadata::declarations::declaration::{Declaration, DeclarationKind};
use crate::metadata::declarations::type_declaration::TypeDeclaration;
use crate::bindings::imeta_data_import2;
use crate::metadata::declarations::struct_field_declaration::StructFieldDeclaration;
use std::borrow::Cow;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct StructDeclaration {
	base: TypeDeclaration,
	fields: Vec<StructFieldDeclaration>,
}

impl Declaration for StructDeclaration {
	fn name<'b>(&self) -> Cow<'b, str> {
		self.base.name()
	}

	fn full_name<'b>(&self) -> Cow<'b, str> {
		self.base.full_name()
	}

	fn kind(&self) -> DeclarationKind {
		self.base.kind
	}
}

impl StructDeclaration {
	pub fn new(metadata: Arc<Mutex<IMetaDataImport2>>, token: mdTypeDef,
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

	pub fn fields(&self) -> &[StructFieldDeclaration] {
		self.fields.as_slice()
	}

	fn make_field_declarations<'b>(metadata: Arc<Mutex<IMetaDataImport2>>, token: mdTypeDef) -> Vec<StructFieldDeclaration<'b>> {
		let metadata_inner = get_mutex_value_mut(&metadata);
		let mut enumerator = std::ptr::null_mut();
		let mut count = 0;
		let mut tokens = [0; 1024];
		let mut enumerator_ptr = &mut enumerator;
		debug_assert!(
			imeta_data_import2::enum_fields(
				metadata_inner, enumerator_ptr, token, tokens.as_mut_ptr(), tokens.len() as u32, &mut count,
			).is_ok()
		);

		debug_assert!(count < (tokens.len() - 1) as u32);

		imeta_data_import2::close_enum(metadata_inner, enumerator);

		let mut result = Vec::new();

		for i in 0..count {
			result.push(
				StructFieldDeclaration::new(Arc::clone(&metadata), tokens[i])
			)
		}


		return result;
	}
}