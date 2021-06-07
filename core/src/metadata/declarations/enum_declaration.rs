use crate::prelude::*;

use super::{
    declaration::{
        Declaration,
        DeclarationKind,
    },
    type_declaration::TypeDeclaration,
};

#[derive(Debug)]
pub struct EnumDeclaration<'a> {
    type_declaration: TypeDeclaration<'a>,
}

impl Declaration for EnumDeclaration {
    fn is_exported(&self) -> bool {
        self.type_declaration.is_exported()
    }

    fn name<'a>(&self) -> &'a str {
       self.type_declaration.name()
    }

    fn full_name<'a>(&self) -> &'a str {
        self.type_declaration.full_name()
    }

    fn kind(&self) -> DeclarationKind {
        self.type_declaration.kind
    }
}

impl EnumDeclaration {
    pub fn new(metadata: *mut c_void, token: mdTypeDef) -> Self {
        Self {
            type_declaration: TypeDeclaration::new(DeclarationKind::Enum, metadata, token)
        }
    }
}