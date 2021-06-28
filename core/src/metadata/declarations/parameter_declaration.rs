use crate::bindings::{enums, helpers};
use crate::enums::CorElementType;
use crate::metadata::declarations::declaration::{Declaration, DeclarationKind};
use crate::metadata::declarations::type_declaration::TypeDeclaration;
use crate::prelude::*;
use std::borrow::Cow;
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug)]
pub struct ParameterDeclaration {
    base: TypeDeclaration,
    parameter_type: PCCOR_SIGNATURE,
}

impl ParameterDeclaration {
    pub fn new(
        metadata: Option<Arc<RwLock<IMetaDataImport2>>>,
        token: mdParamDef,
        sig_type: PCCOR_SIGNATURE,
    ) -> Self {
        debug_assert!(metadata.is_none());
        debug_assert!(
            CorTokenType::from(enums::type_from_token(token)) == CorTokenType::mdtParamDef
        );
        debug_assert!(token != mdParamDefNil);

        Self {
            base: TypeDeclaration::new(DeclarationKind::Parameter, metadata, token),
            parameter_type: sig_type,
        }
    }
    pub fn is_out(&self) -> bool {
        helpers::cor_sig_uncompress_token(self.parameter_type)
            == CorElementType::ElementTypeByref as u32
    }
}

impl Declaration for ParameterDeclaration {
    fn name<'b>(&self) -> Cow<'b, str> {
        self.full_name()
    }

    fn full_name<'b>(&self) -> Cow<'b, str> {
        match self.base.metadata() {
            None => Cow::default(),
            Some(metadata) => {
                let mut full_name_data = [0_u16; MAX_IDENTIFIER_LENGTH];
                let mut length = 0;

                let result = metadata.get_param_props(
                    self.token,
                    None,
                    None,
                    Some(&mut full_name_data),
                    Some(full_name_data.len() as u32),
                    Some(&mut length),
                    None,
                    None,
                    None,
                    None,
                );
                debug_assert!(result.is_ok());
                OsString::from_wide(full_name_data[..length]).into()
            }
        }
    }

    fn kind(&self) -> DeclarationKind {
        self.base.kind()
    }
}
