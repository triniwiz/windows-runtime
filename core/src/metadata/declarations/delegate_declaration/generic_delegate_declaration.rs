use std::borrow::Cow;

use core_bindings::{mdToken};

use crate::bindings::imeta_data_import2;
use crate::metadata::declarations::declaration::{Declaration, DeclarationKind};
use crate::metadata::declarations::delegate_declaration::{DelegateDeclaration, DelegateDeclarationImpl};
use crate::prelude::c_void;
use crate::metadata::declarations::method_declaration::MethodDeclaration;
use crate::metadata::declarations::type_declaration::TypeDeclaration;

#[derive(Clone, Debug)]
pub struct GenericDelegateDeclaration<'a> {
	base: DelegateDeclaration<'a>,
}

impl<'a> GenericDelegateDeclaration<'a> {
	pub fn new(metadata: *mut c_void, token: mdToken) -> Self {
		Self {
			base: DelegateDeclaration::new_overload(
				DeclarationKind::GenericDelegate, metadata, token,
			)
		}
	}

	pub fn number_of_generic_parameters(&self) -> usize {
		let mut count = 0;

		let mut enumerator = std::mem::MaybeUninit::uninit();
		let enumerator_ptr = &mut enumerator;

		debug_assert!(
			imeta_data_import2::enum_generic_params(self.base.base.metadata, enumerator_ptr, self.base.base.token, None, None, None).is_ok()
		);
		debug_assert!(
			imeta_data_import2::count_enum(self.base.base.metadata, enumerator, &mut count).is_ok()
		);
		imeta_data_import2::close_enum(self.base.base.metadata, enumerator);

		return count as usize;
	}
}

impl<'a> Declaration for GenericDelegateDeclaration<'a> {
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

impl<'a> DelegateDeclarationImpl for GenericDelegateDeclaration<'a> {
	fn base(&self) -> &TypeDeclaration {
		&self.base.base
	}

	fn invoke_method(&self) -> &MethodDeclaration {
		&self.base.invoke_method
	}
}