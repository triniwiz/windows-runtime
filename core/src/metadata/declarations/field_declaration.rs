use super::declaration::{Declaration, DeclarationKind};
use crate::prelude::*;

use crate::bindings::{
    imeta_data_import2,
    helpers,
    enums
};
use std::marker;
use std::ffi::{OsString};

pub struct  FieldDeclaration <'a>{
    pub kind: DeclarationKind,
    pub metadata: *const c_void,
    pub token: mdTypeDef,
    _marker: marker::PhantomData<&'a *const c_void>
}

impl Declaration for FieldDeclaration {
    fn kind(&self) -> DeclarationKind{
        self.kind
    }

    fn is_exported(&self) -> bool {
        let mut flags: DWORD = 0;
        assert(
            imeta_data_import2::get_type_def_props(
                self.token, 0, 0,0, &mut flags, 0
            ).is_ok()
        );

        if !helpers::is_td_public(flags) || helpers::is_td_special_name(flags) {
            return false;
        }

        return true;
    }

    fn name<'b>(&self) -> &'b str {
        return self.full_name();
    }

    fn full_name<'b>(&self) -> &'b str {
        let mut full_name_data = vec![0_u16; MAX_IDENTIFIER_LENGTH];

        let length = helpers::(self.metadata, self.token, full_name_data.as_mut_ptr(), full_name_data.len());
        full_name_data.resize(length as usize, 0);
        OsString::from_wide(name.as_slice())
    }
}

impl FieldDeclaration {
    pub fn new(kind: DeclarationKind, metadata: *const c_void, token: mdTypeDef) -> Self {
        let value = Self {
            kind,
            metadata,
            token,
            _marker: marker::PhantomData
        };

        assert!(value.metadata.is_not_null());
        assert!(enums::type_from_token(value.token) == mdtFieldDef);
        assert!(value.token != mdTypeDefNil);
        value
    }
}