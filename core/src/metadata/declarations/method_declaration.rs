use crate::{
	bindings::enums,
	bindings::helpers,
	bindings::imeta_data_import2,
	metadata::declarations::declaration::{Declaration, DeclarationKind},
	metadata::declarations::type_declaration::TypeDeclaration,
	metadata::signature::Signature,
	prelude::*
};
use crate::enums::CorCallingConvention;
use crate::metadata::com_helpers::get_unary_custom_attribute_string_value;
use crate::metadata::declarations::parameter_declaration::ParameterDeclaration;
use std::borrow::Cow;

#[derive(Clone, Debug)]
pub struct MethodDeclaration <'a> {
	base: TypeDeclaration<'a>,
	parameters: Vec<ParameterDeclaration<'a>>,
}

const OVERLOAD_ATTRIBUTE: &str = "Windows.Foundation.Metadata.OverloadAttribute";
const DEFAULT_OVERLOAD_ATTRIBUTE: &str ="Windows.Foundation.Metadata.DefaultOverloadAttribute";

impl<'a> MethodDeclaration <'a> {
	pub fn base (&self) -> &TypeDeclaration<'a> {
		&self.base
	}
	pub fn new(metadata: *mut c_void, token: mdMethodDef) -> Self {
		debug_assert!(!metadata.is_null());
		debug_assert!(enums::type_from_token(token) == CorTokenType::mdtMethodDef as u32);
		assert!(token != mdMethodDefNil);


		let mut signature = std::mem::MaybeUninit::uninit();
		let mut signature_ptr = &mut signature;
		let mut signature_size: ULONG = 0;

		debug_assert!(
			imeta_data_import2::get_method_props(
				metadata, token, None, None, None,
				None,None,Some(signature), Some(&mut signature_size),
				None,None
			).is_ok()
		);


		/*
		#if _DEBUG
        PCCOR_SIGNATURE startSignature{ signature };
#endif
		 */


		if helpers::cor_sig_uncompress_calling_conv(signature as _)  == CorCallingConvention::ImageCeeCsCallconvGeneric as u32 {
			std::unimplemented!()
		}

		let mut arguments_count = { helpers::cor_sig_uncompress_data(signature as *const u8) };

		let return_type = Signature::consume_type(signature as *const u8);

		let mut parameters: Vec<ParameterDeclaration> = Vec::new();
		let mut parameter_enumerator = std::mem::MaybeUninit::uninit();
		let enumerator = &mut parameter_enumerator;
		let mut parameters_count: ULONG = 0;
		let mut parameter_tokens: Vec<mdParamDef> = vec![0; 1024];

		debug_assert!(imeta_data_import2::enum_params(metadata, enumerator, token, parameter_tokens.as_mut_ptr(), parameter_tokens.len() as u32,&mut parameters_count).is_ok());
		debug_assert!(parameters_count < (parameter_tokens.len() - 1) as u32);

		imeta_data_import2::close_enum(metadata, parameter_enumerator);

		let mut start_index = 0_usize;

		if arguments_count + 1 == parameters_count {
			start_index += 1;
		}

		for i in start_index..parameters_count as usize{
			let sig_type = Signature::consume_type(signature as *const u8);
			parameters.push(
				ParameterDeclaration::new(
					metadata, parameter_tokens[i], sig_type
				)
			)
		}

		// todo
		//debug_assert!(signature_size == signature);
		//debug_assert!(start_signature + signature_size == signature);

		Self {
			base: TypeDeclaration::new(DeclarationKind::Method, metadata, token),
			parameters
		}
	}

	pub fn is_initializer(&self) -> bool{
		let mut full_name_data = vec![0_u16; MAX_IDENTIFIER_LENGTH];
		let mut method_flags = 0;
		assert!(
			imeta_data_import2::get_method_props(
				self.base.metadata, self.base.token,
				None,
				Some(full_name_data.as_mut_ptr()), Some(full_name_data.len() as u32),
				None, Some(&mut method_flags), None, None, None, None
			).is_ok()
		);

		helpers::is_md_instance_initializer_w(method_flags, full_name_data.as_ptr())
	}

	pub fn is_static(&self) -> bool {
		let mut method_flags = 0;
		assert!(
			imeta_data_import2::get_method_props(
				self.base.metadata, self.base.token, None,
				None, None, None,Some(&mut method_flags),
				None, None, None, None
			).is_ok()
		);
		helpers::is_md_static(method_flags)
	}

	pub fn is_sealed(&self) -> bool {
		let mut method_flags = 0;
		assert!(
			imeta_data_import2::get_method_props(
				self.base.metadata, self.base.token, None,
				None, None, None,Some(&mut method_flags),
				None, None, None, None
			).is_ok()
		);
		helpers::is_md_static(method_flags) || helpers::is_md_final(method_flags)
	}

	pub fn parameters(&self) -> &Vec<ParameterDeclaration> {
		&self.parameters
	}

	pub fn number_of_parameters(&self) -> usize{
		self.parameters.len()
	}

	pub fn overload_name<'b>(&self) -> Cow<'b, str> {
		get_unary_custom_attribute_string_value(self.base.metadata, self.base.token, OVERLOAD_ATTRIBUTE)
	}

	pub fn is_default_overload(&self) -> bool {
		let data = windows::HSTRING::from(DEFAULT_OVERLOAD_ATTRIBUTE).as_wide();
		let get_attribute_result = imeta_data_import2::get_custom_attribute_by_name(
			self.base.metadata, self.base.token, Some(data.as_ptr()),None,None
		);
		debug_assert!(get_attribute_result.is_ok());
return get_attribute_result.0 == 0;
}
}

impl<'a> Declaration for MethodDeclaration<'a> {
	fn is_exported(&self) -> bool {
		let mut method_flags: DWORD = 0;
		debug_assert!(
			imeta_data_import2::get_method_props(
				self.base.metadata, self.base.token, None, None, None, None, Some(&mut method_flags), None, None, None, None
			).is_ok()
		);

		if !(helpers::is_md_public(method_flags) || helpers::is_md_family(method_flags) || helpers::is_md_fam_orassem(method_flags)) {
			return false;
		}

		if helpers::is_md_special_name(method_flags) {
			return false;
		}

		return true;
	}

	fn name<'b>(&self) -> Cow<'b, str> {
		self.full_name()
	}

	fn full_name<'b>(&self) -> Cow<'b, str>{
		let mut name_length = 0;
		let mut data = [0_u16; MAX_IDENTIFIER_LENGTH];
		debug_assert!(
			imeta_data_import2::get_method_props(
				self.base.metadata, self.base.token, None, Some(data.as_mut_ptr()), Some(data.len() as u32),
				Some(&mut name_length), None, None, None, None, None
			).is_ok()
		);
		OsString::from_wide(&data[..name_length as usize]).to_string_lossy()

	}

	fn kind(&self) -> DeclarationKind {
		self.base.kind
	}
}
