use std::hint::black_box;
use std::mem::MaybeUninit;
use std::sync::Arc;
use parking_lot::RwLock;
use windows::Win32::System::WinRT::Metadata::{CorTokenType, ELEMENT_TYPE_CLASS, ELEMENT_TYPE_GENERICINST, IMetaDataImport2, mdtTypeDef, mdtTypeRef, mdtTypeSpec};
use crate::{cor_sig_uncompress_element_type, cor_sig_uncompress_token};
use crate::declarations::base_class_declaration::BaseClassDeclarationImpl;
use crate::declarations::delegate_declaration::{DelegateDeclaration, DelegateDeclarationImpl};
use crate::declarations::delegate_declaration::generic_delegate_instance_declaration::GenericDelegateInstanceDeclaration;
use crate::declarations::interface_declaration::generic_interface_instance_declaration::GenericInterfaceInstanceDeclaration;
use crate::declarations::interface_declaration::InterfaceDeclaration;
use crate::prelude::*;

pub struct DeclarationFactory {}

impl DeclarationFactory {
    pub fn make_delegate_declaration(metadata: Option<Arc<RwLock<IMetaDataImport2>>>, token: CorTokenType) -> Option<Box<dyn DelegateDeclarationImpl>> {
        let meta = metadata.clone();
        if let Some(metadata) = metadata.as_ref() {
            let metadata = metadata.read();
            return match CorTokenType(type_from_token(token))
            {
                mdtTypeDef => Some(Box::new(DelegateDeclaration::new(meta, token))),
                mdtTypeRef => {
                    let mut external_metadata = MaybeUninit::uninit();
                    let mut external_delegate_token = CorTokenType::default();
                    let is_resolved = unsafe { resolve_type_ref(Some(&*metadata), token, &mut *external_metadata.as_mut_ptr(), &mut external_delegate_token) };
                    debug_assert!(is_resolved);
                    let external_metadata = unsafe { external_metadata.assume_init() };
                    // todo
                    Some(
                        Box::new(
                            DelegateDeclaration::new(
                                Some(Arc::new(RwLock::new(external_metadata))), external_delegate_token,
                            )
                        )
                    )
                }
                mdtTypeSpec => {
                    let mut signature = PCCOR_SIGNATURE::default(); //[0_u8; MAX_IDENTIFIER_LENGTH];
                    // let mut signature_ptr = signature.as_mut_ptr();
                    let mut signature_size = 0;

                    let result = unsafe {
                        metadata.GetTypeSpecFromToken(
                            token.0 as u32, &mut signature.as_abi_mut(), &mut signature_size,
                        )
                    };
                    debug_assert!(
                        result.is_ok()
                    );

                    println!("make_delegate_declaration {:?}", &signature);

                    //let signature = &signature[..signature_size as usize];

                    let type1 = cor_sig_uncompress_element_type(&mut signature);
                    debug_assert!(
                        type1 == ELEMENT_TYPE_GENERICINST.0
                    );

                    let type2 = cor_sig_uncompress_element_type(&mut signature);
                    debug_assert!(
                        type2 == ELEMENT_TYPE_CLASS.0
                    );

                    let open_generic_delegate_token = cor_sig_uncompress_token(&mut signature);
                    let ret = match CorTokenType(type_from_token(CorTokenType(open_generic_delegate_token))) {
                        mdtTypeDef => {
                            Box::new(GenericDelegateInstanceDeclaration::new(meta.clone(), CorTokenType(open_generic_delegate_token), meta, token))
                        }
                        mdtTypeRef => {
                            let mut external_metadata: MaybeUninit<IMetaDataImport2> = MaybeUninit::zeroed();
                            let mut external_delegate_token = CorTokenType::default();

                            let is_resolved = unsafe {
                                resolve_type_ref(
                                    Some(&*metadata), CorTokenType(open_generic_delegate_token as i32), &mut *external_metadata.as_mut_ptr(), &mut external_delegate_token,
                                )
                            };

                            debug_assert!(is_resolved);

                            // todo

                            let external_metadata = unsafe {
                                Some(
                                    Arc::new(
                                        RwLock::new(
                                            external_metadata.assume_init()
                                        )
                                    )
                                )
                            };

                            Box::new(GenericDelegateInstanceDeclaration::new(external_metadata, external_delegate_token, meta, token))
                        }
                        _ => {
                            std::unreachable!()
                        }
                    };

                    Some(ret)
                }
                _ => {
                    std::unreachable!()
                }
            };
        }
        None
    }
    pub fn make_interface_declaration(metadata: Option<Arc<RwLock<IMetaDataImport2>>>, token: CorTokenType) -> Option<Box<dyn BaseClassDeclarationImpl>> {
        if let Some(metadata) = metadata.as_ref() {
            let meta = Arc::clone(metadata);
            let metadata = metadata.read();
            let ret = match CorTokenType(type_from_token(token)) {
                mdtTypeDef => {
                    Box::new(InterfaceDeclaration::new(Some(meta), token)) as Box<dyn BaseClassDeclarationImpl>
                }
                mdtTypeRef => {
                    let mut external_metadata: MaybeUninit<IMetaDataImport2> = MaybeUninit::zeroed();
                    let mut external_interface_token = CorTokenType::default();//mdtTypeDef;

                    let is_resolved = unsafe { resolve_type_ref(Some(&*metadata), token, &mut *external_metadata.as_mut_ptr(), &mut external_interface_token) };

                    debug_assert!(is_resolved);

                    let external_metadata = if is_resolved {
                        unsafe { Some(Arc::new(RwLock::new(external_metadata.assume_init()))) }
                    } else { None };

                    Box::new(InterfaceDeclaration::new(
                        external_metadata, external_interface_token,
                    ))
                }
                mdtTypeSpec => {
                    let mut signature = std::ptr::null_mut();//[0_u8; MAX_IDENTIFIER_LENGTH];
                    let mut signature_size = 0;
                    let result = unsafe {
                        metadata.GetTypeSpecFromToken(
                            token.0 as u32,
                            &mut signature,
                            &mut signature_size,
                        )
                    };
                    debug_assert!(
                        result.is_ok()
                    );

                    let mut signature = PCCOR_SIGNATURE::from_ptr(signature);

                    // let signature = &signature[..signature_size as usize];

                    let type1 = cor_sig_uncompress_element_type(&mut signature);

                    let mut signature = PCCOR_SIGNATURE::from_ptr(unsafe { (signature.0 as *mut u8).offset(1)});

                    debug_assert!(
                        type1 == ELEMENT_TYPE_GENERICINST.0
                    );

                    let type2 = cor_sig_uncompress_element_type(&mut signature);

                    let mut signature = PCCOR_SIGNATURE::from_ptr(unsafe { (signature.0 as *mut u8).offset(1)});

                    debug_assert!(
                        type2 == ELEMENT_TYPE_CLASS.0
                    );

                    let open_generic_delegate_token = CorTokenType(cor_sig_uncompress_token(&mut signature));

                   // let mut signature = PCCOR_SIGNATURE::from_ptr(unsafe { (signature.0 as *mut u8).offset(1)});

                    match CorTokenType(type_from_token(token)) {
                        mdtTypeSpec => match CorTokenType(type_from_token(open_generic_delegate_token)) {
                            mdtTypeDef => {
                                Box::new(GenericInterfaceInstanceDeclaration::new(
                                    Some(meta.clone()), open_generic_delegate_token, Some(meta), token,
                                ))
                            }
                            mdtTypeRef => {
                                let mut external_metadata: MaybeUninit<IMetaDataImport2> = MaybeUninit::zeroed();
                                let mut external_delegate_token = CorTokenType::default();

                                let is_resolved = unsafe {
                                    resolve_type_ref(
                                        Some(&*metadata), open_generic_delegate_token, &mut *external_metadata.as_mut_ptr(), &mut external_delegate_token,
                                    )
                                };

                                debug_assert!(
                                    is_resolved
                                );

                                let external_metadata = if is_resolved {
                                    unsafe { Some(Arc::new(RwLock::new(external_metadata.assume_init()))) }
                                } else { None };

                                Box::new(
                                    GenericInterfaceInstanceDeclaration::new(
                                        external_metadata, external_delegate_token, Some(meta), token,
                                    )
                                )
                            }
                            _ => {
                                std::unreachable!()
                            }
                        }
                        _ => {
                            std::unreachable!()
                        }
                    }
                }
                _ => {
                    std::unreachable!()
                }
            };

            return Some(ret);
        }
        None
    }
}