use crate::{
	bindings::enums,
	bindings::helpers,
	bindings::imeta_data_import2,
	metadata::declarations::declaration::{Declaration, DeclarationKind},
	metadata::declarations::field_declaration::FieldDeclaration,
	metadata::declarations::method_declaration::MethodDeclaration,
	prelude::*
};
use crate::enums::CorCallingConvention;
use std::borrow::Cow;

#[derive(Clone, Debug)]
pub struct PropertyDeclaration <'a>{
	base: FieldDeclaration<'a>,
	getter: MethodDeclaration<'a>,
	setter: Option<MethodDeclaration<'a>>
}

impl <'a> Declaration for PropertyDeclaration<'a> {
	fn is_exported(&self) -> bool {
		   let mut property_flags = 0;
			debug_assert!(
				imeta_data_import2::get_property_props(
					self.base.metadata, self.base.token, None, None, None, None,
					Some(&mut property_flags), None, None, None, None, None,
					None, None, None, None, None
				).is_ok()
			);

		if helpers::is_pr_special_name(property_flags) {
			return false
		}        
		true
	}

	fn name<'b>(&self) -> Cow<'b, str> {
		self.base.name()
	}

	fn full_name<'b>(&self) -> Cow<'b, str> {
		let mut full_name_data = vec![0_u16; MAX_IDENTIFIER_LENGTH];
		let mut name_length = 0;
		debug_assert!(
			imeta_data_import2::get_property_props(
				self.base.metadata,
				self.base.token,
				None,
				Some(full_name_data.as_mut_ptr()),
				Some(full_name_data.len() as u32),
				Some(&mut name_length),
				None, None, None, None, None, None, None,
				None, None, None, None
			).is_ok()
		);
		full_name_data.resize(name_length as usize, 0);
		OsString::from_wide(full_name_data.as_slice()).into()
	}

	fn kind(&self) -> DeclarationKind {
		self.base.kind
	}
}

impl<'a> PropertyDeclaration<'a> {

	fn make_getter<'b>(metadata: *mut c_void, token : mdProperty) -> MethodDeclaration<'b> {
		let mut getter_token = mdTokenNil;
		debug_assert!(
			imeta_data_import2::get_property_props(
				metadata, token, None, None, None,
				None, None, None, None,None,
				None,None,None,Some(&mut getter_token),
				None,None, None
			).is_ok()
		);

		debug_assert!(getter_token != mdMethodDefNil);

		MethodDeclaration::new(
			metadata, getter_token
		)
	}

	fn make_setter<'b>(metadata: *mut c_void, token : mdProperty) -> Option<MethodDeclaration<'b>>{
		let mut setter_token =  mdTokenNil;

		debug_assert!(
			imeta_data_import2::get_property_props(
				metadata, token, None, None, None,
					None, None, None,None,None,
				None, None, Some(&mut setter_token), None,
				None, None,None
			).is_ok()
		);


		if setter_token == mdMethodDefNil {
			return None;
		}


		return Some(MethodDeclaration::new(metadata, setter_token));
	}

	pub fn new(metadata: *mut c_void, token: mdProperty) -> Self{
		debug_assert!(metadata);
		debug_assert!(enums::type_from_token(token) == CorTokenType::mdtProperty as u32);
		debug_assert!(token != mdPropertyNil);
		Self {
			base: FieldDeclaration::new(
				DeclarationKind::Property, metadata, token
			),
			getter: PropertyDeclaration::make_getter(
				metadata, token
			),
			setter: PropertyDeclaration::make_setter(metadata, token)
		}
	}

	pub fn is_static(&self) -> bool {
		let mut signature = vec![0_u8;1];
		let mut signature_count =  0;

		debug_assert!(
			imeta_data_import2::get_property_props(
				self.base.metadata, self.base.token, None, None, None,
				None, None, Some(signature.as_mut_ptr() as _), Some(&mut signature_count),
				None, None, None, None, None, None,
				None, None
			)   .is_ok()
		);

		debug_assert!(signature_count > 0);

		signature[0] & CorCallingConvention::ImageCeeCsCallconvGenericinst == 0
	}

	pub fn is_sealed(&self) -> bool {
		self.getter.is_sealed()
	}

	pub fn getter(&self) -> &MethodDeclaration {
		&self.getter
	}

	pub fn setter(&self) -> Option<&MethodDeclaration<'a>> {
		self.setter.as_ref()
	}
}