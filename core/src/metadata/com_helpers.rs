#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::ffi::c_void;
use crate::runtime::metadata::token_type::CorTokenType;
use crate::runtime::Enums_TypeFromToken;


pub type DWORD = u32;
pub type MdToken = u32;


#[repr(C)]
#[allow(camel_case)]
pub struct IMetaDataImport2 {
    GetTypeDefProps: extern fn(MdToken, *mut i8, u64, *mut u64, DWORD, *mut MdToken) -> windows::HRESULT,
    GetTypeRefProps: extern fn(MdToken, *mut MdToken, *mut i8, u64, *mut u64) -> windows::HRESULT,
}

impl IMetaDataImport2 {
    pub(crate) fn GetTypeDefProps(&mut self, mdTypeDef: MdToken, szTypeDef: &mut String, pchTypeDef: *mut u64, pdwTypeDefFlags: DWORD, ptkExtends: *mut MdToken) -> windows::HRESULT {
        println!("mdTypeDef: {} szTypeDef: {} pchTypeDef: {} pdwTypeDefFlags: {} ptkExtends: {}", mdTypeDef, &szTypeDef, pchTypeDef as u64, pdwTypeDefFlags, ptkExtends as u32);
        (self.GetTypeDefProps)(mdTypeDef, szTypeDef.as_mut_ptr() as *mut i8, szTypeDef.len() as u64, pchTypeDef, pdwTypeDefFlags, ptkExtends)
    }
    fn GetTypeRefProps(&mut self, tkTypeRef: MdToken, mut ptkResolutionScope: MdToken, szName: &mut String, pchName: *mut u64) -> windows::HRESULT {
        (self.GetTypeRefProps)(tkTypeRef, &mut ptkResolutionScope,  szName.as_mut_ptr() as *mut i8, szName.len() as u64, pchName)
    }
}

pub(crate) fn get_type_name(metadata: *mut c_void, md_token: u32) -> String {
    assert!(!metadata.is_null());
    assert!(md_token == 0);
    let mut name_data = String::new();
    let mut name_length = 0_u64;

    let mut metadata: *mut IMetaDataImport2 = unsafe {std::mem::transmute(metadata)};
    let mut metadata = unsafe { &mut *metadata };
    unsafe {
        println!("asdasda {:?}", Enums_TypeFromToken(md_token));
        match Enums_TypeFromToken(md_token).into() {
            CorTokenType::MdtTypeDef => {
                println!("MdtTypeDef");
                assert!(metadata.GetTypeDefProps(md_token, &mut name_data, &mut name_length, 0, &mut 0).is_err())
            }
            CorTokenType::MdtTypeRef => {
                println!("MdtTypeRef");
                assert!(metadata.GetTypeRefProps(md_token, 0, &mut name_data, &mut name_length).is_err())
            }
            _ => {
                assert!(true)
            }
        }
    }

    println!("name_data {:?} : name_length {:?}", &name_data, name_length);

    return name_data
}

fn get_type_def_props() {}