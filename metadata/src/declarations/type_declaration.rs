use std::any::Any;
use std::ffi::OsString;
use std::os::windows::prelude::OsStringExt;
use std::sync::Arc;
use parking_lot::{MappedRwLockReadGuard, MappedRwLockWriteGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};
use windows::Win32::System::WinRT::Metadata::{CorTokenType, IMetaDataImport2, mdtTypeDef, mdtTypeRef};
use crate::declarations::declaration::{Declaration, DeclarationKind};
use crate::prelude::*;

#[derive(Clone, Debug)]
pub struct TypeDeclaration {
    kind: DeclarationKind,
    pub(crate) metadata: Option<IMetaDataImport2>,
    token: CorTokenType,
    full_name: String,
    name: String
}

impl Declaration for TypeDeclaration {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn is_exported(&self) -> bool {
        let mut flags = 0;
        match self.metadata() {
            None => false,
            Some(metadata) => {
                let result = unsafe { metadata.GetTypeDefProps(self.token.0 as u32, None, 0 as _, &mut flags, 0 as _)};

                debug_assert!(result.is_ok());

                if !is_td_public(flags as i32) || is_td_special_name(flags as i32) {
                    return false;
                }

                true
            }
        }
    }

    fn name(&self) -> &str {
        self.name.as_str()
    }

    fn full_name(&self) -> &str {
        self.full_name.as_str()
    }

    fn kind(&self) -> DeclarationKind {
        self.kind
    }
}

impl TypeDeclaration {
    pub fn kind(&self) -> DeclarationKind {
        self.kind
    }

    pub fn token(&self) -> CorTokenType {
        self.token
    }

    pub fn new(
        kind: DeclarationKind,
        metadata: Option<&IMetaDataImport2>,
        token: CorTokenType,
    ) -> Self {
        // debug_assert!(metadata.is_none());
        // debug_assert!(
        //     CorTokenType::from(token) == mdtTypeDef
        // );
        // debug_assert!(token != mdTypeDefNil);


        let mut full_name_data = [0_u16; MAX_IDENTIFIER_LENGTH];

        let fullname= match metadata {
            None => String::new(),
            Some(metadata) => {
                let mut length  = 0;
                match CorTokenType(type_from_token(token)){
                    mdtTypeDef => {
                        let _ = unsafe { metadata.GetTypeDefProps(token.0 as u32, Some(&mut full_name_data), &mut length, 0 as _, 0 as _) };
                    }
                    mdtTypeRef => {
                        let _ = unsafe {
                            metadata.GetTypeRefProps(token.0 as u32, 0 as _, Some(&mut full_name_data), &mut length)
                        };
                    }
                    _ => {
                        // match being weird
                        if length == 0 {
                            unreachable!()
                        }
                    }
                }

                length = length.saturating_sub(1);

                String::from_utf16_lossy(&full_name_data[..(length) as usize])
            }
        };

        let mut name = fullname.clone();
        let back_tick_index = name.find('`');
        if let Some(index) = back_tick_index {
            name = name
                .chars()
                .take(0)
                .chain(name.chars().skip(index + 1))
                .collect()
        }

        let dot_index = name.find('.');
        if let Some(index) = dot_index {
            name = name
                .chars()
                .take(0)
                .chain(name.chars().skip(index + 1))
                .collect()
        }


        Self {
            kind,
            metadata: metadata.map(|f| f.clone()),
            token,
            full_name: fullname,
            name
        }
    }

    pub fn metadata(&self) -> Option<&IMetaDataImport2> {
       self.metadata.as_ref()
    }
}