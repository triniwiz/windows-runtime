use crate::prelude::*;
use crate::bindings::{
	 helpers,
};
use super::{
	declaration::{
		Declaration,
		DeclarationKind,
	},
	type_declaration::TypeDeclaration,
};
use std::borrow::Cow;
use std::sync::{Arc, RwLock};

#[derive(Debug)]
pub struct EnumDeclaration {
	base: TypeDeclaration,
}

impl Declaration for EnumDeclaration {
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
		self.base.kind()
	}
}

impl EnumDeclaration {
	pub fn new(metadata: Option<Arc<RwLock<IMetaDataImport2>>>, token: mdTypeDef) -> Self {
		Self {
			base: TypeDeclaration::new(DeclarationKind::Enum, metadata, token)
		}
	}

	pub fn type_<'b>(&self) -> Cow<'b, &[u8]> {
		let mut type_field = mdTokenNil;
		let name_w = OsString::from(COR_ENUM_FIELD_NAME).to_wide();
		match self.base.metadata() {
			None => Cow::default(),
			Some(metadata) => {
				let result = metadata.find_field(
					self.base.token(),
					Some(name_w.as_ptr()), None, None, Some(&mut type_field),
				);
				debug_assert!(result.is_ok());
				let mut signature = [0_u8; MAX_IDENTIFIER_LENGTH];
				let mut signature_size = 0;
				metadata.get_field_props(
					type_field,
					None, None, None, None, None,
					Some(&mut signature.as_mut_ptr() as _ as *mut *const u8),
					Some(&mut signature_size),
					None, None, None,
				);
				debug_assert!(
					result.is_ok()
				);

				let header = helpers::cor_sig_uncompress_data(signature.as_ptr());
				debug_assert!(
					header == CorCallingConvention::ImageCeeCsCallconvField as u32
				);

				let result: &[u8] = &signature[0..signature_size];

				result.into()
			}
		}
	}
}