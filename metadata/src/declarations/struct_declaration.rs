use std::any::Any;
use crate::prelude::*;
use std::sync::{Arc};
use parking_lot::RwLock;
use windows::Win32::System::WinRT::Metadata::{CorTokenType, IMetaDataImport2};
use crate::declarations::declaration::{Declaration, DeclarationKind};
use crate::declarations::struct_field_declaration::StructFieldDeclaration;
use crate::declarations::type_declaration::TypeDeclaration;

#[derive(Clone, Debug)]
pub struct StructDeclaration {
    base: TypeDeclaration,
    fields: Vec<StructFieldDeclaration>,
}

impl Declaration for StructDeclaration {
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

impl StructDeclaration {
    pub fn new(metadata: Option<Arc<RwLock<IMetaDataImport2>>>, token: CorTokenType) -> Self {
        Self {
            base: TypeDeclaration::new(
                DeclarationKind::Struct,
                metadata.clone(),
                token,
            ),
            fields: StructDeclaration::make_field_declarations(
                metadata.clone(),
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
        token: CorTokenType,
    ) -> Vec<StructFieldDeclaration> {
        let mut result = Vec::new();

        if let Some(metadata) = Option::as_ref(&metadata) {
            let meta = Arc::clone(metadata);
            let lock = metadata.read();
            let mut enumerator = std::ptr::null_mut();
            let mut count = 0;
            let mut tokens = [0_u32; 1024];
            let mut enumerator_ptr = &mut enumerator;
            let result_inner = unsafe { lock.EnumFields(
                enumerator_ptr,
                token.0 as u32,
                tokens.as_mut_ptr(),
                tokens.len() as u32,
                &mut count,
            )};
            assert!(result_inner.is_ok());


            assert!(count < tokens.len().saturating_sub(1) as u32);

           unsafe { lock.CloseEnum(enumerator) };

            result.reserve(count as usize);

            for i in 0..count as usize {
                result.push(StructFieldDeclaration::new(
                    Some(Arc::clone(&meta)),
                    CorTokenType(tokens[i] as i32),
                ))
            }
        }

        return result;
    }
}