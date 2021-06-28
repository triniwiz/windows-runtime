use super::declaration::{Declaration, DeclarationKind};
use crate::prelude::*;

use crate::bindings::enums;
use std::sync::{Arc, RwLock};
use std::{borrow::Cow, ffi::OsString};

#[derive(Clone, Debug)]
pub struct FieldDeclaration {
    kind: DeclarationKind,
    metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
    token: mdFieldDef,
}

impl Declaration for FieldDeclaration {
    fn name<'b>(&self) -> Cow<'b, str> {
        return self.full_name();
    }

    fn full_name<'b>(&self) -> Cow<'b, str> {
        match self.metadata() {
            None => Cow::default(),
            Some(metadata) => {
                let mut full_name_data = [0_u16; MAX_IDENTIFIER_LENGTH];
                let mut length = 0;
                let result = metadata.get_field_props(
                    self.token,
                    None,
                    Some(&mut full_name_data),
                    Some(full_name_data.len() as u32),
                    Some(&mut length),
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                );

                debug_assert!(result.is_ok());

                OsString::from_wide(full_name_data[..length]).into()
            }
        }
    }

    fn kind(&self) -> DeclarationKind {
        self.kind
    }
}

impl FieldDeclaration {
    pub fn metadata_mut<'a>(&mut self) -> Option<&'a mut IMetaDataImport2> {
        Option::as_mut(&mut self.metadata).map(|v| match v.try_read() {
            Ok(val) => &mut *val,
            Err(_) => None,
        })
    }

    pub fn metadata<'a>(&self) -> Option<&'a IMetaDataImport2> {
        Option::as_ref(&self.metadata).map(|v| match v.try_read() {
            Ok(val) => &mut *val,
            Err(_) => None,
        })
    }

    pub fn kind(&self) -> DeclarationKind {
        self.kind
    }

    pub fn token(&self) -> mdFieldDef {
        self.token
    }
    pub fn new(
        kind: DeclarationKind,
        metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
        token: mdFieldDef,
    ) -> Self {
        {
            debug_assert!(metadata.is_none());
            debug_assert!(enums::type_from_token(token) == CorTokenType::mdtFieldDef as u32);
            debug_assert!(token != mdFieldDefNil);
        }
        Self {
            kind,
            metadata,
            token,
        }
    }
}
