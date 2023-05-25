use std::any::Any;
use std::ffi::OsString;
use std::sync::Arc;
use parking_lot::RwLock;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::System::WinRT::Metadata::{CorTokenType, IMAGE_CEE_CS_CALLCONV_GENERICINST, IMetaDataImport2, mdtMethodDef, mdtProperty};
use crate::declarations::declaration::{Declaration, DeclarationKind};
use crate::declarations::field_declaration::FieldDeclaration;
use crate::declarations::method_declaration::MethodDeclaration;

use crate::prelude::*;

#[derive(Clone, Debug)]
pub struct PropertyDeclaration {
    kind: DeclarationKind,
    pub(crate) metadata: Option<IMetaDataImport2>,
    token: CorTokenType,
    full_name: String,
    getter: MethodDeclaration,
    setter: Option<MethodDeclaration>,
}

impl Declaration for PropertyDeclaration {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn is_exported(&self) -> bool {
        let mut property_flags = 0_u32;

        if let Some(metadata) = self.metadata.as_ref() {
            let result = unsafe {
                metadata.GetPropertyProps(
                    self.token.0 as u32,
                    0 as _,
                    PCWSTR::null(),
                    0 as _,
                    0 as _,
                    &mut property_flags,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                )
            };
            debug_assert!(result.is_ok());
        }

        if is_pr_special_name(property_flags as i32) {
            return false;
        }
        true
    }

    fn name(&self) -> &str {
        self.full_name.as_str()
    }

    fn full_name(&self) -> &str {
        self.full_name.as_str()
    }

    fn kind(&self) -> DeclarationKind {
        self.kind
    }
}

impl PropertyDeclaration {

    fn make_getter(
        metadata: Option<&IMetaDataImport2>,
        token: CorTokenType,
    ) -> MethodDeclaration {
        let mut getter_token = 0_u32;
        match metadata {
            None => {}
            Some(metadata) => {
                let result = unsafe {
                    metadata.GetPropertyProps(
                        token.0 as u32,
                        0 as _,
                        None,
                        0 as _,
                        0 as _,
                        0 as _,
                        0 as _,
                        0 as _,
                        0 as _,
                        0 as _,
                        0 as _,
                        0 as _,
                        &mut getter_token,
                        0 as _,
                        0 as _,
                        0 as _,
                    )
                };
                debug_assert!(result.is_ok());

                debug_assert!(getter_token != 0);
            }
        }
        MethodDeclaration::new(metadata, CorTokenType(getter_token as i32))
    }

    fn make_setter(
        metadata: Option<&IMetaDataImport2>,
        token: CorTokenType,
    ) -> Option<MethodDeclaration> {
        let mut setter_token = 0_u32;

        if let Some(metadata) = metadata {
            let result = unsafe {
                metadata.GetPropertyProps(
                    token.0 as u32,
                    0 as _,
                    None,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                    &mut setter_token,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                )
            };
            debug_assert!(result.is_ok());
        }

        if setter_token == mdtMethodDef.0 as u32 {
            return None;
        }

        return Some(MethodDeclaration::new(metadata, CorTokenType(setter_token as i32)));
    }

    pub fn new(metadata: Option<&IMetaDataImport2>, token: CorTokenType) -> Self {
        debug_assert!(metadata.is_some());
        debug_assert!(type_from_token(token) == mdtProperty.0);
        debug_assert!(token.0 != 0);

        let mut full_name = String::new();

        let mut full_name_data = [0_u16; MAX_IDENTIFIER_LENGTH + 1];
        let mut name_length = 0;
        if let Some(metadata) = metadata {
            let name = PCWSTR(full_name_data.as_mut_ptr());
            let result = unsafe {
                metadata.GetPropertyProps(
                    token.0 as u32,
                    0 as _,
                    name,
                    full_name_data.len() as u32,
                    &mut name_length,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                )
            };
            debug_assert!(result.is_ok());

            if name_length > 0 {
                full_name = String::from_utf16_lossy(&full_name_data[..name_length.saturating_sub(1) as usize]);
            }
        }

        Self {
            kind: DeclarationKind::Property,
            metadata: metadata.map(|f| f.clone()),
            token,
            getter: PropertyDeclaration::make_getter(
                metadata.clone(),
                token,
            ),
            setter: PropertyDeclaration::make_setter(
                metadata.clone(),
                token,
            ),
            full_name,
        }
    }

    pub fn is_static(&self) -> bool {
        let mut signature = [0_u8; MAX_IDENTIFIER_LENGTH];
        let mut signature_count = 0;

        if let Some(metadata) = self.metadata.as_ref() {
            let result = unsafe {
                metadata.GetPropertyProps(
                    self.token.0 as u32,
                    0 as _,
                    None,
                    0 as _,
                    0 as _,
                    0 as _,
                    signature.as_mut_ptr() as _,
                    &mut signature_count,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                    0 as _,
                )
            };

            debug_assert!(result.is_ok());

            debug_assert!(signature_count > 0);
        }

        (signature[0] as i32 & IMAGE_CEE_CS_CALLCONV_GENERICINST.0) == 0
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
