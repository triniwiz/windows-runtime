use std::any::Any;
use crate::prelude::*;
use std::borrow::Cow;
use std::sync::{Arc};
use parking_lot::{MappedRwLockReadGuard, MappedRwLockWriteGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};
use windows::Win32::System::WinRT::Metadata::{CorTokenType, ELEMENT_TYPE_BYREF, IMetaDataImport2, mdtParamDef};
use crate::declarations::declaration::{Declaration, DeclarationKind};
use crate::declarations::type_declaration::TypeDeclaration;

#[derive(Clone, Debug)]
pub struct ParameterDeclaration {
    kind: DeclarationKind,
    pub(crate) metadata: Option<IMetaDataImport2>,
    token: CorTokenType,
    parameter_type: PCCOR_SIGNATURE,
    full_name: String,
}

impl ParameterDeclaration {
    pub fn new(
        metadata: Option<&IMetaDataImport2>,
        token: CorTokenType,
        sig_type: PCCOR_SIGNATURE,
    ) -> Self {
        //assert!(metadata.is_none());
        assert_eq!(type_from_token(token), mdtParamDef.0);
        assert_ne!(token.0, 0);

        let full_name = match metadata {
            None => String::new(),
            Some(metadata) => {
                let mut length = 0;

                let result = unsafe {
                    metadata.GetParamProps(
                        token.0 as u32,
                        0 as _,
                        0 as _,
                        None,
                        &mut length,
                        0 as _,
                        0 as _,
                        0 as _,
                        0 as _,
                    )
                };

                assert!(result.is_ok());

                let mut full_name_data = vec![0u16; length as usize];

                let result = unsafe {
                    metadata.GetParamProps(
                        token.0 as u32,
                        0 as _,
                        0 as _,
                        Some(full_name_data.as_mut_slice()),
                        0 as _,
                        0 as _,
                        0 as _,
                        0 as _,
                        0 as _,
                    )
                };
                assert!(result.is_ok());
                String::from_utf16_lossy(&full_name_data[..length.saturating_sub(1) as usize])
            }
        };

        Self {
            kind:DeclarationKind::Parameter,
            metadata: metadata.map(|f| f.clone()),
            token,
            parameter_type: sig_type,
            full_name,
        }
    }

    pub fn is_out(&self) -> bool {
        let mut parameter_type = self.parameter_type.clone();
        cor_sig_uncompress_token(&mut parameter_type)
            == ELEMENT_TYPE_BYREF.0 as u32
    }

    pub fn token(&self) -> CorTokenType {
        self.token
    }

    pub fn type_(&self) -> PCCOR_SIGNATURE {
        self.parameter_type
    }

    pub fn metadata(&self) -> Option<&IMetaDataImport2> {
        self.metadata.as_ref()
    }
}

impl Declaration for ParameterDeclaration {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn name(&self) -> &str {
        self.full_name()
    }

    fn full_name(&self) -> &str {
        self.full_name.as_str()
    }

    fn kind(&self) -> DeclarationKind {
        self.kind
    }
}