use std::borrow::Cow;

use core_bindings::{mdToken, IMetaDataImport2};

use crate::bindings::imeta_data_import2;
use crate::metadata::declarations::declaration::{Declaration, DeclarationKind};
use crate::metadata::declarations::delegate_declaration::{DelegateDeclaration, DelegateDeclarationImpl};
use crate::metadata::declarations::method_declaration::MethodDeclaration;
use crate::metadata::declarations::type_declaration::TypeDeclaration;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct GenericDelegateDeclaration {
	base: DelegateDeclaration,
}

impl GenericDelegateDeclaration {
	pub fn new(metadata: Arc<Mutex<IMetaDataImport2>>, token: mdToken) -> Self {
		Self {
			base: DelegateDeclaration::new_overload(
				DeclarationKind::GenericDelegate, metadata, token,
			)
		}
	}

	pub fn number_of_generic_parameters(&self) -> usize {
		let mut count = 0;

		let mut enumerator = std::ptr::null_mut();
		let enumerator_ptr = &mut enumerator;

		debug_assert!(
			imeta_data_import2::enum_generic_params(self.base.base.metadata_mut(), enumerator_ptr, self.base.base.token, None, None, None).is_ok()
		);
		debug_assert!(
			imeta_data_import2::count_enum(self.base.base.metadata_mut(), enumerator, &mut count).is_ok()
		);
		imeta_data_import2::close_enum(self.base.base.metadata_mut(), enumerator);

		return count as usize;
	}
}

impl Declaration for GenericDelegateDeclaration {
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

impl DelegateDeclarationImpl for GenericDelegateDeclaration {
	fn base<'b>(&self) -> &'b TypeDeclaration {
		&self.base.base
	}

	fn invoke_method<'b>(&self) -> &'b MethodDeclaration {
		&self.base.invoke_method
	}
}