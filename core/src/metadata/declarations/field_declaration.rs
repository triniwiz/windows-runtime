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
    pub metadata: *mut c_void,
    pub token: mdFieldDef,
    _marker: marker::PhantomData<&'a *const c_void>
}

impl Declaration for FieldDeclaration {

    fn name<'b>(&self) -> &'b str {
        return self.full_name();
    }

    fn full_name<'b>(&self) -> &'b str {
        let mut full_name_data = vec![0_u16; MAX_IDENTIFIER_LENGTH];
        let mut length = 0;
        debug_assert!(imeta_data_import2::get_field_props(
            self.metadata, self.token, None,
            Some(full_name_data.as_mut_ptr()),
            Some(full_name_data.len() as u32),
            Some(&mut length),
            None,None,
            None, None,
            None,None
        ).is_ok());

        full_name_data.resize(length as usize, 0);
        OsString::from_wide(name.as_slice()).to_string_lossy().as_ref()
    }

    fn kind(&self) -> DeclarationKind{
        self.kind
    }
}

impl FieldDeclaration {
    pub fn new(kind: DeclarationKind, metadata: *mut c_void, token: mdFieldDef) -> Self {
        let value = Self {
            kind,
            metadata,
            token,
            _marker: marker::PhantomData
        };

        assert!(value.metadata.is_not_null());
        assert!(enums::type_from_token(value.token) == CorTokenType::mdtFieldDef as u32);
        assert!(value.token != mdFieldDefNil);
        value
    }
}