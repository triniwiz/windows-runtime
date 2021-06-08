use crate::prelude::*;
use crate::bindings::{
	imeta_data_import2, helpers,
};
use super::{
	declaration::{
		Declaration,
		DeclarationKind,
	},
	type_declaration::TypeDeclaration,
};
use std::borrow::Cow;

#[derive(Debug)]
pub struct EnumDeclaration<'a> {
	base: TypeDeclaration<'a>,
}

impl<'a> Declaration for EnumDeclaration<'a> {
	fn is_exported(&self) -> bool {
		self.base.is_exported()
	}

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

impl<'a> EnumDeclaration<'a> {
	pub fn new(metadata: *mut c_void, token: mdTypeDef) -> Self {
		Self {
			base: TypeDeclaration::new(DeclarationKind::Enum, metadata, token)
		}
	}

	pub fn type_<'b>(&self) -> Cow<'b, &[u8]> {
		let mut type_field = mdTokenNil;
		let name_w = OsString::from(COR_ENUM_FIELD_NAME).to_wide();
		debug_assert!(imeta_data_import2::find_field(
			self.base.metadata, self.base.token,
			Some(name_w.as_ptr()), None, None, Some(&mut type_field),
		).is_ok());
		let mut signature = [0_u8; MAX_IDENTIFIER_LENGTH];
		let mut signature_size = 0;
		debug_assert!(
			imeta_data_import2::get_field_props(
				self.base.metadata,
				type_field,
				None, None, None, None, None,
				Some(&mut signature.as_mut_ptr() as _ as *mut *const u8),
				Some(&mut signature_size),
				None, None, None,
			).is_ok()
		);

		let header = helpers::cor_sig_uncompress_data(signature.as_ptr());
		debug_assert!(
			header == CorCallingConvention::ImageCeeCsCallconvField as u32
		);

		let result: &[u8] = &signature[0..signature_size];

		result.into()
	}
}