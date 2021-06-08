#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]


#[derive(Copy, Clone, PartialEq, Debug)]
#[repr(C)]
pub enum CorCallingConvention
{
	ImageCeeCsCallconvDefault = 0x0,

	ImageCeeCsCallconvVararg = 0x5,
	ImageCeeCsCallconvField = 0x6,
	ImageCeeCsCallconvLocalSig = 0x7,
	ImageCeeCsCallconvProperty = 0x8,
	ImageCeeCsCallconvUnmgd = 0x9,
	ImageCeeCsCallconvGenericinst = 0xa,
	// generic method instantiation
	ImageCeeCsCallconvNativevararg = 0xb,
	// used ONLY for 64bit vararg PInvoke calls
	ImageCeeCsCallconvMax = 0xc,  // first invalid calling convention


	// The high bits of the calling convention convey additional info
	ImageCeeCsCallconvMask = 0x0f,
	// Calling convention is bottom 4 bits
	ImageCeeCsCallconvHasthis = 0x20,
	// Top bit indicates a 'this' parameter
	ImageCeeCsCallconvExplicitthis = 0x40,
	// This parameter is explicitly in the signature
	ImageCeeCsCallconvGeneric = 0x10,  // Generic method sig with explicit number of type arguments (precedes ordinary parameter count)
	// 0x80 is reserved for internal use
}


#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(C)]
pub enum CorElementType
{
	ElementTypeEnd = 0x00,
	ElementTypeVoid = 0x01,
	ElementTypeBoolean = 0x02,
	ElementTypeChar = 0x03,
	ElementTypeI1 = 0x04,
	ElementTypeU1 = 0x05,
	ElementTypeI2 = 0x06,
	ElementTypeU2 = 0x07,
	ElementTypeI4 = 0x08,
	ElementTypeU4 = 0x09,
	ElementTypeI8 = 0x0a,
	ElementTypeU8 = 0x0b,
	ElementTypeR4 = 0x0c,
	ElementTypeR8 = 0x0d,
	ElementTypeString = 0x0e,

	// every type above PTR will be simple type
	ElementTypePtr = 0x0f,
	// PTR <type>
	ElementTypeByref = 0x10,     // BYREF <type>

	// Please use ElementTypeValuetype. ELEMENT_TYPE_VALUECLASS is deprecated.
	ElementTypeValuetype = 0x11,
	// VALUETYPE <class Token>
	ElementTypeClass = 0x12,
	// CLASS <class Token>
	ElementTypeVar = 0x13,
	// a class type variable VAR <number>
	ElementTypeArray = 0x14,
	// MDARRAY <type> <rank> <bcount> <bound1> ... <lbcount> <lb1> ...
	ElementTypeGenericinst = 0x15,
	// GENERICINST <generic type> <argCnt> <arg1> ... <argn>
	ElementTypeTypedbyref = 0x16,     // TYPEDREF  (it takes no args) a typed referece to some other type

	ElementTypeI = 0x18,
	// native integer size
	ElementTypeU = 0x19,
	// native unsigned integer size
	ElementTypeFnptr = 0x1b,
	// FNPTR <complete sig for the function including calling convention>
	ElementTypeObject = 0x1c,
	// Shortcut for System.Object
	ElementTypeSzarray = 0x1d,
	// Shortcut for single dimension zero lower bound array
// SZARRAY <type>
	ElementTypeMvar = 0x1e,     // a method type variable MVAR <number>

	// This is only for binding
	ElementTypeCmodReqd = 0x1f,
	// required C modifier : E_T_CMOD_REQD <MdTypeRef/MdTypeDef>
	ElementTypeCmodOpt = 0x20,     // optional C modifier : E_T_CMOD_OPT <MdTypeRef/MdTypeDef>

	// This is for signatures generated internally (which will not be persisted in any way).
	ElementTypeInternal = 0x21,     // INTERNAL <typehandle>

	// Note that this is the max of base type excluding modifiers
	ElementTypeMax = 0x22,     // first invalid element type


	ElementTypeModifier = 0x40,
	ElementTypeSentinel = 0x01 | 0x40,
	// sentinel for varargs
	ElementTypePinned = 0x05 | 0x40,

}

impl From<core_bindings::CorElementType> for CorElementType {
	fn from(cet: core_bindings::CorElementType) -> Self {
		match cet {
			core_bindings::CorElementType::ELEMENT_TYPE_END => CorElementType::ElementTypeEnd,
			core_bindings::CorElementType::ELEMENT_TYPE_VOID => CorElementType::ElementTypeVoid,
			core_bindings::CorElementType::ELEMENT_TYPE_BOOLEAN => CorElementType::ElementTypeBoolean,
			core_bindings::CorElementType::ELEMENT_TYPE_CHAR => CorElementType::ElementTypeChar,
			core_bindings::CorElementType::ELEMENT_TYPE_I1 => CorElementType::ElementTypeI1,
			core_bindings::CorElementType::ELEMENT_TYPE_U1 => CorElementType::ElementTypeU1,
			core_bindings::CorElementType::ELEMENT_TYPE_I2 => CorElementType::ElementTypeI2,
			core_bindings::CorElementType::ELEMENT_TYPE_U2 => CorElementType::ElementTypeU2,
			core_bindings::CorElementType::ELEMENT_TYPE_I4 => CorElementType::ElementTypeI4,
			core_bindings::CorElementType::ELEMENT_TYPE_U4 => CorElementType::ElementTypeU4,
			core_bindings::CorElementType::ELEMENT_TYPE_I8 => CorElementType::ElementTypeI8,
			core_bindings::CorElementType::ELEMENT_TYPE_U8 => CorElementType::ElementTypeU8,
			core_bindings::CorElementType::ELEMENT_TYPE_R4 => CorElementType::ElementTypeR4,
			core_bindings::CorElementType::ELEMENT_TYPE_R8 => CorElementType::ElementTypeR8,
			core_bindings::CorElementType::ELEMENT_TYPE_STRING => CorElementType::ElementTypeString,
			core_bindings::CorElementType::ELEMENT_TYPE_PTR => CorElementType::ElementTypePtr,
			core_bindings::CorElementType::ELEMENT_TYPE_BYREF => CorElementType::ElementTypeByref,
			core_bindings::CorElementType::ELEMENT_TYPE_VALUETYPE => CorElementType::ElementTypeValuetype,
			core_bindings::CorElementType::ELEMENT_TYPE_CLASS => CorElementType::ElementTypeClass,
			core_bindings::CorElementType::ELEMENT_TYPE_VAR => CorElementType::ElementTypeVar,
			core_bindings::CorElementType::ELEMENT_TYPE_ARRAY => CorElementType::ElementTypeArray,
			core_bindings::CorElementType::ELEMENT_TYPE_GENERICINST => CorElementType::ElementTypeGenericinst,
			core_bindings::CorElementType::ELEMENT_TYPE_TYPEDBYREF => CorElementType::ElementTypeTypedbyref,
			core_bindings::CorElementType::ELEMENT_TYPE_I => CorElementType::ElementTypeI,
			core_bindings::CorElementType::ELEMENT_TYPE_U => CorElementType::ElementTypeU,
			core_bindings::CorElementType::ELEMENT_TYPE_FNPTR => CorElementType::ElementTypeFnptr,
			core_bindings::CorElementType::ELEMENT_TYPE_OBJECT => CorElementType::ElementTypeObject,
			core_bindings::CorElementType::ELEMENT_TYPE_SZARRAY => CorElementType::ElementTypeSzarray,
			core_bindings::CorElementType::ELEMENT_TYPE_MVAR => CorElementType::ElementTypeMvar,
			core_bindings::CorElementType::ELEMENT_TYPE_CMOD_REQD => CorElementType::ElementTypeCmodReqd,
			core_bindings::CorElementType::ELEMENT_TYPE_CMOD_OPT => CorElementType::ElementTypeCmodOpt,
			core_bindings::CorElementType::ELEMENT_TYPE_INTERNAL => CorElementType::ElementTypeInternal,
			core_bindings::CorElementType::ELEMENT_TYPE_MAX => CorElementType::ElementTypeMax,
			core_bindings::CorElementType::ELEMENT_TYPE_MODIFIER => CorElementType::ElementTypeModifier,
			core_bindings::CorElementType::ELEMENT_TYPE_SENTINEL => CorElementType::ElementTypeSentinel,
			core_bindings::CorElementType::ELEMENT_TYPE_PINNED => CorElementType::ElementTypePinned,
		}
	}
}


#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub enum CorTokenType {
	mdtModule = 0x00000000,
	mdtTypeRef = 0x01000000,
	mdtTypeDef = 0x02000000,
	mdtFieldDef = 0x04000000,
	mdtMethodDef = 0x06000000,
	mdtParamDef = 0x08000000,
	mdtInterfaceImpl = 0x09000000,
	mdtMemberRef = 0x0a000000,
	mdtCustomAttribute = 0x0c000000,
	mdtPermission = 0x0e000000,
	mdtSignature = 0x11000000,
	mdtEvent = 0x14000000,
	mdtProperty = 0x17000000,
	mdtModuleRef = 0x1a000000,
	mdtTypeSpec = 0x1b000000,
	mdtAssembly = 0x20000000,
	mdtAssemblyRef = 0x23000000,
	mdtFile = 0x26000000,
	mdtExportedType = 0x27000000,
	mdtManifestResource = 0x28000000,
	mdtGenericParam = 0x2a000000,
	mdtMethodSpec = 0x2b000000,
	mdtGenericParamConstraint = 0x2c000000,
	mdtString = 0x70000000,
	mdtName = 0x71000000,
	mdtBaseType = 0x72000000,
	mdtError_ = 1868
}

impl From<u32> for CorTokenType {
	fn from(value: u32) -> Self {
		match value {
			0x00000000 => Self::mdtModule,
			0x01000000 => Self::mdtTypeRef,
			0x02000000 => Self::mdtTypeDef,
			0x04000000 => Self::mdtFieldDef,
			0x06000000 => Self::mdtMethodDef,
			0x08000000 => Self::mdtParamDef,
			0x09000000 => Self::mdtInterfaceImpl,
			0x0a000000 => Self::mdtMemberRef,
			0x0c000000 => Self::mdtCustomAttribute,
			0x0e000000 => Self::mdtPermission,
			0x11000000 => Self::mdtSignature,
			0x14000000 => Self::mdtEvent,
			0x17000000 => Self::mdtProperty,
			0x1a000000 => Self::mdtModuleRef,
			0x1b000000 => Self::mdtTypeSpec,
			0x20000000 => Self::mdtAssembly,
			0x23000000 => Self::mdtAssemblyRef,
			0x26000000 => Self::mdtFile,
			0x27000000 => Self::mdtExportedType,
			0x28000000 => Self::mdtManifestResource,
			0x2a000000 => Self::mdtGenericParam,
			0x2b000000 => Self::mdtMethodSpec,
			0x2c000000 => Self::mdtGenericParamConstraint,
			0x70000000 => Self::mdtString,
			0x71000000 => Self::mdtName,
			0x72000000 => Self::mdtBaseType,
			_ => Self::mdtError_
		}
	}
}