use std::ffi::{CString, OsString};
use std::mem::MaybeUninit;
use std::os::windows::prelude::OsStringExt;
use std::sync::Arc;
use parking_lot::{RwLock};
use windows::core::{ComInterface, HRESULT, HSTRING, Interface, PCWSTR, Result};
use windows::{s, w};
use windows::Win32::Foundation::RO_E_METADATA_NAME_IS_NAMESPACE;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoCreateInstanceEx};
use windows::Win32::System::WinRT::Metadata::{CorTokenType, IMetaDataDispenserEx, IMetaDataImport2, mdtTypeDef, mdtTypeRef, RoGetMetaDataFile};
use crate::declarations::class_declaration::ClassDeclaration;
use crate::declarations::declaration::Declaration;
use crate::declarations::declaration::DeclarationKind::Struct;
use crate::declarations::delegate_declaration::DelegateDeclaration;
use crate::declarations::delegate_declaration::generic_delegate_declaration::GenericDelegateDeclaration;
use crate::declarations::enum_declaration::EnumDeclaration;
use crate::declarations::interface_declaration::generic_interface_declaration::GenericInterfaceDeclaration;
use crate::declarations::interface_declaration::InterfaceDeclaration;
use crate::declarations::namespace_declaration::NamespaceDeclaration;
use crate::declarations::struct_declaration::StructDeclaration;
use crate::prelude::*;


#[derive(Debug)]
pub struct MetadataReader {}

impl MetadataReader {
    pub fn find_by_name_w(full_name: PCWSTR) -> Option<Arc<RwLock<dyn Declaration>>> {
        let name = OsString::from_wide(unsafe { full_name.as_wide() });
        let name = name.to_string_lossy();
        MetadataReader::find_by_name(name.as_ref())
    }
    pub fn find_by_name(full_name: &str) -> Option<Arc<RwLock<dyn Declaration>>> {
        if full_name.is_empty() {
            return Some(
                Arc::new(
                    RwLock::new(NamespaceDeclaration::new(""))
                )
            );
        }


        let mut metadata: MaybeUninit<IMetaDataImport2> = MaybeUninit::zeroed();
        let mut token = 0_u32;
        let full_name_hstring = HSTRING::from(full_name);

        let dispenser: MaybeUninit<IMetaDataDispenserEx> = MaybeUninit::zeroed();

        let result = unsafe {
            RoGetMetaDataFile(&full_name_hstring,
                              dispenser.assume_init_ref(),
                              None,
                              Some(metadata.as_mut_ptr() as *mut Option<IMetaDataImport2>), Some(&mut token), /* std::option::Option<*mut u32> */)
        };

        if let Err(error) = result {
            if error.code() == RO_E_METADATA_NAME_IS_NAMESPACE {
                return Some(
                    Arc::new(
                        RwLock::new(NamespaceDeclaration::new(full_name))
                    )
                );
            }
            return None;
        }

        let metadata = unsafe { metadata.assume_init() };

        let mut flags = 0;
        let mut parent_token = 0;
        {
            let result = unsafe {
                metadata.GetTypeDefProps(
                    token, None, 0 as _, &mut flags, &mut parent_token,
                )
            };
            assert!(result.is_ok());
        }


        if is_td_class(flags as i32) {
            let mut parent_name = [0_u16; MAX_IDENTIFIER_LENGTH];
            let pt = CorTokenType(parent_token as i32);
            let tt = type_from_token(pt);
            let mut size = 0_u32;
            match CorTokenType(tt) {
                mdtTypeDef => {
                    let result = unsafe {
                        metadata.GetTypeDefProps(
                            parent_token, Some(&mut parent_name), &mut size, 0 as _, 0 as _)
                    };

                    assert!(
                        result.is_ok()
                    );
                }
                mdtTypeRef => {
                    let result = unsafe { metadata.GetTypeRefProps(parent_token, 0 as _, Some(&mut parent_name), &mut size) };
                    assert!(
                        result.is_ok()
                    );
                }
                _ => {
                    unreachable!()
                }
            }

            let parent_name_buf = &parent_name[0..size as usize];
            let parent_name_string = unsafe { PCWSTR::from_raw(parent_name_buf.as_ptr()).to_string().unwrap_or("".to_string())};

            println!("{}", parent_name_string.as_str());
            // todo find a better way
            // let parent_name_string = String::from_utf16_lossy(&parent_name[0..size as usize]);
            // let parent_name_string = unsafe { CString::from_vec_with_nul_unchecked(parent_name_string.into_bytes())}.into_string().unwrap();

            let metadata = unsafe {
                Arc::new(
                    RwLock::new(
                        metadata
                    )
                )
            };


            if parent_name_string == SYSTEM_ENUM {
                return Some(
                    Arc::new(
                        RwLock::new(EnumDeclaration::new(Some(metadata), CorTokenType(token as i32)))
                    )
                );
            } else if parent_name_string == SYSTEM_VALUETYPE {
                return Some(
                    Arc::new(
                        RwLock::new(StructDeclaration::new(Some(metadata), CorTokenType(token as i32)))
                    )
                );
            } else if parent_name_string == SYSTEM_MULTICASTDELEGATE {
                return if full_name.contains("`") {
                    Some(
                        Arc::new(
                            RwLock::new(GenericDelegateDeclaration::new(Some(metadata), CorTokenType(token as i32)))
                        )
                    )
                } else {
                    Some(
                        Arc::new(
                            RwLock::new(DelegateDeclaration::new(Some(metadata), CorTokenType(token as i32)))
                        )
                    )
                };
            }


            return Some(
                Arc::new(
                    RwLock::new(ClassDeclaration::new(Some(metadata), CorTokenType(token as i32)))
                )
            );

        }


        if is_td_interface(flags as i32) {
            let metadata = unsafe {
                Arc::new(
                    RwLock::new(
                        metadata
                    )
                )
            };
            return if full_name.contains("`") {
                Some(
                    Arc::new(
                        RwLock::new(GenericInterfaceDeclaration::new(Some(metadata), CorTokenType(token as i32)))
                    )
                )
            } else {
                Some(
                    Arc::new(
                        RwLock::new(InterfaceDeclaration::new(Some(metadata), CorTokenType(token as i32)))
                    )
                )
            };
        }


        std::unreachable!();
    }
}