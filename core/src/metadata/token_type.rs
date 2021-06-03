#[derive(Debug, Clone, Copy)]
pub enum CorTokenType {
    MdtModule = 0x00000000,
    MdtTypeRef = 0x01000000,
    MdtTypeDef = 0x02000000,
    MdtFieldDef = 0x04000000,
    MdtMethodDef = 0x06000000,
    MdtParamDef = 0x08000000,
    MdtInterfaceImpl = 0x09000000,
    MdtMemberRef = 0x0a000000,
    MdtCustomAttribute = 0x0c000000,
    MdtPermission = 0x0e000000,
    MdtSignature = 0x11000000,
    MdtEvent = 0x14000000,
    MdtProperty = 0x17000000,
    MdtModuleRef = 0x1a000000,
    MdtTypeSpec = 0x1b000000,
    MdtAssembly = 0x20000000,
    MdtAssemblyRef = 0x23000000,
    MdtFile = 0x26000000,
    MdtExportedType = 0x27000000,
    MdtManifestResource = 0x28000000,
    MdtGenericParam = 0x2a000000,
    MdtMethodSpec = 0x2b000000,
    MdtGenericParamConstraint = 0x2c000000,
    MdtString = 0x70000000,
    MdtName = 0x71000000,
    MdtBaseType = 0x72000000,
    MdtError_ = 1868
}

impl From<u32> for CorTokenType {
    fn from(value: u32) -> Self {
        match value {
            0x00000000 => Self::MdtModule,
            0x01000000 => Self::MdtTypeRef,
            0x02000000 => Self::MdtTypeDef,
            0x04000000 => Self::MdtFieldDef,
            0x06000000 => Self::MdtMethodDef,
            0x08000000 => Self::MdtParamDef,
            0x09000000 =>  Self::MdtInterfaceImpl,
            0x0a000000 => Self::MdtMemberRef,
            0x0c000000 => Self::MdtCustomAttribute,
            0x0e000000 => Self::MdtPermission,
            0x11000000 => Self::MdtSignature,
            0x14000000 => Self::MdtEvent,
            0x17000000 => Self::MdtProperty,
            0x1a000000 => Self::MdtModuleRef,
            0x1b000000 => Self::MdtTypeSpec,
            0x20000000 => Self::MdtAssembly,
            0x23000000 => Self::MdtAssemblyRef,
            0x26000000 => Self::MdtFile,
            0x27000000 => Self::MdtExportedType,
            0x28000000 => Self::MdtManifestResource,
            0x2a000000 => Self::MdtGenericParam,
            0x2b000000 => Self::MdtMethodSpec,
            0x2c000000 => Self::MdtGenericParamConstraint,
            0x70000000 => Self::MdtString,
            0x71000000 => Self::MdtName,
            0x72000000 => Self::MdtBaseType,
            _ => Self::MdtError_
        }
    }
}