use std::borrow::Cow;

use core_bindings::{mdTypeDef, mdTypeSpec, IMetaDataImport2};

use crate::bindings::{enums, imeta_data_import2};
use crate::enums::CorTokenType;
use crate::metadata::declarations::declaration::{Declaration, DeclarationKind};
use crate::metadata::declarations::delegate_declaration::{DelegateDeclaration, DelegateDeclarationImpl};
use crate::metadata::generic_instance_id_builder::GenericInstanceIdBuilder;
use crate::metadata::signature::Signature;
use crate::prelude::*;
use crate::metadata::declarations::method_declaration::MethodDeclaration;
use crate::metadata::declarations::type_declaration::TypeDeclaration;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct GenericDelegateInstanceDeclaration {
	base: DelegateDeclaration,
	closed_token: mdTypeSpec,
	closed_metadata: Arc<Mutex<IMetaDataImport2>>,
}

impl GenericDelegateInstanceDeclaration {
	pub fn new(open_metadata: Arc<Mutex<IMetaDataImport2>>, open_token: mdTypeDef, closed_metadata: Arc<Mutex<IMetaDataImport2>>, closed_token: mdTypeSpec) -> Self {
		debug_assert!(!closed_metadata.is_null());
		debug_assert!(CorTokenType::from(enums::type_from_token(closed_token)) == CorTokenType::mdtTypeSpec);
		debug_assert!(closed_token != mdTypeSpecNil);

		Self {
			base: DelegateDeclaration::new_overload(
				DeclarationKind::GenericDelegateInstance, open_metadata, open_token,
			),
			closed_token,
			closed_metadata,
		}
	}
}

impl Declaration for GenericDelegateInstanceDeclaration {
	fn name<'b>(&self) -> Cow<'b, str> {
		self.base.name()
	}

	fn full_name<'b>(&self) -> Cow<'b, str> {
		let mut signature = vec![0_u8; MAX_IDENTIFIER_LENGTH];
		let mut sig = signature.as_ptr();
		let mut signature_size = 0;
		debug_assert!(
			imeta_data_import2::get_type_spec_from_token(
				self.closed_metadata, self.closed_token, &mut sig, &mut signature_size,
			).is_ok()
		);
		signature.resize(signature_size as usize, 0);
		Signature::to_string(self.closed_metadata, sig).into()
	}

	fn kind(&self) -> DeclarationKind {
		self.base.kind()
	}
}


impl DelegateDeclarationImpl for GenericDelegateInstanceDeclaration {
	fn base<'b>(&self) -> &'b TypeDeclaration {
		&self.base.base
	}

	fn id(&self) -> CLSID {
		GenericInstanceIdBuilder::generate_id(self)
	}

	fn invoke_method<'b>(&self) -> &MethodDeclaration<'b> {
		&self.base.invoke_method
	}
}