use std::borrow::Cow;

use crate::{
	bindings::{imeta_data_import2},
	metadata::com_helpers::get_guid_attribute_value,
	metadata::declarations::declaration::{Declaration, DeclarationKind},
	metadata::declarations::method_declaration::MethodDeclaration,
	metadata::declarations::type_declaration::TypeDeclaration,
	prelude::*,
};
use std::sync::{Arc, Mutex};


pub mod generic_delegate_declaration;
pub mod generic_delegate_instance_declaration;

const INVOKE_METHOD_NAME: &str = "Invoke";

pub fn get_invoke_method_token(meta_data: *mut IMetaDataImport2, token: mdTypeDef) -> mdMethodDef {
	let mut invoke_method_token = mdTokenNil;
	let string = windows::HSTRING::from(INVOKE_METHOD_NAME);
	debug_assert!(
		imeta_data_import2::find_method(
			meta_data, token, string.as_wide().as_ptr(), None, None, Some(&mut invoke_method_token),
		).is_ok()
	);
	return invoke_method_token;
}

pub trait DelegateDeclarationImpl {
	fn base(&self) -> &TypeDeclaration;
	fn id(&self) -> GUID {
		get_guid_attribute_value(self.base().metadata_mut(), self.base().token)
	}
	fn invoke_method(&self) -> &MethodDeclaration;
}

#[derive(Clone, Debug)]
pub struct DelegateDeclaration {
	base: TypeDeclaration,
	invoke_method: MethodDeclaration,
}

impl<'a> DelegateDeclaration {
	pub fn new(metadata: Arc<Mutex<IMetaDataImport2>>, token: mdTypeDef) -> Self {
		Self::new_overload(
			DeclarationKind::Delegate, metadata, token,
		)
	}

	pub fn new_overload(kind: DeclarationKind, metadata: Arc<Mutex<IMetaDataImport2>>, token: mdTypeDef) -> Self {
		Self {
			base: TypeDeclaration::new(
				kind, metadata, token,
			),
			invoke_method: MethodDeclaration::new(
				metadata, get_invoke_method_token(metadata, token),
			),
		}
	}
}

impl Declaration for DelegateDeclaration {
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


impl DelegateDeclarationImpl for DelegateDeclaration {
	fn base<'b>(&self) -> &'b TypeDeclaration {
		&self.base
	}

	fn invoke_method<'b>(&self) -> &'b MethodDeclaration {
		&self.invoke_method
	}
}