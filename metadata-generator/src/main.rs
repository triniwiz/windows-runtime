// metadata-generator CLI
// Reads .dll / .winmd / .nupkg / directory inputs and writes a .bin metadata
// bundle that the runtime can load for dynamic dispatch.

use std::collections::HashSet;
use std::ffi::c_void;
use std::path::PathBuf;
use std::ptr::addr_of_mut;

use windows::core::{GUID, HSTRING, PCWSTR};
use windows::Win32::System::WinRT::Metadata::{
    CorElementType, CorTokenType, IMetaDataImport2, IMAGE_CEE_CS_CALLCONV_GENERIC,
    ELEMENT_TYPE_I1, ELEMENT_TYPE_I2, ELEMENT_TYPE_I4, ELEMENT_TYPE_I8,
    ELEMENT_TYPE_U1, ELEMENT_TYPE_U2, ELEMENT_TYPE_U4, ELEMENT_TYPE_U8,
    mdtTypeDef, mdtTypeRef,
};

use metadata::prelude::{
    cor_sig_uncompress_calling_conv, cor_sig_uncompress_data, get_guid_attribute_value,
    get_type_name, type_from_token, MAX_IDENTIFIER_LENGTH, PCCOR_SIGNATURE,
    SYSTEM_ENUM, SYSTEM_MULTICASTDELEGATE, SYSTEM_VALUETYPE,
};
use metadata::signature::Signature;

use metadata_generator::{
    ClassRecord, DelegateRecord, EnumMemberRecord, EnumRecord, FieldRecord,
    InterfaceRecord, MetadataBundle, MethodRecord, ParamRecord, StructRecord,
    TypeRecord, FORMAT_VERSION,
};

// ─── Type visibility / flags constants (from CorTypeAttr / ECMA-335) ─────────

const TD_VISIBILITY_MASK: u32 = 0x00000007;
const TD_PUBLIC: u32 = 0x00000001;
const TD_NESTED_PUBLIC: u32 = 0x00000002;
const TD_INTERFACE: u32 = 0x00000020; // tdClassSemanticsMask & tdInterface
const TD_WINDOWS_RUNTIME: u32 = 0x00004000;

const MD_PUBLIC: u32 = 0x00000006; // mdMemberAccessMask == mdPublic
const MD_MEMBER_ACCESS_MASK: u32 = 0x00000007;
const MD_STATIC: u32 = 0x00000010;
const MD_SPECIAL_NAME: u32 = 0x00000800;

const FD_STATIC: u32 = 0x00000010;
const FD_LITERAL: u32 = 0x00000040;
const FD_SPECIAL_NAME: u32 = 0x00000200;

// ─── GUID helpers ─────────────────────────────────────────────────────────────

fn guid_to_bytes(guid: &GUID) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    bytes[0..4].copy_from_slice(&guid.data1.to_le_bytes());
    bytes[4..6].copy_from_slice(&guid.data2.to_le_bytes());
    bytes[6..8].copy_from_slice(&guid.data3.to_le_bytes());
    bytes[8..16].copy_from_slice(&guid.data4);
    bytes
}

// ─── Token → type-name ────────────────────────────────────────────────────────

fn token_type_name(metadata: &IMetaDataImport2, token: u32) -> String {
    if token == 0 {
        return String::new();
    }
    let tt = CorTokenType(type_from_token(CorTokenType(token as i32)));
    if tt == mdtTypeDef || tt == mdtTypeRef {
        get_type_name(metadata, CorTokenType(token as i32))
    } else {
        String::new()
    }
}

// ─── Attribute presence check ─────────────────────────────────────────────────

fn has_attribute(metadata: &IMetaDataImport2, token: u32, attr_name: &str) -> bool {
    let hname = HSTRING::from(attr_name);
    // GetCustomAttributeByName returns S_OK when the attribute exists (ppData non-null)
    // and S_FALSE when it does not (ppData null).  Both are is_ok() in windows-rs.
    // We distinguish by checking the data pointer.
    let mut data: *const c_void = std::ptr::null();
    let mut size = 0u32;
    let ok = unsafe {
        metadata.GetCustomAttributeByName(
            token,
            PCWSTR(hname.as_ptr()),
            addr_of_mut!(data) as *const *const c_void,
            &mut size,
        )
    }
    .is_ok();
    ok && !data.is_null()
}

// ─── Parameter name ────────────────────────────────────────────────────────────

fn param_name(metadata: &IMetaDataImport2, param_token: u32) -> String {
    let mut buf = [0u16; 256];
    let mut len = 0u32;
    let ok = unsafe {
        metadata.GetParamProps(
            param_token,
            0 as _,
            0 as _,
            Some(buf.as_mut_slice()),
            &mut len,
            0 as _,
            0 as _,
            0 as _,
            0 as _,
        )
    }
    .is_ok();
    if !ok || len == 0 {
        return "arg".to_string();
    }
    String::from_utf16_lossy(&buf[..len.saturating_sub(1) as usize])
}

// ─── Method extraction ────────────────────────────────────────────────────────

fn extract_methods(metadata: &IMetaDataImport2, type_token: u32) -> Vec<MethodRecord> {
    let mut records = Vec::new();
    let mut enumerator = std::ptr::null_mut();
    let mut method_tokens = [0u32; 1024];
    let mut count = 0u32;

    let ok = unsafe {
        metadata.EnumMethods(
            addr_of_mut!(enumerator),
            type_token,
            method_tokens.as_mut_ptr(),
            method_tokens.len() as u32,
            &mut count,
        )
    }
    .is_ok();
    unsafe { metadata.CloseEnum(enumerator) };
    if !ok {
        return records;
    }

    for (ordinal, &method_token) in method_tokens[..count as usize].iter().enumerate() {
        // vtable slot for WinRT/COM: 0-5 are IUnknown+IInspectable, interface methods start at 6
        let vtable_index = 6 + ordinal as u32;

        let mut name_buf = [0u16; MAX_IDENTIFIER_LENGTH];
        let mut name_len = 0u32;
        let mut flags = 0u32;
        let mut sig_blob: *mut u8 = std::ptr::null_mut();
        let mut sig_len = 0u32;

        let ok = unsafe {
            metadata.GetMethodProps(
                method_token,
                0 as _,
                Some(name_buf.as_mut_slice()),
                &mut name_len,
                &mut flags,
                addr_of_mut!(sig_blob),
                &mut sig_len,
                0 as _,
                0 as _,
            )
        }
        .is_ok();
        if !ok || sig_blob.is_null() {
            continue;
        }

        // Only export public methods.
        if (flags & MD_MEMBER_ACCESS_MASK) != MD_PUBLIC {
            continue;
        }

        let name = String::from_utf16_lossy(&name_buf[..name_len.saturating_sub(1) as usize]);
        let is_static = (flags & MD_STATIC) != 0;
        let is_special = (flags & MD_SPECIAL_NAME) != 0;

        let mut sig = PCCOR_SIGNATURE::from_ptr(sig_blob);
        let calling_conv = cor_sig_uncompress_calling_conv(&mut sig);

        // Skip generic methods — IMAGE_CEE_CS_CALLCONV_GENERIC = 0x10.
        if (calling_conv & IMAGE_CEE_CS_CALLCONV_GENERIC.0 as u32) != 0 {
            continue;
        }

        let param_count = cor_sig_uncompress_data(&mut sig);
        let ret_type_sig = Signature::consume_type(&mut sig);
        let return_type = Signature::to_string(metadata, &ret_type_sig);

        // Enumerate parameter tokens to get their names.
        let mut param_enum = std::ptr::null_mut();
        let mut param_tokens = [0u32; 256];
        let mut actual_param_count = 0u32;
        unsafe {
            let _ = metadata.EnumParams(
                addr_of_mut!(param_enum),
                method_token,
                param_tokens.as_mut_ptr(),
                param_tokens.len() as u32,
                &mut actual_param_count,
            );
            metadata.CloseEnum(param_enum);
        };

        // Some methods have a sentinel "return value" param at sequence 0; skip it.
        let param_start = if actual_param_count > param_count {
            (actual_param_count - param_count) as usize
        } else {
            0
        };

        let mut params = Vec::new();
        for i in 0..param_count as usize {
            let type_sig = Signature::consume_type(&mut sig);
            let type_name = Signature::to_string(metadata, &type_sig);
            let pname = if param_start + i < actual_param_count as usize {
                param_name(metadata, param_tokens[param_start + i])
            } else {
                format!("arg{}", i)
            };
            let is_out = type_name.starts_with("ByRef ");
            params.push(ParamRecord { name: pname, type_name, is_out });
        }

        records.push(MethodRecord { name, vtable_index, is_static, is_special, return_type, params });
    }

    records
}

// ─── Implemented-interface name list ─────────────────────────────────────────

fn implemented_interface_names(metadata: &IMetaDataImport2, type_token: u32) -> Vec<String> {
    let mut names = Vec::new();
    let mut enumerator = std::ptr::null_mut();
    let mut impl_tokens = [0u32; 256];
    let mut count = 0u32;

    let ok = unsafe {
        metadata.EnumInterfaceImpls(
            addr_of_mut!(enumerator),
            type_token,
            impl_tokens.as_mut_ptr(),
            impl_tokens.len() as u32,
            &mut count,
        )
    }
    .is_ok();
    unsafe { metadata.CloseEnum(enumerator) };
    if !ok {
        return names;
    }

    for &impl_token in &impl_tokens[..count as usize] {
        let mut iface_token = 0u32;
        if unsafe { metadata.GetInterfaceImplProps(impl_token, 0 as _, &mut iface_token) }.is_ok() {
            let name = token_type_name(metadata, iface_token);
            if !name.is_empty() {
                names.push(name);
            }
        }
    }
    names
}

// ─── Field helpers (struct fields + enum constants) ──────────────────────────

fn field_name(metadata: &IMetaDataImport2, field_token: u32) -> String {
    let mut buf = [0u16; MAX_IDENTIFIER_LENGTH];
    let mut len = 0u32;
    let ok = unsafe {
        metadata.GetFieldProps(
            field_token,
            0 as _,
            Some(buf.as_mut_slice()),
            &mut len,
            0 as _,
            0 as _,
            0 as _,
            0 as _,
            0 as _,
            0 as _,
        )
    }
    .is_ok();
    if !ok || len == 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..len.saturating_sub(1) as usize])
}

fn field_constant_value(metadata: &IMetaDataImport2, field_token: u32) -> i64 {
    let mut value: *mut c_void = std::ptr::null_mut();
    let mut value_type = 0u32;
    let ok = unsafe {
        metadata.GetFieldProps(
            field_token,
            0 as _, // pClass
            None,   // szField (not needed)
            0 as _, // pchField
            0 as _, // pdwAttr
            0 as _, // ppvSigBlob
            0 as _, // pcbSigBlob
            &mut value_type,     // pdwCPlusTypeFlag
            addr_of_mut!(value), // ppValue
            0 as _, // pcchValue
        )
    }
    .is_ok();
    if !ok || value.is_null() {
        return 0;
    }
    // ppValue points into the raw metadata binary; alignment is not guaranteed.
    unsafe {
        match CorElementType(value_type as u8) {
            ELEMENT_TYPE_I4 => std::ptr::read_unaligned(value as *const i32) as i64,
            ELEMENT_TYPE_U4 => std::ptr::read_unaligned(value as *const u32) as i64,
            ELEMENT_TYPE_I8 => std::ptr::read_unaligned(value as *const i64),
            ELEMENT_TYPE_U8 => std::ptr::read_unaligned(value as *const u64) as i64,
            ELEMENT_TYPE_I2 => std::ptr::read_unaligned(value as *const i16) as i64,
            ELEMENT_TYPE_U2 => std::ptr::read_unaligned(value as *const u16) as i64,
            ELEMENT_TYPE_I1 => std::ptr::read_unaligned(value as *const i8) as i64,
            ELEMENT_TYPE_U1 => std::ptr::read_unaligned(value as *const u8) as i64,
            _ => 0,
        }
    }
}

fn extract_enum_members(metadata: &IMetaDataImport2, type_token: u32) -> Vec<EnumMemberRecord> {
    let mut members = Vec::new();
    let mut enumerator = std::ptr::null_mut();
    let mut field_tokens = [0u32; 512];
    let mut count = 0u32;

    let ok = unsafe {
        metadata.EnumFields(
            addr_of_mut!(enumerator),
            type_token,
            field_tokens.as_mut_ptr(),
            field_tokens.len() as u32,
            &mut count,
        )
    }
    .is_ok();
    unsafe { metadata.CloseEnum(enumerator) };
    if !ok {
        return members;
    }

    for &ft in &field_tokens[..count as usize] {
        let mut attr = 0u32;
        unsafe {
            let _ = metadata.GetFieldProps(
                ft, 0 as _, None, 0 as _, &mut attr, 0 as _, 0 as _, 0 as _, 0 as _, 0 as _,
            );
        };

        // Enum constant fields are fdPublic | fdStatic | fdLiteral.
        let is_literal = (attr & FD_LITERAL) != 0;
        let is_static = (attr & FD_STATIC) != 0;
        if !is_literal || !is_static {
            continue;
        }

        let name = field_name(metadata, ft);
        if name.is_empty() {
            continue;
        }

        let value = field_constant_value(metadata, ft);
        members.push(EnumMemberRecord { name, value });
    }
    members
}

fn extract_struct_fields(metadata: &IMetaDataImport2, type_token: u32) -> Vec<FieldRecord> {
    let mut fields = Vec::new();
    let mut enumerator = std::ptr::null_mut();
    let mut field_tokens = [0u32; 256];
    let mut count = 0u32;

    let ok = unsafe {
        metadata.EnumFields(
            addr_of_mut!(enumerator),
            type_token,
            field_tokens.as_mut_ptr(),
            field_tokens.len() as u32,
            &mut count,
        )
    }
    .is_ok();
    unsafe { metadata.CloseEnum(enumerator) };
    if !ok {
        return fields;
    }

    for &ft in &field_tokens[..count as usize] {
        let mut attr = 0u32;
        let mut sig_blob: *mut u8 = std::ptr::null_mut();
        let mut sig_len = 0u32;

        let ok = unsafe {
            metadata.GetFieldProps(
                ft,
                0 as _,
                None,
                0 as _,
                &mut attr,
                addr_of_mut!(sig_blob),
                &mut sig_len,
                0 as _,
                0 as _,
                0 as _,
            )
        }
        .is_ok();
        if !ok {
            continue;
        }

        // Skip static, literal, and compiler-generated fields.
        if (attr & FD_STATIC) != 0 || (attr & FD_LITERAL) != 0 || (attr & FD_SPECIAL_NAME) != 0 {
            continue;
        }

        let name = field_name(metadata, ft);
        if name.is_empty() || name.starts_with('<') {
            continue;
        }

        let type_name = if !sig_blob.is_null() && sig_len > 0 {
            let mut sig = PCCOR_SIGNATURE::from_ptr(sig_blob);
            // Field signature starts with a FIELD calling-convention byte (0x06); skip it.
            cor_sig_uncompress_calling_conv(&mut sig);
            let type_sig = Signature::consume_type(&mut sig);
            Signature::to_string(metadata, &type_sig)
        } else {
            "Object".to_string()
        };

        fields.push(FieldRecord { name, type_name });
    }
    fields
}

// ─── Type classification ──────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
enum TypeKind {
    Interface,
    Class,
    Enum,
    Struct,
    Delegate,
    Unknown,
}

fn classify_typedef(
    metadata: &IMetaDataImport2,
    type_token: u32,
) -> (TypeKind, String, u32) {
    let mut flags = 0u32;
    let mut extends_token = 0u32;

    let ok = unsafe {
        metadata.GetTypeDefProps(
            type_token,
            None,
            0 as _,
            &mut flags,
            &mut extends_token,
        )
    }
    .is_ok();
    if !ok {
        return (TypeKind::Unknown, String::new(), 0);
    }

    if (flags & TD_INTERFACE) != 0 {
        return (TypeKind::Interface, String::new(), flags);
    }

    let base_name = token_type_name(metadata, extends_token);
    let kind = match base_name.as_str() {
        SYSTEM_ENUM => TypeKind::Enum,
        SYSTEM_VALUETYPE => TypeKind::Struct,
        SYSTEM_MULTICASTDELEGATE => TypeKind::Delegate,
        _ => TypeKind::Class,
    };

    (kind, base_name, flags)
}

// ─── Full type enumeration from a metadata scope ─────────────────────────────

fn extract_all_types(metadata: &IMetaDataImport2) -> Vec<TypeRecord> {
    let mut records = Vec::new();
    let mut enumerator = std::ptr::null_mut();

    loop {
        let mut tokens = [0u32; 256];
        let mut fetched = 0u32;
        let result = unsafe {
            metadata.EnumTypeDefs(
                &mut enumerator,
                tokens.as_mut_ptr(),
                tokens.len() as u32,
                &mut fetched,
            )
        };
        if result.is_err() || fetched == 0 {
            break;
        }

        for &token in &tokens[..fetched as usize] {
            let mut name_buf = [0u16; MAX_IDENTIFIER_LENGTH];
            let mut name_len = 0u32;
            let ok = unsafe {
                metadata.GetTypeDefProps(
                    token,
                    Some(name_buf.as_mut_slice()),
                    &mut name_len,
                    0 as _,
                    0 as _,
                )
            }
            .is_ok();
            if !ok || name_len == 0 {
                continue;
            }

            let full_name =
                String::from_utf16_lossy(&name_buf[..name_len.saturating_sub(1) as usize]);

            // Skip anonymous or compiler-generated types.
            if full_name.is_empty() || full_name.contains('<') {
                continue;
            }

            let (kind, base_name, flags) = classify_typedef(metadata, token);
            if kind == TypeKind::Unknown {
                continue;
            }

            // Only export public and nested-public types.
            let visibility = flags & TD_VISIBILITY_MASK;
            if visibility != TD_PUBLIC && visibility != TD_NESTED_PUBLIC {
                continue;
            }

            let is_winrt = (flags & TD_WINDOWS_RUNTIME) != 0;

            let record = match kind {
                TypeKind::Interface => {
                    let guid = guid_to_bytes(&get_guid_attribute_value(
                        Some(metadata),
                        CorTokenType(token as i32),
                    ));
                    let methods = extract_methods(metadata, token);
                    TypeRecord::Interface(InterfaceRecord { full_name, guid, is_winrt, methods })
                }
                TypeKind::Class => {
                    let interface_names = implemented_interface_names(metadata, token);
                    let methods = extract_methods(metadata, token);
                    let base = if base_name == "System.Object" {
                        String::new()
                    } else {
                        base_name
                    };
                    TypeRecord::Class(ClassRecord {
                        full_name,
                        is_winrt,
                        base_name: base,
                        interface_names,
                        methods,
                    })
                }
                TypeKind::Enum => {
                    let is_flags =
                        has_attribute(metadata, token, "Windows.Foundation.Metadata.FlagsAttribute")
                            || has_attribute(metadata, token, "System.FlagsAttribute");
                    let members = extract_enum_members(metadata, token);
                    TypeRecord::Enum(EnumRecord { full_name, is_flags, members })
                }
                TypeKind::Struct => {
                    let fields = extract_struct_fields(metadata, token);
                    TypeRecord::Struct(StructRecord { full_name, fields })
                }
                TypeKind::Delegate => {
                    let guid = guid_to_bytes(&get_guid_attribute_value(
                        Some(metadata),
                        CorTokenType(token as i32),
                    ));
                    let invoke = extract_methods(metadata, token)
                        .into_iter()
                        .find(|m| m.name == "Invoke")
                        .unwrap_or_default();
                    TypeRecord::Delegate(DelegateRecord {
                        full_name,
                        guid,
                        params: invoke.params,
                        return_type: invoke.return_type,
                    })
                }
                TypeKind::Unknown => continue,
            };

            records.push(record);
        }
    }

    if !enumerator.is_null() {
        unsafe { metadata.CloseEnum(enumerator) };
    }

    records
}

// ─── Input path expansion ─────────────────────────────────────────────────────

fn expand_input(path: &PathBuf) -> Vec<PathBuf> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if ext == "dll" || ext == "winmd" {
        return vec![path.clone()];
    }
    if ext == "nupkg" {
        return expand_nupkg(path);
    }
    if path.is_dir() {
        return scan_dir(path);
    }
    Vec::new()
}

fn expand_nupkg(nupkg: &PathBuf) -> Vec<PathBuf> {
    use std::io::{Read, Write};

    let Ok(file) = std::fs::File::open(nupkg) else {
        eprintln!("warning: could not open NuGet package {}", nupkg.display());
        return Vec::new();
    };

    let mut archive = match zip::ZipArchive::new(file) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("warning: {} is not a valid .nupkg: {}", nupkg.display(), e);
            return Vec::new();
        }
    };

    let stem = nupkg.file_stem().and_then(|s| s.to_str()).unwrap_or("pkg");
    let tmp_dir = std::env::temp_dir().join(format!("nswrt_mg_{}", stem));
    let _ = std::fs::create_dir_all(&tmp_dir);

    let mut extracted = Vec::new();
    for i in 0..archive.len() {
        let mut entry = match archive.by_index(i) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let name_lower = entry.name().to_ascii_lowercase();
        if !name_lower.starts_with("lib/") {
            continue;
        }
        let ext = std::path::Path::new(entry.name())
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext != "dll" && ext != "winmd" {
            continue;
        }
        let file_name = std::path::Path::new(entry.name())
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if file_name.is_empty() {
            continue;
        }
        let out_path = tmp_dir.join(&file_name);
        if let Ok(mut out_file) = std::fs::File::create(&out_path) {
            let mut buf = Vec::new();
            if entry.read_to_end(&mut buf).is_ok() {
                let _ = out_file.write_all(&buf);
                extracted.push(out_path);
            }
        }
    }
    extracted
}

fn scan_dir(dir: &PathBuf) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut result = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            result.extend(scan_dir(&path));
        } else {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if ext == "dll" || ext == "winmd" {
                result.push(path);
            }
        }
    }
    result
}

// ─── CLI arg parsing ──────────────────────────────────────────────────────────

struct Config {
    inputs: Vec<PathBuf>,
    output: PathBuf,
    verbose: bool,
}

fn parse_args() -> Config {
    let mut args = std::env::args().skip(1);
    let mut inputs = Vec::new();
    let mut output = PathBuf::from("metadata.bin");
    let mut verbose = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--output" | "-o" => {
                if let Some(v) = args.next() {
                    output = PathBuf::from(v);
                }
            }
            "--verbose" | "-v" => {
                verbose = true;
            }
            "--input" | "-i" => {
                if let Some(v) = args.next() {
                    let path = PathBuf::from(&v);
                    let expanded = expand_input(&path);
                    if expanded.is_empty() {
                        eprintln!("warning: no .dll/.winmd files found at: {}", v);
                    }
                    inputs.extend(expanded);
                }
            }
            other if !other.starts_with('-') => {
                let path = PathBuf::from(other);
                let expanded = expand_input(&path);
                if expanded.is_empty() {
                    eprintln!("warning: no .dll/.winmd files found at: {}", other);
                }
                inputs.extend(expanded);
            }
            _ => {
                eprintln!("warning: unrecognised argument: {}", arg);
            }
        }
    }

    Config { inputs, output, verbose }
}

// ─── Entry point ─────────────────────────────────────────────────────────────

fn main() {
    // CoCreateInstance(CLSID_CorMetaDataDispenser) in OpenMetadataScope requires COM.
    unsafe {
        windows::Win32::System::Com::CoInitializeEx(
            None,
            windows::Win32::System::Com::COINIT_MULTITHREADED,
        )
        .ok()
        .expect("CoInitializeEx failed");
    }

    let config = parse_args();

    if config.inputs.is_empty() {
        eprintln!(
            "usage: metadata-generator [--output <file.bin>] [--verbose] \
             <input.dll|.winmd|.nupkg|dir> ..."
        );
        std::process::exit(1);
    }

    let mut bundle = MetadataBundle { version: FORMAT_VERSION, types: Vec::new() };

    for path in &config.inputs {
        if config.verbose {
            println!("Processing: {}", path.display());
        }
        match metadata::open_metadata_scope_from_file(path) {
            Some(md) => {
                let types = extract_all_types(&md);
                if config.verbose {
                    println!("  {} types extracted", types.len());
                }
                bundle.types.extend(types);
            }
            None => {
                eprintln!("warning: could not open metadata scope for: {}", path.display());
            }
        }
    }

    // Deduplicate: keep the first occurrence of each fully-qualified name.
    {
        let mut seen: HashSet<String> = HashSet::new();
        bundle.types.retain(|r| seen.insert(r.full_name().to_string()));
    }

    let encoded =
        bincode::serialize(&bundle).expect("failed to serialise metadata bundle");

    if let Some(parent) = config.output.parent() {
        if !parent.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(parent);
        }
    }

    std::fs::write(&config.output, &encoded).expect("failed to write output .bin");

    println!(
        "Written {} types to {} ({} bytes)",
        bundle.types.len(),
        config.output.display(),
        encoded.len()
    );
}
