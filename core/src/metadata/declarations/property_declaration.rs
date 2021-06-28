use crate::enums::CorCallingConvention;
use crate::{
    bindings::enums,
    bindings::helpers,
    metadata::declarations::declaration::{Declaration, DeclarationKind},
    metadata::declarations::field_declaration::FieldDeclaration,
    metadata::declarations::method_declaration::MethodDeclaration,
    prelude::*,
};
use std::borrow::Cow;
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug)]
pub struct PropertyDeclaration {
    base: FieldDeclaration,
    getter: MethodDeclaration,
    setter: Option<MethodDeclaration>,
}

impl Declaration for PropertyDeclaration {
    fn is_exported(&self) -> bool {
        let mut property_flags = 0;
        {
            match self.base.metadata() {
                None => {}
                Some(metadata) => {
                    let result = metadata.get_property_props(
                        self.base.token(),
                        None,
                        None,
                        None,
                        None,
                        Some(&mut property_flags),
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                    );
                    debug_assert!(result.is_ok());
                }
            }
        }

        if helpers::is_pr_special_name(property_flags) {
            return false;
        }
        true
    }

    fn name<'b>(&self) -> Cow<'b, str> {
        self.base.name()
    }

    fn full_name<'b>(&self) -> Cow<'b, str> {
        let mut full_name_data = [0_u16; MAX_IDENTIFIER_LENGTH];
        let mut name_length = 0;
        if let Some(metadata) = self.base.metadata() {
            let result = metadata.get_property_props(
                self.base.token(),
                None,
                Some(&mut full_name_data),
                Some(full_name_data.len() as u32),
                Some(&mut name_length),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            );
            debug_assert!(result.is_ok());
        }
        OsString::from_wide(&full_name_data[..name_length]).into()
    }

    fn kind(&self) -> DeclarationKind {
        self.base.kind()
    }
}

impl PropertyDeclaration {
    fn make_getter(
        metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
        token: mdProperty,
    ) -> MethodDeclaration {
        let mut getter_token = mdTokenNil;
        match Option::as_ref(&metadata) {
            None => {}
            Some(metadata) => match metadata.try_read() {
                Ok(metadata) => {
                    let result = metadata.get_property_props(
                        token,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        Some(&mut getter_token),
                        None,
                        None,
                        None,
                    );
                    debug_assert!(result.is_ok());

                    debug_assert!(getter_token != mdMethodDefNil);
                }
                Err(_) => {}
            },
        }
        MethodDeclaration::new(metadata, getter_token)
    }

    fn make_setter<'b>(
        metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
        token: mdProperty,
    ) -> Option<MethodDeclaration> {
        let mut setter_token = mdTokenNil;

        if let Some(metadata) = Option::as_ref(&metadata) {
            match metadata.try_read() {
                Ok(metadata) => {
                    let result = metadata.get_property_props(
                        token,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        Some(&mut setter_token),
                        None,
                        None,
                        None,
                        None,
                    );
                    debug_assert!(result.is_ok());
                }
                Err(_) => {}
            }
        }

        if setter_token == mdMethodDefNil {
            return None;
        }

        return Some(MethodDeclaration::new(metadata, setter_token));
    }

    pub fn new(metadata: Option<Arc<RwLock<IMetaDataImport2>>>, token: mdProperty) -> Self {
        debug_assert!(metadata.is_none());
        debug_assert!(enums::type_from_token(token) == CorTokenType::mdtProperty as u32);
        debug_assert!(token != mdPropertyNil);

        Self {
            base: FieldDeclaration::new(
                DeclarationKind::Property,
                Option::as_ref(&metadata).map(|v| Arc::clone(v)),
                token,
            ),
            getter: PropertyDeclaration::make_getter(
                Option::as_ref(&metadata).map(|v| Arc::clone(v)),
                token,
            ),
            setter: PropertyDeclaration::make_setter(
                Option::as_ref(&metadata).map(|v| Arc::clone(v)),
                token,
            ),
        }
    }

    pub fn is_static(&self) -> bool {
        let mut signature = [0_u8; MAX_IDENTIFIER_LENGTH];
        let mut signature_count = 0;

        if let Some(metadata) = self.base.metadata() {
            let result = metadata.get_property_props(
                self.base.token(),
                None,
                None,
                None,
                None,
                None,
                Some(signature.as_mut_ptr() as _),
                Some(&mut signature_count),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            );

            debug_assert!(result.is_ok());

            debug_assert!(signature_count > 0);
        }

        (signature[0] & CorCallingConvention::ImageCeeCsCallconvGenericinst) == 0
    }

    pub fn is_sealed(&self) -> bool {
        self.getter.is_sealed()
    }

    pub fn getter(&self) -> &MethodDeclaration {
        &self.getter
    }

    pub fn setter(&self) -> Option<&MethodDeclaration> {
        self.setter.as_ref()
    }
}
