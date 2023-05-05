use std::any::Any;
use std::fmt::{Display, Formatter};
use std::sync::Arc;
use parking_lot::{RwLock};
use windows::Win32::System::WinRT::Metadata::{CorElementType, CorTokenType, IMetaDataImport2};
use crate::declarations::declaration::{Declaration, DeclarationKind};
use crate::declarations::field_declaration::FieldDeclaration;
use crate::value::Value;

#[derive(Debug)]
pub struct EnumMemberDeclaration {
    base: FieldDeclaration,
}

impl Display for EnumMemberDeclaration {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "FieldDeclaration({})", self.full_name())
    }
}

impl EnumMemberDeclaration {
    pub fn new(metadata: Option<Arc<RwLock<IMetaDataImport2>>>, token: CorTokenType) -> Self {
        Self {
            base: FieldDeclaration::new(DeclarationKind::EnumMember, metadata, token)
        }
    }

    pub fn value(&self) -> Value {
        let mut value = std::ptr::null_mut();
        let mut value_type = 0_u32;
        match self.base.metadata() {
            None => {
                unreachable!()
            }
            Some(metadata) => {
                let result = unsafe {
                    metadata.GetFieldProps(
                        self.base.token().0 as u32,
                        0 as _,
                        None,
                        0 as _,
                        0 as _,
                        0 as _,
                        0 as _,
                        &mut value_type,
                        &mut value,
                        0 as _,
                    )
                };

                assert!(result.is_ok());

                match CorElementType(value_type as i32) {
                    windows::Win32::System::WinRT::Metadata::ELEMENT_TYPE_I4 => {
                        let value = unsafe { &mut *(value).cast::<i32>()};
                        Value::Int32(*value)
                    }

                    windows::Win32::System::WinRT::Metadata::ELEMENT_TYPE_U4 => {
                        let value = unsafe { &mut *(value).cast::<u32>()};
                        Value::Uint32(*value)
                    }
                    _ => {
                        unreachable!()
                    }
                }
            }
        }
    }
}

impl Declaration for EnumMemberDeclaration {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn is_exported(&self) -> bool {
        self.base.is_exported()
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