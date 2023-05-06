pub mod generic_delegate_declaration;
pub mod generic_delegate_instance_declaration;


use std::any::Any;
use std::fmt::{Debug, Formatter};
use std::sync::{Arc};
use parking_lot::RwLock;
use windows::core::{GUID, HSTRING, PCWSTR};
use windows::w;
use windows::Win32::System::WinRT::Metadata::{CorTokenType, IMetaDataImport2};
use crate::declarations::declaration::{Declaration, DeclarationKind};
use crate::declarations::method_declaration::MethodDeclaration;
use crate::declarations::type_declaration::TypeDeclaration;
use crate::prelude::get_guid_attribute_value;

const INVOKE_METHOD_NAME: &str = "Invoke";

pub fn get_invoke_method_token(
    metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
    token: CorTokenType,
) -> CorTokenType {
    let mut invoke_method_token = 0;
    if let Some(metadata) = Option::as_ref(&metadata) {
        let metadata = metadata.read();
        let name = HSTRING::from(INVOKE_METHOD_NAME);
        let result = unsafe {
            metadata.FindMethod(
                token.0 as u32,
                PCWSTR(name.as_ptr()),
                0 as _,
                0 as _,
                &mut invoke_method_token,
            )
        };
        assert!(result.is_ok());
    }
    CorTokenType(invoke_method_token as i32)
}

pub trait DelegateDeclarationImpl {
    fn as_declaration(&self) -> &dyn Declaration;
    fn as_declaration_mut(&mut self) -> &mut dyn Declaration;
    fn base(&self) -> &TypeDeclaration;
    fn id(&self) -> GUID {
        match self.base().metadata.as_ref() {
            None => GUID::zeroed(),
            Some(metadata) => {
                let metadata = metadata.read();
                get_guid_attribute_value(Some(&*metadata), self.base().token())
            }
        }
    }
    fn invoke_method(&self) -> &MethodDeclaration;
}


impl Debug for dyn DelegateDeclarationImpl {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DelegateDeclarationImpl")
            .field("base", self.base())
            //.field("id", self.id())
            .field("invoke_method", self.invoke_method())
            .finish()
    }
}

#[derive(Clone, Debug)]
pub struct DelegateDeclaration {
    base: TypeDeclaration,
    invoke_method: MethodDeclaration,
}

impl<'a> DelegateDeclaration {
    pub fn new(metadata: Option<Arc<RwLock<IMetaDataImport2>>>, token: CorTokenType) -> Self {
        Self::new_overload(DeclarationKind::Delegate, metadata, token)
    }

    pub fn new_overload(
        kind: DeclarationKind,
        metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
        token: CorTokenType,
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
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn name(&self) -> &str {
        self.base.name()
    }

    fn full_name(&self) -> &str {
        self.base.full_name()
    }

    fn kind(&self) -> DeclarationKind {
        self.base.kind()
    }
}

impl DelegateDeclarationImpl for DelegateDeclaration {
    fn as_declaration(&self) -> &dyn Declaration {
        self
    }

    fn as_declaration_mut(&mut self) -> &mut dyn Declaration {
        self
    }

    fn base(&self) -> &TypeDeclaration {
        &self.base
    }

    fn invoke_method(&self) -> &MethodDeclaration {
        &self.invoke_method
    }
}