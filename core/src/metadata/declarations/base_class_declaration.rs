use crate::{
	prelude::*,
	metadata::declarations::type_declaration::TypeDeclaration,
	metadata::declarations::interface_declaration::InterfaceDeclaration,
	metadata::declarations::method_declaration::MethodDeclaration,
	metadata::declarations::property_declaration::PropertyDeclaration,
	metadata::declarations::event_declaration::EventDeclaration,
	bindings::imeta_data_import2,
};
use crate::metadata::declarations::declaration::{DeclarationKind, Declaration};

use crate::metadata::declaration_factory::DeclarationFactory;

use std::str::FromStr;
use std::borrow::Cow;

pub struct BaseClassDeclaration<'a> {
	base: TypeDeclaration<'a>,
	implemented_interfaces: Vec<InterfaceDeclaration<'a>>,
	methods: Vec<MethodDeclaration<'a>>,
	properties: Vec<PropertyDeclaration<'a>>,
	events: Vec<EventDeclaration<'a>>,
}

impl BaseClassDeclaration {
	fn make_implemented_interfaces_declarations(metadata: *mut c_void, token: mdTypeDef) {
		let mut enumerator = std::ptr::null_mut();
		let enumer = &mut enumerator;
		let mut count = 0;
		let mut tokens = [0; 1024];

		debug_assert!(
			imeta_data_import2::enum_interface_impls(
				metadata, enumer, Some(token), Some(tokens.as_mut_ptr()), Some(tokens.len() as u32), Some(&mut count),
			).is_ok()
		);

		debug_assert!(count < (tokens.len() - 1) as u32);
		imeta_data_import2::close_enum(metadata, enumerator);

		let mut result = Vec::new();
		for i in 0..count {
			let mut interface_token = mdTokenNil;
			debug_assert!(
				imeta_data_import2::get_interface_impl_props(
					metadata, tokens[i], None, Some(&mut interface_token),
				).is_ok()
			);
			result.push(
				DeclarationFactory::make_interface_declaration(
					metadata, interface_token,
				)
			);
		}

		result
	}

	fn make_method_declarations(metadata: *mut c_void, token: mdTypeDef) -> Vec<MethodDeclaration> {
		let enumerator = std::ptr::null_mut();
		let mut count = 0;
		let mut tokens = [0; 1024];
		debug_assert!(
			imeta_data_import2::enum_methods(
				metadata, enumerator, token, tokens.as_mut_ptr(), tokens.len(), &mut count,
			).is_ok()
		);

		debug_assert!(count < tokens.len() - 1);
		imeta_data_import2::close_enum(metadata, enumerator);

		let mut result = Vec::new();
		for i in 0..count {
			let method = MethodDeclaration::new(metadata, tokens[i]);
			if !method.is_exported() {
				continue;
			}
			result.push(method);
		}

		return result;
	}

	fn make_property_declarations(metadata: *mut c_void, token: mdTypeDef) -> Vec<PropertyDeclaration> {
		let enumerator = std::ptr::null_mut();
		let mut count = 0;
		let mut tokens = [0; 1024];
		debug_assert!(
			imeta_data_import2::enum_pr
		);

		debug_assert!(
			imeta_data_import2::enum_properties(
				metadata, enumerator, token, tokens.as_mut_ptr(), tokens.len(), &mut count,
			).is_ok()
		);
		debug_assert!(count < tokens.len() - 1);
		imeta_data_import2::close_enum(metadata, enumerator);

		let mut result = Vec::new();
		for i in 0..count {
			let property = PropertyDeclaration::new(metadata, tokens[i]);
			if !property.is_exported() {
				continue;
			}
			result.push(property);
		}
		result
	}

	fn make_event_declarations(metadata: *mut c_void, token: mdTypeDef) -> Vec<EventDeclaration> {
		let enumerator = std::ptr::null_mut();
		let mut count = 0;
		let mut tokens = [0; 1024];

		debug_assert!(
			imeta_data_import2::enum_events(
				metadata, enumerator, token, tokens.as_mut_ptr(), tokens.len(), &mut count,
			).is_ok()
		);
		debug_assert!(count < tokens.len() - 1);
		imeta_data_import2::close_enum(metadata, enumerator);

		let mut result = Vec::new();
		for i in 0..count {
			let event = EventDeclaration::new(metadata, tokens[i]);
			if !event.is_exported() {
				continue;
			}
			result.push(event);
		}

		result
	}

	pub fn new(kind: DeclarationKind, metadata: *mut c_void, token: mdTypeDef) -> Self {
		Self {
			base: TypeDeclaration::new(kind, metadata, token),
			implemented_interfaces: BaseClassDeclaration::make_implemented_interfaces_declarations(metadata, token),
			methods: BaseClassDeclaration::make_method_declarations(metadata, token),
			properties: BaseClassDeclaration::make_property_declarations(metadata, token),
			events: BaseClassDeclaration::make_event_declarations(metadata, token),
		}
	}
}

pub trait BaseClassDeclarationImpl {
	fn base(&self) -> &TypeDeclaration;

	fn implemented_interfaces(&self) -> &Vec<InterfaceDeclaration>;

	fn methods(&self) -> &Vec<MethodDeclaration>;

	fn properties(&self) -> &Vec<PropertyDeclaration>;

	fn events(&self) -> &Vec<EventDeclaration>;

	fn find_members_with_name(&self, name: &str) -> Vec<dyn Declaration> {
		debug_assert!(name);

		let mut result = Vec::new();

		let mut methods: Vec<dyn Declaration> = self.find_methods_with_name(name);
		result.append(&mut methods);

		let mut properties = self.properties().iter().filter(|prop| prop.full_name() == name).collect();
		result.append(&mut properties);

		let mut events = self.events().iter().filter(|event| event.full_name() == name).collect();
		result.append(&mut events);

		return result;
	}

	fn find_methods_with_name(&self, name: &str) -> Vec<MethodDeclaration> {
		debug_assert!(name);

		let enumerator = std::ptr::null_mut();
		let mut method_tokens: Vec<mdMethodDef> = vec![0; 1024];
		let mut methods_count = 0;
		let name = OsString::from_str(name).unwrap_or_default().to_wide();
		let base = self.base();
		debug_assert!(
			imeta_data_import2::enum_methods_with_name(
				base.metadata, base.token, enumerator, name.as_ptr(),
				method_tokens.as_mut_ptr(), method_tokens.len(), &mut methods_count,
			).is_ok()
		);
		imeta_data_import2::close_enum(base.metadata, enumerator);

		let mut result = Vec::new();
		for i in 0..methods_count {
			let method_token = method_tokens[i];
			result.push(
				MethodDeclaration::new(base.metadata, method_token)
			)
		}

		return result;
	}
}

impl BaseClassDeclarationImpl for BaseClassDeclaration {
	fn base<'a>(&self) -> &TypeDeclaration<'a> {
		&self.base
	}

	fn implemented_interfaces<'a>(&self) -> &Vec<InterfaceDeclaration<'a>> {
		&self.implemented_interfaces
	}

	fn methods<'a>(&self) -> &Vec<MethodDeclaration<'a>> {
		&self.methods
	}

	fn properties<'a>(&self) -> &Vec<PropertyDeclaration<'a>> {
		&self.properties
	}

	fn events<'a>(&self) -> &Vec<EventDeclaration<'a>> {
		&self.events
	}
}

impl Declaration for BaseClassDeclaration {
	fn name<'a>(&self) -> Cow<'a, str> {
		self.full_name()
	}

	fn full_name<'a>(&self) -> Cow<'a, str> {
		self.base().full_name()
	}

	fn kind(&self) -> DeclarationKind {
		self.base().kind
	}
}

