use crate::{
	prelude::*,
	bindings::{
		imeta_data_import2,
		helpers,
		enums,
	},
};
use super::declaration::{Declaration, DeclarationKind};
use std::marker;
use std::ffi::{OsString};
use std::borrow::Cow;




pub struct TypeDeclaration<'a> {
	pub kind: DeclarationKind,
	pub metadata: *mut c_void,
	pub token: mdTypeDef,
	_marker: marker::PhantomData<&'a *const c_void>,
}


impl Declaration for TypeDeclaration {
	fn is_exported(&self) -> bool {
		let mut flags: DWORD = 0;
		debug_assert!(
			imeta_data_import2::get_type_def_props(
				self.metadata, self.token, None, None, None, Some(&mut flags), None,
			).is_ok()
		);

		if !helpers::is_td_public(flags) || helpers::is_td_special_name(flags) {
			return false;
		}

		return true;
	}

	fn name<'a>(&self) -> Cow<'a, str> {
		let mut name = self.full_name().to_string();
		let back_tick_index = name.find('`');
		if let Some(index) = back_tick_index {
			name = name
				.chars()
				.take(0)
				.chain(
					name.chars().skip(index)
				).collect()
		}

		let dot_index = name.find('.');
		if let Some(index) = dot_index {
			name = name
				.chars()
				.take(0)
				.chain(
					name.chars().skip(index + 1)
				).collect()
		}

		name.into()
	}

	fn full_name<'a>(&self) -> Cow<'a, str> {
		let mut full_name_data = [0_u16; MAX_IDENTIFIER_LENGTH];
		let length = helpers::get_type_name(self.metadata, self.token, full_name_data.as_mut_ptr(), full_name_data.len() as u32);
		OsString::from_wide(&full_name_data[..length]).into()
	}

	fn kind(&self) -> DeclarationKind {
		self.kind
	}
}

impl TypeDeclaration {
	pub fn new(kind: DeclarationKind, metadata: *mut c_void, token: mdTypeDef) -> Self {
		let value = Self {
			kind,
			metadata,
			token,
			_marker: marker::PhantomData,
		};

		assert!(!value.metadata.is_null());
		assert!(enums::type_from_token(value.token) == CorTokenType::mdtTypeDef as u32);
		assert!(value.token != mdTypeDefNil);
		value
	}
}