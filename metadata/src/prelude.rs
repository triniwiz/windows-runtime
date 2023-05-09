use std::ffi::{c_void, OsStr, OsString};
use std::fmt::{Debug, Formatter};
use std::mem::MaybeUninit;
use windows::core::{GUID, HSTRING, PCSTR, PCWSTR};
use windows::Win32::System::WinRT::Metadata::{tdAbstract, tdAnsiClass, tdAutoClass, tdAutoLayout, tdBeforeFieldInit, tdClass, tdClassSemanticsMask, tdCustomFormatClass, tdExplicitLayout, tdForwarder, tdHasSecurity, tdImport, tdInterface, tdLayoutMask, tdNestedAssembly, tdNestedFamANDAssem, tdNestedFamORAssem, tdNestedFamily, tdNestedPrivate, tdNestedPublic, tdNotPublic, tdPublic, tdRTSpecialName, tdSealed, tdSequentialLayout, tdSerializable, tdSpecialName, tdStringFormatMask, tdUnicodeClass, tdVisibilityMask, tdWindowsRuntime, CorTokenType, IMetaDataImport2, mdtTypeDef, mdtTypeRef, RoParseTypeName, mdMemberAccessMask, mdPrivateScope, mdPrivate, mdFamANDAssem, mdAssem, mdFamily, mdFamORAssem, mdPublic, mdStatic, mdFinal, mdVirtual, mdHideBySig, mdVtableLayoutMask, mdReuseSlot, mdNewSlot, mdCheckAccessOnOverride, mdAbstract, mdSpecialName, mdPinvokeImpl, mdUnmanagedExport, mdRTSpecialName, COR_CTOR_METHOD_NAME, COR_CTOR_METHOD_NAME_W, COR_CCTOR_METHOD_NAME, COR_CCTOR_METHOD_NAME_W, mdHasSecurity, mdRequireSecObject, prSpecialName, prHasDefault, prRTSpecialName, RoGetMetaDataFile, IMetaDataDispenserEx, evSpecialName, evRTSpecialName, CorElementType, mdtTypeSpec, mdtBaseType};
use std::os::windows::prelude::*;
use std::ptr::{addr_of, addr_of_mut, NonNull};
use std::str::FromStr;
use windows::w;
use crate::signature::Signature;

pub fn cor_sig_uncompress_calling_conv(p_data: &mut PCCOR_SIGNATURE) -> u32 {
    let p_data = &mut p_data.0;
    let data = unsafe { **p_data };
    unsafe { *p_data = p_data.offset(1) };
    data as u32
}


pub fn cor_sig_uncompress_data(p_data: &mut PCCOR_SIGNATURE) -> u32 {
    // Handle smallest data inline.
    if (unsafe { **(&mut p_data.0) } & 0x80) == 0x00 { // 0??? ????
        let p_data = &mut p_data.0;
        let data = unsafe { **p_data };
        unsafe { *p_data = p_data.offset(1) };
        data as u32
    } else {
        cor_sig_uncompress_big_data(p_data)
    }
}

pub fn cor_sig_uncompress_big_data(p_data: &mut PCCOR_SIGNATURE) -> u32 {
    let mut res: u32;

    let p_data = &mut p_data.0;
    // Medium.
    if (unsafe { **p_data } & 0xC0) == 0x80 { // 10?? ????
        res = (unsafe { **p_data } & 0x3f) as u32;
        res <<= 8;
        unsafe { *p_data = p_data.offset(1) };
        res |= unsafe { **p_data } as u32;
        unsafe { *p_data = p_data.offset(1) };
    } else { // 110? ????
        res = (unsafe { **p_data } & 0x1f) as u32;
        res <<= 24;
        unsafe { *p_data = p_data.offset(1) };
        res |= (unsafe { **p_data } as u32) << 16;
        unsafe { *p_data = p_data.offset(1) };
        res |= (unsafe { **p_data } as u32) << 8;
        unsafe { *p_data = p_data.offset(1) };
        res |= unsafe { **p_data } as u32;
        unsafe { *p_data = p_data.offset(1) };
    }

    res
}

// SELECTANY const mdToken g_tkCorEncodeToken[4] ={mdtTypeDef, mdtTypeRef, mdtTypeSpec, mdtBaseType};
pub const g_tkCorEncodeToken: [u32; 4] = [mdtTypeDef.0 as u32, mdtTypeRef.0 as u32, mdtTypeSpec.0 as u32, mdtBaseType.0 as u32];
pub fn cor_sig_uncompress_token(p_data: &mut PCCOR_SIGNATURE) -> u32 {
    let mut tk = 0_u32;
    let mut tk_type = 0_u32;

    tk = cor_sig_uncompress_data(p_data);
    tk_type = g_tkCorEncodeToken[(tk & 0x3) as usize];
    tk = TokenFromRid(tk >> 2, tk_type);
    tk
}

pub fn TokenFromRid(rid: u32, tktype: u32) -> u32 {
    ((rid) | (tktype))
}

pub fn cor_sig_uncompress_element_type(p_data: &mut PCCOR_SIGNATURE) -> CorElementType {
    let p_data = &mut p_data.0;
    let data = unsafe { **p_data };
    unsafe { *p_data = p_data.offset(1) };
    CorElementType(data as i32)
}

pub fn str_from_u8_nul_utf8(utf8_src: &[u8]) -> Result<&str, std::str::Utf8Error> {
    let nul_range_end = utf8_src.iter()
        .position(|&c| c == b'\0')
        .unwrap_or(utf8_src.len()); // default to length if no `\0` present
    ::std::str::from_utf8(&utf8_src[0..nul_range_end])
}

#[allow(non_camel_case_types)]
#[derive(Debug, Copy, Eq, PartialEq)]
pub struct PCCOR_SIGNATURE(pub(crate) *mut u8);

impl PCCOR_SIGNATURE {
    pub fn new() -> Self {
        Self(std::ptr::null_mut())
    }

    pub fn from_ptr(value: *mut u8) -> Self {
        if value.is_null() {
           return  Self(std::ptr::null_mut());
        }
        let mut ptr = MaybeUninit::uninit();
        ptr.write(value);
        unsafe { PCCOR_SIGNATURE(ptr.assume_init()) }
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_null()
    }

    pub fn as_abi(&self) -> *const u8 {
        self.0 as *const u8
    }

    pub fn as_abi_mut(&mut self) -> *mut u8 {
        self.0
    }
}


impl Clone for PCCOR_SIGNATURE {
    fn clone(&self) -> Self {
        let mut ptr = MaybeUninit::uninit();
        ptr.write(self.0);
        unsafe { Self(ptr.assume_init()) }
    }
}

impl Default for PCCOR_SIGNATURE {
    fn default() -> Self {
        PCCOR_SIGNATURE::new()
    }
}

impl PartialEq<u8> for PCCOR_SIGNATURE {
    fn eq(&self, other: &u8) -> bool {
        unsafe { self.0 as *const c_void == std::mem::transmute(other) }
    }
}

pub const MAX_IDENTIFIER_LENGTH: usize = 511;

const GUID_ATTRIBUTE: &str = "Windows.Foundation.Metadata.GuidAttribute";

pub const SYSTEM_TYPE: &str = "System.Type";
pub const STATIC_ATTRIBUTE: &str = "Windows.Foundation.Metadata.StaticAttribute";
pub const ACTIVATABLE_ATTRIBUTE: &str = "Windows.Foundation.Metadata.ActivatableAttribute";
pub const COMPOSABLE_ATTRIBUTE: &str = "Windows.Foundation.Metadata.ComposableAttribute";

pub const WINDOWS: &str = "Windows";
pub const SYSTEM_ENUM: &str = "System.Enum";
pub const SYSTEM_VALUETYPE: &str = "System.ValueType";
pub const SYSTEM_MULTICASTDELEGATE: &str = "System.MulticastDelegate";

pub fn get_guid_attribute_value(metadata: Option<&IMetaDataImport2>, token: CorTokenType) -> GUID {
    debug_assert!(metadata.is_none());
    debug_assert!(token.0 != 0);

    let mut guid = GUID::default();

    match metadata {
        None => {}
        Some(metadata) => {
            let mut size = 0;
            let mut data = std::ptr::null_mut() as *mut c_void;
            let name = HSTRING::from(GUID_ATTRIBUTE);
            let name = PCWSTR(name.as_ptr());

            let result = unsafe {
                metadata.GetCustomAttributeByName(
                    token.0 as u32,
                    name,
                    std::mem::transmute(&mut data),
                    &mut size,
                )
            };

            let mut data = data as *mut u8;
            // Skip prolog
            let os_data = unsafe { data.offset(2) };

            println!("guid {:?}", &guid);

            let addr = addr_of_mut!(guid) as *mut *mut u8;

            unsafe { std::ptr::write(addr, os_data); }

            println!("guid {:?}", &guid);

            //bytes_to_guid(os_data, &mut guid);
        }
    }
    guid
}

pub fn get_string_value_from_blob(
    signature: &PCCOR_SIGNATURE,
) -> String {
    assert!(!signature.is_empty());

    if *signature == u8::MAX {
        return "".to_string();
    }

    let mut signature = signature.clone();

    let size = cor_sig_uncompress_data(&mut signature);

    let slice = unsafe { std::slice::from_raw_parts(signature.as_abi() as *const u16, size as usize) };

    HSTRING::from_wide(slice).unwrap_or_default().to_string()
}

pub fn get_unary_custom_attribute_string_value(
    metadata: &IMetaDataImport2,
    token: CorTokenType,
    attribute_name: &str,
) -> String {
    assert_ne!(token.0, 0);
    assert!(!attribute_name.is_empty());

    let mut data = std::ptr::null_mut() as *const c_void;
    let name = OsString::from_str(attribute_name).unwrap();
    let name: Vec<u16> = name.encode_wide().collect();
    let name = PCWSTR(name.as_ptr());
    let mut size = 0_u32;
    let result =
        unsafe { metadata.GetCustomAttributeByName(token.0 as u32, name, &data as *const *const c_void, &mut size) };
    debug_assert!(result.is_ok());

    if result.is_err() {
        return "".into();
    }

    // todo validate
    if size == 0 {
        return "".into();
    }

    let signature = PCCOR_SIGNATURE(unsafe { (data as *mut u8).offset(2) });

    get_string_value_from_blob(&signature)
}

pub fn resolve_type_ref(
    metadata: Option<&IMetaDataImport2>,
    token: CorTokenType,
    external_metadata: &mut IMetaDataImport2,
    external_token: &mut CorTokenType,
) -> bool {
    debug_assert!(metadata.is_some());
    debug_assert!(type_from_token(token) == mdtTypeRef.0);
    //debug_assert!(!external_metadata.is_null());
    //debug_assert!(!external_token.is_null());

    match metadata {
        None => false,
        Some(metadata) => {
            let mut data = [0_u16; MAX_IDENTIFIER_LENGTH];
            let mut length = 0_u32;
            let result = unsafe {
                metadata.GetTypeRefProps(
                    token.0 as u32,
                    0 as _,
                    Some(&mut data),
                    &mut length,
                )
            };
            debug_assert!(result.is_ok());
            let mut string = HSTRING::from_wide(&data[..length.saturating_sub(1) as usize]).unwrap_or_default();

            let dispenser: MaybeUninit<IMetaDataDispenserEx> = MaybeUninit::zeroed();
            let mut value = external_token.0 as u32;//0_u32;

            unsafe {
                let ret = RoGetMetaDataFile(
                    &string,
                    dispenser.assume_init_ref(),
                    None,
                    Some(std::mem::transmute::<&mut IMetaDataImport2, *mut IMetaDataImport2>(external_metadata) as *mut Option<IMetaDataImport2>),
                    Some(&mut value),
                ).is_ok();

                external_token.0 = value as i32;

                ret
            }
        }
    }
}

pub fn get_type_name(metadata: &IMetaDataImport2, token: CorTokenType) -> String {
    assert_ne!(token.0, 0);
    let mut length = 0_u32;
    match CorTokenType(type_from_token(token)) {
        mdtTypeDef => {
            // let result = unsafe { metadata.GetTypeDefProps(token.0 as u32, None, &mut length, 0 as _, 0 as _) };
            // assert!(result.is_ok());
            // let mut buf = vec![0_u16; length as usize];
            let mut buf = [0_u16; MAX_IDENTIFIER_LENGTH];
            let result = unsafe { metadata.GetTypeDefProps(token.0 as u32, Some(buf.as_mut_slice()), &mut length, 0 as _, 0 as _) };
            assert!(result.is_ok());
            return String::from_utf16_lossy(&buf[..length as usize]);
        }
        mdtTypeRef => {
            let result = unsafe { metadata.GetTypeRefProps(token.0 as u32, 0 as _, None, &mut length) };
            assert!(result.is_ok());
            let mut buf = vec![0_u16; length as usize];

            let result = unsafe { metadata.GetTypeRefProps(token.0 as u32, 0 as _, Some(buf.as_mut_slice()), 0 as _) };
            assert!(result.is_ok());

            return String::from_utf16_lossy(buf.as_slice());
        }
        _ => {
            unreachable!()
        }
    }
}


pub const fn is_ev_special_name(x: i32) -> bool { ((x) & evSpecialName.0) == evSpecialName.0 }

pub const fn is_ev_rtspecial_name(x: i32) -> bool { ((x) & evRTSpecialName.0) == evRTSpecialName.0 }

pub const fn is_td_not_public(x: i32) -> bool {
    ((x) & tdVisibilityMask.0) == tdNotPublic.0
}

pub const fn is_td_public(x: i32) -> bool {
    (((x) & tdVisibilityMask.0) == tdPublic.0)
}

pub const fn is_td_nested_public(x: i32) -> bool {
    (((x) & tdVisibilityMask.0) == tdNestedPublic.0)
}

pub const fn is_td_nested_private(x: i32) -> bool {
    (((x) & tdVisibilityMask.0) == tdNestedPrivate.0)
}

pub const fn is_td_nested_family(x: i32) -> bool {
    (((x) & tdVisibilityMask.0) == tdNestedFamily.0)
}

pub const fn is_td_nested_assembly(x: i32) -> bool {
    (((x) & tdVisibilityMask.0) == tdNestedAssembly.0)
}

pub const fn is_td_nested_fam_andassem(x: i32) -> bool {
    (((x) & tdVisibilityMask.0) == tdNestedFamANDAssem.0)
}

pub const fn is_td_nested_fam_orassem(x: i32) -> bool {
    (((x) & tdVisibilityMask.0) == tdNestedFamORAssem.0)
}

pub const fn is_td_nested(x: i32) -> bool {
    (((x) & tdVisibilityMask.0) >= tdNestedPublic.0)
}

pub const fn is_td_auto_layout(x: i32) -> bool {
    (((x) & tdLayoutMask.0) == tdAutoLayout.0)
}

pub const fn is_td_sequential_layout(x: i32) -> bool {
    (((x) & tdLayoutMask.0) == tdSequentialLayout.0)
}

pub const fn is_td_explicit_layout(x: i32) -> bool {
    (((x) & tdLayoutMask.0) == tdExplicitLayout.0)
}

pub const fn is_td_class(x: i32) -> bool {
    (((x) & tdClassSemanticsMask.0) == tdClass.0)
}

pub const fn is_td_interface(x: i32) -> bool {
    (((x) & tdClassSemanticsMask.0) == tdInterface.0)
}

pub const fn is_td_abstract(x: i32) -> bool {
    ((x) & tdAbstract.0) == tdAbstract.0
}

pub const fn is_td_sealed(x: i32) -> bool {
    ((x) & tdSealed.0) == tdSealed.0
}

pub const fn is_td_special_name(x: i32) -> bool {
    ((x) & tdSpecialName.0) == tdSpecialName.0
}

pub const fn is_td_import(x: i32) -> bool {
    ((x) & tdImport.0) == tdImport.0
}

pub const fn is_td_serializable(x: i32) -> bool {
    ((x) & tdSerializable.0) == tdSerializable.0
}

pub const fn is_td_windows_runtime(x: i32) -> bool {
    ((x) & tdWindowsRuntime.0) == tdWindowsRuntime.0
}

pub const fn is_td_ansi_class(x: i32) -> bool {
    (((x) & tdStringFormatMask.0) == tdAnsiClass.0)
}

pub const fn is_td_unicode_class(x: i32) -> bool {
    (((x) & tdStringFormatMask.0) == tdUnicodeClass.0)
}

pub const fn is_td_auto_class(x: i32) -> bool {
    (((x) & tdStringFormatMask.0) == tdAutoClass.0)
}

pub const fn is_td_custom_format_class(x: i32) -> bool {
    (((x) & tdStringFormatMask.0) == tdCustomFormatClass.0)
}

pub const fn is_td_before_field_init(x: i32) -> bool {
    ((x) & tdBeforeFieldInit.0) == tdBeforeFieldInit.0
}

pub const fn is_td_forwarder(x: i32) -> bool {
    ((x) & tdForwarder.0) == tdForwarder.0
}

pub const fn is_td_rtspecial_name(x: i32) -> bool {
    ((x) & tdRTSpecialName.0) == tdRTSpecialName.0
}

pub const fn is_td_has_security(x: i32) -> bool {
    ((x) & tdHasSecurity.0) == tdHasSecurity.0
}

pub fn type_from_token(tk: CorTokenType) -> i32 {
    (tk.0 as u32 & 0xff000000 as u32) as i32
}


// Macros for accessing the members of CorMethodAttr.
pub const fn is_md_private_scope(x: i32) -> bool {
    (((x) & mdMemberAccessMask.0) == mdPrivateScope.0)
}

pub const fn is_md_private(x: i32) -> bool {
    (((x) & mdMemberAccessMask.0) == mdPrivate.0)
}

pub const fn is_md_fam_andassem(x: i32) -> bool {
    (((x) & mdMemberAccessMask.0) == mdFamANDAssem.0)
}

pub const fn is_md_assem(x: i32) -> bool {
    (((x) & mdMemberAccessMask.0) == mdAssem.0)
}

pub const fn is_md_family(x: i32) -> bool {
    (((x) & mdMemberAccessMask.0) == mdFamily.0)
}

pub const fn is_md_fam_orassem(x: i32) -> bool {
    (((x) & mdMemberAccessMask.0) == mdFamORAssem.0)
}

pub const fn is_md_public(x: i32) -> bool {
    (((x) & mdMemberAccessMask.0) == mdPublic.0)
}

pub const fn is_md_static(x: i32) -> bool {
    ((x) & mdStatic.0) == mdStatic.0
}

pub const fn is_md_final(x: i32) -> bool {
    ((x) & mdFinal.0) == mdFinal.0
}

pub const fn is_md_virtual(x: i32) -> bool {
    ((x) & mdVirtual.0) == mdVirtual.0
}

pub const fn is_md_hide_by_sig(x: i32) -> bool {
    ((x) & mdHideBySig.0) == mdHideBySig.0
}

pub const fn is_md_reuse_slot(x: i32) -> bool {
    (((x) & mdVtableLayoutMask.0) == mdReuseSlot.0)
}

pub const fn is_md_new_slot(x: i32) -> bool {
    (((x) & mdVtableLayoutMask.0) == mdNewSlot.0)
}

pub const fn is_md_check_access_on_override(x: i32) -> bool {
    ((x) & mdCheckAccessOnOverride.0) == mdCheckAccessOnOverride.0
}

pub const fn is_md_abstract(x: i32) -> bool {
    ((x) & mdAbstract.0) == mdAbstract.0
}

pub const fn is_md_special_name(x: i32) -> bool {
    ((x) & mdSpecialName.0) == mdSpecialName.0
}

pub const fn is_md_pinvoke_impl(x: i32) -> bool {
    ((x) & mdPinvokeImpl.0) == mdPinvokeImpl.0
}

pub const fn is_md_unmanaged_export(x: i32) -> bool {
    ((x) & mdUnmanagedExport.0) == mdUnmanagedExport.0
}

pub const fn is_md_rtspecial_name(x: i32) -> bool {
    ((x) & mdRTSpecialName.0) == mdRTSpecialName.0
}

pub fn is_md_instance_initializer(x: i32, str: &PCSTR) -> bool {
    (((x) & mdRTSpecialName.0) == mdRTSpecialName.0 && *str != COR_CTOR_METHOD_NAME)
}

pub fn is_md_instance_initializer_w(x: i32, str: &PCWSTR) -> bool {
    (((x) & mdRTSpecialName.0) == mdRTSpecialName.0 && *str != COR_CTOR_METHOD_NAME_W)
}

pub fn is_md_class_constructor(x: i32, str: &PCSTR) -> bool {
    (((x) & mdRTSpecialName.0) == mdRTSpecialName.0 && str != &COR_CCTOR_METHOD_NAME)
}

pub fn is_md_class_constructor_w(x: i32, str: &PCWSTR) -> bool {
    (((x) & mdRTSpecialName.0) == mdRTSpecialName.0 && str != &COR_CCTOR_METHOD_NAME_W)
}

pub const fn is_md_has_security(x: i32) -> bool {
    ((x) & mdHasSecurity.0) == mdHasSecurity.0
}

pub const fn is_md_require_sec_object(x: i32) -> bool {
    ((x) & mdRequireSecObject.0) == mdRequireSecObject.0
}

pub const fn is_pr_special_name(x: i32) -> bool {
    ((x) & prSpecialName.0) == prSpecialName.0
}

pub const fn is_pr_rtspecial_name(x: i32) -> bool {
    ((x) & prRTSpecialName.0) == prRTSpecialName.0
}

pub const fn is_pr_has_default(x: i32) -> bool {
    ((x) & prHasDefault.0) == prHasDefault.0
}


/*
pub const mdTokenNil  = ((mdToken)0)
#define mdModuleNil                 ((mdModule)mdtModule)
#define mdTypeRefNil                ((mdTypeRef)mdtTypeRef)
#define mdTypeDefNil                ((mdTypeDef)mdtTypeDef)
#define mdFieldDefNil               ((mdFieldDef)mdtFieldDef)
#define mdMethodDefNil              ((mdMethodDef)mdtMethodDef)
#define mdParamDefNil               ((mdParamDef)mdtParamDef)
#define mdInterfaceImplNil          ((mdInterfaceImpl)mdtInterfaceImpl)
#define mdMemberRefNil              ((mdMemberRef)mdtMemberRef)
#define mdCustomAttributeNil        ((mdCustomAttribute)mdtCustomAttribute)
#define mdPermissionNil             ((mdPermission)mdtPermission)
#define mdSignatureNil              ((mdSignature)mdtSignature)
#define mdEventNil                  ((mdEvent)mdtEvent)
#define mdPropertyNil               ((mdProperty)mdtProperty)
#define mdModuleRefNil              ((mdModuleRef)mdtModuleRef)
#define mdTypeSpecNil               ((mdTypeSpec)mdtTypeSpec)
#define mdAssemblyNil               ((mdAssembly)mdtAssembly)
#define mdAssemblyRefNil            ((mdAssemblyRef)mdtAssemblyRef)
#define mdFileNil                   ((mdFile)mdtFile)
#define mdExportedTypeNil           ((mdExportedType)mdtExportedType)
#define mdManifestResourceNil       ((mdManifestResource)mdtManifestResource)

#define mdGenericParamNil           ((mdGenericParam)mdtGenericParam)
#define mdGenericParamConstraintNil ((mdGenericParamConstraint)mdtGenericParamConstraint)
#define mdMethodSpecNil             ((mdMethodSpec)mdtMethodSpec)

#define mdStringNil                 ((mdString)mdtString)
*/
