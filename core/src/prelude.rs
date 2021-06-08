#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

pub use core_bindings::*;
pub use std::ffi::{OsString};
pub use std::os::raw::{c_long, c_uint, c_void};
pub use libc::wchar_t;
pub use std::os::windows::ffi::*;

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

pub use super::enums::*;

pub type CLSID = GUID;

pub const RO_E_METADATA_NAME_IS_NAMESPACE: HRESULT = Helpers_RO_E_METADATA_NAME_IS_NAMESPACE;

pub const MAX_IDENTIFIER_LENGTH: usize = 511;


pub const COR_CTOR_METHOD_NAME: &'static str = ".ctor";
pub const COR_CCTOR_METHOD_NAME: &'static str = ".cctor";
pub const COR_ENUM_FIELD_NAME: &'static str = "value__";
