use crate::prelude::*;
use crate::metadata::declarations::type_declaration::TypeDeclaration;
use crate::metadata::declarations::method_declaration::MethodDeclaration;
use crate::bindings::{imeta_data_import2, enums};
use crate::metadata::declarations::declaration::{DeclarationKind, Declaration};
use crate::metadata::com_helpers::get_guid_attribute_value;
use crate::metadata::signature::Signature;

const INVOKE_METHOD_NAME: &'static str = "Invoke";

pub fn get_invoke_method_token(meta_data: *mut c_void, token: mdTypeDef) -> mdMethodDef {
	let mut invoke_method_token = mdTokenNil;
	let string = OsString::from(INVOKE_METHOD_NAME);
	debug_assert!(
		imeta_data_import2::find_method(
			meta_data, token, string.to_wide().as_ptr(), None, 0, &mut invoke_method_token,
		)
	);
	return invoke_method_token;
}

pub struct DelegateDeclaration<'a> {
	base: TypeDeclaration<'a>,
	invoke_method: MethodDeclaration<'a>,
}

impl DelegateDeclaration {
	pub fn new(metadata: *mut c_void, token: mdTypeDef) -> Self {
		Self::new_overload(
			DeclarationKind::Delegate, metadata, token,
		)
	}

	pub fn new_overload(kind: DeclarationKind, metadata: *mut c_void, token: mdTypeDef) -> Self {
		Self {
			base: TypeDeclaration::new(
				kind, metadata, token,
			),
			invoke_method: MethodDeclaration::new(
				metadata, get_invoke_method_token(metadata, token),
			),
		}
	}

	pub fn id(&self) -> GUID {
		get_guid_attribute_value(self.base.metadata, self.base.token)
	}

	pub fn invoke_method<'a>(&self) -> &MethodDeclaration<'a> {
		&self.invoke_method
	}
}

pub struct GenericDelegateDeclaration<'a> {
	base: DelegateDeclaration<'a>,
}

impl GenericDelegateDeclaration {
	pub fn new(metadata: *mut c_void, token: mdToken) -> Self {
		Self {
			base: DelegateDeclaration::new_overload(
				DeclarationKind::GenericDelegate, metadata, token,
			)
		}
	}

	pub fn number_of_generic_parameters(&self) -> usize {
		let mut count = 0;

		let mut enumerator = std::ptr::null_mut();
		let enumer = &mut enumerator;

		debug_assert!(
			imeta_data_import2::enum_generic_params(self.base.base.metadata, enumer, self.base.base.token, None, None, None).is_ok()
		);
		debug_assert!(
			imeta_data_import2::count_enum(self.base.base.metadata, enumerator, &mut count).is_ok()
		);
		imeta_data_import2::close_enum(self.base.base.metadata, enumerator);

		return count as usize;
	}
}

pub struct GenericDelegateInstanceDeclaration<'a> {
	base: DelegateDeclaration<'a>,
	closed_token: mdTypeSpec,
	closed_metadata: *mut c_void,
}

impl GenericDelegateInstanceDeclaration {
	pub fn new(open_metadata: *mut c_void, open_token: mdTypeDef, closed_metadata: *mut c_void, closed_token: mdTypeSpec) -> Self {

		debug_assert!(!closed_metadata.is_null());
		debug_assert!(enums::type_from_token(closed_token) == CorTokenType::ComdtTypeSpec as u32);
		debug_assert!(closed_token != mdTypeSpecNil);

		Self {
			base: DelegateDeclaration::new_overload(
				DeclarationKind::GenericDelegateInstance, open_metadata, open_token,
			),
			closed_token,
			closed_metadata,
		}
	}

pub fn id(&self) -> CLSID {
	GenericInstanceIdBuilder::generate_Id(self)
}
}

impl Declaration for DelegateDeclaration {
	fn name<'a>(&self) -> &'a str {
		self.base.name()
	}

	fn full_name<'a>(&self) -> &'a str {
		self.base.full_name()
	}

	fn kind(&self) -> DeclarationKind {
		self.base.kind
	}
}

impl Declaration for GenericDelegateDeclaration {
	fn name<'a>(&self) -> &'a str {
		self.base.name()
	}

	fn full_name<'a>(&self) -> &'a str {
		self.base.full_name()
	}

	fn kind(&self) -> DeclarationKind {
		self.base.kind
	}
}

impl Declaration for GenericDelegateInstanceDeclaration {
	fn name<'a>(&self) -> &'a str {
		self.base.name()
	}

	fn full_name<'a>(&self) -> &'a str {
		let mut signature = vec![0_u8; MAX_IDENTIFIER_LENGTH];
		let mut sig = signature.as_ptr();
		let mut signature_size =  0;
		debug_assert!(
			imeta_data_import2::get_type_spec_from_token(
				self.closed_metadata, self.closed_token,&mut sig,&mut signature_size
			)
		);
		signature.resize(signature_size as usize, 0);
		return Signature::to_string(self.closed_metadata, sig);
	}

	fn kind(&self) -> DeclarationKind {
		self.base.kind
	}
}