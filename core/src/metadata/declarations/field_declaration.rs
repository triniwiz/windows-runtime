use super::declaration::{Declaration, DeclarationKind};
use crate::prelude::*;

use crate::bindings::{
	imeta_data_import2,
	enums,
};
use std::{
	marker,
	ffi::{OsString},
	borrow::Cow,
};

#[derive(Clone, Debug)]
pub struct FieldDeclaration<'a> {
	pub kind: DeclarationKind,
	pub metadata: *mut c_void,
	pub token: mdFieldDef,
	_marker: marker::PhantomData<&'a *const c_void>,
}

impl<'a> Declaration for FieldDeclaration<'a> {
	fn name<'b>(&self) -> Cow<'b, str> {
		return self.full_name();
	}

	fn full_name<'b>(&self) -> Cow<'b, str> {
		let mut full_name_data = [0_u16; MAX_IDENTIFIER_LENGTH];
		let mut length = 0;
		debug_assert!(imeta_data_import2::get_field_props(
			self.metadata, self.token, None,
			Some(full_name_data.as_mut_ptr()),
			Some(full_name_data.len() as u32),
			Some(&mut length),
			None, None,
			None, None,
			None, None,
		).is_ok());

		OsString::from_wide(full_name_data[..length]).into()
	}

	fn kind(&self) -> DeclarationKind {
		self.kind
	}
}

impl<'a> FieldDeclaration<'a> {
	pub fn new(kind: DeclarationKind, metadata: *mut c_void, token: mdFieldDef) -> Self {
		let value = Self {
			kind,
			metadata,
			token,
			_marker: marker::PhantomData,
		};

		assert!(value.metadata.is_not_null());
		assert!(enums::type_from_token(value.token) == CorTokenType::mdtFieldDef as u32);
		assert!(value.token != mdFieldDefNil);
		value
	}
}