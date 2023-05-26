use std::any::Any;
use std::ffi::{c_void};
use std::ptr::addr_of_mut;
use std::sync::Arc;
use parking_lot::RwLock;
use windows::Win32::System::WinRT::Metadata::COR_ENUM_FIELD_NAME_W;
use windows::Win32::System::WinRT::Metadata::CorTokenType;
use windows::Win32::System::WinRT::Metadata::IMAGE_CEE_CS_CALLCONV_FIELD;
use windows::Win32::System::WinRT::Metadata::IMetaDataImport2;
use crate::declarations::declaration::{Declaration, DeclarationKind};

use crate::declarations::enum_member_declaration::EnumMemberDeclaration;
use crate::declarations::type_declaration::TypeDeclaration;
pub use crate::prelude::*;

#[derive(Debug)]
pub struct EnumDeclaration {
    base: TypeDeclaration,
}

impl Declaration for EnumDeclaration {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn is_exported(&self) -> bool {
        self.base.is_exported()
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

impl EnumDeclaration {
    pub fn new(metadata: Option<&IMetaDataImport2>, token: CorTokenType) -> Self {
        Self {
            base: TypeDeclaration::new(DeclarationKind::Enum, metadata, token)
        }
    }

    pub fn size(&self) -> isize {
        let mut size: u32 = 0;

        if let Some(metadata) = self.base.metadata() {
            let mut enumerator = std::ptr::null_mut() as *mut c_void;

            let result = unsafe { metadata.EnumFields(&mut enumerator as *mut *mut c_void, self.base.token().0 as u32, 0 as _, 0, 0 as _) };
            ;

            assert!(result.is_ok());

            let result = unsafe {
                metadata.CountEnum(enumerator, &mut size)
            };

            assert!(result.is_ok());

            unsafe { metadata.CloseEnum(enumerator) };

            size -= 1;
        }

        size as isize
    }

    pub fn type_(&self) -> PCCOR_SIGNATURE {
        let mut type_field = 0_u32;
        match self.base.metadata() {
            None => PCCOR_SIGNATURE::default(),
            Some(metadata) => {
                let result = unsafe {
                    metadata.FindField(
                        self.base.token().0 as u32,
                        COR_ENUM_FIELD_NAME_W, 0 as _, 0 as _, &mut type_field,
                    )
                };
                assert!(result.is_ok());

                let mut signature = PCCOR_SIGNATURE::default();
                let mut signature_size = 0;
                let result = unsafe {
                    metadata.GetFieldProps(
                        type_field,
                        0 as _, None, 0 as _, 0 as _,
                        addr_of_mut!(signature.0),
                        &mut signature_size,
                        0 as _,
                        0 as _,
                        0 as _,
                    )
                };
                assert!(
                    result.is_ok()
                );

                let header = cor_sig_uncompress_data(&mut signature);

                assert_eq!(header, IMAGE_CEE_CS_CALLCONV_FIELD.0 as u32);

                signature
            }
        }
    }

    pub fn enum_for_name(&self, name: &str) -> Option<EnumMemberDeclaration> {
        let enums = self.enums();
        let mut ret = None;
        for item in enums.into_iter() {
            if item.name() == name {
                ret = Some(item);
                break;
            }
        }
        ret
    }

    pub fn enums(&self) -> Vec<EnumMemberDeclaration> {
        let mut enums = vec![];

        if let Some(metadata) = self.base.metadata() {
            let mut enumerator = std::ptr::null_mut();
            let mut size = 0_u32;
            let result = unsafe { metadata.EnumFields(&mut enumerator, self.base.token().0 as u32, 0 as _, 0, 0 as _) };
            assert!(result.is_ok());
            // offset by 1 to remove the __value enum
            let result = unsafe { metadata.ResetEnum(enumerator, 1) };
            assert!(result.is_ok());

            let result = unsafe { metadata.CountEnum(enumerator, &mut size) };
            size -= 1;
            enums.reserve(size as usize);
            assert!(result.is_ok());
            for _ in 0..size {
                let mut field = 0_u32;
                let result = unsafe { metadata.EnumFields(&mut enumerator, self.base.token().0 as u32, &mut field, 1, 0 as _) };
                assert!(result.is_ok());
                enums.push(EnumMemberDeclaration::new(self.base.metadata(), CorTokenType(field as i32)))
            }
            unsafe { metadata.CloseEnum(enumerator) };
        }

        enums
    }
}