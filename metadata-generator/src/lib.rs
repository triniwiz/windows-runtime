// Serializable binary format types for metadata .bin files.
// The runtime imports this crate to deserialize the .bin files produced by the
// metadata-generator CLI tool.

use serde::{Deserialize, Serialize};

pub const FORMAT_VERSION: u32 = 1;

/// Top-level container.  Produced by `metadata-generator --output <file.bin>`.
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct MetadataBundle {
    /// Format version; currently 1.
    pub version: u32,
    /// Fully-qualified type records extracted from the input files.
    pub types: Vec<TypeRecord>,
}

/// One record per public TypeDef in the metadata.
#[derive(Serialize, Deserialize, Debug)]
pub enum TypeRecord {
    Interface(InterfaceRecord),
    Class(ClassRecord),
    Enum(EnumRecord),
    Struct(StructRecord),
    Delegate(DelegateRecord),
}

impl TypeRecord {
    pub fn full_name(&self) -> &str {
        match self {
            TypeRecord::Interface(r) => &r.full_name,
            TypeRecord::Class(r) => &r.full_name,
            TypeRecord::Enum(r) => &r.full_name,
            TypeRecord::Struct(r) => &r.full_name,
            TypeRecord::Delegate(r) => &r.full_name,
        }
    }
}

/// A COM/WinRT interface (or any CLI interface type).
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct InterfaceRecord {
    pub full_name: String,
    /// Interface IID in Windows wire-order bytes
    /// (Data1 little-endian, Data2 LE, Data3 LE, Data4[0..8] as-is).
    pub guid: [u8; 16],
    /// True when the `tdWindowsRuntime` typedef flag is set.
    pub is_winrt: bool,
    /// All public methods in vtable declaration order.
    pub methods: Vec<MethodRecord>,
}

/// A concrete class or instantiable runtime class.
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct ClassRecord {
    pub full_name: String,
    pub is_winrt: bool,
    /// Fully-qualified base class name; empty when the base is `System.Object`.
    pub base_name: String,
    /// Fully-qualified names of all directly implemented interfaces.
    pub interface_names: Vec<String>,
    /// Public methods directly on the class TypeDef (static and instance).
    pub methods: Vec<MethodRecord>,
}

/// A method defined in a TypeDef.
#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct MethodRecord {
    pub name: String,
    /// Absolute vtable slot: `6 + ordinal` for WinRT/COM interfaces
    /// (slots 0-5 are IUnknown + IInspectable); 0 for plain .NET types.
    pub vtable_index: u32,
    pub is_static: bool,
    /// True for property getters/setters and event adders/removers.
    pub is_special: bool,
    pub return_type: String,
    pub params: Vec<ParamRecord>,
}

/// A method parameter.
#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct ParamRecord {
    pub name: String,
    pub type_name: String,
    pub is_out: bool,
}

/// An enum type.
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct EnumRecord {
    pub full_name: String,
    /// True when `FlagsAttribute` (WinRT or .NET) is present.
    pub is_flags: bool,
    pub members: Vec<EnumMemberRecord>,
}

/// One member of an enum.
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct EnumMemberRecord {
    pub name: String,
    pub value: i64,
}

/// A value type (CLI struct / WinRT struct).
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct StructRecord {
    pub full_name: String,
    pub fields: Vec<FieldRecord>,
}

/// A field on a struct.
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct FieldRecord {
    pub name: String,
    pub type_name: String,
}

/// A delegate type.
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct DelegateRecord {
    pub full_name: String,
    /// IID from `GuidAttribute`; all-zero if absent.
    pub guid: [u8; 16],
    pub params: Vec<ParamRecord>,
    pub return_type: String,
}
