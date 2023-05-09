use std::any::Any;
use std::fmt::{Display, Formatter};
use std::sync::Arc;
use parking_lot::{MappedRwLockReadGuard, MappedRwLockWriteGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};
use windows::Win32::System::WinRT::Metadata::{CorTokenType, IMetaDataImport2, mdtFieldDef};
use crate::declarations::declaration::{Declaration, DeclarationKind};

use crate::declarations::type_declaration::TypeDeclaration;
use crate::prelude::*;

#[derive(Clone, Debug)]
pub struct FieldDeclaration {
    kind: DeclarationKind,
    pub(crate) metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
    token: CorTokenType,
    fullname: String,
}

impl Display for FieldDeclaration {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "FieldDeclaration({})", self.full_name())
    }
}

impl Declaration for FieldDeclaration {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn name(&self) -> &str {
        self.fullname.as_str()
    }

    fn full_name(&self) -> &str {
        self.fullname.as_str()
    }

    fn kind(&self) -> DeclarationKind {
        self.kind
    }
}

impl FieldDeclaration {
    pub fn new(
        kind: DeclarationKind,
        metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
        token: CorTokenType,
    ) -> Self {
        assert!(metadata.is_some());
        assert_eq!(type_from_token(token), mdtFieldDef.0);
        assert_ne!(token.0, 0);
        let fullname = match metadata.as_ref() {
            None => "".to_string(),
            Some(metadata) => {
                let metadata = metadata.read();
                let mut full_name_data = [0_u16; MAX_IDENTIFIER_LENGTH];
                let mut length = 0;
                let result = unsafe {
                    metadata.GetFieldProps(
                        token.0 as u32,
                        0 as _,
                        Some(&mut full_name_data),
                        &mut length,
                        0 as _,
                        0 as _,
                        0 as _,
                        0 as _,
                        0 as _,
                        0 as _,
                    )
                };

                debug_assert!(result.is_ok());

                length = length.saturating_sub(1);

                String::from_utf16_lossy(&full_name_data[..length as usize])
            }
        };

        Self {
            kind,
            metadata,
            token,
            fullname,
        }
    }

    pub fn kind(&self) -> DeclarationKind {
        self.kind
    }

    pub fn token(&self) -> CorTokenType {
        self.token
    }

    pub fn metadata(&self) -> Option<MappedRwLockReadGuard<'_, IMetaDataImport2>> {
        self.metadata.as_ref().map(|metadata| {
            RwLockReadGuard::map(
                metadata.read(),
                |metadata| metadata,
            )
        })
    }

    pub fn metadata_mut(&self) -> Option<MappedRwLockWriteGuard<'_, IMetaDataImport2>> {
        self.metadata.as_ref().map(|metadata| {
            RwLockWriteGuard::map(
                metadata.write(),
                |metadata| metadata,
            )
        })
    }
}