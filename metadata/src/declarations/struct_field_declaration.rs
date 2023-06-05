use std::any::Any;
use std::borrow::Cow;
use std::ptr::addr_of_mut;
use std::sync::Arc;

use parking_lot::RwLock;
use windows::Win32::System::WinRT::Metadata::{CorTokenType, IMAGE_CEE_CS_CALLCONV_FIELD, IMetaDataImport2};

use crate::declarations::declaration::{Declaration, DeclarationKind};
use crate::declarations::field_declaration::FieldDeclaration;
use crate::prelude::*;

#[derive(Clone, Debug)]
pub struct StructFieldDeclaration {
    base: FieldDeclaration,
}

impl Declaration for StructFieldDeclaration {
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

impl StructFieldDeclaration {
    pub fn base(&self) -> &FieldDeclaration {
        &self.base
    }
    pub fn new(metadata: Option<&IMetaDataImport2>, token: CorTokenType) -> Self {
        Self {
            base: FieldDeclaration::new(DeclarationKind::StructField, metadata, token),
        }
    }

    pub fn type_(&self) -> PCCOR_SIGNATURE {
        let mut signature = PCCOR_SIGNATURE::new();
        let mut signature_size = 0;
        match self.base.metadata() {
            None => {}
            Some(metadata) => {
                let result = unsafe {
                    metadata.GetFieldProps(
                        self.base.token().0 as u32,
                        0 as _,
                        None,
                        0 as _,
                        0 as _,
                        addr_of_mut!(signature.0),
                        &mut signature_size,
                        0 as _,
                        0 as _,
                        0 as _,
                    )
                };

                assert!(result.is_ok());

                let header = cor_sig_uncompress_data(&mut signature);

                assert_eq!(header, IMAGE_CEE_CS_CALLCONV_FIELD.0 as u32);
            }
        }

        signature
    }
}