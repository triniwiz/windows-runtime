use std::any::Any;
use std::borrow::Cow;


use std::sync::{Arc, Mutex};

use crate::{
	bindings::imeta_data_import2,
	metadata::declaration_factory::DeclarationFactory,
	metadata::declarations::declaration::{Declaration, DeclarationKind},
	metadata::declarations::event_declaration::EventDeclaration,
	metadata::declarations::method_declaration::MethodDeclaration,
	metadata::declarations::property_declaration::PropertyDeclaration,
	metadata::declarations::type_declaration::TypeDeclaration,
	prelude::*,
};
use crate::metadata::declarations::interface_declaration::InterfaceDeclaration;


#[derive(Clone)]
pub struct BaseClassDeclaration<'a> {
	base: TypeDeclaration<'a>,
	implemented_interfaces: Vec<Arc<Mutex<dyn BaseClassDeclarationImpl>>>,
	methods: Vec<MethodDeclaration<'a>>,
	properties: Vec<PropertyDeclaration<'a>>,
	events: Vec<EventDeclaration<'a>>,
}

impl<'a> BaseClassDeclaration<'a> {
	fn make_implemented_interfaces_declarations(metadata: *mut IMetaDataImport2, token: mdTypeDef) -> Vec<Arc<Mutex<dyn BaseClassDeclarationImpl>>> {
		let mut enumerator = std::ptr::null_mut();
		let enumerator_ptr = &mut enumerator;
		let mut count = 0;
		let mut tokens = [0; 1024];

		debug_assert!(
			imeta_data_import2::enum_interface_impls(
				metadata, enumerator_ptr, Some(token), Some(tokens.as_mut_ptr()), Some(tokens.len() as u32), Some(&mut count),
			).is_ok()
		);

		debug_assert!(count < (tokens.len() - 1) as u32);
		imeta_data_import2::close_enum(metadata, enumerator);

		let mut result = Vec::new();

		for token in tokens.into_iter() {
			let mut interface_token = mdTokenNil;
			debug_assert!(
				imeta_data_import2::get_interface_impl_props(
					metadata, *token, None, Some(&mut interface_token),
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

	fn make_method_declarations<'b>(metadata: *mut c_void, token: mdTypeDef) -> Vec<MethodDeclaration<'b>> {
		let mut enumerator = std::ptr::null_mut();
		let enumerator_ptr = &mut enumerator;
		let mut count = 0;
		let mut tokens = [0; 1024];
		debug_assert!(
			imeta_data_import2::enum_methods(
				metadata, enumerator_ptr, token, tokens.as_mut_ptr(), tokens.len() as u32, &mut count,
			).is_ok()
		);

		debug_assert!(count < (tokens.len() - 1) as u32);
		imeta_data_import2::close_enum(metadata, enumerator);

		let mut result = Vec::new();
		for token in tokens.iter() {
			let method = MethodDeclaration::new(metadata, *token);
			if !method.is_exported() {
				continue;
			}
			result.push(method);
		}

		return result;
	}

	fn make_property_declarations<'b>(metadata: *mut c_void, token: mdTypeDef) -> Vec<PropertyDeclaration<'b>> {
		let mut enumerator = std::ptr::null_mut();
		let enumerator_ptr = &mut enumerator;
		let mut count = 0;
		let mut tokens = [0; 1024];

		debug_assert!(
			imeta_data_import2::enum_properties(
				metadata, enumerator_ptr, token, tokens.as_mut_ptr(), tokens.len() as u32, &mut count,
			).is_ok()
		);
		debug_assert!(count < (tokens.len() - 1) as u32);
		imeta_data_import2::close_enum(metadata, enumerator);

		let mut result = Vec::new();
		for token in tokens.iter() {
			let property = PropertyDeclaration::new(metadata, *token);
			if !property.is_exported() {
				continue;
			}
			result.push(property);
		}
		result
	}

	fn make_event_declarations<'b>(metadata: *mut c_void, token: mdTypeDef) -> Vec<EventDeclaration<'b>> {
		let mut enumerator = std::ptr::null_mut();
		let enumerator_ptr = &mut enumerator;
		let mut count = 0;
		let mut tokens = [0; 1024];

		debug_assert!(
			imeta_data_import2::enum_events(
				metadata, enumerator_ptr, token, tokens.as_mut_ptr(), tokens.len() as u32, &mut count,
			).is_ok()
		);
		debug_assert!(count < (tokens.len() - 1) as u32);
		imeta_data_import2::close_enum(metadata, enumerator);

		let mut result = Vec::new();
		for token in tokens.iter() {
			let event = EventDeclaration::new(metadata, *token);
			if !event.is_exported() {
				continue;
			}
			result.push(event);
		}

		result
	}

	pub fn new(kind: DeclarationKind, metadata: *mut IMetaDataImport2, token: mdTypeDef) -> Self {
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

	fn implemented_interfaces<'a>(&self) -> &Vec<InterfaceDeclaration<'a>>;

	fn methods<'a>(&self) -> &Vec<MethodDeclaration<'a>>;

	fn properties<'a>(&self) -> &Vec<PropertyDeclaration<'a>>;

	fn events<'a>(&self) -> &Vec<EventDeclaration<'a>>;

	fn find_members_with_name(&self, name: &str) -> Vec<dyn Declaration> {
		debug_assert!(!name.is_empty());

		let mut result: Vec<Box<dyn Declaration>> = Vec::new();

		// let mut methods = self.find_methods_with_name(name).into_iter().map(|item| Box::new(item)).collect();
		// result.append(&mut methods);

		let mut methods = self.find_methods_with_name(name);
		for method in methods.into_iter() {
			result.push(
				Box::new(method)
			);
		}

		// let mut properties = self.properties().into_iter().filter(|prop| prop.full_name() == name).collect();
		// result.append(&mut properties);

		let mut properties = self.properties().clone();

		for property in properties.into_iter() {
			if property.full_name() == name {
				result.push(
					Box::new(property)
				);
			}
		}

		// let mut events = self.events().into_iter().filter(|event| event.full_name() == name).collect();
		// result.append(&mut events);

		let mut events = self.events().clone();

		for event in events {
			if event.full_name() == name {
				result.push(
					Box::new(event)
				)
			}
		}

		return result;
	}

	fn find_methods_with_name<'b>(&self, name: &str) -> Vec<MethodDeclaration<'b>> {
		debug_assert!(!name.is_empty());

		let mut enumerator = std::mem::MaybeUninit::uninit();
		let enumerator_ptr = &mut enumerator.as_mut_ptr();
		let mut method_tokens: Vec<mdMethodDef> = vec![0; 1024];
		let mut methods_count = 0;
		let name = windows::HSTRING::from(name);
		let name = name.as_wide();
		let base = self.base();
		debug_assert!(
			imeta_data_import2::enum_methods_with_name(
				base.metadata, enumerator_ptr, base.token, name.as_ptr(),
				method_tokens.as_mut_ptr(), method_tokens.len() as u32, &mut methods_count,
			).is_ok()
		);
		imeta_data_import2::close_enum(base.metadata, enumerator.as_mut_ptr());

		method_tokens.into_iter().map(|method_token| MethodDeclaration::new(base.metadata, method_token)).collect()
	}
}

impl<'a> BaseClassDeclarationImpl for BaseClassDeclaration<'a> {
	fn base(&self) -> &TypeDeclaration<'a> {
		&self.base
	}

	fn implemented_interfaces(&self) -> &Vec<Arc<Mutex<dyn BaseClassDeclarationImpl>>> {
		&self.implemented_interfaces
	}

	fn methods(&self) -> &Vec<MethodDeclaration<'a>> {
		&self.methods
	}

	fn properties(&self) -> &Vec<PropertyDeclaration<'a>> {
		&self.properties
	}

	fn events(&self) -> &Vec<EventDeclaration<'a>> {
		&self.events
	}
}

impl<'a> Declaration for BaseClassDeclaration<'a> {
	fn name<'b>(&self) -> Cow<'b, str> {
		self.full_name()
	}

	fn full_name<'b>(&self) -> Cow<'b, str> {
		self.base().full_name()
	}

	fn kind(&self) -> DeclarationKind {
		self.base().kind
	}
}

