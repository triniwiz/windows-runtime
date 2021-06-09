use crate::prelude::*;
use crate::metadata::declarations::declaration::{Declaration, DeclarationKind};
use crate::metadata::declarations::field_declaration::FieldDeclaration;
use crate::bindings::{imeta_data_import2, helpers};
use std::borrow::Cow;


#[derive(Clone,Debug)]
pub struct StructFieldDeclaration<'a> {
	base: FieldDeclaration<'a>,
}

impl<'a> Declaration for StructFieldDeclaration <'a>{
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

impl<'a> StructFieldDeclaration<'a> {
	pub fn base(&self)-> &FieldDeclaration {
		&self.base
	}
	pub fn new(metadata: *mut c_void, token: mdFieldDef) -> Self {
		Self {
			base: FieldDeclaration::new(
				DeclarationKind::StructField,
				metadata,
				token,
			)
		}
	}

	pub fn type_<'b>(&self) -> Cow<'b, [u8]> {
		let mut signature = [0_u8; MAX_IDENTIFIER_LENGTH];
		let mut signature_size = 0;

		debug_assert!(
			imeta_data_import2::get_field_props(
				self.base.metadata, self.base.token, None, None, None,
				None, None, Some(signature.as_mut_ptr() as *mut *const u8), Some(&mut signature_size),
				None, None, None,
			).is_ok()
		);

		let header = helpers::cor_sig_uncompress_data(signature.as_mut_ptr());
		debug_assert!(
			header == CorCallingConvention::ImageCeeCsCallconvField as u32
		);

		let result = &signature[..signature_size as usize];
		result.into()
	}
}

