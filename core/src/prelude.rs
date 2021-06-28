#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

pub use libc::wchar_t;
pub use std::ffi::OsString;
pub use std::os::raw::{c_long, c_uint, c_void};
pub use std::os::windows::ffi::*;

pub use super::enums::*;
pub use core_bindings::{
    mdCustomAttribute, mdEvent, mdFieldDef, mdGenericParam, mdInterfaceImpl, mdMemberRef,
    mdMethodDef, mdParamDef, mdProperty, mdToken, mdTypeDef, mdTypeRef, mdTypeSpec,
    IMetaDataImport2 as IMetaDataImport2_, BYTE, DWORD, HCORENUM, LPCWSTR, LPWSTR, PCCOR_SIGNATURE,
    ULONG, ULONG32, UVCP_CONSTANT,
};
use core_bindings::{
    Helpers_RO_E_METADATA_NAME_IS_NAMESPACE, Helpers_mdAssemblyNil, Helpers_mdAssemblyRefNil,
    Helpers_mdCustomAttributeNil, Helpers_mdEventNil, Helpers_mdExportedTypeNil,
    Helpers_mdFieldDefNil, Helpers_mdFileNil, Helpers_mdGenericParamConstraintNil,
    Helpers_mdGenericParamNil, Helpers_mdInterfaceImplNil, Helpers_mdManifestResourceNil,
    Helpers_mdMemberRefNil, Helpers_mdMethodDefNil, Helpers_mdMethodSpecNil, Helpers_mdModuleNil,
    Helpers_mdModuleRefNil, Helpers_mdParamDefNil, Helpers_mdPermissionNil, Helpers_mdPropertyNil,
    Helpers_mdSignatureNil, Helpers_mdStringNil, Helpers_mdTokenNil, Helpers_mdTypeDefNil,
    Helpers_mdTypeRefNil, Helpers_mdTypeSpecNil, GUID, HRESULT,
};

use std::sync::{Arc, Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard, TryLockError};

pub const mdTokenNil: mdToken = Helpers_mdTokenNil;
pub const mdModuleNil: mdToken = Helpers_mdModuleNil;
pub const mdTypeRefNil: mdToken = Helpers_mdTypeRefNil;
pub const mdTypeDefNil: mdToken = Helpers_mdTypeDefNil;
pub const mdFieldDefNil: mdToken = Helpers_mdFieldDefNil;
pub const mdMethodDefNil: mdToken = Helpers_mdMethodDefNil;
pub const mdParamDefNil: mdToken = Helpers_mdParamDefNil;
pub const mdInterfaceImplNil: mdToken = Helpers_mdInterfaceImplNil;
pub const mdMemberRefNil: mdToken = Helpers_mdMemberRefNil;
pub const mdCustomAttributeNil: mdToken = Helpers_mdCustomAttributeNil;
pub const mdPermissionNil: mdToken = Helpers_mdPermissionNil;
pub const mdSignatureNil: mdToken = Helpers_mdSignatureNil;
pub const mdEventNil: mdToken = Helpers_mdEventNil;
pub const mdPropertyNil: mdToken = Helpers_mdPropertyNil;
pub const mdModuleRefNil: mdToken = Helpers_mdModuleRefNil;
pub const mdTypeSpecNil: mdToken = Helpers_mdTypeSpecNil;
pub const mdAssemblyNil: mdToken = Helpers_mdAssemblyNil;
pub const mdAssemblyRefNil: mdToken = Helpers_mdAssemblyRefNil;
pub const mdFileNil: mdToken = Helpers_mdFileNil;
pub const mdExportedTypeNil: mdToken = Helpers_mdExportedTypeNil;
pub const mdManifestResourceNil: mdToken = Helpers_mdManifestResourceNil;
pub const mdGenericParamNil: mdToken = Helpers_mdGenericParamNil;
pub const mdGenericParamConstraintNil: mdToken = Helpers_mdGenericParamConstraintNil;
pub const mdMethodSpecNil: mdToken = Helpers_mdMethodSpecNil;
pub const mdStringNil: mdToken = Helpers_mdStringNil;

pub type CLSID = GUID;

pub const RO_E_METADATA_NAME_IS_NAMESPACE: HRESULT = Helpers_RO_E_METADATA_NAME_IS_NAMESPACE;

pub const MAX_IDENTIFIER_LENGTH: usize = 511;

pub const COR_CTOR_METHOD_NAME: &str = ".ctor";
pub const COR_CCTOR_METHOD_NAME: &str = ".cctor";
pub const COR_ENUM_FIELD_NAME: &str = "value__";

pub fn get_lock_value<'a, T>(value: &Option<Arc<RwLock<T>>>) -> Option<&T> {
    match Option::as_ref(&value) {
        None => None,
        Some(value) => match value.try_read() {
            Ok(value) => Some(&*value),
            Err(_) => None,
        },
    }
}

pub fn get_mutex_value_mut<'a, T>(value: &Option<Arc<RwLock<T>>>) -> Option<&mut T> {
    match Option::as_ref(&value) {
        None => None,
        Some(value) => match value.try_write() {
            Ok(value) => Some(&mut *value),
            Err(_) => None,
        },
    }
}

#[cfg(debug_assertions)]
pub fn core_unreachable() {
    std::unreachable!()
}


#[cfg(debug_assertions)]
pub fn core_unimplemented() {
    std::unimplemented!()
}

pub use crate::bindings::imeta_data_import2::*;
