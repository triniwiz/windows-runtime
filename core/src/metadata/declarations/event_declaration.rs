use crate::prelude::*;
use crate::bindings::{imeta_data_import2, helpers};
use crate::metadata::declarations::declaration::{Declaration, DeclarationKind};
use crate::metadata::declarations::type_declaration::TypeDeclaration;
use crate::metadata::declarations::delegate_declaration::DelegateDeclaration;
use crate::metadata::declarations::method_declaration::MethodDeclaration;
use std::ffi::c_void;
use crate::metadata::declaration_factory::DeclarationFactory;
use std::borrow::Cow;

#[derive(Clone, Debug)]
pub struct EventDeclaration<'a> {
	base: TypeDeclaration<'a>,
	// Arc ??
	type_: DelegateDeclaration<'a>,
	add_method: MethodDeclaration<'a>,
	remove_method: MethodDeclaration<'a>,
}

impl<'a> EventDeclaration <'a> {
	pub fn make_add_method<'b>(metadata: *mut c_void, token: mdEvent) -> MethodDeclaration<'b> {
		let mut add_method_token = mdTokenNil;
		debug_assert!(
			imeta_data_import2::get_event_props(
				metadata, token, None, None, None,
				None, None, None, Some(&mut add_method_token),
				None, None, None, None, None,
			).is_ok()
		);
		MethodDeclaration::new(metadata, add_method_token)
	}

	pub fn make_remove_method<'b>(metadata: *mut c_void, token: mdEvent) -> MethodDeclaration<'b> {
		let mut remove_method_token = mdTokenNil;
		debug_assert!(
			imeta_data_import2::get_event_props(
				metadata, token, None, None, None,
				None, None, None, None,
				Some(&mut remove_method_token), None, None, None,
				None,
			)
		);

		MethodDeclaration::new(metadata, remove_method_token)
	}

	pub fn make_type<'b>(metadata: *mut c_void, token: mdEvent) -> DelegateDeclaration<'b> {
		let mut delegate_token = mdTokenNil;
		debug_assert!(
			imeta_data_import2::get_event_props(
				metadata, token, None, None, None, None, None,
				Some(&mut delegate_token), None, None, None, None, None, None,
			).is_ok()
		);
		return DeclarationFactory::make_delegate_declaration(metadata, delegate_token);
	}

	pub fn new(metadata: *mut c_void, token: mdEvent) -> Self {
		Self {
			base: TypeDeclaration::new(DeclarationKind::Event, metadata, token),
			type_: EventDeclaration::make_type(metadata, token),
			add_method: EventDeclaration::make_add_method(metadata, token),
			remove_method: EventDeclaration::make_remove_method(metadata, token),
		}
	}

	pub fn is_static(&self) -> bool {
		self.add_method.is_static()
	}

	pub fn is_sealed(&self) -> bool {
		self.add_method.is_sealed()
	}

	pub fn type_(&self) -> &DelegateDeclaration {
		&self.type_
	}

	pub fn add_method(&self) -> &MethodDeclaration {
		&self.add_method
	}

	pub fn remove_method(&self) -> &MethodDeclaration {
		&self.remove_method
	}
}

impl<'a> Declaration for EventDeclaration<'a> {
	fn is_exported(&self) -> bool {
		let mut flags = 0;
		debug_assert!(
			imeta_data_import2::get_event_props(
				self.base.metadata, self.base.token,
				None, None, None, None,
				Some(&mut flags), None, None,
				None, None, None, None, None,
			).is_ok()
		);
		if helpers::is_ev_special_name(flags) {
			return false;
		}

		return true;
	}

	fn name<'b>(&self) -> Cow<'b, str> {
		self.full_name()
	}

	fn full_name<'b>(&self) -> Cow<'b, str> {
		let mut name_data = [0_u16; MAX_IDENTIFIER_LENGTH];
		let mut name_data_length = 0;

		debug_assert!(
			imeta_data_import2::get_event_props(
				self.base.metadata, self.base.token,
				None, Some(name_data.as_mut_ptr()), Some(name_data.len() as u32),
				Some(&mut name_data_length), None, None, None,
				None, None, None, None, None,
			).is_ok()
		);
		OsString::from_wide(&name_data[..name_data_length]).into()
	}

	fn kind(&self) -> DeclarationKind {
		self.base.kind
	}
}