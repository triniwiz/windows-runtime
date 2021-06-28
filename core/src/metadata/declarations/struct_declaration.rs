use crate::metadata::declarations::declaration::{Declaration, DeclarationKind};
use crate::metadata::declarations::struct_field_declaration::StructFieldDeclaration;
use crate::metadata::declarations::type_declaration::TypeDeclaration;
use crate::prelude::*;
use std::borrow::Cow;
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug)]
pub struct StructDeclaration {
    base: TypeDeclaration,
    fields: Vec<StructFieldDeclaration>,
}

impl Declaration for StructDeclaration {
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

impl StructDeclaration {
    pub fn new(metadata: Option<Arc<RwLock<IMetaDataImport2>>>, token: mdTypeDef) -> Self {
        Self {
            base: TypeDeclaration::new(
                DeclarationKind::Struct,
                Option::as_ref(&metadata).map(|v| Arc::clone(v)),
                token,
            ),
            fields: StructDeclaration::make_field_declarations(
                Option::as_ref(&metadata).map(|v| Arc::clone(v)),
                token,
            ),
        }
    }

    pub fn size(&self) -> usize {
        self.fields.len()
    }

    pub fn fields(&self) -> &[StructFieldDeclaration] {
        self.fields.as_slice()
    }

    fn make_field_declarations(
        metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
        token: mdTypeDef,
    ) -> Vec<StructFieldDeclaration> {
        let mut result = Vec::new();

        if let Some(metadata) = Option::as_ref(&metadata) {
            let meta = Arc::clone(metadata);
            if let Ok(metadata) = metadata.try_read() {
                let mut enumerator = std::ptr::null_mut();
                let mut count = 0;
                let mut tokens = [0; 1024];
                let mut enumerator_ptr = &mut enumerator;
                let result_inner = metadata.enum_fields(
                    enumerator_ptr,
                    token,
                    &mut tokens,
                    tokens.len() as u32,
                    &mut count,
                );
                debug_assert!(result_inner.is_ok());

                debug_assert!(count < (tokens.len() - 1) as u32);

                metadata.close_enum(enumerator);

                for i in 0..count {
                    result.push(StructFieldDeclaration::new(
                        Some(Arc::clone(&meta)),
                        tokens[i],
                    ))
                }
            }
        }

        return result;
    }
}
