use crate::bindings::helpers;
use crate::metadata::declarations::declaration::{Declaration, DeclarationKind};
use crate::metadata::declarations::field_declaration::FieldDeclaration;
use crate::prelude::*;
use std::borrow::Cow;
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug)]
pub struct StructFieldDeclaration {
    base: FieldDeclaration,
}

impl Declaration for StructFieldDeclaration {
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

impl StructFieldDeclaration {
    pub fn base(&self) -> &FieldDeclaration {
        &self.base
    }
    pub fn new(metadata: Option<Arc<RwLock<IMetaDataImport2>>>, token: mdFieldDef) -> Self {
        Self {
            base: FieldDeclaration::new(DeclarationKind::StructField, metadata, token),
        }
    }

    pub fn type_<'b>(&self) -> Cow<'b, [u8]> {
        let mut signature = [0_u8; MAX_IDENTIFIER_LENGTH];
        let mut signature_size = 0;
        match self.base.metadata() {
            None => Cow::default(),
            Some(metadata) => {
                let result = metadata.get_field_props(
                    self.base.token(),
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(signature.as_mut_ptr() as *mut *const u8),
                    Some(&mut signature_size),
                    None,
                    None,
                    None,
                );

                debug_assert!(result.is_ok());

                let header = helpers::cor_sig_uncompress_data(signature.as_mut_ptr());
                debug_assert!(
                    CorCallingConvention::from(header)
                        == CorCallingConvention::ImageCeeCsCallconvField
                );

                let result = &signature[..signature_size as usize];
                result.into()
            }
        }
    }
}
