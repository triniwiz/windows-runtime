use windows::Win32::System::WinRT::Metadata::{tdAbstract, tdAnsiClass, tdAutoClass, tdAutoLayout, tdBeforeFieldInit, tdClass, tdClassSemanticsMask, tdCustomFormatClass, tdExplicitLayout, tdForwarder, tdHasSecurity, tdImport, tdInterface, tdLayoutMask, tdNestedAssembly, tdNestedFamANDAssem, tdNestedFamORAssem, tdNestedFamily, tdNestedPrivate, tdNestedPublic, tdNotPublic, tdPublic, tdRTSpecialName, tdSealed, tdSequentialLayout, tdSerializable, tdSpecialName, tdStringFormatMask, tdUnicodeClass, tdVisibilityMask, tdWindowsRuntime, CorTokenType};

pub fn str_from_u8_nul_utf8(utf8_src: &[u8]) -> Result<&str, std::str::Utf8Error> {
    let nul_range_end = utf8_src.iter()
        .position(|&c| c == b'\0')
        .unwrap_or(utf8_src.len()); // default to length if no `\0` present
    ::std::str::from_utf8(&utf8_src[0..nul_range_end])
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
    ((tk.0 as u32) & 0xff000000_u32) as i32
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
