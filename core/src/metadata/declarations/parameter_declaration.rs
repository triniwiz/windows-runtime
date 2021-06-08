use crate::prelude::*;
use crate::metadata::declarations::declaration::{Declaration, DeclarationKind};
use crate::metadata::declarations::type_declaration::TypeDeclaration;
use crate::bindings::{
	enums, helpers, imeta_data_import2,
};
use crate::enums::CorElementType;
use std::borrow::Cow;

#[derive(Clone, Debug)]
pub struct ParameterDeclaration<'a> {
	base: TypeDeclaration<'a>,
	parameter_type: PCCOR_SIGNATURE,
}

impl<'a> ParameterDeclaration<'a> {
	pub fn new(metadata: *mut c_void, token: mdParamDef, sig_type: PCCOR_SIGNATURE) -> Self {
		debug_assert!(!metadata.is_null());
		debug_assert!(enums::type_from_token(token) == CorTokenType::mdtParamDef as u32);
		debug_assert!(token != mdParamDefNil);

		Self {
			base: TypeDeclaration::new(
				DeclarationKind::Parameter, metadata, token,
			),
			parameter_type: sig_type,
		}
	}
	pub fn is_out(&self) -> bool {
		helpers::cor_sig_uncompress_token(self.parameter_type) == CorElementType::ElementTypeByref as u32
	}
}

impl<'a> Declaration for ParameterDeclaration<'a> {
	fn name<'b>(&self) -> Cow<'b, str> {
		self.full_name()
	}

	fn full_name<'b>(&self) -> Cow<'b, str> {
		let mut full_name_data = [0_u16; MAX_IDENTIFIER_LENGTH];
		let mut length = 0;
		assert!(imeta_data_import2::get_param_props(self.metadata, self.token,
													None, None,
													Some(full_name_data.as_mut_ptr()), Some(full_name_data.len() as u32),
													Some(&mut length), None, None, None, None,
		).is_ok());
		OsString::from_wide(
			full_name_data[..length]
		).into()
	}

	fn kind(&self) -> DeclarationKind {
		self.base.kind
	}
}