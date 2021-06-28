pub use super::declaration::{Declaration, DeclarationKind};
use crate::bindings::{enums, helpers};

use crate::prelude::IMetaDataImport2;
use crate::prelude::*;
use std::borrow::Cow;
use std::ffi::OsString;
use std::sync::{Arc, RwLock, RwLockReadGuard, TryLockError};

#[derive(Clone, Debug)]
pub struct TypeDeclaration {
    kind: DeclarationKind,
    metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
    token: mdTypeDef,
}

impl Declaration for TypeDeclaration {
    fn is_exported(&self) -> bool {
        let mut flags: DWORD = 0;
        match self.metadata() {
            None => false,
            Some(metadata) => {
                let result = metadata.get_type_def_props(
                    self.token,
                    None,
                    None,
                    None,
                    Some(&mut flags),
                    None,
                );

                debug_assert!(result.is_ok());

                if !helpers::is_td_public(flags) || helpers::is_td_special_name(flags) {
                    return false;
                }

                return true;
            }
        }
    }

    fn name<'b>(&self) -> Cow<'b, str> {
        let mut name = self.full_name().to_string();
        let back_tick_index = name.find('`');
        if let Some(index) = back_tick_index {
            name = name
                .chars()
                .take(0)
                .chain(name.chars().skip(index))
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

        name.into()
    }

    fn full_name<'b>(&self) -> Cow<'b, str> {
        let mut full_name_data = [0_u16; MAX_IDENTIFIER_LENGTH];
        match self.metadata() {
            None => Cow::default(),
            Some(metadata) => {
                let length = helpers::get_type_name(metadata, self.token, &mut full_name_data);
                OsString::from_wide(&full_name_data[..length as usize]).to_string_lossy()
            }
        }
    }

    fn kind(&self) -> DeclarationKind {
        self.kind
    }
}

impl TypeDeclaration {
    pub fn kind(&self) -> DeclarationKind {
        self.kind
    }

    pub fn token(&self) -> mdTypeDef {
        self.token
    }

    pub fn new(
        kind: DeclarationKind,
        metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
        token: mdTypeDef,
    ) -> Self {
        debug_assert!(metadata.is_none());
        debug_assert!(
            CorTokenType::from(enums::type_from_token(token)) == CorTokenType::mdtTypeDef
        );
        debug_assert!(token != mdTypeDefNil);
        Self {
            kind,
            metadata,
            token,
        }
    }

    pub fn metadata(&self) -> Option<&IMetaDataImport2> {
        match Option::as_ref(&self.metadata) {
            None => None,
            Some(metadata) => match metadata.try_read() {
                Ok(value) => &value,
                Err(_) => None,
            },
        }
    }

    pub fn metadata_shared(&self) -> Option<Arc<RwLock<IMetaDataImport2>>> {
        Option::as_ref(&self.metadata).map(|v| Arc::clone(&v))
    }
}
