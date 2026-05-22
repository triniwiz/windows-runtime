use std::ffi::OsString;
use std::mem::MaybeUninit;
use std::os::windows::prelude::OsStringExt;
use std::sync::Arc;
use std::cell::RefCell;
use ahash::{AHashMap, AHashSet};
use parking_lot::{RwLock};
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::RO_E_METADATA_NAME_IS_NAMESPACE;
use windows::Win32::System::WinRT::Metadata::{CorTokenType, IMetaDataDispenserEx, IMetaDataImport2, mdtTypeDef, mdtTypeRef, RoGetMetaDataFile};
use crate::declarations::class_declaration::ClassDeclaration;
use crate::declarations::declaration::Declaration;
use crate::declarations::delegate_declaration::DelegateDeclaration;
use crate::declarations::delegate_declaration::generic_delegate_declaration::GenericDelegateDeclaration;
use crate::declarations::enum_declaration::EnumDeclaration;
use crate::declarations::interface_declaration::generic_interface_declaration::GenericInterfaceDeclaration;
use crate::declarations::interface_declaration::generic_interface_instance_declaration::GenericInterfaceInstanceDeclaration;
use crate::declarations::interface_declaration::InterfaceDeclaration;
use crate::declarations::namespace_declaration::NamespaceDeclaration;
use crate::declarations::struct_declaration::StructDeclaration;
use crate::prelude::*;

// Thread-local cache for resolved declarations.  V8 (and therefore all metadata
// lookups) runs on a single thread, so a thread_local avoids the Send+Sync
// requirements that a global static cache would impose on `dyn Declaration`.
thread_local! {
    static DECLARATION_CACHE: RefCell<AHashMap<String, Arc<RwLock<dyn Declaration>>>> =
        RefCell::new(AHashMap::new());
    static DECLARATION_MISS_CACHE: RefCell<AHashSet<String>> =
        RefCell::new(AHashSet::new());
}


#[derive(Debug)]
pub struct MetadataReader {}

impl MetadataReader {
    pub fn find_by_name_w(full_name: PCWSTR) -> Option<Arc<RwLock<dyn Declaration>>> {
        let name = OsString::from_wide(unsafe { full_name.as_wide() });
        let name = name.to_string_lossy();
        MetadataReader::find_by_name(name.as_ref())
    }
    /// Look up a type by full name, falling back to generic arities 1-4 when
    /// the plain name doesn't exist.  Allows JS code to reference generic
    /// delegates/interfaces without the CLR backtick-arity suffix:
    /// `Windows.Foundation.EventHandler` → `Windows.Foundation.EventHandler`1`.
    pub fn find_by_name_or_generic(full_name: &str) -> Option<Arc<RwLock<dyn Declaration>>> {
        if let Some(d) = MetadataReader::find_by_name(full_name) {
            return Some(d);
        }
        for arity in 1u8..=4 {
            let candidate = format!("{}`{}", full_name, arity);
            if let Some(d) = MetadataReader::find_by_name(&candidate) {
                return Some(d);
            }
        }
        None
    }

    pub fn find_by_name(full_name: &str) -> Option<Arc<RwLock<dyn Declaration>>> {
        let cached = DECLARATION_CACHE.with(|cache| {
            cache.borrow().get(full_name).map(Arc::clone)
        });
        if let Some(arc) = cached {
            return Some(arc);
        }

        let known_miss = DECLARATION_MISS_CACHE.with(|cache| cache.borrow().contains(full_name));
        if known_miss {
            return None;
        }

        let Some(declaration) = MetadataReader::find_by_name_uncached(full_name) else {
            DECLARATION_MISS_CACHE.with(|cache| {
                cache.borrow_mut().insert(full_name.to_string());
            });
            return None;
        };
        
        DECLARATION_CACHE.with(|cache| {
            cache.borrow_mut().insert(full_name.to_string(), Arc::clone(&declaration));
        });
        Some(declaration)
    }

    fn find_by_name_uncached(full_name: &str) -> Option<Arc<RwLock<dyn Declaration>>> {
        if full_name.is_empty() {
            return Some(
                Arc::new(
                    RwLock::new(NamespaceDeclaration::new(""))
                )
            );
        }

        // Closed generic instance (e.g. "IAsyncOperation`1<Windows.Foundation.Uri>").
        // Look up the open generic, then wrap it as a GenericInterfaceInstanceDeclaration
        // so that the correct parameterized COM IID is used for QueryInterface.
        if let Some(angle_pos) = full_name.find('<') {
            let open_name = &full_name[..angle_pos];
            let mut open_metadata: MaybeUninit<IMetaDataImport2> = MaybeUninit::zeroed();
            let mut open_token = 0_u32;
            let open_name_hstring = HSTRING::from(open_name);
            let open_dispenser: MaybeUninit<IMetaDataDispenserEx> = MaybeUninit::zeroed();
            let open_result = unsafe {
                RoGetMetaDataFile(
                    &open_name_hstring,
                    open_dispenser.assume_init_ref(),
                    None,
                    Some(open_metadata.as_mut_ptr() as *mut Option<IMetaDataImport2>),
                    Some(&mut open_token),
                )
            };
            if open_result.is_ok() {
                let open_metadata = unsafe { open_metadata.assume_init() };
                let open_token = CorTokenType(open_token as i32);
                let mut flags = 0u32;
                let mut _parent = 0u32;
                let props_ok = unsafe {
                    open_metadata.GetTypeDefProps(open_token.0 as u32, None, 0 as _, &mut flags, &mut _parent)
                }.is_ok();
                if props_ok && !is_td_class(flags as i32) {
                    let declaration = GenericInterfaceInstanceDeclaration::new_from_names(
                        Some(&open_metadata),
                        open_token,
                        full_name.to_string(),
                        full_name.to_string(),
                    );
                    return Some(Arc::new(RwLock::new(declaration)));
                }
            }
            return None;
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
                    // Unexpected parent token type — not a known WinRT declaration.
                    return None;
                }
            }

            let parent_name_buf = &parent_name[0..size.saturating_sub(1) as usize];
            let parent_name_string = String::from_utf16_lossy(parent_name_buf);


            if parent_name_string == SYSTEM_ENUM {
                return Some(
                    Arc::new(
                        RwLock::new(EnumDeclaration::new(Some(&metadata), CorTokenType(token as i32)))
                    )
                );
            } else if parent_name_string == SYSTEM_VALUETYPE {
                return Some(
                    Arc::new(
                        RwLock::new(StructDeclaration::new(Some(&metadata), CorTokenType(token as i32)))
                    )
                );
            } else if parent_name_string == SYSTEM_MULTICASTDELEGATE {
                return if full_name.contains("`") {
                    Some(
                        Arc::new(
                            RwLock::new(GenericDelegateDeclaration::new(Some(&metadata), CorTokenType(token as i32)))
                        )
                    )
                } else {
                    Some(
                        Arc::new(
                            RwLock::new(DelegateDeclaration::new(Some(&metadata), CorTokenType(token as i32)))
                        )
                    )
                };
            }


            return Some(
                Arc::new(
                    RwLock::new(ClassDeclaration::new(Some(&metadata), CorTokenType(token as i32)))
                )
            );

        }


        if is_td_interface(flags as i32) {
            return if full_name.contains("`") {
                Some(
                    Arc::new(
                        RwLock::new(GenericInterfaceDeclaration::new(Some(&metadata), CorTokenType(token as i32)))
                    )
                )
            } else {
                Some(
                    Arc::new(
                        RwLock::new(InterfaceDeclaration::new(Some(&metadata), CorTokenType(token as i32)))
                    )
                )
            };
        }


        // The token is neither a class-family type nor an interface — not a
        // recognised WinRT declaration.  Return None rather than panicking so
        // callers can handle unknown types gracefully.
        None
    } // end find_by_name_uncached
}
