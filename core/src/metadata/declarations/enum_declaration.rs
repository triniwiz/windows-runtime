use crate::prelude::*;

use super::{
    declaration::{
        Declaration,
        DeclarationKind,
    },
    type_declaration::TypeDeclaration,
};

pub struct EnumDeclaration {
    type_declaration: TypeDeclaration,
}

impl Declaration for EnumDeclaration {
    fn kind(&self) -> DeclarationKind {
        self.type_declaration.kind
    }
}

impl EnumDeclaration {
    pub fn new(metadata: *const c_void, token: mdTypeDef) -> Self {
        Self {
            type_declaration: TypeDeclaration::new(DeclarationKind::Enum, metadata, token)
        }
    }
}