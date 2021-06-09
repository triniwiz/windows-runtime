use crate::{
	prelude::*,
	metadata::declarations::type_declaration::TypeDeclaration,
	metadata::declarations::declaration::{Declaration, DeclarationKind}
};
use std::option::Option::Some;
use std::borrow::Cow;
use crate::bindings::ro_resolve_namespace;

#[derive(Clone, Debug)]
pub struct NamespaceDeclaration<'a> {
	base: TypeDeclaration<'a>,
	children: Vec<String>,
	full_name: &'a str
}


impl<'a> Declaration for NamespaceDeclaration<'a> {
	fn name<'b>(&self) -> Cow<'b, str> {
		let mut fully_qualified_name = self.full_name().to_owned();
		if let Some(index) = fully_qualified_name.find(".") {
			fully_qualified_name = fully_qualified_name.chars().skip(index + 1).collect()
		}
		return fully_qualified_name;
	}

	fn full_name<'b>(&self) -> Cow<'b, str> {
		self.full_name.into()
	}

	fn kind(&self) -> DeclarationKind {
		self.base.kind
	}
}

impl <'a>NamespaceDeclaration <'a>{

	pub fn new(full_name: &str) -> Self {
		// ASSERT(fullName);

		debug_assert!(!full_name.is_empty());

		let mut namespaces_count = 0;

		// Grab size
		unsafe {
			ro_resolve_namespace(
				std::mem::transmute(
					windows::HSTRING::from(full_name)
				),
				None,
				0,
				None,
				None,
				None,
				&mut namespaces_count,
				None
			);
		}



		let mut namespaces: Vec<HSTRING__> = Vec::with_capacity(namespaces_count as usize);
		let namespaces_ptr = &mut namespaces.as_mut_ptr();

		// https://docs.microsoft.com/en-us/windows/win32/api/rometadataresolution/nf-rometadataresolution-roresolvenamespace
		// RoResolveNamespace gives incomplete results, find a better way.


		let hr = ro_resolve_namespace(
			unsafe {
				std::mem::transmute(
					windows::HSTRING::from(full_name)
				)
			},
			None,
			0,
			None,
			None,
			None,
			&mut namespaces_count,
			Some(namespaces_ptr as _)
		);


		let children: Vec<String> = unsafe { namespaces.into_iter().map(|val| {
			std::mem::transmute::<HSTRING__, windows::HSTRING>(val).to_string_lossy()
		}).collect() };



		// The search for the "Windows" namespace on Windows Phone 8.1 fails both on a device and on an emulator with corrupted metadata error.

		// if (FAILED(hr)) {
		// 	return;
		// }


		Self {
			base: TypeDeclaration::new(
				DeclarationKind::Namespace,
				std::mem::MaybeUninit::uninit(),
				0
			),
			children,
			full_name
		}
	}
	pub fn children(&self) -> &[String] {
		self.children.as_slice()
	}
}