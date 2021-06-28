use std::borrow::Cow;

use crate::{
    metadata::com_helpers::get_guid_attribute_value,
    metadata::declarations::declaration::{Declaration, DeclarationKind},
    metadata::declarations::method_declaration::MethodDeclaration,
    metadata::declarations::type_declaration::TypeDeclaration,
    prelude::*,
};
use core_bindings::GUID;
use std::sync::{Arc, RwLock};

pub mod generic_delegate_declaration;
pub mod generic_delegate_instance_declaration;

const INVOKE_METHOD_NAME: &str = "Invoke";

pub fn get_invoke_method_token(
    metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
    token: mdTypeDef,
) -> mdMethodDef {
    let mut invoke_method_token = mdTokenNil;
    let string = windows::HSTRING::from(INVOKE_METHOD_NAME);
    if let Some(metadata) = Option::as_ref(&metadata) {
        if let Ok(metadata) = metadata.try_read() {
            let result = metadata.find_method(
                token,
                string.as_wide().as_ptr(),
                None,
                None,
                Some(&mut invoke_method_token),
            );
            debug_assert!(result.is_ok());
        }
    }
    return invoke_method_token;
}

pub trait DelegateDeclarationImpl {
    fn base(&self) -> &TypeDeclaration;
    fn id(&self) -> GUID {
        get_guid_attribute_value(self.base().metadata_mut(), self.base().token())
    }
    fn invoke_method(&self) -> &MethodDeclaration;
}

#[derive(Clone, Debug)]
pub struct DelegateDeclaration {
    base: TypeDeclaration,
    invoke_method: MethodDeclaration,
}

impl<'a> DelegateDeclaration {
    pub fn new(metadata: Option<Arc<RwLock<IMetaDataImport2>>>, token: mdTypeDef) -> Self {
        Self::new_overload(DeclarationKind::Delegate, metadata, token)
    }

    pub fn new_overload(
        kind: DeclarationKind,
        metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
        token: mdTypeDef,
    ) -> Self {
        Self {
            base: TypeDeclaration::new(
                kind,
                Option::as_ref(&metadata).map(|v| Arc::clone(v)),
                token,
            ),
            invoke_method: MethodDeclaration::new(
                metadata,
                get_invoke_method_token(Option::as_ref(&metadata).map(|v| Arc::clone(v)), token),
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
        self.base.kind()
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
