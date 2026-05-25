mod value;
mod interop;
mod method_call;
mod property_call;
mod error;
mod globals;
mod generic_method_call;
mod helpers;
mod ffi;
mod name_space;
mod proxy_manifest_loader;
mod message_port;
mod worker_support;
mod hmr_support;
mod worker_threads;
mod livesync;
mod class_helpers;
mod type_description;
mod global_fns;
pub mod inspector;
pub mod timers;
mod ns_proxy;
pub(crate) mod dotnet;
pub(crate) mod win32;
pub(crate) mod win32_known_fns;
pub mod ui_dispatcher;

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::ffi::{c_void, CString};
use std::fs;
use std::hash::{Hash, Hasher};
use ahash::{AHashMap, AHasher, AHashSet};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Once, OnceLock};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering as AtomicOrdering};
use std::time::{Duration, Instant};
use parking_lot::{Mutex, RawRwLock, RwLock};
use parking_lot::lock_api::{MappedRwLockReadGuard, MappedRwLockWriteGuard, RwLockReadGuard, RwLockWriteGuard};
use v8::{FunctionTemplate, Local};
use windows::core::{HSTRING, IUnknown, GUID, HRESULT, Interface, PCWSTR, IInspectable, Error};
use windows::Win32::System::Console::GetConsoleWindow;
use windows::Win32::System::Diagnostics::Debug::OutputDebugStringW;
use windows::Win32::System::WinRT::{IActivationFactory, RoGetActivationFactory, RoInitialize, RoUninitialize, RO_INIT_SINGLETHREADED};
use windows::Win32::UI::Shell::IInitializeWithWindow;
use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, MSG, PeekMessageW, PM_REMOVE, TranslateMessage};
use metadata::declarations::base_class_declaration::BaseClassDeclarationImpl;
use metadata::declarations::class_declaration::ClassDeclaration;
use metadata::declarations::declaration::{
    DeclarationKind,
    Declaration,
};
use metadata::declarations::delegate_declaration::DelegateDeclaration;
use metadata::declarations::delegate_declaration::DelegateDeclarationImpl;
use metadata::declarations::delegate_declaration::generic_delegate_declaration::GenericDelegateDeclaration;
use metadata::declarations::delegate_declaration::generic_delegate_instance_declaration::GenericDelegateInstanceDeclaration;
use metadata::declarations::enum_declaration::EnumDeclaration;
use metadata::generic_instance_id_builder::GenericInstanceIdBuilder;
use metadata::declarations::interface_declaration::InterfaceDeclaration;
use metadata::declarations::namespace_declaration::NamespaceDeclaration;
use metadata::meta_data_reader::MetadataReader;
use metadata::declarations::interface_declaration::generic_interface_declaration::GenericInterfaceDeclaration;
use metadata::declarations::event_declaration::EventDeclaration;
use metadata::declarations::method_declaration::MethodDeclaration;
use metadata::declarations::property_declaration::PropertyDeclaration;
use metadata::declarations::struct_declaration::StructDeclaration;
use metadata::signature::Signature;
use metadata::value::Value;
use runtime_binding_gen::{RuntimeExtensionMetadata, RuntimeExtensionRegistry, RuntimeMethodMetadata, RuntimeParameterMetadata, RuntimePropertyMetadata};
use crate::value::{ffi_parse_bool_arg, ffi_parse_buffer_arg, ffi_parse_f32_arg, ffi_parse_f64_arg, ffi_parse_function_arg, ffi_parse_i16_arg, ffi_parse_i32_arg, ffi_parse_i64_arg, ffi_parse_i8_arg, ffi_parse_isize_arg, ffi_parse_pointer_arg, ffi_parse_string_arg, ffi_parse_struct_arg, ffi_parse_u16_arg, ffi_parse_u32_arg, ffi_parse_u64_arg, ffi_parse_u8_arg, ffi_parse_usize_arg, MAX_SAFE_INTEGER, MIN_SAFE_INTEGER, NativeType, NativeValue, set_ret_val, read_value_from_ptr};
use crate::proxy_manifest_loader::SbgManifestLoader;

thread_local!(static ISOLATE: RefCell<Option<&'static mut v8::Isolate>> = RefCell::new(None));

/// Raw pointer to the V8 isolate, set once during Runtime::new so that
/// JS delegate Invoke trampolines can enter V8 without a scope on the stack.
thread_local!(pub(crate) static DELEGATE_ISOLATE_PTR: Cell<*mut v8::Isolate> = Cell::new(std::ptr::null_mut()));

/// JS functions registered via NSWinRT.asDelegate so managed .NET delegates can
/// call back into V8. Keyed by the integer id sent to C# as the callback id.
/// Thread-local because V8 globals must be accessed on the isolate's thread.
thread_local!(pub(crate) static DOTNET_JS_CALLBACKS: RefCell<HashMap<i32, v8::Global<v8::Function>>> = RefCell::new(HashMap::new()));
pub(crate) static DOTNET_NEXT_CB_ID: AtomicI32 = AtomicI32::new(1);
// JS callbacks that should be removed after a single invocation (oneshot).
thread_local!(pub(crate) static DOTNET_ONESHOT_JS_CALLBACKS: RefCell<HashSet<i32>> = RefCell::new(HashSet::new()));

/// Optional hook called from the async-wait message loop so external tools
/// (e.g. the devtools server) can pump their own messages without the runtime
/// needing to depend on those crates directly.
thread_local!(pub static ASYNC_PUMP_HOOK: RefCell<Option<Box<dyn FnMut()>>> = RefCell::new(None));

/// Native ESM module registry: resolved absolute path → compiled V8 Module handle.
/// Pre-populated by `compile_module_graph` before `instantiate_module` is called.
thread_local!(static ESM_MODULE_REGISTRY: RefCell<HashMap<String, v8::Global<v8::Module>>> = RefCell::new(HashMap::new()));

/// Maps a V8 Module identity hash (i32) to its resolved absolute path.
/// Used by `resolve_module_callback` to locate the referrer's directory for relative imports.
thread_local!(static ESM_HASH_TO_PATH: RefCell<HashMap<i32, String>> = RefCell::new(HashMap::new()));

// Tracks constructors currently being built on this thread to avoid
// re-entrant template/property mutations that can corrupt V8 descriptor
// arrays when a constructor build recursively triggers building the same
// constructor (observed as a V8 internal DescriptorArray append failure).
thread_local!(static CREATING_CTORS: RefCell<AHashSet<String>> = RefCell::new(AHashSet::new()));

/// Stores the most recent JS error (message + stack trace) captured during
/// script execution or V8 callbacks. Retrieved by `get_last_js_error()`.
thread_local!(pub static LAST_JS_ERROR: RefCell<Option<String>> = RefCell::new(None));

/// Store a JS error string so it can be retrieved via the FFI.
pub fn store_last_js_error(error: String) {
    LAST_JS_ERROR.with(|e| { *e.borrow_mut() = Some(error); });
}

/// Retrieve (and clear) the last stored JS error.
pub fn get_last_js_error() -> Option<String> {
    LAST_JS_ERROR.with(|e| e.borrow_mut().take())
}

/// Test helper: call Windows.Data.Json.JsonValue::CreateStringValue via the
/// typed `windows` crate and return the raw (leaked) IInspectable pointer.
pub fn diag_direct_create_string_value(s: &str) -> *mut std::ffi::c_void {
    use windows::Data::Json::JsonValue;
    let h = HSTRING::from(s);
    match JsonValue::CreateStringValue(&h) {
        Ok(jv) => {
            let raw = jv.as_raw();
            std::mem::forget(jv);
            raw as *mut std::ffi::c_void
        }
        Err(_) => std::ptr::null_mut(),
    }
}

/// Diagnostic helper: call Windows.Data.Json.JsonValue::CreateStringValue
/// via the runtime's libffi preparation path and return the created string.
pub fn diag_libffi_create_string_value_via_runtime(s: &str) -> Option<String> {
    use libffi::middle::{Cif, Type, Arg, CodePtr};
    use std::mem::ManuallyDrop;
    use windows::Data::Json::{IJsonValueStatics, IJsonValue};
    use windows::Win32::System::WinRT::{RoGetActivationFactory, RoInitialize, RO_INIT_MULTITHREADED};
    use windows::core::IUnknown;
    use crate::value::{NativeValue, NativeType};

    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);

        let class_name: HSTRING = HSTRING::from("Windows.Data.Json.JsonValue");
        let statics = RoGetActivationFactory::<IJsonValueStatics>(&class_name).ok()?;
        let statics_ptr: *mut c_void = statics.as_raw() as *mut c_void;
        let vtable_ptr_ptr: *mut *mut c_void = std::mem::transmute(statics_ptr);
        let vtable_ptr = *vtable_ptr_ptr as *mut *mut c_void;

        // CreateStringValue is at vtable slot 10 for IJsonValueStatics.
        let create_str_off = 10isize;
        let func_ptr = *vtable_ptr.offset(create_str_off) as *const c_void;

        // Build argument buffer matching the runtime calling convention.
        let mut argument_buf: Vec<NativeValue> = Vec::new();
        let mut argument_parse_types: Vec<Option<NativeType>> = Vec::new();

        // `this` pointer
        argument_buf.push(NativeValue { pointer: statics.as_raw() as *mut c_void });
        argument_parse_types.push(None);

        // HSTRING argument (stored as ManuallyDrop inside NativeValue)
        let h = HSTRING::from(s);
        argument_buf.push(NativeValue { string: ManuallyDrop::new(h.clone()) });
        argument_parse_types.push(Some(NativeType::String));

        // out-param for result
        let mut result: *mut c_void = std::ptr::null_mut();
        argument_buf.push(NativeValue { pointer: &mut result as *mut _ as *mut c_void });
        argument_parse_types.push(None);

        let parameter_types = vec![NativeType::Pointer, NativeType::String, NativeType::Pointer];

        // Use runtime helpers to prepare stable HSTRING storage and build args.
        let mut prep = match crate::ffi::prepare_string_storage(&argument_buf, &parameter_types, &argument_parse_types) {
            Ok(p) => p,
            Err(_) => return None,
        };

        let call_args = crate::ffi::build_call_args(&prep, &argument_buf, &argument_parse_types);

        let cif = Cif::new(vec![Type::usize(), Type::usize(), Type::usize()], Type::i32());

        // Perform the libffi call.
        let _ret: i32 = cif.call(CodePtr::from_ptr(func_ptr), &call_args);

        if result.is_null() {
            return None;
        }

        // Convert result pointer to IJsonValue and read the string.
        let unknown = IUnknown::from_raw(result);
        let ijv: IJsonValue = unknown.cast::<IJsonValue>().ok()?;
        let h_res = ijv.GetString().ok()?;
        Some(h_res.to_string())
    }
}

pub struct Runtime {
    isolate: v8::OwnedIsolate,
    global_context: v8::Global<v8::Context>,
    app_root: String,
    winrt_initialized: bool,
}

static INIT: Once = Once::new();
static PROXY_MANIFESTS: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
static LOG_TO_CONSOLE: OnceLock<AtomicBool> = OnceLock::new();

/// COM identity → JS wrapper object cache. Keyed on the canonical IUnknown pointer
/// (obtained via QueryInterface(IID_IUnknown)), so the same underlying COM object
/// always maps to the same JS proxy.
thread_local!(pub(crate) static INSTANCE_CACHE: RefCell<HashMap<usize, v8::Weak<v8::Object>>> = RefCell::new(HashMap::new()));

/// When the cache exceeds this size, request an incremental GC so that weak
/// finalizers can drain dead entries.
pub(crate) const INSTANCE_CACHE_GC_THRESHOLD: usize = 512;

/// Set when the cache grows past INSTANCE_CACHE_GC_THRESHOLD. Cleared after the
/// GC nudge is delivered. Using Cell<bool> avoids RefCell overhead on the fast path.
thread_local!(pub(crate) static GC_NUDGE_PENDING: std::cell::Cell<bool> = std::cell::Cell::new(false));

/// Called with an active isolate reference after inserting into the cache.
/// Requests an incremental V8 GC when the soft threshold is exceeded.
#[inline]
pub(crate) fn maybe_request_gc_nudge(cache_size: usize, isolate: &mut v8::Isolate) {
    if cache_size > INSTANCE_CACHE_GC_THRESHOLD {
        let already_pending = GC_NUDGE_PENDING.with(|f| f.get());
        if !already_pending {
            GC_NUDGE_PENDING.with(|f| f.set(true));
            isolate.memory_pressure_notification(v8::MemoryPressureLevel::Moderate);
        }
    } else {
        // Cache shrank back below threshold (GC ran) — reset the flag.
        GC_NUDGE_PENDING.with(|f| f.set(false));
    }
}

pub(crate) fn proxy_manifests() -> &'static Mutex<Vec<String>> {
    PROXY_MANIFESTS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Tracks hashes of already-loaded manifests to avoid O(N×size) string comparison.
static MANIFEST_HASHES: OnceLock<Mutex<HashSet<u64>>> = OnceLock::new();

pub(crate) fn manifest_hashes() -> &'static Mutex<HashSet<u64>> {
    MANIFEST_HASHES.get_or_init(|| Mutex::new(HashSet::new()))
}

pub(crate) fn content_hash(s: &str) -> u64 {
    let mut h = AHasher::default();
    s.hash(&mut h);
    h.finish()
}

fn default_sbg_manifest_path() -> PathBuf {
    if let Ok(explicit) = std::env::var("SBG_MANIFEST_PATH") {
        return PathBuf::from(explicit);
    }
    if let Ok(out_dir) = std::env::var("SBG_OUTPUT_DIR") {
        return PathBuf::from(out_dir).join("sbg-manifest.json");
    }
    PathBuf::from("obj").join("_ns_").join("gen").join("sbg-manifest.json")
}

fn preload_sbg_manifest() {
    let manifest_path = default_sbg_manifest_path();
    if !manifest_path.exists() {
        return;
    }

    // Read once; feed the same string to both the loader and the dedup check.
    let Ok(content) = fs::read_to_string(&manifest_path) else { return; };

    let hash = content_hash(&content);
    {
        let mut hashes = manifest_hashes().lock();
        if !hashes.insert(hash) {
            return; // already loaded
        }
    }

    let mut loader = SbgManifestLoader::new();
    if loader.load_manifest_json(&content).is_ok() {
        proxy_manifests().lock().push(content);
    }
}

fn split_type_name(type_name: &str) -> (Option<String>, String) {
    match type_name.rsplit_once('.') {
        Some((namespace, class_name)) => (Some(namespace.to_string()), class_name.to_string()),
        None => (None, type_name.to_string()),
    }
}

fn extend_class_methods(class_declaration: &ClassDeclaration, methods: &mut Vec<MethodDeclaration>, seen: &mut HashSet<String>) {
    // Use contains() first so we only allocate a String when the method is new.
    // HashSet<String> supports Borrow<str>, so contains(&str) does not allocate.
    for method in class_declaration.methods() {
        let key = if !method.overload_name().is_empty() { method.overload_name() } else { method.name() };
        if !seen.contains(key) {
            seen.insert(key.to_string());
            methods.push(method.clone());
        }
    }

    if let Some(default_interface) = class_declaration.default_interface() {
        for method in default_interface.methods() {
            let key = if !method.overload_name().is_empty() { method.overload_name() } else { method.name() };
            if !seen.contains(key) {
                seen.insert(key.to_string());
                methods.push(method.clone());
            }
        }
    }

    for interface in class_declaration.implemented_interfaces() {
        for method in interface.methods() {
            let key = if !method.overload_name().is_empty() { method.overload_name() } else { method.name() };
            if !seen.contains(key) {
                seen.insert(key.to_string());
                methods.push(method.clone());
            }
        }
    }

    if !class_declaration.base_full_name().is_empty() {
        if let Some(base_declaration) = MetadataReader::find_by_name(class_declaration.base_full_name()) {
            let base_lock = base_declaration.read();
            if let Some(base_class) = base_lock.as_any().downcast_ref::<ClassDeclaration>() {
                extend_class_methods(base_class, methods, seen);
            }
        }
    }
}

fn extend_class_properties(class_declaration: &ClassDeclaration, properties: &mut Vec<PropertyDeclaration>, seen: &mut HashSet<String>) {
    for property in class_declaration.properties() {
        let key = property.name();
        if !seen.contains(key) {
            seen.insert(key.to_string());
            properties.push(property.clone());
        }
    }

    if let Some(default_interface) = class_declaration.default_interface() {
        for property in default_interface.properties() {
            let key = property.name();
            if !seen.contains(key) {
                seen.insert(key.to_string());
                properties.push(property.clone());
            }
        }
    }

    for interface in class_declaration.implemented_interfaces() {
        for property in interface.properties() {
            let key = property.name();
            if !seen.contains(key) {
                seen.insert(key.to_string());
                properties.push(property.clone());
            }
        }
    }

    if !class_declaration.base_full_name().is_empty() {
        if let Some(base_declaration) = MetadataReader::find_by_name(class_declaration.base_full_name()) {
            let base_lock = base_declaration.read();
            if let Some(base_class) = base_lock.as_any().downcast_ref::<ClassDeclaration>() {
                extend_class_properties(base_class, properties, seen);
            }
        }
    }
}

fn collect_class_methods(class_declaration: &ClassDeclaration) -> Vec<MethodDeclaration> {
    let mut methods = Vec::new();
    let mut seen = HashSet::new();
    extend_class_methods(class_declaration, &mut methods, &mut seen);
    methods
}

fn collect_class_properties(class_declaration: &ClassDeclaration) -> Vec<PropertyDeclaration> {
    let mut properties = Vec::new();
    let mut seen = HashSet::new();
    extend_class_properties(class_declaration, &mut properties, &mut seen);
    properties
}

struct ClassMembers {
    properties: AHashMap<String, PropertyDeclaration>,
    /// Keyed by overload name when present, plain name otherwise.
    methods: AHashMap<String, MethodDeclaration>,
}

/// Per-thread because `PropertyDeclaration` / `MethodDeclaration` carry raw
/// WinMD pointers that aren't `Send`. UWP runs single-threaded, so this is
/// effectively a global cache.
thread_local!(static CLASS_MEMBERS_CACHE: RefCell<AHashMap<String, ClassMembers>> = RefCell::new(AHashMap::new()));

fn fill_class_members(
    class_declaration: &ClassDeclaration,
    properties: &mut AHashMap<String, PropertyDeclaration>,
    methods: &mut AHashMap<String, MethodDeclaration>,
) {
    // Use contains_key() before inserting to avoid allocating the String key on every
    // call when the entry already exists; String-keyed maps can borrow &str for
    // contains_key / get, so the lookup is allocation-free on the common hit path.
    let mut absorb_props = |list: &[PropertyDeclaration]| {
        for p in list {
            let key = p.name();
            if !properties.contains_key(key) {
                properties.insert(key.to_string(), p.clone());
            }
        }
    };
    let mut absorb_methods = |list: &[MethodDeclaration]| {
        for m in list {
            let key = if !m.overload_name().is_empty() { m.overload_name() } else { m.name() };
            if !methods.contains_key(key) {
                methods.insert(key.to_string(), m.clone());
            }
        }
    };
    absorb_props(class_declaration.properties());
    absorb_methods(class_declaration.methods());
    if let Some(di) = class_declaration.default_interface() {
        absorb_props(di.properties());
        absorb_methods(di.methods());
    }
    for iface in class_declaration.implemented_interfaces() {
        absorb_props(iface.properties());
        absorb_methods(iface.methods());
    }
    if !class_declaration.base_full_name().is_empty() {
        if let Some(base_decl) = MetadataReader::find_by_name(class_declaration.base_full_name()) {
            let lock = base_decl.read();
            if let Some(base) = lock.as_any().downcast_ref::<ClassDeclaration>() {
                fill_class_members(base, properties, methods);
            }
        }
    }
}

fn with_class_members<R>(class_declaration: &ClassDeclaration, f: impl FnOnce(&ClassMembers) -> R) -> R {
    CLASS_MEMBERS_CACHE.with(|cache| {
        let full_name = class_declaration.full_name();

        // Fast path: read with a shared borrow — no String allocation for the key.
        // String-keyed maps accept &str via the Borrow<str> blanket impl.
        {
            let borrow = cache.borrow();
            if let Some(entry) = borrow.get(full_name) {
                return f(entry);
            }
        }

        // Cache miss: build the member maps, then insert.
        // fill_class_members calls itself recursively but never re-enters
        // with_class_members, so the RefCell is free to borrow_mut here.
        let mut properties = AHashMap::new();
        let mut methods = AHashMap::new();
        fill_class_members(class_declaration, &mut properties, &mut methods);
        let mut borrow = cache.borrow_mut();
        let entry = borrow
            .entry(full_name.to_string())
            .or_insert(ClassMembers { properties, methods });
        f(entry)
    })
}

fn find_class_property(class_declaration: &ClassDeclaration, name: &str) -> Option<PropertyDeclaration> {
    with_class_members(class_declaration, |m| m.properties.get(name).cloned())
}

fn find_class_method(class_declaration: &ClassDeclaration, name: &str) -> Option<MethodDeclaration> {
    with_class_members(class_declaration, |m| m.methods.get(name).cloned())
}

fn class_method_matches(class_declaration: &ClassDeclaration, name: &str) -> bool {
    let method_match = |m: &MethodDeclaration| {
        let on = m.overload_name();
        (!on.is_empty() && on == name) || m.name() == name
    };

    if class_declaration.methods().iter().any(method_match) { return true; }

    if let Some(di) = class_declaration.default_interface() {
        if di.methods().iter().any(method_match) { return true; }
    }

    for iface in class_declaration.implemented_interfaces() {
        if iface.methods().iter().any(method_match) { return true; }
    }

    if !class_declaration.base_full_name().is_empty() {
        if let Some(base_decl) = MetadataReader::find_by_name(class_declaration.base_full_name()) {
            let lock = base_decl.read();
            if let Some(base) = lock.as_any().downcast_ref::<ClassDeclaration>() {
                return class_method_matches(base, name);
            }
        }
    }
    false
}

fn class_property_matches(class_declaration: &ClassDeclaration, name: &str) -> bool {
    if class_declaration.properties().iter().any(|p| p.name() == name) { return true; }

    if let Some(di) = class_declaration.default_interface() {
        if di.properties().iter().any(|p| p.name() == name) { return true; }
    }

    for iface in class_declaration.implemented_interfaces() {
        if iface.properties().iter().any(|p| p.name() == name) { return true; }
    }

    if !class_declaration.base_full_name().is_empty() {
        if let Some(base_decl) = MetadataReader::find_by_name(class_declaration.base_full_name()) {
            let lock = base_decl.read();
            if let Some(base) = lock.as_any().downcast_ref::<ClassDeclaration>() {
                return class_property_matches(base, name);
            }
        }
    }
    false
}

fn class_has_member_named(class_declaration: &ClassDeclaration, name: &str) -> bool {
    class_method_matches(class_declaration, name) || class_property_matches(class_declaration, name)
}

fn find_event_methods(class_declaration: &ClassDeclaration, name: &str) -> Option<(MethodDeclaration, MethodDeclaration)> {
    let check = |events: &[EventDeclaration]| -> Option<(MethodDeclaration, MethodDeclaration)> {
        events.iter().find(|e| e.name() == name)
            .map(|e| (e.add_method().clone(), e.remove_method().clone()))
    };
    if let Some(m) = check(class_declaration.events()) { return Some(m); }
    if let Some(di) = class_declaration.default_interface() {
        if let Some(m) = check(di.events()) { return Some(m); }
    }
    for iface in class_declaration.implemented_interfaces() {
        if let Some(m) = check(iface.events()) { return Some(m); }
    }
    if !class_declaration.base_full_name().is_empty() {
        if let Some(base_decl) = MetadataReader::find_by_name(class_declaration.base_full_name()) {
            let lock = base_decl.read();
            if let Some(base) = lock.as_any().downcast_ref::<ClassDeclaration>() {
                return find_event_methods(base, name);
            }
        }
    }
    None
}

fn runtime_method_metadata_from_method(method: &MethodDeclaration) -> RuntimeMethodMetadata {
    let return_type = method.metadata()
        .map(|m| Signature::to_string(m, &method.return_type()))
        .unwrap_or_default();
    let parameters = method
        .parameters()
        .iter()
        .enumerate()
        .map(|(index, parameter)| {
            let type_name = parameter
                .metadata()
                .map(|metadata| Signature::to_string(metadata, &parameter.type_()))
                .unwrap_or_else(|| "Object".to_string());
            let name = if parameter.name().is_empty() {
                format!("arg{}", index)
            } else {
                parameter.name().to_string()
            };
            RuntimeParameterMetadata { name, type_name }
        })
        .collect::<Vec<_>>();

    RuntimeMethodMetadata {
        name: method.name().to_string(),
        return_type,
        parameters,
    }
}

fn runtime_property_metadata_from_property(property: &PropertyDeclaration) -> RuntimePropertyMetadata {
    let prop_type = property.getter().metadata()
        .map(|m| Signature::to_string(m, &property.getter().return_type()))
        .unwrap_or_else(|| "Object".to_string());

    RuntimePropertyMetadata {
        name: property.name().to_string(),
        prop_type,
        readable: true,
        writable: property.setter().is_some(),
    }
}

fn base_declaration_descriptor(
    full_name: String,
    namespace: Option<String>,
    class_name: String,
    declaration: &dyn BaseClassDeclarationImpl,
) -> serde_json::Value {
    let methods = declaration
        .methods()
        .iter()
        .filter(|method| method.is_exported())
        .map(runtime_method_metadata_from_method)
        .collect::<Vec<_>>();
    let properties = declaration
        .properties()
        .iter()
        .filter(|property| property.is_exported())
        .map(runtime_property_metadata_from_property)
        .collect::<Vec<_>>();
    let interfaces = declaration
        .implemented_interfaces()
        .iter()
        .map(|interface| interface.full_name().to_string())
        .collect::<Vec<_>>();

    serde_json::json!({
        "typeName": full_name,
        "className": class_name,
        "namespace": namespace,
        "methods": methods,
        "properties": properties,
        "interfaces": interfaces,
    })
}

fn build_runtime_type_descriptor(type_name: &str) -> Option<serde_json::Value> {
    let declaration = MetadataReader::find_by_name(type_name)?;
    let lock = declaration.read();
    let full_name = lock.full_name().to_string();
    let (namespace, class_name) = split_type_name(full_name.as_str());

    match lock.kind() {
        DeclarationKind::Class => {
            let class = lock.as_any().downcast_ref::<ClassDeclaration>()?;
            Some(base_declaration_descriptor(full_name, namespace, class_name, class))
        }
        DeclarationKind::Interface => lock
            .as_any()
            .downcast_ref::<InterfaceDeclaration>()
            .map(|interface| base_declaration_descriptor(full_name, namespace, class_name, interface)),
        DeclarationKind::GenericInterface => lock
            .as_any()
            .downcast_ref::<GenericInterfaceDeclaration>()
            .map(|interface| base_declaration_descriptor(full_name, namespace, class_name, interface)),
        DeclarationKind::GenericInterfaceInstance => lock
            .as_any()
            .downcast_ref::<GenericInterfaceInstanceDeclaration>()
            .map(|interface| base_declaration_descriptor(full_name, namespace, class_name, interface)),
        _ => None,
    }
}

fn handle_describe_winrt_type(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 1 {
        throw_js_error(scope, "__nsDescribeWinRTType(typeName) expects 1 argument");
        return;
    }

    let Some(type_name) = value_to_string(scope, args.get(0)) else {
        throw_js_error(scope, "Unable to convert typeName argument to string");
        return;
    };

    let Some(descriptor) = build_runtime_type_descriptor(type_name.as_str()) else {
        retval.set_null();
        return;
    };

    match serde_json::to_string(&descriptor) {
        Ok(json) => {
            if let Some(value) = v8::String::new(scope, json.as_str()) {
                retval.set(value.into());
            } else {
                retval.set_null();
            }
        }
        Err(error) => throw_js_error(scope, format!("Failed to serialize WinRT descriptor: {error}").as_str()),
    }
}


#[derive(Clone)]
pub(crate) struct DeclarationFFI {
    pub(crate) inner: Arc<RwLock<dyn Declaration>>,
    pub(crate) instance: Option<IUnknown>,
    pub(crate) parent: Option<Arc<RwLock<dyn Declaration>>>,
    pub(crate) struct_instance: Option<(Vec<u8>, Vec<NativeType>)>,
    pub(crate) event_tokens: std::collections::HashMap<String, i64>,
}

unsafe impl Sync for DeclarationFFI {}

unsafe impl Send for DeclarationFFI {}

impl DeclarationFFI {
    pub fn new(declaration: Arc<RwLock<dyn Declaration>>) -> Self {
        Self { inner: declaration, instance: None, parent: None, struct_instance: None, event_tokens: std::collections::HashMap::new() }
    }

    pub fn new_with_instance(declaration: Arc<RwLock<dyn Declaration>>, instance: Option<IUnknown>) -> Self {
        Self { inner: declaration, instance, parent: None, struct_instance: None, event_tokens: std::collections::HashMap::new() }
    }

    pub fn as_any(&self) -> MappedRwLockReadGuard<'_, RawRwLock, dyn Any> {
        RwLockReadGuard::map(self.inner.read(), |dec| dec.as_any())
    }

    pub fn read(&self) -> MappedRwLockReadGuard<'_, RawRwLock, dyn Declaration> {
        RwLockReadGuard::map(self.inner.read(), |dec| dec)
    }

    pub fn write(&self) -> MappedRwLockWriteGuard<'_, RawRwLock, dyn Declaration> {
        RwLockWriteGuard::map(self.inner.write(), |dec| dec)
    }
}

impl Deref for DeclarationFFI {
    type Target = RwLock<dyn Declaration>;

    fn deref(&self) -> &Self::Target {
        self.inner.deref()
    }
}

use metadata::declarations::interface_declaration::generic_interface_instance_declaration::GenericInterfaceInstanceDeclaration;
use crate::generic_method_call::GenericMethodCall;
use crate::method_call::MethodCall;
use crate::property_call::PropertyCall;

fn init_global(scope: &mut v8::ContextScope<v8::HandleScope<v8::Context>>, context: v8::Local<v8::Context>) {
    let global = context.global(scope);
    let value = v8::String::new(
        scope, "global",
    ).unwrap().into();
    global.define_own_property(scope, value, global.into(), v8::PropertyAttribute::READ_ONLY);
}

pub fn debug_output(msg: &str) {
    // Only emit verbose debug logs when `NS_DEBUG` is present. Always allow
    // important severities through (ERROR/WARN/DEVTOOLS/NativeScript).
    let important = msg.starts_with("[ERROR]")
        || msg.starts_with("[WARN]")
        || msg.starts_with("[DEVTOOLS]")
        || msg.starts_with("[NativeScript]");
    // Runtime-configurable flag: default true.
    let enabled = LOG_TO_CONSOLE.get_or_init(|| AtomicBool::new(true)).load(AtomicOrdering::Relaxed);

    if !enabled && !important {
        return;
    }

    // Send UTF-16 string to debugger for reliable Unicode output
    let mut wide: Vec<u16> = msg.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe { OutputDebugStringW(PCWSTR::from_raw(wide.as_ptr())) };
    eprint!("{}", msg);
    use std::io::Write;
    LOG_FILE.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            static LOG_PATH: OnceLock<String> = OnceLock::new();
            let path = LOG_PATH.get_or_init(|| {
                let mut p = std::env::temp_dir();
                p.push("console.log");
                let chosen = if std::fs::OpenOptions::new().create(true).append(true).open(&p).is_ok() {
                    p.to_string_lossy().into_owned()
                } else {
                    let base = std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\Users\\fortu".into());
                    format!("{}\\console.log", base)
                };
                let banner = format!("[NativeScript] log file: {}\n", chosen);
                let mut wide_banner: Vec<u16> = banner.encode_utf16().chain(std::iter::once(0)).collect();
                unsafe { OutputDebugStringW(PCWSTR::from_raw(wide_banner.as_ptr())) };
                chosen
            });
            *slot = std::fs::OpenOptions::new().create(true).append(true).open(path).ok();
        }
        if let Some(f) = slot.as_mut() {
            let _ = f.write_all(msg.as_bytes());
        }
    });

    // If this message came from the DevTools forwarder, map severity to
    // Windows Event Log so administrators can see important errors/warnings.
    if msg.starts_with("[DEVTOOLS]") {
        if let Some(rest) = msg.strip_prefix("[DEVTOOLS] ") {
            if rest.starts_with('[') {
                if let Some(end) = rest.find(']') {
                    let level = &rest[1..end];
                    use windows::Win32::System::EventLog::{EVENTLOG_ERROR_TYPE, EVENTLOG_WARNING_TYPE, EVENTLOG_INFORMATION_TYPE};
                    let event_type = match level {
                        "ERROR" | "EXCEPTION" => EVENTLOG_ERROR_TYPE,
                        "WARN" | "WARNING" => EVENTLOG_WARNING_TYPE,
                        _ => EVENTLOG_INFORMATION_TYPE,
                    };
                    crate::globals::console::report_event(msg, event_type);
                }
            }
        }
    }
}

/// Enable or disable logging to console at runtime. Default is `true`.
pub fn set_log_to_console(enabled: bool) {
    LOG_TO_CONSOLE.get_or_init(|| AtomicBool::new(true)).store(enabled, AtomicOrdering::Relaxed);
}

/// Query whether logging to console is enabled.
pub fn is_log_to_console() -> bool {
    LOG_TO_CONSOLE.get_or_init(|| AtomicBool::new(true)).load(AtomicOrdering::Relaxed)
}

/// C ABI: toggle logging to console from native hosts (e.g. C# P/Invoke).
/// Returns 1 on success (toggle applied), 0 if toggling is disabled (release builds).
#[no_mangle]
pub extern "C" fn ns_set_log_to_console(enabled: std::os::raw::c_int) -> std::os::raw::c_int {
    if cfg!(debug_assertions) {
        set_log_to_console(enabled != 0);
        1
    } else {
        0
    }
}

thread_local!(static LOG_FILE: RefCell<Option<fs::File>> = RefCell::new(None));

pub(crate) fn throw_js_error(scope: &mut v8::PinScope<'_, '_>, message: &str) {
    if let Some(msg) = v8::String::new(scope, message) {
        let err = v8::Exception::error(scope, msg.into());
        scope.throw_exception(err);
    }
}

pub(crate) fn class_activation_factory(full_name: &str) -> windows::core::Result<IUnknown> {
    let clazz_name = HSTRING::from(full_name);
    unsafe { RoGetActivationFactory::<IUnknown>(&clazz_name) }
}

pub(crate) fn resolve_class_factory_from_parent(dec: &DeclarationFFI) -> windows::core::Result<IUnknown> {
    if let Some(instance) = dec.instance.clone() {
        return Ok(instance);
    }

    let Some(parent) = dec.parent.as_ref() else {
        return Err(Error::new(
            HRESULT(0x80004005u32 as i32),
            "Static WinRT member is missing its owning class declaration",
        ));
    };

    let parent = parent.read();
    let Some(clazz) = parent.as_any().downcast_ref::<ClassDeclaration>() else {
        return Err(Error::new(
            HRESULT(0x80004005u32 as i32),
            "Static WinRT member parent is not a class declaration",
        ));
    };

    class_activation_factory(clazz.full_name())
}

fn try_get_async_status(
    scope: &mut v8::PinScope<'_, '_>,
    value: v8::Local<v8::Value>,
) -> Result<i32, String> {
    if !value.is_object() {
        return Err("Expected a wrapped WinRT async object".to_string());
    }

    let object = value
        .to_object(scope)
        .ok_or_else(|| "Expected a wrapped WinRT async object".to_string())?;
    let status_key = v8::String::new(scope, "Status")
        .ok_or_else(|| "Unable to allocate V8 string for async status lookup".to_string())?;
    let status = object
        .get(scope, status_key.into())
        .ok_or_else(|| "Async object does not expose Status".to_string())?;

    if let Ok(value) = v8::Local::<v8::Int32>::try_from(status) {
        return Ok(value.value());
    }

    if let Ok(value) = v8::Local::<v8::Uint32>::try_from(status) {
        return Ok(value.value() as i32);
    }

    if let Ok(value) = v8::Local::<v8::Number>::try_from(status) {
        return Ok(value.value() as i32);
    }

    if let Some(value) = status.integer_value(scope) {
        return Ok(value as i32);
    }

    if let Some(value) = status.number_value(scope) {
        if value.is_finite() {
            return Ok(value as i32);
        }
    }

    if let Some(value) = status.to_string(scope) {
        let status_text = value.to_rust_string_lossy(scope).to_ascii_lowercase();
        return match status_text.as_str() {
            "started" => Ok(0),
            "completed" => Ok(1),
            "canceled" | "cancelled" => Ok(2),
            "error" => Ok(3),
            _ => Err(format!("Async Status is not a recognized value: {status_text}")),
        };
    }

    Err("Async Status is not a numeric value".to_string())
}

fn handle_host_wait_for_async(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 1 {
        throw_js_error(scope, "__nsHostWaitForAsync expects a WinRT async object");
        return;
    }

    let op_value = args.get(0);
    let timeout_ms = if args.length() >= 2 {
        let timeout = args.get(1);
        if let Some(value) = timeout.integer_value(scope) {
            if value >= 0 { value as u64 } else { 0 }
        } else if let Some(value) = timeout.number_value(scope) {
            if value.is_finite() && value >= 0.0 {
                value as u64
            } else {
                0
            }
        } else {
            0
        }
    } else {
        0
    };

    let deadline = if timeout_ms == 0 { None } else { Some(Instant::now() + Duration::from_millis(timeout_ms)) };

    let mut message = MSG::default();

    loop {
        match try_get_async_status(scope, op_value) {
            Ok(0) => {
                // deadline == None means timeout_ms == 0: non-blocking check requested.
                // Return the op as-is rather than spinning forever.
                if deadline.is_none() {
                    retval.set(op_value);
                    return;
                }
                while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.into() {
                    unsafe {
                        let _ = TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
                ASYNC_PUMP_HOOK.with(|hook| {
                    if let Ok(mut guard) = hook.try_borrow_mut() {
                        if let Some(f) = guard.as_mut() { f(); }
                    }
                });
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(_) => {
                retval.set(op_value);
                return;
            }
            Err(msg) => {
                throw_js_error(scope, msg.as_str());
                return;
            }
        }

        if let Some(dl) = deadline {
            if Instant::now() >= dl {
                throw_js_error(scope, format!("Timed out waiting for WinRT async operation after {timeout_ms}ms").as_str());
                return;
            }
        }
    }
    }

fn handle_enqueue_microtask(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 1 {
        throw_js_error(scope, "__nsEnqueueMicrotask(callback) expects 1 argument");
        return;
    }

    let callback = match v8::Local::<v8::Function>::try_from(args.get(0)) {
        Ok(callback) => callback,
        Err(_) => {
            throw_js_error(scope, "__nsEnqueueMicrotask(callback) expects callback to be a function");
            return;
        }
    };

    scope.enqueue_microtask(callback);
    retval.set_undefined();
}

fn try_extract_pointer_from_value(
    scope: &mut v8::PinScope<'_, '_>,
    value: v8::Local<v8::Value>,
) -> Option<*mut c_void> {
    if value.is_null_or_undefined() {
        return Some(std::ptr::null_mut());
    }

    if let Ok(external) = v8::Local::<v8::External>::try_from(value) {
        return Some(external.value());
    }

    if !value.is_object() {
        return None;
    }

    let object = value.to_object(scope)?;

    if let Some(handle_key) = v8::String::new(scope, "handle") {
        if let Some(handle) = object.get(scope, handle_key.into()) {
            if let Ok(external) = v8::Local::<v8::External>::try_from(handle) {
                return Some(external.value());
            }
            if handle.is_null_or_undefined() {
                return Some(std::ptr::null_mut());
            }
        }
    }

    None
}

fn handle_pointer_key(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 1 {
        throw_js_error(scope, "__nsPointerKey expects a pointer-like value");
        return;
    }

    let pointer = match try_extract_pointer_from_value(scope, args.get(0)) {
        Some(pointer) => pointer,
        None => {
            throw_js_error(scope, "Unable to extract native pointer from value");
            return;
        }
    };

    let key = format!("0x{:x}", pointer as usize);
    if let Some(value) = v8::String::new(scope, key.as_str()) {
        retval.set(value.into());
    } else {
        retval.set_undefined();
    }
}

fn handle_buffer_to_pointer(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 1 {
        throw_js_error(scope, "__nsBufferToPointer expects an ArrayBuffer or ArrayBufferView");
        return;
    }

    let value = args.get(0);

    let pointer = if let Ok(array_buffer) = v8::Local::<v8::ArrayBuffer>::try_from(value) {
        match array_buffer.data() {
            Some(data) => data.as_ptr(),
            None => std::ptr::null_mut(),
        }
    } else if let Ok(view) = v8::Local::<v8::ArrayBufferView>::try_from(value) {
        let byte_offset = view.byte_offset();
        let Some(buffer) = view.buffer(scope) else {
            throw_js_error(scope, "ArrayBufferView does not expose a backing buffer");
            return;
        };
        match buffer.data() {
            Some(data) => unsafe { data.as_ptr().add(byte_offset) },
            None => std::ptr::null_mut(),
        }
    } else if value.is_null_or_undefined() {
        std::ptr::null_mut()
    } else {
        throw_js_error(scope, "__nsBufferToPointer expects an ArrayBuffer or ArrayBufferView");
        return;
    };

    if pointer.is_null() {
        retval.set_null();
    } else {
        let external = v8::External::new(scope, pointer);
        retval.set(external.into());
    }
}

fn value_to_string(scope: &mut v8::PinScope<'_, '_>, value: v8::Local<v8::Value>) -> Option<String> {
    let value = value.to_string(scope)?;
    Some(value.to_rust_string_lossy(scope))
}

fn handle_proxy_write_text_file(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 2 {
        throw_js_error(scope, "__nsProxyWriteTextFile(path, content) expects 2 arguments");
        return;
    }

    let Some(path) = value_to_string(scope, args.get(0)) else {
        throw_js_error(scope, "Unable to convert path argument to string");
        return;
    };
    let Some(content) = value_to_string(scope, args.get(1)) else {
        throw_js_error(scope, "Unable to convert content argument to string");
        return;
    };

    let path_buf = PathBuf::from(path);
    if let Some(parent) = path_buf.parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(err) = fs::create_dir_all(parent) {
                throw_js_error(scope, format!("Failed to create directory: {err}").as_str());
                return;
            }
        }
    }

    if let Err(err) = fs::write(&path_buf, content) {
        throw_js_error(scope, format!("Failed to write file: {err}").as_str());
        return;
    }

    retval.set_bool(true);
}

fn handle_proxy_compile_project(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 1 {
        throw_js_error(scope, "__nsProxyCompileProject(csprojPath[, configuration]) expects at least 1 argument");
        return;
    }

    let Some(project_path) = value_to_string(scope, args.get(0)) else {
        throw_js_error(scope, "Unable to convert csprojPath argument to string");
        return;
    };

    let configuration = if args.length() >= 2 {
        value_to_string(scope, args.get(1)).unwrap_or_else(|| "Debug".to_string())
    } else {
        "Debug".to_string()
    };

    let output = match Command::new("dotnet")
        .arg("build")
        .arg(project_path.as_str())
        .arg("-c")
        .arg(configuration.as_str())
        .arg("-v")
        .arg("minimal")
        .output()
    {
        Ok(output) => output,
        Err(err) => {
            throw_js_error(scope, format!("Failed to execute dotnet build: {err}").as_str());
            return;
        }
    };

    let result = v8::Object::new(scope);
    let success = output.status.success();
    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if let Some(key) = v8::String::new(scope, "success") {
        result.set(scope, key.into(), v8::Boolean::new(scope, success).into());
    }
    if let Some(key) = v8::String::new(scope, "exitCode") {
        result.set(scope, key.into(), v8::Integer::new(scope, exit_code).into());
    }
    if let Some(key) = v8::String::new(scope, "stdout") {
        if let Some(value) = v8::String::new(scope, stdout.as_str()) {
            result.set(scope, key.into(), value.into());
        }
    }
    if let Some(key) = v8::String::new(scope, "stderr") {
        if let Some(value) = v8::String::new(scope, stderr.as_str()) {
            result.set(scope, key.into(), value.into());
        }
    }

    retval.set(result.into());
}

fn handle_proxy_register_manifest(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 1 {
        throw_js_error(scope, "__nsProxyRegisterManifest(manifestJson) expects 1 argument");
        return;
    }

    let Some(manifest) = value_to_string(scope, args.get(0)) else {
        throw_js_error(scope, "Unable to convert manifest argument to string");
        return;
    };

    let mut manifests = proxy_manifests().lock();
    manifests.push(manifest);
    let index = manifests.len() as i32 - 1;
    retval.set(v8::Integer::new(scope, index).into());
}

fn default_auto_capture_path() -> PathBuf {
    if let Ok(explicit) = std::env::var("NSWINRT_AUTO_METADATA_PATH") {
        return PathBuf::from(explicit);
    }
    if let Ok(out_dir) = std::env::var("SBG_OUTPUT_DIR") {
        return PathBuf::from(out_dir).join("sbg_metadata.json");
    }
    PathBuf::from("sbg_output").join("sbg_metadata.json")
}

fn handle_proxy_auto_capture(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 1 {
        throw_js_error(scope, "__nsProxyAutoCapture(metadataJson) expects 1 argument");
        return;
    }

    let Some(metadata_json) = value_to_string(scope, args.get(0)) else {
        throw_js_error(scope, "Unable to convert metadata argument to string");
        return;
    };

    let path_buf = default_auto_capture_path();
    if let Some(parent) = path_buf.parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(err) = fs::create_dir_all(parent) {
                throw_js_error(scope, format!("Failed to create metadata directory: {err}").as_str());
                return;
            }
        }
    }

    let normalized = match serde_json::from_str::<Vec<RuntimeExtensionMetadata>>(metadata_json.as_str()) {
        Ok(extensions) => {
            let mut registry = RuntimeExtensionRegistry::new();
            for extension in extensions.iter().cloned() {
                registry.register(extension);
            }
            match serde_json::to_string_pretty(&extensions) {
                Ok(json) => json,
                Err(err) => {
                    throw_js_error(scope, format!("Failed to normalize captured metadata: {err}").as_str());
                    return;
                }
            }
        }
        Err(_) => metadata_json,
    };

    if let Err(err) = fs::write(&path_buf, normalized) {
        throw_js_error(scope, format!("Failed to write captured metadata: {err}").as_str());
        return;
    }

    if let Some(path) = path_buf.to_str() {
        if let Some(path_value) = v8::String::new(scope, path) {
            retval.set(path_value.into());
            return;
        }
    }
    retval.set_bool(true);
}

fn handle_read_text_file(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 1 {
        throw_js_error(scope, "__nsReadTextFile(path) expects 1 argument");
        return;
    }

    let Some(path) = value_to_string(scope, args.get(0)) else {
        throw_js_error(scope, "Unable to convert path argument to string");
        return;
    };

    match fs::read_to_string(Path::new(path.as_str())) {
        Ok(content) => {
            if let Some(value) = v8::String::new(scope, content.as_str()) {
                retval.set(value.into());
            } else {
                retval.set_null();
            }
        }
        Err(err) => throw_js_error(scope, format!("Failed to read module file: {err}").as_str()),
    }
}

fn handle_livesync_copy_file(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 2 {
        throw_js_error(scope, "__nsLiveSyncCopyFile(sourcePath, destPath) expects 2 arguments");
        return;
    }

    let Some(source_path) = value_to_string(scope, args.get(0)) else {
        throw_js_error(scope, "Unable to convert sourcePath argument to string");
        return;
    };
    let Some(dest_path) = value_to_string(scope, args.get(1)) else {
        throw_js_error(scope, "Unable to convert destPath argument to string");
        return;
    };

    if let Err(err) = livesync::copy_file(source_path.as_str(), dest_path.as_str()) {
        throw_js_error(scope, err.as_str());
        return;
    }

    retval.set_bool(true);
}

fn normalize_js_path(path: &str) -> PathBuf {
    if let Some(raw) = path.strip_prefix("file:///") {
        return PathBuf::from(raw.replace('/', "\\"));
    }
    if let Some(raw) = path.strip_prefix("file://") {
        return PathBuf::from(raw.replace('/', "\\"));
    }
    PathBuf::from(path)
}

fn try_resolve_with_known_extensions(candidate: PathBuf) -> PathBuf {
    if candidate.exists() {
        return candidate;
    }

    if candidate.extension().is_none() {
        for ext in ["js", "mjs", "cjs"] {
            let with_ext = candidate.with_extension(ext);
            if with_ext.exists() {
                return with_ext;
            }
        }
    }

    if candidate.is_dir() {
        for index_file in ["index.js", "index.mjs", "index.cjs"] {
            let with_index = candidate.join(index_file);
            if with_index.exists() {
                return with_index;
            }
        }
    }

    candidate
}

/// Resolve a module specifier to an absolute path given the referrer's absolute path.
/// Only handles relative (`./`, `../`) and absolute specifiers — bare specifiers are
/// treated as already-absolute paths (webpack bundles only emit relative imports).
fn resolve_esm_path(specifier: &str, referrer_path: Option<&str>) -> String {
    let candidate = if specifier.starts_with("./") || specifier.starts_with("../") {
        let parent = referrer_path
            .map(normalize_js_path)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let base = if parent.is_file() {
            parent.parent().map(Path::to_path_buf).unwrap_or(parent)
        } else {
            parent
        };
        base.join(specifier)
    } else {
        normalize_js_path(specifier)
    };
    let candidate = try_resolve_with_known_extensions(candidate);
    candidate.canonicalize().unwrap_or(candidate).to_string_lossy().into_owned()
}

/// Stateless V8 resolve-module callback used during `instantiate_module`.
/// All modules must have been pre-compiled by `compile_module_graph` and stored
/// in `ESM_MODULE_REGISTRY` / `ESM_HASH_TO_PATH` before this is called.
fn resolve_module_callback<'s>(
    context: v8::Local<'s, v8::Context>,
    specifier: v8::Local<'s, v8::String>,
    _import_attributes: v8::Local<'s, v8::FixedArray>,
    referrer: v8::Local<'s, v8::Module>,
) -> Option<v8::Local<'s, v8::Module>> {
    v8::callback_scope!(unsafe scope, context);

    let spec = specifier.to_rust_string_lossy(scope);
    let referrer_hash = referrer.get_identity_hash().get();
    let referrer_path = ESM_HASH_TO_PATH.with(|m| m.borrow().get(&referrer_hash).cloned());
    let resolved = resolve_esm_path(&spec, referrer_path.as_deref());

    ESM_MODULE_REGISTRY.with(|registry| {
        let registry = registry.borrow();
        registry.get(&resolved).map(|global| v8::Local::new(scope, global))
    })
}

/// Walk and pre-compile the entire transitive module graph starting from `path`.
/// Compiled modules are stored in `ESM_MODULE_REGISTRY` and `ESM_HASH_TO_PATH`.
/// Must be called before `instantiate_module`.
fn compile_module_graph(scope: &mut v8::PinScope<'_, '_>, source: &str, path: &str) {
    if ESM_MODULE_REGISTRY.with(|r| r.borrow().contains_key(path)) {
        return;
    }

    let Some(source_str) = v8::String::new(scope, source) else { return };
    let Some(name_str) = v8::String::new(scope, path) else { return };
    let name_val: v8::Local<v8::Value> = name_str.into();
    let origin = v8::ScriptOrigin::new(
        scope, name_val, 0, 0, false, -1, None, false, false, true, None,
    );
    let mut compiler_source = v8::script_compiler::Source::new(source_str, Some(&origin));
    let Some(module) = v8::script_compiler::compile_module(scope, &mut compiler_source) else {
        return;
    };

    let identity_hash = module.get_identity_hash().get();

    // Collect child specifiers as Rust strings before any mutable borrow of scope.
    let requests = module.get_module_requests();
    let child_specifiers: Vec<String> = (0..requests.length())
        .filter_map(|i| {
            let data = requests.get(scope, i)?;
            let request: v8::Local<v8::ModuleRequest> = data.try_into().ok()?;
            Some(request.get_specifier().to_rust_string_lossy(scope))
        })
        .collect();

    // Store the compiled module (breaks import cycles on re-entry).
    let global = v8::Global::new(scope, module);
    ESM_MODULE_REGISTRY.with(|r| r.borrow_mut().insert(path.to_string(), global));
    ESM_HASH_TO_PATH.with(|m| m.borrow_mut().insert(identity_hash, path.to_string()));

    // Recurse into each dependency.
    for spec in child_specifiers {
        let child_path = resolve_esm_path(&spec, Some(path));
        if ESM_MODULE_REGISTRY.with(|r| r.borrow().contains_key(&child_path)) {
            continue;
        }
        match fs::read_to_string(&child_path) {
            Ok(content) => compile_module_graph(scope, &content, &child_path),
            Err(e) => debug_output(&format!(
                "[NativeScript] ESM: cannot read dependency {child_path}: {e}\n"
            )),
        }
    }
}


fn handle_resolve_module_path(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 1 {
        throw_js_error(scope, "__nsResolveModulePath(specifier[, parentPath, appRoot]) expects at least 1 argument");
        return;
    }

    let Some(specifier) = value_to_string(scope, args.get(0)) else {
        throw_js_error(scope, "Unable to convert module specifier to string");
        return;
    };

    let parent_path = if args.length() >= 2 {
        value_to_string(scope, args.get(1))
    } else {
        None
    };

    let app_root = if args.length() >= 3 {
        value_to_string(scope, args.get(2)).unwrap_or_default()
    } else {
        String::new()
    };

    let mut candidate = if specifier.starts_with("./") || specifier.starts_with("../") {
        let parent = parent_path
            .map(|value| normalize_js_path(value.as_str()))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let base = if parent.is_file() {
            parent.parent().map(Path::to_path_buf).unwrap_or(parent)
        } else {
            parent
        };
        base.join(specifier)
    } else {
        let direct = normalize_js_path(specifier.as_str());
        if direct.is_absolute() {
            direct
        } else {
            let app_base = if app_root.is_empty() {
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
            } else {
                let lower = PathBuf::from(&app_root).join("app");
                if lower.exists() {
                    lower
                } else {
                    PathBuf::from(&app_root).join("App")
                }
            };
            app_base.join(direct)
        }
    };

    candidate = try_resolve_with_known_extensions(candidate);
    let resolved = candidate.canonicalize().unwrap_or(candidate);

    if let Some(value) = resolved.to_str().and_then(|path| v8::String::new(scope, path)) {
        retval.set(value.into());
    } else {
        retval.set_null();
    }
}

fn handle_proxy_list_manifests(
    scope: &mut v8::PinScope<'_, '_>,
    _args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let manifests = proxy_manifests().lock();
    let array = v8::Array::new(scope, manifests.len() as i32);
    for (i, entry) in manifests.iter().enumerate() {
        if let Some(value) = v8::String::new(scope, entry.as_str()) {
            array.set_index(scope, i as u32, value.into());
        }
    }
    retval.set(array.into());
}

fn value_to_json_string(scope: &mut v8::PinScope<'_, '_>, value: v8::Local<v8::Value>) -> Option<String> {
    let json = v8::json::stringify(scope, value)?;
    Some(json.to_rust_string_lossy(scope))
}


fn handle_worker_create_threaded(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 3 {
        throw_js_error(scope, "__nsWorkerCreateThreaded(source, filename, appRoot) expects 3 arguments");
        return;
    }

    let Some(source) = value_to_string(scope, args.get(0)) else {
        throw_js_error(scope, "Unable to convert worker source to string");
        return;
    };

    let Some(filename) = value_to_string(scope, args.get(1)) else {
        throw_js_error(scope, "Unable to convert worker filename to string");
        return;
    };

    let Some(app_root) = value_to_string(scope, args.get(2)) else {
        throw_js_error(scope, "Unable to convert appRoot to string");
        return;
    };

    match worker_threads::create_worker(app_root, source, filename) {
        Ok(worker_id) => retval.set_double(worker_id as f64),
        Err(err) => throw_js_error(scope, err.as_str()),
    }
}

fn handle_worker_post_message(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    _retval: v8::ReturnValue,
) {
    if args.length() < 2 {
        throw_js_error(scope, "__nsWorkerPostMessage(workerId, value) expects 2 arguments");
        return;
    }

    let worker_id = args.get(0).number_value(scope).unwrap_or(-1.0);
    if worker_id < 0.0 {
        throw_js_error(scope, "Invalid worker id");
        return;
    }

    let value = args.get(1);
    let Some(bytes) = Runtime::serialize_value(scope, value) else {
        throw_js_error(scope, "DataCloneError: value could not be cloned.");
        return;
    };

    if let Err(err) = worker_threads::post_message(worker_id as u64, bytes) {
        throw_js_error(scope, err.as_str());
    }
}

/// Convert a `PolledWorkerEvent` to a V8 value suitable for returning to JS.
/// `Message` bytes are deserialized via V8 structured clone.
fn polled_event_to_v8<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    event: worker_threads::PolledWorkerEvent,
) -> Option<v8::Local<'s, v8::Value>> {
    match event {
        worker_threads::PolledWorkerEvent::Message(bytes) => {
            Runtime::deserialize_value(scope, &bytes)
        }
        worker_threads::PolledWorkerEvent::Error(error) => {
            let obj = v8::Object::new(scope);
            if let Some(key) = v8::String::new(scope, "__workerError") {
                if let Some(val) = v8::String::new(scope, error.as_str()) {
                    obj.set(scope, key.into(), val.into());
                }
            }
            Some(obj.into())
        }
        worker_threads::PolledWorkerEvent::Exited => {
            let obj = v8::Object::new(scope);
            if let Some(key) = v8::String::new(scope, "__workerExit") {
                obj.set(scope, key.into(), v8::Boolean::new(scope, true).into());
            }
            Some(obj.into())
        }
    }
}

fn handle_worker_poll_messages(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 1 {
        throw_js_error(scope, "__nsWorkerPollMessages(workerId) expects 1 argument");
        return;
    }

    let worker_id = args.get(0).number_value(scope).unwrap_or(-1.0);
    if worker_id < 0.0 {
        throw_js_error(scope, "Invalid worker id");
        return;
    }

    let events = match worker_threads::poll_events(worker_id as u64) {
        Ok(events) => events,
        Err(err) => {
            throw_js_error(scope, err.as_str());
            return;
        }
    };

    let array = v8::Array::new(scope, events.len() as i32);
    for (index, event) in events.into_iter().enumerate() {
        if let Some(value) = polled_event_to_v8(scope, event) {
            array.set_index(scope, index as u32, value);
        }
    }
    retval.set(array.into());
}

fn handle_worker_terminate(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    _retval: v8::ReturnValue,
) {
    if args.length() < 1 {
        throw_js_error(scope, "__nsWorkerTerminate(workerId) expects 1 argument");
        return;
    }

    let worker_id = args.get(0).number_value(scope).unwrap_or(-1.0);
    if worker_id < 0.0 {
        throw_js_error(scope, "Invalid worker id");
        return;
    }

    if let Err(err) = worker_threads::terminate_worker(worker_id as u64) {
        throw_js_error(scope, err.as_str());
    }
}

fn handle_worker_poll_messages_blocking(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 2 {
        throw_js_error(scope, "__nsWorkerPollMessagesBlocking(workerId, timeoutMs) expects 2 arguments");
        return;
    }

    let worker_id = args.get(0).number_value(scope).unwrap_or(-1.0);
    if worker_id < 0.0 {
        throw_js_error(scope, "Invalid worker id");
        return;
    }

    let timeout_ms = args.get(1).number_value(scope).unwrap_or(0.0);
    let timeout_ms = if timeout_ms.is_sign_negative() { 0_u64 } else { timeout_ms as u64 };

    let events = match worker_threads::poll_events_blocking(worker_id as u64, timeout_ms) {
        Ok(events) => events,
        Err(err) => {
            throw_js_error(scope, err.as_str());
            return;
        }
    };

    let array = v8::Array::new(scope, events.len() as i32);
    for (index, event) in events.into_iter().enumerate() {
        if let Some(value) = polled_event_to_v8(scope, event) {
            array.set_index(scope, index as u32, value);
        }
    }
    retval.set(array.into());
}

fn init_async_helpers(scope: &mut v8::ContextScope<v8::HandleScope<v8::Context>>, app_root: &str) {
    let global = scope.get_current_context().global(scope);
    if let Some(wait_name) = v8::String::new(scope, "__nsHostWaitForAsync") {
        if let Some(wait_fn) = v8::Function::new(scope, handle_host_wait_for_async) {
            global.define_own_property(scope, wait_name.into(), wait_fn.into(), v8::PropertyAttribute::READ_ONLY);
        }
    }

    if let Some(enqueue_name) = v8::String::new(scope, "__nsEnqueueMicrotask") {
        if let Some(enqueue_fn) = v8::Function::new(scope, handle_enqueue_microtask) {
            global.define_own_property(scope, enqueue_name.into(), enqueue_fn.into(), v8::PropertyAttribute::READ_ONLY);
        }
    }

    if let Some(pointer_key_name) = v8::String::new(scope, "__nsPointerKey") {
        if let Some(pointer_key_fn) = v8::Function::new(scope, handle_pointer_key) {
            global.define_own_property(scope, pointer_key_name.into(), pointer_key_fn.into(), v8::PropertyAttribute::READ_ONLY);
        }
    }

    if let Some(buffer_to_pointer_name) = v8::String::new(scope, "__nsBufferToPointer") {
        if let Some(buffer_to_pointer_fn) = v8::Function::new(scope, handle_buffer_to_pointer) {
            global.define_own_property(scope, buffer_to_pointer_name.into(), buffer_to_pointer_fn.into(), v8::PropertyAttribute::READ_ONLY);
        }
    }

    if let Some(write_text_name) = v8::String::new(scope, "__nsProxyWriteTextFile") {
        if let Some(write_text_fn) = v8::Function::new(scope, handle_proxy_write_text_file) {
            global.define_own_property(scope, write_text_name.into(), write_text_fn.into(), v8::PropertyAttribute::READ_ONLY);
        }
    }

    if let Some(compile_name) = v8::String::new(scope, "__nsProxyCompileProject") {
        if let Some(compile_fn) = v8::Function::new(scope, handle_proxy_compile_project) {
            global.define_own_property(scope, compile_name.into(), compile_fn.into(), v8::PropertyAttribute::READ_ONLY);
        }
    }

    if let Some(register_name) = v8::String::new(scope, "__nsProxyRegisterManifest") {
        if let Some(register_fn) = v8::Function::new(scope, handle_proxy_register_manifest) {
            global.define_own_property(scope, register_name.into(), register_fn.into(), v8::PropertyAttribute::READ_ONLY);
        }
    }

    if let Some(list_name) = v8::String::new(scope, "__nsProxyListManifests") {
        if let Some(list_fn) = v8::Function::new(scope, handle_proxy_list_manifests) {
            global.define_own_property(scope, list_name.into(), list_fn.into(), v8::PropertyAttribute::READ_ONLY);
        }
    }

    if let Some(capture_name) = v8::String::new(scope, "__nsProxyAutoCapture") {
        if let Some(capture_fn) = v8::Function::new(scope, handle_proxy_auto_capture) {
            global.define_own_property(scope, capture_name.into(), capture_fn.into(), v8::PropertyAttribute::READ_ONLY);
        }
    }

    if let Some(read_file_name) = v8::String::new(scope, "__nsReadTextFile") {
        if let Some(read_file_fn) = v8::Function::new(scope, handle_read_text_file) {
            global.define_own_property(scope, read_file_name.into(), read_file_fn.into(), v8::PropertyAttribute::READ_ONLY);
        }
    }

    if let Some(resolve_module_name) = v8::String::new(scope, "__nsResolveModulePath") {
        if let Some(resolve_module_fn) = v8::Function::new(scope, handle_resolve_module_path) {
            global.define_own_property(scope, resolve_module_name.into(), resolve_module_fn.into(), v8::PropertyAttribute::READ_ONLY);
        }
    }

    if let Some(app_root_name) = v8::String::new(scope, "__nsAppRoot") {
        if let Some(app_root_value) = v8::String::new(scope, app_root) {
            global.define_own_property(scope, app_root_name.into(), app_root_value.into(), v8::PropertyAttribute::READ_ONLY);
        }
    }

    if let Some(describe_name) = v8::String::new(scope, "__nsDescribeWinRTType") {
        if let Some(describe_fn) = v8::Function::new(scope, handle_describe_winrt_type) {
            global.define_own_property(scope, describe_name.into(), describe_fn.into(), v8::PropertyAttribute::READ_ONLY);
        }
    }

    if let Some(worker_create_name) = v8::String::new(scope, "__nsWorkerCreateThreaded") {
        if let Some(worker_create_fn) = v8::Function::new(scope, handle_worker_create_threaded) {
            global.define_own_property(scope, worker_create_name.into(), worker_create_fn.into(), v8::PropertyAttribute::READ_ONLY);
        }
    }

    if let Some(worker_post_name) = v8::String::new(scope, "__nsWorkerPostMessage") {
        if let Some(worker_post_fn) = v8::Function::new(scope, handle_worker_post_message) {
            global.define_own_property(scope, worker_post_name.into(), worker_post_fn.into(), v8::PropertyAttribute::READ_ONLY);
        }
    }

    if let Some(worker_poll_name) = v8::String::new(scope, "__nsWorkerPollMessages") {
        if let Some(worker_poll_fn) = v8::Function::new(scope, handle_worker_poll_messages) {
            global.define_own_property(scope, worker_poll_name.into(), worker_poll_fn.into(), v8::PropertyAttribute::READ_ONLY);
        }
    }

    if let Some(worker_terminate_name) = v8::String::new(scope, "__nsWorkerTerminate") {
        if let Some(worker_terminate_fn) = v8::Function::new(scope, handle_worker_terminate) {
            global.define_own_property(scope, worker_terminate_name.into(), worker_terminate_fn.into(), v8::PropertyAttribute::READ_ONLY);
        }
    }

    if let Some(worker_poll_blocking_name) = v8::String::new(scope, "__nsWorkerPollMessagesBlocking") {
        if let Some(worker_poll_blocking_fn) = v8::Function::new(scope, handle_worker_poll_messages_blocking) {
            global.define_own_property(scope, worker_poll_blocking_name.into(), worker_poll_blocking_fn.into(), v8::PropertyAttribute::READ_ONLY);
        }
    }

    if let Some(livesync_copy_name) = v8::String::new(scope, "__nsLiveSyncCopyFile") {
        if let Some(livesync_copy_fn) = v8::Function::new(scope, handle_livesync_copy_file) {
            global.define_own_property(scope, livesync_copy_name.into(), livesync_copy_fn.into(), v8::PropertyAttribute::READ_ONLY);
        }
    }

    // DevTools inspector host functions exposed to JS.
    if let Some(reg_name) = v8::String::new(scope, "__registerDomainDispatcher") {
        if let Some(reg_fn) = v8::Function::new(scope, global_fns::handle_register_domain_dispatcher) {
            global.define_own_property(scope, reg_name.into(), reg_fn.into(), v8::PropertyAttribute::READ_ONLY);
        }
    }

    if let Some(send_name) = v8::String::new(scope, "__inspectorSendEvent") {
        if let Some(send_fn) = v8::Function::new(scope, global_fns::handle_inspector_send_event) {
            global.define_own_property(scope, send_name.into(), send_fn.into(), v8::PropertyAttribute::READ_ONLY);
        }
    }

    if let Some(ts_name) = v8::String::new(scope, "__inspectorTimestamp") {
        if let Some(ts_fn) = v8::Function::new(scope, global_fns::handle_inspector_timestamp) {
            global.define_own_property(scope, ts_name.into(), ts_fn.into(), v8::PropertyAttribute::READ_ONLY);
        }
    }

        let helper_source = r#"
        (function () {
            if (typeof globalThis.queueMicrotask !== 'function') {
                globalThis.queueMicrotask = function (callback) {
                    if (typeof callback !== 'function') {
                        throw new TypeError('queueMicrotask callback must be a function');
                    }

                    if (typeof globalThis.__nsEnqueueMicrotask === 'function') {
                        globalThis.__nsEnqueueMicrotask(callback);
                        return;
                    }

                    Promise.resolve().then(callback).catch(function (err) {
                        if (typeof globalThis.__ns__setTimeout === 'function') {
                            globalThis.__ns__setTimeout(function () { throw err; }, 0);
                        } else if (typeof globalThis.setTimeout === 'function') {
                            globalThis.setTimeout(function () { throw err; }, 0);
                        } else {
                            throw err;
                        }
                    });
                };
            }

            var defaultTimeoutMs = 0;
            var statusEnum =
                (globalThis.Windows &&
                    globalThis.Windows.Foundation &&
                    globalThis.Windows.Foundation.AsyncStatus) ||
                { Started: 0, Completed: 1, Canceled: 2, Error: 3 };

            function normalizeTimeoutMs(options) {
                if (typeof options === 'number' && Number.isFinite(options) && options >= 0) {
                    return Math.floor(options);
                }

                if (options && typeof options === 'object') {
                    if (typeof options.timeoutMs === 'number' && Number.isFinite(options.timeoutMs) && options.timeoutMs >= 0) {
                        return Math.floor(options.timeoutMs);
                    }
                }

                return defaultTimeoutMs;
            }

            function normalizeStatus(status) {
                if (status == null) {
                    return Number.NaN;
                }
                if (typeof status === 'number') {
                    return status;
                }
                if (typeof status === 'string') {
                    var lower = status.toLowerCase();
                    if (lower === 'started') return 0;
                    if (lower === 'completed') return 1;
                    if (lower === 'canceled' || lower === 'cancelled') return 2;
                    if (lower === 'error') return 3;
                }
                if (typeof status.valueOf === 'function') {
                    var value = status.valueOf();
                    if (typeof value === 'number') {
                        return value;
                    }
                    if (typeof value === 'string') {
                        return normalizeStatus(value);
                    }
                }
                var coerced = Number(status);
                return Number.isNaN(coerced) ? Number.NaN : coerced;
            }

            function setDefaultTimeoutMs(timeoutMs) {
                if (typeof timeoutMs !== 'number' || !Number.isFinite(timeoutMs) || timeoutMs < 0) {
                    throw new Error('NSWinRT.setDefaultTimeoutMs(timeoutMs) expects a finite number >= 0');
                }
                defaultTimeoutMs = Math.floor(timeoutMs);
                return defaultTimeoutMs;
            }

            function wait(op, options) {
                if (typeof globalThis.__nsHostWaitForAsync === 'function') {
                    globalThis.__nsHostWaitForAsync(op, normalizeTimeoutMs(options));
                }
                return op;
            }

            function getStatus(op) {
                return normalizeStatus(op && op.Status);
            }

            function getResults(op) {
                if (op && typeof op.GetResults === 'function') {
                    return op.GetResults();
                }
                return undefined;
            }

            function toPromise(op, options) {
                if (op == null) {
                    return Promise.resolve(op);
                }

                // Match NativeScript runtime style: native objects are returned as-is.
                // Promise conversion is opt-in via this helper.
                // Skip for objects with a 'Completed' property (WinRT IAsyncOperation /
                // .NET TaskToAsyncOperationAdapter) — the proxy makes them appear thenable
                // but calling .then(resolve, reject) on them fails at the .NET boundary.
                if (typeof op.then === 'function' && !('Completed' in op)) {
                    return op;
                }

                return new Promise(function (resolve, reject) {
                    var settled = false;

                    function settleFromStatus(overrideStatus) {
                        if (settled) { return; }
                        try {
                            var status = normalizeStatus(
                                overrideStatus !== undefined ? overrideStatus : (op && op.Status)
                            );

                            if (status === statusEnum.Completed || status === 1) {
                                settled = true;
                                resolve(getResults(op));
                                return;
                            }
                            if (status === statusEnum.Canceled || status === 2) {
                                settled = true;
                                reject(new Error('WinRT async operation was canceled'));
                                return;
                            }
                            if (status === statusEnum.Error || status === 3) {
                                settled = true;
                                reject((op && op.ErrorCode) || new Error('WinRT async operation failed'));
                                return;
                            }
                            // status === 0 (Started): still running — do not settle yet.
                        } catch (err) {
                            settled = true;
                            reject(err);
                        }
                    }

                    try {
                        if (op && 'Completed' in op) {
                            // Settle immediately if the operation already finished before we
                            // could register the handler — WinRT will not fire Completed
                            // retroactively once the operation has left the Started state.
                            var initialStatus = normalizeStatus(op.Status);
                            if (!Number.isNaN(initialStatus) && initialStatus !== 0) {
                                settleFromStatus(initialStatus);
                                return;
                            }

                            op.Completed = function (asyncInfo, asyncStatus) {
                                settleFromStatus(asyncStatus);
                            };

                            // Re-check after assignment: the op may have completed in the
                            // narrow window between the status read and the handler assignment.
                            var raceStatus = normalizeStatus(op.Status);
                            if (!Number.isNaN(raceStatus) && raceStatus !== 0) {
                                settleFromStatus(raceStatus);
                            }
                            return;
                        }
                    } catch (_) {
                        // Completed setter not available; fall through to synchronous wait.
                    }

                    // Synchronous wait fallback — only safe when a finite timeout is given.
                    // Calling wait() with timeout=0 enters an infinite spin in the host.
                    var timeoutMs = normalizeTimeoutMs(options);
                    if (timeoutMs === 0) {
                        var currentStatus = normalizeStatus(op && op.Status);
                        if (!Number.isNaN(currentStatus) && currentStatus !== 0) {
                            settleFromStatus(currentStatus);
                        } else {
                            reject(new Error(
                                'Cannot await this WinRT async operation: it has no Completed property ' +
                                'and no timeoutMs was specified. Pass { timeoutMs: N } as the second argument.'
                            ));
                        }
                        return;
                    }

                    try {
                        wait(op, options);
                        settleFromStatus();
                    } catch (err) {
                        reject(err);
                    }
                });
            }

            function onCompleted(op, callback, options) {
                if (typeof callback !== 'function') {
                    throw new Error('NSWinRT.onCompleted(op, callback[, options]) expects callback to be a function');
                }

                try {
                    if (op && 'Completed' in op) {
                        op.Completed = function (asyncInfo, asyncStatus) {
                            callback(asyncInfo || op, normalizeStatus(asyncStatus));
                        };
                        return op;
                    }
                } catch (_) {
                    // Fall through to polling fallback.
                }

                Promise.resolve().then(function () {
                    wait(op, options);
                    callback(op, getStatus(op));
                });

                return op;
            }

            globalThis.__nsWinRTToPromise = toPromise;
            globalThis.NSWinRT = globalThis.NSWinRT || {};
            globalThis.NSWinRT.toPromise = toPromise;
            globalThis.NSWinRT.wait = wait;
            globalThis.NSWinRT.getStatus = getStatus;
            globalThis.NSWinRT.getResults = getResults;
            globalThis.NSWinRT.onCompleted = onCompleted;
            globalThis.NSWinRT.setDefaultTimeoutMs = setDefaultTimeoutMs;

            function Pointer(handle) {
                this.handle = handle == null ? null : handle;
            }

            Pointer.prototype.isNull = function () {
                return this.handle == null;
            };

            Pointer.prototype.unwrap = function () {
                return this.handle;
            };

            Pointer.prototype.toString = function () {
                return this.isNull() ? '[Pointer null]' : '[Pointer external]';
            };

            function asPointer(value) {
                return value instanceof Pointer ? value : new Pointer(value);
            }

            function handleOf(value) {
                return value instanceof Pointer ? value.handle : value;
            }

            function asBufferSource(value) {
                if (value == null) {
                    return value;
                }

                if (value instanceof ArrayBuffer || ArrayBuffer.isView(value)) {
                    return value;
                }

                throw new Error('NSWinRT.interop.asBufferSource(value) expects ArrayBuffer or ArrayBufferView');
            }

            function asUint8View(value) {
                var source = asBufferSource(value);
                if (source == null) {
                    return new Uint8Array(0);
                }

                if (source instanceof ArrayBuffer) {
                    return new Uint8Array(source);
                }

                return new Uint8Array(source.buffer, source.byteOffset, source.byteLength);
            }

            function asDataView(value) {
                var source = asBufferSource(value);
                if (source == null) {
                    return new DataView(new ArrayBuffer(0));
                }

                if (source instanceof ArrayBuffer) {
                    return new DataView(source);
                }

                return new DataView(source.buffer, source.byteOffset, source.byteLength);
            }

            // WinRT DateTime stores 100ns ticks since 1601-01-01T00:00:00Z.
            var winRtUnixEpochOffsetTicks = 116444736000000000n;

            function toWinRTDateTimeTicks(input) {
                var ms;
                if (input instanceof Date) {
                    ms = input.getTime();
                } else if (typeof input === 'number') {
                    ms = input;
                } else {
                    throw new Error('NSWinRT.interop.toWinRTDateTimeTicks expects Date or millisecond timestamp');
                }

                if (!Number.isFinite(ms)) {
                    throw new Error('Invalid Date/time value for WinRT conversion');
                }

                return BigInt(Math.trunc(ms)) * 10000n + winRtUnixEpochOffsetTicks;
            }

            function fromWinRTDateTimeTicks(value) {
                if (value == null) {
                    return new Date(Number.NaN);
                }

                var ticks = typeof value === 'bigint' ? value : BigInt(Math.trunc(Number(value)));
                var unixTicks = ticks - winRtUnixEpochOffsetTicks;
                var ms = Number(unixTicks / 10000n);
                return new Date(ms);
            }

            var pointerBufferRegistry = new Map();

            function pointerKey(value) {
                if (typeof globalThis.__nsPointerKey !== 'function') {
                    return null;
                }
                return globalThis.__nsPointerKey(value);
            }

            function pointerFromBuffer(value) {
                var source = asBufferSource(value);
                if (source == null || typeof globalThis.__nsBufferToPointer !== 'function') {
                    return null;
                }
                return globalThis.__nsBufferToPointer(source);
            }

            function trackBufferSource(value) {
                var source = asBufferSource(value);
                if (source == null) {
                    return null;
                }

                var pointer = pointerFromBuffer(source);
                var key = pointerKey(pointer);
                if (key != null) {
                    pointerBufferRegistry.set(String(key), source);
                }
                return pointer;
            }

            function resolveTrackedBuffer(pointerLike) {
                var key = pointerKey(pointerLike);
                if (key == null) {
                    return undefined;
                }
                return pointerBufferRegistry.get(String(key));
            }

            globalThis.NSWinRT.interop = {
                Pointer: Pointer,
                pointer: asPointer,
                isPointer: function (value) {
                    return value instanceof Pointer;
                },
                handleOf: handleOf,
                asBufferSource: asBufferSource,
                asUint8View: asUint8View,
                asDataView: asDataView,
                toWinRTDateTimeTicks: toWinRTDateTimeTicks,
                fromWinRTDateTimeTicks: fromWinRTDateTimeTicks,
                pointerKey: pointerKey,
                pointerFromBuffer: pointerFromBuffer,
                trackBufferSource: trackBufferSource,
                resolveTrackedBuffer: resolveTrackedBuffer,
                byteLengthOf: function (value) {
                    var buffer = asBufferSource(value);
                    if (buffer == null) {
                        return 0;
                    }
                    return typeof buffer.byteLength === 'number' ? buffer.byteLength : 0;
                },
                byteOffsetOf: function (value) {
                    if (ArrayBuffer.isView(value)) {
                        return value.byteOffset;
                    }
                    return 0;
                },
                readU8: function (value, offset) {
                    return asDataView(value).getUint8(offset >>> 0);
                },
                writeU8: function (value, offset, input) {
                    asDataView(value).setUint8(offset >>> 0, input >>> 0);
                    return value;
                },
                readI32: function (value, offset, littleEndian) {
                    return asDataView(value).getInt32(offset >>> 0, littleEndian !== false);
                },
                writeI32: function (value, offset, input, littleEndian) {
                    asDataView(value).setInt32(offset >>> 0, input | 0, littleEndian !== false);
                    return value;
                },
                readF32: function (value, offset, littleEndian) {
                    return asDataView(value).getFloat32(offset >>> 0, littleEndian !== false);
                },
                writeF32: function (value, offset, input, littleEndian) {
                    asDataView(value).setFloat32(offset >>> 0, +input, littleEndian !== false);
                    return value;
                },
                readF64: function (value, offset, littleEndian) {
                    return asDataView(value).getFloat64(offset >>> 0, littleEndian !== false);
                },
                writeF64: function (value, offset, input, littleEndian) {
                    asDataView(value).setFloat64(offset >>> 0, +input, littleEndian !== false);
                    return value;
                },
            };

            var proxyExtensions = [];
            var proxyInstances = new Map();
            var nextProxyId = 1;

            function ctorName(ctor) {
                return (ctor && (ctor.__typeName__ || ctor.name)) || 'Object';
            }

            var typeDescriptorCache = Object.create(null);

            function describeWinRTType(typeName) {
                if (!typeName || typeof globalThis.__nsDescribeWinRTType !== 'function') {
                    return null;
                }
                if (Object.prototype.hasOwnProperty.call(typeDescriptorCache, typeName)) {
                    return typeDescriptorCache[typeName];
                }
                try {
                    var raw = globalThis.__nsDescribeWinRTType(typeName);
                    typeDescriptorCache[typeName] = raw ? JSON.parse(raw) : null;
                } catch (_) {
                    typeDescriptorCache[typeName] = null;
                }
                return typeDescriptorCache[typeName];
            }

            function buildFallbackParameterMetadata(fn) {
                var params = [];
                var count = typeof fn === 'function' && Number.isFinite(fn.length) ? fn.length : 0;
                for (var i = 0; i < count; i++) {
                    params.push({ name: 'arg' + i, type: 'Object' });
                }
                return params;
            }

            function normalizeMethodMetadata(name, value, descriptors) {
                for (var i = 0; i < descriptors.length; i++) {
                    var descriptor = descriptors[i];
                    if (!descriptor || !Array.isArray(descriptor.methods)) {
                        continue;
                    }
                    for (var j = 0; j < descriptor.methods.length; j++) {
                        var method = descriptor.methods[j];
                        if (method && method.name === name) {
                            return {
                                name: method.name,
                                returnType: method.returnType || method.return_type || 'Void',
                                parameters: Array.isArray(method.parameters) ? method.parameters : [],
                            };
                        }
                    }
                }

                return {
                    name: name,
                    returnType: name === 'init' ? 'Void' : 'Object',
                    parameters: buildFallbackParameterMetadata(value),
                };
            }

            function normalizePropertyMetadata(name, descriptors) {
                for (var i = 0; i < descriptors.length; i++) {
                    var descriptor = descriptors[i];
                    if (!descriptor || !Array.isArray(descriptor.properties)) {
                        continue;
                    }
                    for (var j = 0; j < descriptor.properties.length; j++) {
                        var property = descriptor.properties[j];
                        if (property && property.name === name) {
                            return {
                                name: property.name,
                                propType: property.propType || property.prop_type || 'Object',
                                readable: property.readable !== false,
                                writable: property.writable !== false,
                            };
                        }
                    }
                }

                return {
                    name: name,
                    propType: 'Object',
                    readable: true,
                    writable: true,
                };
            }

            function collectProxyMethods(overrides, descriptors) {
                var methods = [];
                for (var key in overrides) {
                    if (!Object.prototype.hasOwnProperty.call(overrides, key)) {
                        continue;
                    }
                    if (key === 'interfaces') {
                        continue;
                    }
                    if (typeof overrides[key] === 'function') {
                        methods.push(normalizeMethodMetadata(key, overrides[key], descriptors));
                    }
                }
                methods.sort(function (left, right) {
                    return String(left && left.name || '').localeCompare(String(right && right.name || ''));
                });
                return methods;
            }

            function collectProxyProperties(overrides, descriptors) {
                var props = [];
                for (var key in overrides) {
                    if (!Object.prototype.hasOwnProperty.call(overrides, key)) {
                        continue;
                    }
                    if (key === 'interfaces') {
                        continue;
                    }
                    if (typeof overrides[key] !== 'function') {
                        props.push(normalizePropertyMetadata(key, descriptors));
                    }
                }
                props.sort(function (left, right) {
                    return String(left && left.name || '').localeCompare(String(right && right.name || ''));
                });
                return props;
            }

            function safeIdentifier(name) {
                return String(name || '')
                    .replace(/[^A-Za-z0-9_]/g, '_')
                    .replace(/^([^A-Za-z_])/, '_$1') || 'ProxyType';
            }

            function autoProxyTypeName(baseCtor) {
                var baseType = ctorName(baseCtor) || 'Object';
                var baseShort = safeIdentifier(baseType.split('.').pop());
                var baseNamespace = 'windows';
                var namespaceIndex = baseType.lastIndexOf('.');
                if (namespaceIndex >= 0) {
                    baseNamespace = baseType
                        .slice(0, namespaceIndex)
                        .toLowerCase()
                        .replace(/[^a-z0-9_.]/g, '_');
                }
                return 'com.tns.gen.winrt.' + baseNamespace + '.' + baseShort + '_AutoProxy_' + (proxyExtensions.length + 1);
            }

            function renderProxyCSharp(meta) {
                var typeName = meta.typeName || ('GeneratedProxy' + (proxyExtensions.length + 1));
                var safeTypeName = safeIdentifier(typeName.split('.').pop());
                var baseType = meta.baseType || 'object';
                var methodStubs = '';
                for (var i = 0; i < meta.methods.length; i++) {
                    var methodMeta = meta.methods[i];
                    var methodName = safeIdentifier((methodMeta && methodMeta.name) || methodMeta);
                    methodStubs +=
                        '    public object __ns_' + methodName + '(params object[] args)\\n' +
                        '    {\\n' +
                        '        return ProxyDispatcher.Invoke(this.__proxyId, "' + methodName + '", args);\\n' +
                        '    }\\n\\n';
                }

                return (
                    'using System;\\n\\n' +
                    'namespace NativeScriptGeneratedProxies\\n' +
                    '{\\n' +
                    '    public static class ProxyDispatcher\\n' +
                    '    {\\n' +
                    '        public static Func<int, string, object[], object> JsInvoke;\\n' +
                    '        public static object Invoke(int id, string method, object[] args)\\n' +
                    '        {\\n' +
                    '            var cb = JsInvoke;\\n' +
                    '            if (cb == null) throw new InvalidOperationException("JsInvoke callback is not registered.");\\n' +
                    '            return cb(id, method, args);\\n' +
                    '        }\\n' +
                    '    }\\n\\n' +
                    '    public class ' + safeTypeName + ' : ' + baseType + '\\n' +
                    '    {\\n' +
                    '        private readonly int __proxyId;\\n\\n' +
                    '        public ' + safeTypeName + '(int proxyId)\\n' +
                    '        {\\n' +
                    '            this.__proxyId = proxyId;\\n' +
                    '        }\\n\\n' +
                    methodStubs +
                    '    }\\n' +
                    '}\\n'
                );
            }

            function renderProxyCsproj(meta) {
                var asmName = safeIdentifier((meta.typeName || 'GeneratedProxy').split('.').pop());
                return (
                    '<Project Sdk="Microsoft.NET.Sdk">\\n' +
                    '  <PropertyGroup>\\n' +
                    '    <TargetFramework>net8.0-windows10.0.19041.0</TargetFramework>\\n' +
                    '    <AssemblyName>' + asmName + '</AssemblyName>\\n' +
                    '    <RootNamespace>NativeScriptGeneratedProxies</RootNamespace>\\n' +
                    '    <ImplicitUsings>enable</ImplicitUsings>\\n' +
                    '    <Nullable>disable</Nullable>\\n' +
                    '    <LangVersion>latest</LangVersion>\\n' +
                    '  </PropertyGroup>\\n' +
                    '</Project>\\n'
                );
            }

            function buildProxyMetadata(baseCtor, typeName, overrides, Extended) {
                var baseType = ctorName(baseCtor);
                var interfaceNames = Array.isArray(overrides.interfaces)
                    ? overrides.interfaces.map(function (iface) { return ctorName(iface); })
                    : [];
                var descriptors = [describeWinRTType(baseType)];
                for (var i = 0; i < interfaceNames.length; i++) {
                    descriptors.push(describeWinRTType(interfaceNames[i]));
                }
                var namespace = '';
                var className = typeName || '';
                if (typeName) {
                    var splitIndex = typeName.lastIndexOf('.');
                    if (splitIndex >= 0) {
                        namespace = typeName.slice(0, splitIndex);
                        className = typeName.slice(splitIndex + 1);
                    }
                }
                var meta = {
                    kind: 'windows-proxy',
                    typeName: typeName || '',
                    className: className || safeIdentifier((typeName || baseType || 'GeneratedProxy').split('.').pop()),
                    namespace: namespace || null,
                    baseType: baseType,
                    baseClass: baseType,
                    interfaces: interfaceNames,
                    methods: collectProxyMethods(overrides, descriptors.filter(Boolean)),
                    properties: collectProxyProperties(overrides, descriptors.filter(Boolean)),
                    isAutoGeneratedName: !typeName,
                    registeredAt: new Date().toISOString(),
                    registered: false,
                    generated: null,
                };
                try {
                    Object.defineProperty(Extended, '__proxyMetadata__', {
                        value: meta,
                        writable: true,
                        configurable: true,
                        enumerable: false,
                    });
                } catch (_) {
                    Extended.__proxyMetadata__ = meta;
                }
                proxyExtensions.push(meta);
                if (typeof globalThis.__nsProxyAutoCapture === 'function') {
                    try {
                        globalThis.__nsProxyAutoCapture(JSON.stringify(proxyExtensions));
                    } catch (_) {
                        // Capture is best-effort; runtime behavior should remain unaffected.
                    }
                }
                return meta;
            }

            function ensureProxyInstance(instance, overrides, ctor) {
                if (!instance || typeof instance !== 'object') {
                    return -1;
                }

                var proxyId = instance.__proxyId;
                if (typeof proxyId !== 'number' || !Number.isFinite(proxyId)) {
                    proxyId = nextProxyId++;
                    try {
                        Object.defineProperty(instance, '__proxyId', {
                            value: proxyId,
                            writable: false,
                            configurable: true,
                            enumerable: false,
                        });
                    } catch (_) {
                        instance.__proxyId = proxyId;
                    }
                }

                proxyInstances.set(proxyId, {
                    instance: instance,
                    overrides: overrides,
                    constructor: ctor,
                });

                return proxyId;
            }

            function makeExtendedConstructor(baseCtor, nameOrOverrides, maybeOverrides) {
                var hasName = typeof nameOrOverrides === 'string';
                var explicitTypeName = hasName ? nameOrOverrides : '';
                var typeName = explicitTypeName || autoProxyTypeName(baseCtor);
                var overrides = hasName ? maybeOverrides : nameOrOverrides;
                if (!overrides || typeof overrides !== 'object') {
                    overrides = {};
                }

                function Extended() {
                    var instance;
                    var args = Array.prototype.slice.call(arguments);
                    try {
                        instance = Reflect.construct(baseCtor, args);
                    } catch (_) {
                        instance = {};
                    }

                    for (var key in overrides) {
                        if (key === 'interfaces') {
                            continue;
                        }
                        var value = overrides[key];
                        if (typeof value === 'function') {
                            try {
                                Object.defineProperty(instance, key, {
                                    value: value,
                                    writable: true,
                                    configurable: true,
                                    enumerable: true,
                                });
                            } catch (_) {
                                instance[key] = value;
                            }
                        } else {
                            instance[key] = value;
                        }
                    }

                    // NativeScript-style behavior: call init automatically on construction
                    // if provided by the extension object.
                    if (typeof overrides.init === 'function') {
                        try {
                            var initResult = overrides.init.apply(instance, args);
                            if (initResult && typeof initResult === 'object') {
                                instance = initResult;
                            }
                        } catch (_) {
                            // Keep constructor resilient; init errors should not crash runtime.
                        }
                    }

                    if (Array.isArray(overrides.interfaces)) {
                        try {
                            Object.defineProperty(instance, '__interfaces__', {
                                value: overrides.interfaces.slice(),
                                writable: false,
                                configurable: true,
                                enumerable: false,
                            });
                        } catch (_) {
                            instance.__interfaces__ = overrides.interfaces.slice();
                        }
                    }

                    ensureProxyInstance(instance, overrides, Extended);

                    return instance;
                }

                Extended.prototype = Object.create((baseCtor && baseCtor.prototype) || Object.prototype);
                Extended.prototype.constructor = Extended;

                for (var protoKey in overrides) {
                    if (protoKey === 'interfaces') {
                        continue;
                    }
                    Extended.prototype[protoKey] = overrides[protoKey];
                }

                if (typeName) {
                    try {
                        Object.defineProperty(Extended, 'name', {
                            value: typeName,
                            configurable: true,
                        });
                    } catch (_) {
                        // Non-critical metadata assignment.
                    }
                }

                Extended.__typeName__ = typeName || ctorName(baseCtor);
                var metadata = buildProxyMetadata(baseCtor, typeName, overrides, Extended);

                Extended.extend = function (nextNameOrOverrides, nextMaybeOverrides) {
                    return makeExtendedConstructor(Extended, nextNameOrOverrides, nextMaybeOverrides);
                };

                try {
                    Object.defineProperty(Extended, 'emitProxy', {
                        value: function (outDir) {
                            return NSWinRT.proxy.emit(metadata, outDir);
                        },
                        writable: true,
                        configurable: true,
                        enumerable: false,
                    });
                } catch (_) {
                    Extended.emitProxy = function (outDir) {
                        return NSWinRT.proxy.emit(metadata, outDir);
                    };
                }

                return Extended;
            }

            if (typeof Function.prototype.extend !== 'function') {
                Object.defineProperty(Function.prototype, 'extend', {
                    value: function (nameOrOverrides, maybeOverrides) {
                        return makeExtendedConstructor(this, nameOrOverrides, maybeOverrides);
                    },
                    writable: true,
                    configurable: true,
                    enumerable: false,
                });
            }

            if (typeof Object.extend !== 'function') {
                Object.defineProperty(Object, 'extend', {
                    value: function (nameOrOverrides, maybeOverrides) {
                        return makeExtendedConstructor(Object, nameOrOverrides, maybeOverrides);
                    },
                    writable: true,
                    configurable: true,
                    enumerable: false,
                });
            }

            function defaultProxyOutDir(meta) {
                var typeName = (meta && meta.typeName) ? meta.typeName : 'GeneratedProxy';
                var safe = safeIdentifier(typeName.split('.').pop());
                return './generated-proxies/' + safe;
            }

            function emitProxy(meta, outDir) {
                if (!meta || typeof meta !== 'object') {
                    throw new Error('NSWinRT.proxy.emit(meta[, outDir]) expects a proxy metadata object');
                }
                if (typeof globalThis.__nsProxyWriteTextFile !== 'function') {
                    throw new Error('Host proxy file emitter is not available');
                }

                var dir = outDir || defaultProxyOutDir(meta);
                var csprojPath = dir + '/Proxy.csproj';
                var csPath = dir + '/Proxy.g.cs';
                var csproj = renderProxyCsproj(meta);
                var source = renderProxyCSharp(meta);

                globalThis.__nsProxyWriteTextFile(csprojPath, csproj);
                globalThis.__nsProxyWriteTextFile(csPath, source);

                meta.generated = {
                    dir: dir,
                    csprojPath: csprojPath,
                    csPath: csPath,
                };

                return meta.generated;
            }

            function compileProxy(meta, outDir, configuration) {
                if (typeof globalThis.__nsProxyCompileProject !== 'function') {
                    throw new Error('Host proxy compiler is not available');
                }
                var generated = emitProxy(meta, outDir);
                var result = globalThis.__nsProxyCompileProject(generated.csprojPath, configuration || 'Debug');
                generated.build = result;
                return generated;
            }

            function registerProxy(meta, outDir, configuration) {
                var generated = compileProxy(meta, outDir, configuration);
                var manifest = {
                    kind: 'windows-proxy',
                    typeName: meta.typeName,
                    baseType: meta.baseType,
                    interfaces: meta.interfaces,
                    methods: meta.methods,
                    properties: meta.properties,
                    generated: generated,
                    registration: {
                        hostCanLoadAssemblies: false,
                        note: 'Assembly build succeeded, but runtime CLR proxy activation is not wired yet. Dynamic JS fallback remains active.',
                    },
                };
                if (typeof globalThis.__nsProxyRegisterManifest === 'function') {
                    globalThis.__nsProxyRegisterManifest(JSON.stringify(manifest));
                }
                meta.registered = true;
                meta.registration = manifest.registration;
                return manifest;
            }

            function invokeProxyById(proxyId, methodName, argsArray) {
                var entry = proxyInstances.get(proxyId);
                if (!entry) {
                    throw new Error('Proxy instance not found for id ' + proxyId);
                }

                var target = entry.instance;
                var method = target && target[methodName];
                if (typeof method !== 'function') {
                    throw new Error('Proxy method "' + methodName + '" is not defined on proxy id ' + proxyId);
                }

                return method.apply(target, Array.isArray(argsArray) ? argsArray : []);
            }

            globalThis.__nsInvokeProxyJs = invokeProxyById;

            globalThis.NSWinRT.proxy = {
                getExtensions: function () {
                    return proxyExtensions.slice();
                },
                emit: emitProxy,
                compile: compileProxy,
                register: registerProxy,
                invokeById: invokeProxyById,
                listRegisteredManifests: function () {
                    if (typeof globalThis.__nsProxyListManifests === 'function') {
                        return globalThis.__nsProxyListManifests();
                    }
                    return [];
                },
            };

            function asDelegate(handler) {
                if (typeof handler === 'function') {
                    return handler;
                }
                if (handler && typeof handler.invoke === 'function') {
                    return handler.invoke.bind(handler);
                }
                throw new Error('NSWinRT.asDelegate(handler) expects a function or { invoke() } object');
            }

            function createEventEmitter() {
                var listeners = [];
                return {
                    add: function (handler) {
                        var normalized = asDelegate(handler);
                        listeners.push(normalized);
                        return {
                            dispose: function () {
                                var idx = listeners.indexOf(normalized);
                                if (idx >= 0) {
                                    listeners.splice(idx, 1);
                                }
                            },
                        };
                    },
                    emit: function () {
                        var args = Array.prototype.slice.call(arguments);
                        listeners.slice().forEach(function (listener) {
                            listener.apply(undefined, args);
                        });
                    },
                    count: function () {
                        return listeners.length;
                    },
                };
            }

            globalThis.NSWinRT.asDelegate = asDelegate;
            globalThis.NSWinRT.createEventEmitter = createEventEmitter;

            var moduleCache = new Map();

            function esmExportAliasAssignments(list) {
                if (!list || !list.trim()) {
                    return '';
                }

                var pairs = [];
                list.split(',').forEach(function (part) {
                    var token = part.trim();
                    if (!token) {
                        return;
                    }
                    var pieces = token.split(/\s+as\s+/i);
                    if (pieces.length === 2) {
                        pairs.push('exports.' + pieces[1].trim() + ' = ' + pieces[0].trim() + ';');
                    } else {
                        pairs.push('exports.' + token + ' = ' + token + ';');
                    }
                });

                return pairs.join('\n');
            }

            function transformEsmToRuntimeModule(source) {
                var transformed = String(source || '');

                transformed = transformed.replace(/^[ \t]*import\s+\*\s+as\s+([A-Za-z_$][\w$]*)\s+from\s+['\"]([^'\"]+)['\"];?[ \t]*$/gm,
                    'const $1 = __nsImport("$2", __filename);');
                transformed = transformed.replace(/^[ \t]*import\s+\{\s*([^}]+)\s*\}\s+from\s+['\"]([^'\"]+)['\"];?[ \t]*$/gm,
                    'const { $1 } = __nsImport("$2", __filename);');
                transformed = transformed.replace(/^[ \t]*import\s+([A-Za-z_$][\w$]*)\s+from\s+['\"]([^'\"]+)['\"];?[ \t]*$/gm,
                    'const $1 = (function(m){ return (m && Object.prototype.hasOwnProperty.call(m, "default")) ? m.default : m; })(__nsImport("$2", __filename));');
                transformed = transformed.replace(/^[ \t]*import\s+['\"]([^'\"]+)['\"];?[ \t]*$/gm,
                    '__nsImport("$1", __filename);');
                transformed = transformed.replace(/\bimport\s*\(/g, '__nsDynamicImport(');

                transformed = transformed.replace(/\bexport\s+default\s+/g, 'exports.default = ');
                transformed = transformed.replace(/^[ \t]*export\s+\{\s*([^}]+)\s*\};?[ \t]*$/gm, function (_, list) {
                    return esmExportAliasAssignments(list);
                });

                var exportedNames = [];
                transformed = transformed.replace(/^[ \t]*export\s+(const|let|var)\s+([A-Za-z_$][\w$]*)/gm, function (_, keyword, name) {
                    exportedNames.push(name);
                    return keyword + ' ' + name;
                });
                transformed = transformed.replace(/^[ \t]*export\s+function\s+([A-Za-z_$][\w$]*)/gm, function (_, name) {
                    exportedNames.push(name);
                    return 'function ' + name;
                });
                transformed = transformed.replace(/^[ \t]*export\s+class\s+([A-Za-z_$][\w$]*)/gm, function (_, name) {
                    exportedNames.push(name);
                    return 'class ' + name;
                });

                if (exportedNames.length > 0) {
                    transformed += '\n' + exportedNames.map(function (name) {
                        return 'exports.' + name + ' = ' + name + ';';
                    }).join('\n') + '\n';
                }

                return transformed;
            }

            function executeRuntimeModule(source, filename) {
                var modulePath = String(filename || '');
                if (!modulePath) {
                    throw new Error('executeRuntimeModule requires a module path');
                }
                if (moduleCache.has(modulePath)) {
                    return moduleCache.get(modulePath);
                }

                var module = { exports: {} };
                moduleCache.set(modulePath, module.exports);

                var transformed = transformEsmToRuntimeModule(source);
                var dirname = modulePath.replace(/[\\/][^\\/]*$/, '');
                var executor = new Function('exports', 'module', '__nsImport', '__nsDynamicImport', '__filename', '__dirname', transformed);
                executor(module.exports, module, __nsImport, __nsDynamicImport, modulePath, dirname);
                moduleCache.set(modulePath, module.exports);
                return module.exports;
            }

            function __nsImport(specifier, parentPath) {
                if (typeof globalThis.__nsResolveModulePath !== 'function') {
                    throw new Error('Module resolver host function is not available');
                }
                if (typeof globalThis.__nsReadTextFile !== 'function') {
                    throw new Error('Module file reader host function is not available');
                }

                var resolved = globalThis.__nsResolveModulePath(
                    String(specifier || ''),
                    parentPath ? String(parentPath) : '',
                    globalThis.__nsAppRoot || ''
                );
                if (!resolved) {
                    throw new Error('Unable to resolve module: ' + specifier);
                }
                if (moduleCache.has(resolved)) {
                    return moduleCache.get(resolved);
                }

                var source = globalThis.__nsReadTextFile(resolved);
                return executeRuntimeModule(source, resolved);
            }

            function __nsDynamicImport(specifier, parentPath) {
                return Promise.resolve().then(function () {
                    return __nsImport(specifier, parentPath);
                });
            }

            globalThis.__nsInvalidateModuleCacheEntry = function (resolvedPath) {
                moduleCache.delete(String(resolvedPath || ''));
            };

            globalThis.__nsClearModuleCache = function () {
                moduleCache.clear();
            };

            globalThis.__nsEvalAsModule = function (source, filename) {
                return executeRuntimeModule(source, filename);
            };
            globalThis.NSWinRT.import = __nsImport;
            globalThis.__nsDynamicImport = __nsDynamicImport;
            globalThis.NSWinRT.dynamicImport = __nsDynamicImport;
        })();
        "#;

        let Some(source) = v8::String::new(scope, helper_source) else { return };
        if let Some(script) = v8::Script::compile(scope, source, None) {
                script.run(scope);
        }

        message_port::install_message_port_runtime(scope);
        worker_support::install_worker_runtime(scope);
        hmr_support::install_hmr_support(scope);
        livesync::install_livesync_support(scope);
        // Attempt to attach the DevTools server (no-op if built without `devtools`).
        global_fns::maybe_attach_devtools(scope);
}

fn create_ns_object<'a>(name: &str, declaration: Arc<RwLock<dyn Declaration>>, scope: &mut v8::PinScope<'a, '_>) -> Local<'a, v8::Value> {

    let Some(name_str) = v8::String::new(scope, name) else { return v8::undefined(scope).into(); };
    let tmpl = FunctionTemplate::new(scope, handle_ns_func);
    tmpl.set_class_name(name_str);
    let object_tmpl = tmpl.instance_template(scope);
    object_tmpl.set_named_property_handler(
        v8::NamedPropertyHandlerConfiguration::new()
            .query(handle_named_property_query)
            .getter(handle_named_property_getter)
            .setter(handle_named_property_setter)
    );
    object_tmpl.set_internal_field_count(2);

    let Some(object) = object_tmpl.new_instance(scope) else { return v8::undefined(scope).into(); };
    let declaration = Box::new(DeclarationFFI::new(declaration));
    let ext = v8::External::new(scope, Box::into_raw(declaration) as _);
    object.set_internal_field(0, ext.into());

    let object_store = v8::Map::new(scope);
    object.set_internal_field(1, object_store.into());

    let ret = object;

    ret.into()
}

/// Properties exposed on the returned JS object:
///   .data1   – UInt32
///   .data2   – UInt16
///   .data3   – UInt16
///   .data4   – Array<number> (8 bytes)
///   .toString() / .valueOf() – "{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}"
unsafe fn guid_ptr_to_js_object<'a>(
    ptr: *mut std::ffi::c_void,
    scope: &mut v8::PinScope<'a, '_>,
) -> v8::Local<'a, v8::Object> {
    use windows::core::GUID;
    let g = &*(ptr as *const GUID);

    let guid_str = format!(
        "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        g.data1, g.data2, g.data3,
        g.data4[0], g.data4[1],
        g.data4[2], g.data4[3], g.data4[4], g.data4[5], g.data4[6], g.data4[7]
    );

    let obj = v8::Object::new(scope);

    // Scalar fields
    let key_data1 = v8::String::new(scope, "data1").unwrap();
    let val_data1 = v8::Integer::new_from_unsigned(scope, g.data1);
    obj.set(scope, key_data1.into(), val_data1.into());

    let key_data2 = v8::String::new(scope, "data2").unwrap();
    let val_data2 = v8::Integer::new_from_unsigned(scope, g.data2 as u32);
    obj.set(scope, key_data2.into(), val_data2.into());

    let key_data3 = v8::String::new(scope, "data3").unwrap();
    let val_data3 = v8::Integer::new_from_unsigned(scope, g.data3 as u32);
    obj.set(scope, key_data3.into(), val_data3.into());

    // data4 as a JS Array of 8 numbers
    let arr = v8::Array::new(scope, 8);
    for (i, &byte) in g.data4.iter().enumerate() {
        let byte_val = v8::Integer::new_from_unsigned(scope, byte as u32);
        arr.set_index(scope, i as u32, byte_val.into());
    }
    let key_data4 = v8::String::new(scope, "data4").unwrap();
    obj.set(scope, key_data4.into(), arr.into());

    // toString() / valueOf() both return the GUID string
    let guid_v8 = v8::String::new(scope, &guid_str).unwrap();
    let to_string_fn = v8::FunctionTemplate::builder(
        |_scope: &mut v8::PinScope<'_, '_>,
         args: v8::FunctionCallbackArguments,
         mut retval: v8::ReturnValue| {
            let s = unsafe { args.data().cast::<v8::String>() };
            retval.set(s.into());
        },
    )
    .data(guid_v8.into())
    .build(scope)
    .get_function(scope)
    .unwrap();

    let key_to_string = v8::String::new(scope, "toString").unwrap();
    obj.set(scope, key_to_string.into(), to_string_fn.into());

    let key_value_of = v8::String::new(scope, "valueOf").unwrap();
    obj.set(scope, key_value_of.into(), to_string_fn.into());

    obj
}

/// Captured data for calling a method on a `GenericInterfaceInstance` via the getter interceptor.
struct IfaceMethodCallData {
    method: MethodDeclaration,
    instance: IUnknown,
    iid: GUID,
    type_args: Vec<String>,
}

/// Extract the comma-separated type arguments from a closed generic type name.
/// E.g. `IFoo`2<Windows.X.Bar, Windows.X.Baz>` → `["Windows.X.Bar", "Windows.X.Baz"]`
fn extract_generic_type_args(full_name: &str) -> Vec<String> {
    let Some(start) = full_name.find('<') else { return Vec::new(); };
    let end = full_name.rfind('>').unwrap_or(full_name.len());
    let inner = &full_name[start + 1..end];
    let mut args = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();
    for ch in inner.chars() {
        match ch {
            '<' => { depth += 1; current.push(ch); }
            '>' => { depth -= 1; current.push(ch); }
            ',' if depth == 0 => {
                let trimmed = current.trim().to_owned();
                if !trimmed.is_empty() { args.push(trimmed); }
                current = String::new();
            }
            _ => { current.push(ch); }
        }
    }
    let trimmed = current.trim().to_owned();
    if !trimmed.is_empty() { args.push(trimmed); }
    args
}

fn create_ns_ctor_instance_object<'a>(name: &str, factory: Option<IUnknown>, parent: Option<Arc<RwLock<dyn Declaration>>>, declaration: Arc<RwLock<dyn Declaration>>, instance: Option<IUnknown>, scope: &mut v8::PinScope<'a, '_>) -> Local<'a, v8::Value> {
    // COM identity key: QI(IID_IUnknown) gives the canonical pointer regardless of which
    // interface we hold.
    let identity_key: Option<usize> = instance.as_ref().and_then(|unk| {
        unk.cast::<IUnknown>().ok().map(|id| id.as_raw() as usize)
    });
    if let Some(key) = identity_key {
        let hit = INSTANCE_CACHE.with(|cache| {
            cache.borrow().get(&key).and_then(|weak| weak.to_local(scope))
        });
        if let Some(local) = hit {
            return local.into();
        }
    }

    let class_name = v8::String::new(scope, name).unwrap();

    let tmpl = FunctionTemplate::new(scope, handle_ns_func);
    let object_tmpl = tmpl.instance_template(scope);

    // Two internal fields: [0] = DeclarationFFI external, [1] = per-instance side store (Map)
    object_tmpl.set_internal_field_count(2);

    let declaration_ffi = Box::into_raw(Box::new(DeclarationFFI::new_with_instance(declaration.clone(), instance.clone())));
    let ext = v8::External::new(scope, declaration_ffi as _);

    object_tmpl.set_named_property_handler(
        v8::NamedPropertyHandlerConfiguration::new()
            .getter(|scope: &mut v8::PinScope<'_, '_>,
                     key: Local<v8::Name>,
                     args: v8::PropertyCallbackArguments,
                     mut rv: v8::ReturnValue<v8::Value>| -> v8::Intercepted {
                if !key.is_string() {
                    return v8::Intercepted::kNo;
                }

                let name = key.to_rust_string_lossy(scope);
                if name == "__probe__" {
                    let value = v8::String::new(scope, "instance-handler-active").unwrap();
                    rv.set(value.into());
                    return v8::Intercepted::kYes;
                }
                // Prefer the DeclarationFFI stored on the instance (holder internal field[0]).
                // If the holder doesn't have the internal field (e.g. the property
                // lives on the prototype), prefer the `this` object's internal
                // DeclarationFFI when available. Fall back to the callback data
                // otherwise.
                let holder = args.holder();
                let dec_field_opt = holder.get_internal_field(scope, 0);
                let dec = if let Some(dec_field) = dec_field_opt {
                    let dec_ext = unsafe { dec_field.cast::<v8::External>() };
                    let dec_ptr = dec_ext.value() as *mut DeclarationFFI;
                    unsafe { &*dec_ptr }
                } else {
                    // If the holder doesn't have the DeclarationFFI, fall back
                    // to the callback data. (Avoid using `args.this()` here to
                    // remain compatible with the v8 bindings available.)
                    let dec_ext = unsafe { args.data().cast::<v8::External>() };
                    let dec_ptr = dec_ext.value() as *mut DeclarationFFI;
                    unsafe { &*dec_ptr }
                };

                let lock = dec.read();

                // Handle GenericInterfaceInstance properties and methods dynamically
                if let Some(iface) = lock.as_any().downcast_ref::<GenericInterfaceInstanceDeclaration>() {
                    let iid = iface.id();
                    let type_args = extract_generic_type_args(iface.full_name());

                    // Side-store check for JS-assigned values
                    let this = holder;
                    if let Some(store_field) = this.get_internal_field(scope, 1) {
                        let store = unsafe { store_field.cast::<v8::Map>() };
                        if let Some(cache) = store.get(scope, key.into()) {
                            if !cache.is_null_or_undefined() {
                                rv.set(cache);
                                return v8::Intercepted::kYes;
                            }
                        }
                    }

                    // Property getter (e.g. Completed, Progress)
                    if let Some(property) = iface.properties().iter().find(|p| p.name() == name.as_str()) {
                        let property_clone = property.clone();
                        drop(lock);
                        let Some(ns_instance) = dec.instance.clone() else { return v8::Intercepted::kNo; };
                        let Some(mut property_call) = PropertyCall::new_for_interface(&property_clone, false, ns_instance, false, iid, type_args) else {
                            return v8::Intercepted::kNo;
                        };
                        let (ret, result, _outs) = property_call.call_with_values(scope, &[]);
                        if ret.is_err() {
                            let detail = format!("Property get '{}' failed: {} (0x{:08X})", name, ret.message(), ret.0 as u32);
                            let message = v8::String::new(scope, &detail).unwrap();
                            let error = v8::Exception::error(scope, message);
                            scope.throw_exception(error);
                            return v8::Intercepted::kYes;
                        }
                        if property_call.is_void() {
                            rv.set_undefined();
                            return v8::Intercepted::kYes;
                        }
                        let return_sig = property_call.return_type().to_string();
                        if return_sig.contains('.') {
                            if let Some(declaration) = MetadataReader::find_by_name(return_sig.as_str()) {
                                let ret_val: Local<v8::Value> = if matches!(declaration.read().kind(), DeclarationKind::Struct) {
                                    create_struct_object_from_raw(declaration, result, scope).into()
                                } else if result.is_null() {
                                    v8::null(scope).into()
                                } else {
                                    let instance = unsafe { IUnknown::from_raw(result) };
                                    create_ns_ctor_instance_object(&return_sig, None, None, declaration, Some(instance), scope).into()
                                };
                                rv.set(ret_val);
                                return v8::Intercepted::kYes;
                            }
                        }
                        if let Ok(native_type) = NativeType::try_from(return_sig.as_str()) {
                            unsafe { set_ret_val(result, scope, rv, native_type); }
                            return v8::Intercepted::kYes;
                        }
                        return v8::Intercepted::kNo;
                    }

                    // Method access — return a JS function that calls via QI + vtable
                    if let Some(method_decl) = iface.methods().iter().find(|m| m.name() == name.as_str()) {
                        let method_clone = method_decl.clone();
                        let Some(ns_instance) = dec.instance.clone() else {
                            drop(lock);
                            return v8::Intercepted::kNo;
                        };
                        drop(lock);

                        let call_data = Box::into_raw(Box::new(IfaceMethodCallData {
                            method: method_clone,
                            instance: ns_instance,
                            iid,
                            type_args,
                        }));
                        let ext = v8::External::new(scope, call_data as _);

                        let func = v8::Function::builder(|scope: &mut v8::PinScope<'_, '_>,
                                                          args: v8::FunctionCallbackArguments,
                                                          mut retval: v8::ReturnValue| {
                            let data = unsafe { &*(args.data().cast::<v8::External>().value() as *const IfaceMethodCallData) };
                            let Some(mut method_call) = PropertyCall::new_method_for_interface(
                                &data.method, data.instance.clone(), data.iid, data.type_args.clone(),
                            ) else { return; };

                            let mut arg_vals: Vec<Local<v8::Value>> = Vec::with_capacity(args.length() as usize);
                            for i in 0..args.length() {
                                arg_vals.push(args.get(i));
                            }

                            let (ret, result, _outs) = method_call.call_with_values(scope, &arg_vals);

                            if ret.is_err() {
                                let detail = crate::error::format_hresult_message(ret);
                                let msg = v8::String::new(scope, &detail).unwrap();
                                let err = v8::Exception::error(scope, msg);
                                scope.throw_exception(err);
                                return;
                            }

                            if method_call.is_void() {
                                retval.set_undefined();
                                return;
                            }

                            let return_sig = method_call.return_type().to_string();
                            if return_sig.contains('.') {
                                if let Some(declaration) = MetadataReader::find_by_name(return_sig.as_str()) {
                                    let ret_val: Local<v8::Value> = if matches!(declaration.read().kind(), DeclarationKind::Struct) {
                                        create_struct_object_from_raw(declaration, result, scope).into()
                                    } else if result.is_null() {
                                        v8::null(scope).into()
                                    } else {
                                        let instance = unsafe { IUnknown::from_raw(result) };
                                        create_ns_ctor_instance_object(&return_sig, None, None, declaration, Some(instance), scope).into()
                                    };
                                    retval.set(ret_val);
                                    return;
                                }
                            }
                            if let Ok(native_type) = NativeType::try_from(return_sig.as_str()) {
                                unsafe { set_ret_val(result, scope, retval, native_type); }
                            }
                        })
                        .data(ext.into())
                        .build(scope)
                        .unwrap();

                        let func: Local<v8::Value> = func.into();
                        if let Some(store_field) = holder.get_internal_field(scope, 1) {
                            let store = unsafe { store_field.cast::<v8::Map>() };
                            store.set(scope, key.into(), func);
                        }
                        rv.set(func);
                        return v8::Intercepted::kYes;
                    }

                    return v8::Intercepted::kNo;
                }

                // Handle plain (non-generic) interface instances returned from properties/methods.
                // e.g. IHttpContent returned by HttpResponseMessage.Content.
                if let Some(iface) = lock.as_any().downcast_ref::<InterfaceDeclaration>() {
                    let iid = iface.id();
                    let type_args: Vec<String> = vec![];

                    let this = holder;
                    if let Some(store_field) = this.get_internal_field(scope, 1) {
                        let store = unsafe { store_field.cast::<v8::Map>() };
                        if let Some(cache) = store.get(scope, key.into()) {
                            if !cache.is_null_or_undefined() {
                                rv.set(cache);
                                return v8::Intercepted::kYes;
                            }
                        }
                    }

                    if let Some(property) = iface.properties().iter().find(|p| p.name() == name.as_str()) {
                        let property_clone = property.clone();
                        drop(lock);
                        let Some(ns_instance) = dec.instance.clone() else { return v8::Intercepted::kNo; };
                        let Some(mut property_call) = PropertyCall::new_for_interface(&property_clone, false, ns_instance, false, iid, type_args) else {
                            return v8::Intercepted::kNo;
                        };
                        let (ret, result, _outs) = property_call.call_with_values(scope, &[]);
                        if ret.is_err() {
                            let detail = format!("Property get '{}' failed: {} (0x{:08X})", name, ret.message(), ret.0 as u32);
                            let message = v8::String::new(scope, &detail).unwrap();
                            let error = v8::Exception::error(scope, message);
                            scope.throw_exception(error);
                            return v8::Intercepted::kYes;
                        }
                        if property_call.is_void() {
                            rv.set_undefined();
                            return v8::Intercepted::kYes;
                        }
                        let return_sig = property_call.return_type().to_string();
                        if return_sig.contains('.') {
                            if let Some(declaration) = MetadataReader::find_by_name(return_sig.as_str()) {
                                let ret_val: Local<v8::Value> = if matches!(declaration.read().kind(), DeclarationKind::Struct) {
                                    create_struct_object_from_raw(declaration, result, scope).into()
                                } else if result.is_null() {
                                    v8::null(scope).into()
                                } else {
                                    let instance = unsafe { IUnknown::from_raw(result) };
                                    create_ns_ctor_instance_object(&return_sig, None, None, declaration, Some(instance), scope).into()
                                };
                                rv.set(ret_val);
                                return v8::Intercepted::kYes;
                            }
                        }
                        if let Ok(native_type) = NativeType::try_from(return_sig.as_str()) {
                            unsafe { set_ret_val(result, scope, rv, native_type); }
                            return v8::Intercepted::kYes;
                        }
                        return v8::Intercepted::kNo;
                    }

                    if let Some(method_decl) = iface.methods().iter().find(|m| m.name() == name.as_str()) {
                        let method_clone = method_decl.clone();
                        let Some(ns_instance) = dec.instance.clone() else {
                            drop(lock);
                            return v8::Intercepted::kNo;
                        };
                        drop(lock);

                        let call_data = Box::into_raw(Box::new(IfaceMethodCallData {
                            method: method_clone,
                            instance: ns_instance,
                            iid,
                            type_args,
                        }));
                        let ext = v8::External::new(scope, call_data as _);

                        let func = v8::Function::builder(|scope: &mut v8::PinScope<'_, '_>,
                                                          args: v8::FunctionCallbackArguments,
                                                          mut retval: v8::ReturnValue| {
                            let data = unsafe { &*(args.data().cast::<v8::External>().value() as *const IfaceMethodCallData) };
                            let Some(mut method_call) = PropertyCall::new_method_for_interface(
                                &data.method, data.instance.clone(), data.iid, data.type_args.clone(),
                            ) else { return; };

                            let mut arg_vals: Vec<Local<v8::Value>> = Vec::with_capacity(args.length() as usize);
                            for i in 0..args.length() {
                                arg_vals.push(args.get(i));
                            }

                            let (ret, result, _outs) = method_call.call_with_values(scope, &arg_vals);

                            if ret.is_err() {
                                let detail = crate::error::format_hresult_message(ret);
                                let msg = v8::String::new(scope, &detail).unwrap();
                                let err = v8::Exception::error(scope, msg);
                                scope.throw_exception(err);
                                return;
                            }

                            if method_call.is_void() {
                                retval.set_undefined();
                                return;
                            }

                            let return_sig = method_call.return_type().to_string();
                            if return_sig.contains('.') {
                                if let Some(declaration) = MetadataReader::find_by_name(return_sig.as_str()) {
                                    let ret_val: Local<v8::Value> = if matches!(declaration.read().kind(), DeclarationKind::Struct) {
                                        create_struct_object_from_raw(declaration, result, scope).into()
                                    } else if result.is_null() {
                                        v8::null(scope).into()
                                    } else {
                                        let instance = unsafe { IUnknown::from_raw(result) };
                                        create_ns_ctor_instance_object(&return_sig, None, None, declaration, Some(instance), scope).into()
                                    };
                                    retval.set(ret_val);
                                    return;
                                }
                            }
                            if let Ok(native_type) = NativeType::try_from(return_sig.as_str()) {
                                unsafe { set_ret_val(result, scope, retval, native_type); }
                            }
                        })
                        .data(ext.into())
                        .build(scope)
                        .unwrap();

                        let func: Local<v8::Value> = func.into();
                        if let Some(store_field) = holder.get_internal_field(scope, 1) {
                            let store = unsafe { store_field.cast::<v8::Map>() };
                            store.set(scope, key.into(), func);
                        }
                        rv.set(func);
                        return v8::Intercepted::kYes;
                    }

                    return v8::Intercepted::kNo;
                }

                let Some(clazz) = lock.as_any().downcast_ref::<ClassDeclaration>() else {
                    return v8::Intercepted::kNo;
                };

                // If a JS-assigned override exists in the per-instance store, return it.
                // Use the holder (where the property was found) to access the side-store map.
                let this = holder;
                let store_field_opt = this.get_internal_field(scope, 1);
                if let Some(store_field) = store_field_opt {
                    let store = unsafe { store_field.cast::<v8::Map>() };
                    if let Some(cache) = store.get(scope, key.into()) {
                        if !cache.is_null_or_undefined() {
                            rv.set(cache);
                            return v8::Intercepted::kYes;
                        }
                    }
                }

                if let Some(property) = find_class_property(clazz, &name) {
                    let Some(ns_instance) = dec.instance.clone() else { return v8::Intercepted::kNo; };
                    let Some(mut property_call) = PropertyCall::new(&property, false, ns_instance, false) else {
                        return v8::Intercepted::kNo;
                    };
                    let (ret, result, _outs) = property_call.call_with_values(scope, &[]);

                    if ret.is_err() {
                        let detail = format!("Property get '{}' failed: {} (0x{:08X})", name, ret.message(), ret.0 as u32);
                        let message = v8::String::new(scope, &detail).unwrap();
                        let error = v8::Exception::error(scope, message);
                        scope.throw_exception(error);
                        return v8::Intercepted::kYes;
                    }

                    if property_call.is_void() {
                        rv.set_undefined();
                        return v8::Intercepted::kYes;
                    }

                    let return_sig = property_call.return_type().to_string();
                    if return_sig.contains('.') {
                        if let Some(declaration) = MetadataReader::find_by_name(return_sig.as_str()) {
                            let ret: Local<v8::Value> = if matches!(declaration.read().kind(), DeclarationKind::Struct) {
                                create_struct_object_from_raw(declaration, result, scope).into()
                            } else if result.is_null() {
                                v8::null(scope).into()
                            } else {
                                let instance = unsafe { IUnknown::from_raw(result) };
                                create_ns_ctor_instance_object(return_sig.as_str(), None, None, declaration, Some(instance), scope).into()
                            };
                            rv.set(ret);
                            return v8::Intercepted::kYes;
                        }
                    }

                    if let Ok(return_type) = NativeType::try_from(return_sig.as_str()) {
                        unsafe { set_ret_val(result, scope, rv, return_type); }
                        return v8::Intercepted::kYes;
                    }

                    return v8::Intercepted::kNo;
                }

                if let Some(method) = find_class_method(clazz, &name) {
                    let declaration = Arc::new(RwLock::new(method.clone()));
                    let declaration = Box::into_raw(Box::new(DeclarationFFI::new_with_instance(declaration, dec.instance.clone())));
                    let ext = v8::External::new(scope, declaration as _);

                    let builder = v8::Function::builder(|scope: &mut v8::PinScope<'_, '_>,
                                                         args: v8::FunctionCallbackArguments,
                                                         mut retval: v8::ReturnValue| {
                        let dec = unsafe { args.data().cast::<v8::External>() };
                        let dec = dec.value() as *mut DeclarationFFI;
                        let dec = unsafe { &*dec };
                        let lock = dec.read();
                        let Some(method) = lock.as_any().downcast_ref::<MethodDeclaration>() else { return; };
                        let Some(ns_instance) = dec.instance.clone() else { return; };
                        let mut method = MethodCall::new(method, method.is_sealed(), ns_instance, false);
                        let (ret, result, outs) = method.call(scope, &args);

                        if ret.is_err() {
                            let detail = crate::error::format_hresult_message(ret);
                            let message = v8::String::new(scope, &detail).unwrap();
                            let error = v8::Exception::error(scope, message);
                            scope.throw_exception(error);
                            return;
                        }

                        // If there are out-parameters, return an array containing the
                        // primary return (if present) followed by the out-values.
                        if !outs.is_empty() {
                            let mut arr_len = outs.len();
                            if !method.is_void() { arr_len += 1; }
                            let arr = v8::Array::new(scope, arr_len as i32);
                            let mut idx = 0u32;

                            if !method.is_void() {
                                let return_sig = method.return_type().to_string();
                                let mut return_value_opt: Option<Local<v8::Value>> = None;
                                if return_sig.contains('.') {
                                    if let Some(declaration) = MetadataReader::find_by_name(return_sig.as_str()) {
                                        if matches!(declaration.read().kind(), DeclarationKind::Struct) {
                                            let obj = crate::create_struct_object_from_raw(declaration, result, scope).into();
                                            return_value_opt = Some(obj);
                                        } else if !result.is_null() {
                                            let instance = unsafe { IUnknown::from_raw(result) };
                                            let retv: Local<v8::Value> = create_ns_ctor_instance_object(return_sig.as_str(), None, dec.parent.clone(), declaration, Some(instance), scope).into();
                                            return_value_opt = Some(retv);
                                        } else {
                                            return_value_opt = Some(v8::null(scope).into());
                                        }
                                    }
                                }
                                if return_value_opt.is_none() {
                                    if let Ok(return_type) = NativeType::try_from(return_sig.as_str()) {
                                        let v = unsafe { read_value_from_ptr(result as *const c_void, scope, return_type) };
                                        return_value_opt = Some(v);
                                    }
                                }
                                if let Some(rv) = return_value_opt {
                                    arr.set_index(scope, idx, rv);
                                    idx += 1;
                                }
                            }

                            for outv in outs.into_iter() {
                                arr.set_index(scope, idx, outv);
                                idx += 1;
                            }
                            retval.set(arr.into());
                            return;
                        }

                        if method.is_void() {
                            retval.set_undefined();
                            return;
                        }

                        let return_sig = method.return_type().to_string();
                        if return_sig.contains('.') {
                            if let Some(declaration) = MetadataReader::find_by_name(return_sig.as_str()) {
                                let ret: Local<v8::Value> = if matches!(declaration.read().kind(), DeclarationKind::Struct) {
                                    create_struct_object_from_raw(declaration, result, scope).into()
                                } else if result.is_null() {
                                    v8::null(scope).into()
                                } else {
                                    let instance = unsafe { IUnknown::from_raw(result) };
                                    create_ns_ctor_instance_object(return_sig.as_str(), None, dec.parent.clone(), declaration, Some(instance), scope).into()
                                };
                                retval.set(ret);
                                return;
                            }
                        }

                        if let Ok(return_type) = NativeType::try_from(return_sig.as_str()) {
                            unsafe { set_ret_val(result, scope, retval, return_type); }
                        }
                    })
                    .data(ext.into())
                    .build(scope)
                    .unwrap();

                    let func: Local<v8::Value> = builder.into();
                    if let Some(store_field) = holder.get_internal_field(scope, 1) {
                        let store = unsafe { store_field.cast::<v8::Map>() };
                        store.set(scope, key.into(), func);
                    }
                    rv.set(func);
                    return v8::Intercepted::kYes;
                }

                v8::Intercepted::kNo
            })
            .setter(|scope: &mut v8::PinScope<'_, '_>,
                     key: Local<v8::Name>,
                     val: Local<v8::Value>,
                     args: v8::PropertyCallbackArguments,
                     mut _rv: v8::ReturnValue<()>| -> v8::Intercepted {
                if !key.is_string() {
                    return v8::Intercepted::kNo;
                }

                let name = key.to_rust_string_lossy(scope);
                let dec = unsafe { args.data().cast::<v8::External>() };
                let dec = dec.value() as *mut DeclarationFFI;
                let dec = unsafe { &mut *dec };
                let lock = dec.read();

                if let Some(iface) = lock.as_any().downcast_ref::<GenericInterfaceInstanceDeclaration>() {
                    let iid = iface.id();
                    let type_args = extract_generic_type_args(iface.full_name());
                    if let Some(property) = iface.properties().iter().find(|p| p.name() == name.as_str()) {
                        if property.setter().is_some() {
                            let property_clone = property.clone();
                            drop(lock);
                            let Some(ns_instance) = dec.instance.clone() else {
                                return v8::Intercepted::kNo;
                            };
                            let Some(mut property_call) = PropertyCall::new_for_interface(&property_clone, true, ns_instance, false, iid, type_args) else {
                                return v8::Intercepted::kNo;
                            };
                            let (ret, _, _outs) = property_call.call_with_values(scope, &[val]);
                            if ret.is_err() {
                                let detail = format!("Property set '{}' failed: {} (0x{:08X})", name, ret.message(), ret.0 as u32);
                                let message = v8::String::new(scope, &detail).unwrap();
                                let error = v8::Exception::error(scope, message);
                                scope.throw_exception(error);
                            }
                            return v8::Intercepted::kYes;
                        }
                    }
                    return v8::Intercepted::kNo;
                }

                let Some(clazz) = lock.as_any().downcast_ref::<ClassDeclaration>() else {
                    return v8::Intercepted::kNo;
                };

                if let Some(property) = find_class_property(clazz, &name) {
                    if property.setter().is_none() {
                        return v8::Intercepted::kNo;
                    }

                    let Some(ns_instance) = dec.instance.clone() else { return v8::Intercepted::kNo; };
                    let Some(mut property_call) = PropertyCall::new(&property, true, ns_instance, false) else {
                        return v8::Intercepted::kNo;
                    };
                    let (ret, _, _outs) = property_call.call_with_values(scope, &[val]);
                    if ret.is_err() {
                        let detail = format!("Property set '{}' failed: {} (0x{:08X})", name, ret.message(), ret.0 as u32);
                        let message = v8::String::new(scope, &detail).unwrap();
                        let error = v8::Exception::error(scope, message);
                        scope.throw_exception(error);
                    }
                    return v8::Intercepted::kYes;
                }

                // Try WinRT events: `instance.EventName = new DelegateType({ Invoke: fn })`.
                if let Some((add_method, remove_method)) = find_event_methods(clazz, &name) {
                    let instance = dec.instance.clone();
                    drop(lock);

                    if let Some(&old_token) = dec.event_tokens.get(&name) {
                        if let Some(inst) = instance.clone() {
                            let mut mc = MethodCall::new(&remove_method, remove_method.is_sealed(), inst, false);
                            let _ = mc.call_with_event_token(old_token);
                        }
                        dec.event_tokens.remove(&name);
                    }

                        if val.is_object() {
                            if let Some(obj) = val.to_object(scope) {
                                if let Some(handle_key) = v8::String::new(scope, "handle") {
                                    if let Some(handle_val) = obj.get(scope, handle_key.into()) {
                                        if let Ok(ext) = v8::Local::<v8::External>::try_from(handle_val) {
                                            let delegate_ptr = ext.value();
                                            if let Some(inst) = instance {
                                                let mut mc = MethodCall::new(&add_method, add_method.is_sealed(), inst, false);
                                                let (ret, token) = mc.call_with_raw_ptr(delegate_ptr);
                                                if ret.is_ok() {
                                                    dec.event_tokens.insert(name, token);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                    return v8::Intercepted::kYes;
                }

                v8::Intercepted::kNo
            })
            .data(ext.into())
    );

    tmpl.set_class_name(class_name);

    let proto = tmpl.prototype_template(scope);

    {
        let lock = declaration.read();

        let kind = lock.kind();


        match kind {
            DeclarationKind::Class => {
                let Some(clazz) = lock.as_any().downcast_ref::<ClassDeclaration>() else { return v8::undefined(scope).into(); };
                let class_methods = collect_class_methods(clazz);
                let class_properties = collect_class_properties(clazz);
                let mut seen_member_names: AHashSet<String> = AHashSet::new();


                let to_string_func = FunctionTemplate::builder(|_scope: &mut v8::PinScope<'_, '_>,
                                                                args: v8::FunctionCallbackArguments,
                                                                mut retval: v8::ReturnValue| {
                    retval.set(args.data());
                })
                    .data(class_name.into())
                    .build(scope);

                let to_string = v8::String::new(scope, "toString").unwrap();

                object_tmpl.set(to_string.into(), to_string_func.into());

                for method in class_methods.iter() {
                    let method_name = if method.overload_name().is_empty() {
                        method.name().to_string()
                    } else {
                        method.overload_name().to_string()
                    };

                    let is_static = method.is_static();

                    let key = format!("{}:{}", method_name, if is_static { "static" } else { "instance" });
                    if !seen_member_names.insert(key) {
                        continue;
                    }

                    let name = v8::String::new(scope, method_name.as_str());

                    let declaration = DeclarationFFI::new_with_instance(
                        Arc::new(
                            RwLock::new(
                                method.clone()
                            )
                        ),
                        if is_static {
                            factory.clone()
                        } else {
                            instance.clone()
                        },
                    );

                    let declaration = Box::into_raw(Box::new(declaration));


                    let ext = v8::External::new(scope, declaration as _);


                    extern "C" fn callback(callback: *const v8::FunctionCallbackInfo) {
                        let info = unsafe { &*callback };

                        v8::callback_scope!(unsafe scope, info);
                        let args = unsafe { v8::FunctionCallbackArguments::from_function_callback_info(info) };
                        let mut retval = v8::ReturnValue::from_function_callback_info(info);


                        let dec = unsafe { args.data().cast::<v8::External>() };

                        let dec = dec.value() as *mut DeclarationFFI;

                        let dec = unsafe { &*dec };

                        let lock = dec.read();

                        let Some(method) = lock.as_any().downcast_ref::<MethodDeclaration>() else { return; };

                        let _nam = method.name();
                        let Some(ns_instance) = dec.instance.clone() else { return; };
                        let mut method = MethodCall::new(
                            method, method.is_sealed(), ns_instance, false,
                        );

                        let (ret, result, outs) = method.call(scope, &args);

                        if ret.is_err() {
                            let detail = crate::error::format_hresult_message(ret);
                            let msg = v8::String::new(scope, &detail).unwrap();
                            let err = v8::Exception::error(scope, msg.into());
                            scope.throw_exception(err);
                            return;
                        }

                        if !outs.is_empty() {
                            let mut arr_len = outs.len();
                            if !method.is_void() { arr_len += 1; }
                            let arr = v8::Array::new(scope, arr_len as i32);
                            let mut idx = 0u32;

                            if !method.is_void() {
                                let return_sig = method.return_type().to_string();
                                let mut return_value_opt: Option<Local<v8::Value>> = None;
                                if return_sig.contains('.') {
                                    if let Some(declaration) = MetadataReader::find_by_name(return_sig.as_str()) {
                                        if matches!(declaration.read().kind(), DeclarationKind::Struct) {
                                            let obj = crate::create_struct_object_from_raw(declaration, result, scope).into();
                                            return_value_opt = Some(obj);
                                        } else if !result.is_null() {
                                            let instance = unsafe { IUnknown::from_raw(result) };
                                            let retv: Local<v8::Value> = create_ns_ctor_instance_object(return_sig.as_str(), None, dec.parent.clone(), declaration, Some(instance), scope).into();
                                            return_value_opt = Some(retv);
                                        } else {
                                            return_value_opt = Some(v8::null(scope).into());
                                        }
                                    }
                                }
                                if return_value_opt.is_none() {
                                    if let Ok(return_type) = NativeType::try_from(return_sig.as_str()) {
                                        let v = unsafe { read_value_from_ptr(result as *const c_void, scope, return_type) };
                                        return_value_opt = Some(v);
                                    }
                                }
                                if let Some(rv) = return_value_opt {
                                    arr.set_index(scope, idx, rv);
                                    idx += 1;
                                }
                            }

                            for outv in outs.into_iter() {
                                arr.set_index(scope, idx, outv);
                                idx += 1;
                            }
                            retval.set(arr.into());
                            return;
                        }

                        if method.is_void() {
                            retval.set_undefined();
                            return;
                        }

                        let return_sig = method.return_type().to_string();
                        if return_sig == "Guid" {
                            let obj = unsafe { guid_ptr_to_js_object(result, scope) };
                            retval.set(obj.into());
                        } else {
                            match NativeType::try_from(return_sig.as_str()) {
                                Ok(return_type) => {
                                    if return_sig.contains('.') {
                                        if result.is_null() {
                                            retval.set(v8::null(scope).into());
                                            return;
                                        }
                                        let instance = unsafe { IUnknown::from_raw(result) };
                                        let declaration = MetadataReader::find_by_name(return_sig.as_str())
                                            .unwrap_or_else(|| dec.inner.clone());
                                        let ret: Local<v8::Value> = create_ns_ctor_instance_object(return_sig.as_str(), None, dec.parent.clone(), declaration, Some(instance), scope).into();
                                        retval.set(ret.into());
                                        return;
                                    }
                                    unsafe { set_ret_val(result, scope, retval, return_type); }
                                }
                                Err(_) => {}
                            }
                        }

                        // todo
                    }

                    let func = FunctionTemplate::builder_raw(callback)
                        .data(ext.into())
                        .build(scope);

                    if is_static {
                        tmpl.set_with_attr(name.unwrap().into(), func.into(), v8::PropertyAttribute::DONT_DELETE);
                    } else {
                        object_tmpl.set_with_attr(name.unwrap().into(), func.into(), v8::PropertyAttribute::DONT_DELETE);
                    }
                }

                for property in class_properties.iter() {
                    let property_name = property.name().to_string();
                    let is_static = property.is_static();

                    let key = format!("{}:{}", property_name, if is_static { "static" } else { "instance" });
                    if !seen_member_names.insert(key) {
                        continue;
                    }

                    let name = v8::String::new(scope, property_name.as_str());

                    let declaration = DeclarationFFI::new_with_instance(
                        Arc::new(
                            RwLock::new(
                                property.clone()
                            )
                        ),
                        if is_static { factory.clone() } else { instance.clone() },
                    );


                    let getter_declaration = declaration.clone();

                    let getter_declaration = Box::into_raw(Box::new(getter_declaration));

                    let getter_declaration_ext = v8::External::new(scope, getter_declaration as _);


                    let getter = FunctionTemplate::builder(|scope: &mut v8::PinScope<'_, '_>,
                                                            args: v8::FunctionCallbackArguments,
                                                            mut retval: v8::ReturnValue| {
                        let dec = unsafe { args.data().cast::<v8::External>() };

                        let dec = dec.value() as *mut DeclarationFFI;

                        let dec = unsafe { &*dec };

                        let lock = dec.read();

                        let Some(method) = lock.as_any().downcast_ref::<PropertyDeclaration>() else { return; };

                        let Some(ns_instance) = dec.instance.clone() else { return; };
                        let Some(mut method) = PropertyCall::new(
                            method, false, ns_instance, false,
                        ) else { return; };

                        let (ret, result, outs) = method.call(scope, &args);

                        if ret.is_err() {
                            let detail = crate::error::format_hresult_message(ret);
                            let msg = v8::String::new(scope, &detail).unwrap();
                            let err = v8::Exception::error(scope, msg.into());
                            scope.throw_exception(err);
                            return;
                        }

                        if !outs.is_empty() {
                            let mut arr_len = outs.len();
                            if !method.is_void() { arr_len += 1; }
                            let arr = v8::Array::new(scope, arr_len as i32);
                            let mut idx = 0u32;

                            if !method.is_void() {
                                let return_sig = method.return_type().to_string();
                                let mut return_value_opt: Option<Local<v8::Value>> = None;
                                if return_sig.contains('.') {
                                    if let Some(declaration) = MetadataReader::find_by_name(return_sig.as_str()) {
                                        if matches!(declaration.read().kind(), DeclarationKind::Struct) {
                                            let obj = crate::create_struct_object_from_raw(declaration, result, scope).into();
                                            return_value_opt = Some(obj);
                                        } else if !result.is_null() {
                                            let instance = unsafe { IUnknown::from_raw(result) };
                                            let retv: Local<v8::Value> = create_ns_ctor_instance_object(return_sig.as_str(), None, None, declaration, Some(instance), scope).into();
                                            return_value_opt = Some(retv);
                                        } else {
                                            return_value_opt = Some(v8::null(scope).into());
                                        }
                                    }
                                }
                                if return_value_opt.is_none() {
                                    if let Ok(return_type) = NativeType::try_from(return_sig.as_str()) {
                                        let v = unsafe { read_value_from_ptr(result as *const c_void, scope, return_type) };
                                        return_value_opt = Some(v);
                                    }
                                }
                                if let Some(rv) = return_value_opt {
                                    arr.set_index(scope, idx, rv);
                                    idx += 1;
                                }
                            }

                            for outv in outs.into_iter() {
                                arr.set_index(scope, idx, outv);
                                idx += 1;
                            }
                            retval.set(arr.into());
                            return;
                        }

                        if method.is_void() {
                            retval.set_undefined();
                            return;
                        }

                        let return_sig = method.return_type().to_string();
                        if return_sig.contains('.') {
                            if let Some(declaration) = MetadataReader::find_by_name(return_sig.as_str()) {
                                let ret: Local<v8::Value> = if matches!(declaration.read().kind(), DeclarationKind::Struct) {
                                    create_struct_object_from_raw(declaration, result, scope).into()
                                } else if result.is_null() {
                                    v8::null(scope).into()
                                } else {
                                    let instance = unsafe { IUnknown::from_raw(result) };
                                    create_ns_ctor_instance_object(return_sig.as_str(), None, None, declaration, Some(instance), scope).into()
                                };
                                retval.set(ret.into());
                                return;
                            }
                        }

                        match NativeType::try_from(return_sig.as_str()) {
                            Ok(return_type) => {
                                unsafe { set_ret_val(result, scope, retval, return_type); }
                            }
                            Err(_) => {}
                        }
                    })
                        .data(getter_declaration_ext.into())
                        .build(scope);


                    let mut setter: Option<Local<FunctionTemplate>> = None;


                    if property.setter().is_some() {
                        let setter_declaration = declaration;

                        let setter_declaration = Box::into_raw(Box::new(setter_declaration));

                        let setter_declaration_ext = v8::External::new(scope, setter_declaration as _);


                        setter = Some(FunctionTemplate::builder(|scope: &mut v8::PinScope<'_, '_>,
                                                                 args: v8::FunctionCallbackArguments,
                                                                 _retval: v8::ReturnValue| {
                            let dec = unsafe { args.data().cast::<v8::External>() };
                            let dec = dec.value() as *mut DeclarationFFI;
                            let dec = unsafe { &*dec };
                            let lock = dec.read();
                            let Some(prop) = lock.as_any().downcast_ref::<PropertyDeclaration>() else { return; };
                            let Some(ns_instance) = dec.instance.clone() else { return; };
                            let Some(mut method) = PropertyCall::new(prop, true, ns_instance, false) else { return; };
                            let (ret, _, _outs) = method.call(scope, &args);
                            if ret.is_err() {
                                let detail = crate::error::format_hresult_message(ret);
                                let msg = v8::String::new(scope, &detail).unwrap();
                                let err = v8::Exception::error(scope, msg);
                                scope.throw_exception(err);
                            }
                        })
                            .data(setter_declaration_ext.into())
                            .build(scope));
                    }


                    if property.is_static() {
                        // Static properties live on the constructor, not the prototype.
                        let name = name.unwrap();
                        tmpl.set_accessor_property(name.into(), Some(getter), setter, v8::PropertyAttribute::DONT_DELETE);
                    } else {
                        let name = name.unwrap();
                        object_tmpl.set_accessor_property(name.into(), Some(getter), setter, v8::PropertyAttribute::NONE);
                    }
                }
            }
            DeclarationKind::Interface
            | DeclarationKind::GenericInterface
            | DeclarationKind::GenericInterfaceInstance => {
                // SAFETY: outer match arm filtered to exactly these three kinds.
                let clazz: &dyn BaseClassDeclarationImpl = match kind {
                    DeclarationKind::Interface => match lock.as_any().downcast_ref::<InterfaceDeclaration>() {
                        Some(d) => d,
                        None => return v8::undefined(scope).into(),
                    },
                    DeclarationKind::GenericInterface => match lock.as_any().downcast_ref::<GenericInterfaceDeclaration>() {
                        Some(d) => d,
                        None => return v8::undefined(scope).into(),
                    },
                    DeclarationKind::GenericInterfaceInstance => match lock.as_any().downcast_ref::<GenericInterfaceInstanceDeclaration>() {
                        Some(d) => d,
                        None => return v8::undefined(scope).into(),
                    },
                    _ => unsafe { std::hint::unreachable_unchecked() },
                };


                let to_string_func = FunctionTemplate::builder(|_scope: &mut v8::PinScope<'_, '_>,
                                                                args: v8::FunctionCallbackArguments,
                                                                mut retval: v8::ReturnValue| {
                    retval.set(args.data());
                })
                    .data(class_name.into())
                    .build(scope);

                let to_string = v8::String::new(scope, "toString").unwrap();
                proto.set(to_string.into(), to_string_func.into());

                if let Some(clazz) = parent {
                    let clazz = clazz.read();
                    let kind = clazz.kind();

                    match kind {
                        DeclarationKind::Class => {
                            if let Some(clazz) = clazz.as_any().downcast_ref::<ClassDeclaration>() {

                            for method in clazz.methods().iter() {
                                let name = v8::String::new(scope, method.name());
                                let is_static = method.is_static();

                                let declaration = DeclarationFFI::new_with_instance(
                                    Arc::new(
                                        RwLock::new(
                                            method.clone()
                                        )
                                    ),
                                    if is_static { factory.clone() } else { instance.clone() },
                                );

                                let declaration = Box::into_raw(Box::new(declaration));

                                let ext = v8::External::new(scope, declaration as _);

                                let func = v8::FunctionTemplate::builder(|scope: &mut v8::PinScope<'_, '_>,
                                                                          args: v8::FunctionCallbackArguments,
                                                                          mut retval: v8::ReturnValue| {
                                    let dec = unsafe { args.data().cast::<v8::External>() };

                                    let dec = dec.value() as *mut DeclarationFFI;

                                    let dec = unsafe { &*dec };

                                    let lock = dec.read();

                                    let Some(method) = lock.as_any().downcast_ref::<MethodDeclaration>() else { return; };

                                    let Some(ns_instance) = dec.instance.clone() else { return; };
                                    let mut method = MethodCall::new(
                                        method, method.is_sealed(), ns_instance, false,
                                    );

                                    let (ret, result, outs) = method.call(scope, &args);

                                    if ret.is_err() {
                                        let detail = crate::error::format_hresult_message(ret);
                                        let msg = v8::String::new(scope, &detail).unwrap();
                                        let err = v8::Exception::error(scope, msg.into());
                                        scope.throw_exception(err);
                                        return;
                                    }

                                    if !outs.is_empty() {
                                        let mut arr_len = outs.len();
                                        if !method.is_void() { arr_len += 1; }
                                        let arr = v8::Array::new(scope, arr_len as i32);
                                        let mut idx = 0u32;

                                        if !method.is_void() {
                                            let return_sig = method.return_type().to_string();
                                            let mut return_value_opt: Option<Local<v8::Value>> = None;
                                            if return_sig.contains('.') {
                                                if let Some(declaration) = MetadataReader::find_by_name(return_sig.as_str()) {
                                                    if matches!(declaration.read().kind(), DeclarationKind::Struct) {
                                                        let obj = crate::create_struct_object_from_raw(declaration, result, scope).into();
                                                        return_value_opt = Some(obj);
                                                    } else if !result.is_null() {
                                                        let instance = unsafe { IUnknown::from_raw(result) };
                                                        let retv: Local<v8::Value> = create_ns_ctor_instance_object(return_sig.as_str(), None, None, declaration, Some(instance), scope).into();
                                                        return_value_opt = Some(retv);
                                                    } else {
                                                        return_value_opt = Some(v8::null(scope).into());
                                                    }
                                                }
                                            }
                                            if return_value_opt.is_none() {
                                                if let Ok(return_type) = NativeType::try_from(return_sig.as_str()) {
                                                    let v = unsafe { read_value_from_ptr(result as *const c_void, scope, return_type) };
                                                    return_value_opt = Some(v);
                                                }
                                            }
                                            if let Some(rv) = return_value_opt {
                                                arr.set_index(scope, idx, rv);
                                                idx += 1;
                                            }
                                        }

                                        for outv in outs.into_iter() {
                                            arr.set_index(scope, idx, outv);
                                            idx += 1;
                                        }
                                        retval.set(arr.into());
                                        return;
                                    }

                                    if method.is_void() {
                                        retval.set_undefined();
                                        return;
                                    }

                                    match NativeType::try_from(method.return_type()) {
                                        Ok(return_type) => {
                                            unsafe { set_ret_val(result, scope, retval, return_type); }
                                        }
                                        Err(_) => {}
                                    }

                                    // todo
                                })
                                    .data(ext.into())
                                    .build(scope);


                                if is_static {
                                    tmpl.set(name.unwrap().into(), func.into());
                                } else {
                                    proto.set(name.unwrap().into(), func.into());
                                }
                            }

                            for property in clazz.properties().iter() {
                                let name = v8::String::new(scope, property.name());
                                let is_static = property.is_static();

                                let declaration = DeclarationFFI::new_with_instance(
                                    Arc::new(
                                        RwLock::new(
                                            property.clone()
                                        )
                                    ),
                                    if is_static { factory.clone() } else { instance.clone() },
                                );


                                let getter_declaration = declaration.clone();

                                let getter_declaration = Box::into_raw(Box::new(getter_declaration));

                                let getter_declaration_ext = v8::External::new(scope, getter_declaration as _);

                                let getter = FunctionTemplate::builder(|scope: &mut v8::PinScope<'_, '_>,
                                                                        args: v8::FunctionCallbackArguments,
                                                                        mut retval: v8::ReturnValue| {
                                    let dec = unsafe { args.data().cast::<v8::External>() };

                                    let dec = dec.value() as *mut DeclarationFFI;

                                    let dec = unsafe { &*dec };

                                    let lock = dec.read();

                                    let _kind = lock.kind();

                                    let Some(property) = lock.as_any().downcast_ref::<PropertyDeclaration>() else { return; };

                                    let Some(ns_instance) = dec.instance.clone() else { return; };
                                    let mut method = MethodCall::new(
                                        property.getter(), false, ns_instance, false,
                                    );


                                    let (ret, result, outs) = method.call(scope, &args);

                                    if ret.is_err() {
                                        let detail = crate::error::format_hresult_message(ret);
                                        let msg = v8::String::new(scope, &detail).unwrap();
                                        let err = v8::Exception::error(scope, msg.into());
                                        scope.throw_exception(err);
                                        return;
                                    }

                                    if !outs.is_empty() {
                                        let mut arr_len = outs.len();
                                        if !method.is_void() { arr_len += 1; }
                                        let arr = v8::Array::new(scope, arr_len as i32);
                                        let mut idx = 0u32;

                                        if !method.is_void() {
                                            let return_sig = method.return_type().to_string();
                                            let mut return_value_opt: Option<Local<v8::Value>> = None;
                                            if return_sig.contains('.') {
                                                if let Some(declaration) = MetadataReader::find_by_name(return_sig.as_str()) {
                                                    if matches!(declaration.read().kind(), DeclarationKind::Struct) {
                                                        let obj = crate::create_struct_object_from_raw(declaration, result, scope).into();
                                                        return_value_opt = Some(obj);
                                                    } else if !result.is_null() {
                                                        let instance = unsafe { IUnknown::from_raw(result) };
                                                        let retv: Local<v8::Value> = create_ns_ctor_instance_object(return_sig.as_str(), None, None, declaration, Some(instance), scope).into();
                                                        return_value_opt = Some(retv);
                                                    } else {
                                                        return_value_opt = Some(v8::null(scope).into());
                                                    }
                                                }
                                            }
                                            if return_value_opt.is_none() {
                                                if let Ok(return_type) = NativeType::try_from(return_sig.as_str()) {
                                                    let v = unsafe { read_value_from_ptr(result as *const c_void, scope, return_type) };
                                                    return_value_opt = Some(v);
                                                }
                                            }
                                            if let Some(rv) = return_value_opt {
                                                arr.set_index(scope, idx, rv);
                                                idx += 1;
                                            }
                                        }

                                        for outv in outs.into_iter() {
                                            arr.set_index(scope, idx, outv);
                                            idx += 1;
                                        }
                                        retval.set(arr.into());
                                        return;
                                    }

                                    if method.is_void() {
                                        retval.set_undefined();
                                        return;
                                    }

                                    match NativeType::try_from(method.return_type()) {
                                        Ok(return_type) => {
                                            unsafe { set_ret_val(result, scope, retval, return_type); }
                                        }
                                        Err(_) => {}
                                    }
                                })
                                    .data(getter_declaration_ext.into())
                                    .build(scope);


                                let mut setter: Option<Local<FunctionTemplate>> = None;


                                if property.setter().is_some() {
                                    let setter_declaration = declaration;

                                    let setter_declaration = Box::into_raw(Box::new(setter_declaration));

                                    let setter_declaration_ext = v8::External::new(scope, setter_declaration as _);


                                    setter = Some(FunctionTemplate::builder(|_scope: &mut v8::PinScope<'_, '_>,
                                                                             _args: v8::FunctionCallbackArguments,
                                                                             _retval: v8::ReturnValue| {})
                                        .data(setter_declaration_ext.into())
                                        .build(scope));
                                }


                                if property.is_static() {
                                    let name = name.unwrap();
                                    tmpl.set_accessor_property(name.into(), Some(getter), setter, v8::PropertyAttribute::DONT_DELETE);
                                } else {
                                    let name = name.unwrap();
                                    proto.set_accessor_property(name.into(), Some(getter), setter, v8::PropertyAttribute::READ_ONLY | v8::PropertyAttribute::DONT_DELETE);
                                }
                            }
                            } // end if let Some(clazz) = downcast ClassDeclaration
                        }
                        DeclarationKind::Interface
                        | DeclarationKind::GenericInterface
                        | DeclarationKind::GenericInterfaceInstance => {
                            let clazz_opt: Option<&dyn BaseClassDeclarationImpl> = match kind {
                                DeclarationKind::Interface => clazz.as_any().downcast_ref::<InterfaceDeclaration>().map(|d| d as _),
                                DeclarationKind::GenericInterface => clazz.as_any().downcast_ref::<GenericInterfaceDeclaration>().map(|d| d as _),
                                DeclarationKind::GenericInterfaceInstance => clazz.as_any().downcast_ref::<GenericInterfaceInstanceDeclaration>().map(|d| d as _),
                                _ => None,
                            };
                            if let Some(clazz) = clazz_opt {

                            for method in clazz.methods().iter() {
                                let name = v8::String::new(scope, method.name());
                                let is_static = method.is_static();

                                let declaration = DeclarationFFI::new_with_instance(
                                    Arc::new(
                                        RwLock::new(
                                            method.clone()
                                        )
                                    ),
                                    if is_static { factory.clone() } else { instance.clone() },
                                );

                                let declaration = Box::into_raw(Box::new(declaration));

                                let ext = v8::External::new(scope, declaration as _);

                                let func = v8::FunctionTemplate::builder(|scope: &mut v8::PinScope<'_, '_>,
                                                                          args: v8::FunctionCallbackArguments,
                                                                          mut retval: v8::ReturnValue| {
                                    let dec = unsafe { args.data().cast::<v8::External>() };

                                    let dec = dec.value() as *mut DeclarationFFI;

                                    let dec = unsafe { &*dec };

                                    let lock = dec.read();

                                    let Some(method) = lock.as_any().downcast_ref::<MethodDeclaration>() else { return; };

                                    let Some(ns_instance) = dec.instance.clone() else { return; };
                                    let mut method = MethodCall::new(
                                        method, method.is_sealed(), ns_instance, false,
                                    );

                                    let (ret, result, outs) = method.call(scope, &args);

                                    if ret.is_err() {
                                        let detail = crate::error::format_hresult_message(ret);
                                        let msg = v8::String::new(scope, &detail).unwrap();
                                        let err = v8::Exception::error(scope, msg.into());
                                        scope.throw_exception(err);
                                        return;
                                    }

                                    if !outs.is_empty() {
                                        let mut arr_len = outs.len();
                                        if !method.is_void() { arr_len += 1; }
                                        let arr = v8::Array::new(scope, arr_len as i32);
                                        let mut idx = 0u32;

                                        if !method.is_void() {
                                            let return_sig = method.return_type().to_string();
                                            let mut return_value_opt: Option<Local<v8::Value>> = None;
                                            if return_sig.contains('.') {
                                                if let Some(declaration) = MetadataReader::find_by_name(return_sig.as_str()) {
                                                    if matches!(declaration.read().kind(), DeclarationKind::Struct) {
                                                        let obj = crate::create_struct_object_from_raw(declaration, result, scope).into();
                                                        return_value_opt = Some(obj);
                                                    } else if !result.is_null() {
                                                        let instance = unsafe { IUnknown::from_raw(result) };
                                                        let retv: Local<v8::Value> = create_ns_ctor_instance_object(return_sig.as_str(), None, None, declaration, Some(instance), scope).into();
                                                        return_value_opt = Some(retv);
                                                    } else {
                                                        return_value_opt = Some(v8::null(scope).into());
                                                    }
                                                }
                                            }
                                            if return_value_opt.is_none() {
                                                if let Ok(return_type) = NativeType::try_from(return_sig.as_str()) {
                                                    let v = unsafe { read_value_from_ptr(result as *const c_void, scope, return_type) };
                                                    return_value_opt = Some(v);
                                                }
                                            }
                                            if let Some(rv) = return_value_opt {
                                                arr.set_index(scope, idx, rv);
                                                idx += 1;
                                            }
                                        }

                                        for outv in outs.into_iter() {
                                            arr.set_index(scope, idx, outv);
                                            idx += 1;
                                        }
                                        retval.set(arr.into());
                                        return;
                                    }

                                    if method.is_void() {
                                        retval.set_undefined();
                                        return;
                                    }

                                    match NativeType::try_from(method.return_type()) {
                                        Ok(return_type) => {
                                            unsafe { set_ret_val(result, scope, retval, return_type); }
                                        }
                                        Err(_) => {}
                                    }

                                    // todo
                                })
                                    .data(ext.into())
                                    .build(scope);


                                if is_static {
                                    tmpl.set(name.unwrap().into(), func.into());
                                } else {
                                    proto.set(name.unwrap().into(), func.into());
                                }
                            }

                            for property in clazz.properties().iter() {
                                let name = v8::String::new(scope, property.name());
                                let is_static = property.is_static();

                                let declaration = DeclarationFFI::new_with_instance(
                                    Arc::new(
                                        RwLock::new(
                                            property.clone()
                                        )
                                    ),
                                    if is_static { factory.clone() } else { instance.clone() },
                                );


                                let getter_declaration = declaration.clone();

                                let getter_declaration = Box::into_raw(Box::new(getter_declaration));

                                let getter_declaration_ext = v8::External::new(scope, getter_declaration as _);


                                let getter = FunctionTemplate::builder(|scope: &mut v8::PinScope<'_, '_>,
                                                                        args: v8::FunctionCallbackArguments,
                                                                        mut retval: v8::ReturnValue| {
                                    let dec = unsafe { args.data().cast::<v8::External>() };

                                    let dec = dec.value() as *mut DeclarationFFI;

                                    let dec = unsafe { &*dec };

                                    let lock = dec.read();

                                    let _kind = lock.kind();

                                    let Some(method) = lock.as_any().downcast_ref::<PropertyDeclaration>() else { return; };

                                    let Some(ns_instance) = dec.instance.clone() else { return; };
                                    let Some(mut method) = PropertyCall::new(
                                        method, false, ns_instance, false,
                                    ) else { return; };


                                    let (ret, result, outs) = method.call(scope, &args);

                                    if ret.is_err() {
                                        let detail = crate::error::format_hresult_message(ret);
                                        let msg = v8::String::new(scope, &detail).unwrap();
                                        let err = v8::Exception::error(scope, msg.into());
                                        scope.throw_exception(err);
                                        return;
                                    }

                                    if !outs.is_empty() {
                                        let mut arr_len = outs.len();
                                        if !method.is_void() { arr_len += 1; }
                                        let arr = v8::Array::new(scope, arr_len as i32);
                                        let mut idx = 0u32;

                                        if !method.is_void() {
                                            let return_sig = method.return_type().to_string();
                                            let mut return_value_opt: Option<Local<v8::Value>> = None;
                                            if return_sig.contains('.') {
                                                if let Some(declaration) = MetadataReader::find_by_name(return_sig.as_str()) {
                                                    if matches!(declaration.read().kind(), DeclarationKind::Struct) {
                                                        let obj = crate::create_struct_object_from_raw(declaration, result, scope).into();
                                                        return_value_opt = Some(obj);
                                                    } else if !result.is_null() {
                                                        let instance = unsafe { IUnknown::from_raw(result) };
                                                        let retv: Local<v8::Value> = create_ns_ctor_instance_object(return_sig.as_str(), None, None, declaration, Some(instance), scope).into();
                                                        return_value_opt = Some(retv);
                                                    } else {
                                                        return_value_opt = Some(v8::null(scope).into());
                                                    }
                                                }
                                            }
                                            if return_value_opt.is_none() {
                                                if let Ok(return_type) = NativeType::try_from(return_sig.as_str()) {
                                                    let v = unsafe { read_value_from_ptr(result as *const c_void, scope, return_type) };
                                                    return_value_opt = Some(v);
                                                }
                                            }
                                            if let Some(rv) = return_value_opt {
                                                arr.set_index(scope, idx, rv);
                                                idx += 1;
                                            }
                                        }

                                        for outv in outs.into_iter() {
                                            arr.set_index(scope, idx, outv);
                                            idx += 1;
                                        }
                                        retval.set(arr.into());
                                        return;
                                    }

                                    if method.is_void() {
                                        retval.set_undefined();
                                        return;
                                    }

                                    match NativeType::try_from(method.return_type()) {
                                        Ok(return_type) => {
                                            unsafe { set_ret_val(result, scope, retval, return_type); }
                                        }
                                        Err(_) => {}
                                    }
                                })
                                    .data(getter_declaration_ext.into())
                                    .build(scope);


                                let mut setter: Option<Local<FunctionTemplate>> = None;


                                if property.setter().is_some() {
                                    let setter_declaration = declaration;

                                    let setter_declaration = Box::into_raw(Box::new(setter_declaration));

                                    let setter_declaration_ext = v8::External::new(scope, setter_declaration as _);


                                    setter = Some(FunctionTemplate::builder(|_scope: &mut v8::PinScope<'_, '_>,
                                                                             _args: v8::FunctionCallbackArguments,
                                                                             _retval: v8::ReturnValue| {})
                                        .data(setter_declaration_ext.into())
                                        .build(scope));
                                }


                                if property.is_static() {
                                    let name = name.unwrap();
                                    tmpl.set_accessor_property(name.into(), Some(getter), setter, v8::PropertyAttribute::DONT_DELETE);
                                } else {
                                    let name = name.unwrap();
                                    proto.set_accessor_property(name.into(), Some(getter), setter, v8::PropertyAttribute::READ_ONLY | v8::PropertyAttribute::DONT_DELETE);
                                }
                            }
                            } // end if let Some(clazz) = clazz_opt
                        }
                        DeclarationKind::Delegate
                        | DeclarationKind::GenericDelegate
                        | DeclarationKind::GenericDelegateInstance => {
                            let method = match kind {
                                DeclarationKind::Delegate => clazz
                                    .as_any()
                                    .downcast_ref::<DelegateDeclaration>()
                                    .map(|delegate| delegate.invoke_method().clone()),
                                DeclarationKind::GenericDelegate => clazz
                                    .as_any()
                                    .downcast_ref::<GenericDelegateDeclaration>()
                                    .map(|delegate| delegate.invoke_method().clone()),
                                DeclarationKind::GenericDelegateInstance => clazz
                                    .as_any()
                                    .downcast_ref::<GenericDelegateInstanceDeclaration>()
                                    .map(|delegate| delegate.invoke_method().clone()),
                                _ => None,
                            };

                            if let Some(method) = method {
                                let name = v8::String::new(scope, method.name());
                                let declaration = DeclarationFFI::new_with_instance(
                                    Arc::new(RwLock::new(method)),
                                    instance.clone(),
                                );
                                let declaration = Box::into_raw(Box::new(declaration));
                                let ext = v8::External::new(scope, declaration as _);

                                let func = v8::FunctionTemplate::builder(|scope: &mut v8::PinScope<'_, '_>,
                                                                          args: v8::FunctionCallbackArguments,
                                                                          mut retval: v8::ReturnValue| {
                                    let dec = unsafe { args.data().cast::<v8::External>() };
                                    let dec = dec.value() as *mut DeclarationFFI;
                                    let dec = unsafe { &*dec };
                                    let lock = dec.read();
                                    let Some(method) = lock.as_any().downcast_ref::<MethodDeclaration>() else { return; };

                                    let Some(ns_instance) = dec.instance.clone() else { return; };
                                    let mut method = MethodCall::new(
                                        method,
                                        method.is_sealed(),
                                        ns_instance,
                                        false,
                                    );

                                    let (ret, result, outs) = method.call(scope, &args);
                                    if ret.is_err() {
                                        let detail = crate::error::format_hresult_message(ret);
                                        let msg = v8::String::new(scope, &detail).unwrap();
                                        let err = v8::Exception::error(scope, msg.into());
                                        scope.throw_exception(err);
                                        return;
                                    }

                                    if !outs.is_empty() {
                                        let mut arr_len = outs.len();
                                        if !method.is_void() { arr_len += 1; }
                                        let arr = v8::Array::new(scope, arr_len as i32);
                                        let mut idx = 0u32;

                                        if !method.is_void() {
                                            let return_sig = method.return_type().to_string();
                                            let mut return_value_opt: Option<Local<v8::Value>> = None;
                                            if return_sig.contains('.') {
                                                if let Some(declaration) = MetadataReader::find_by_name(return_sig.as_str()) {
                                                    if matches!(declaration.read().kind(), DeclarationKind::Struct) {
                                                        let obj = crate::create_struct_object_from_raw(declaration, result, scope).into();
                                                        return_value_opt = Some(obj);
                                                    } else if !result.is_null() {
                                                        let instance = unsafe { IUnknown::from_raw(result) };
                                                        let retv: Local<v8::Value> = create_ns_ctor_instance_object(return_sig.as_str(), None, None, declaration, Some(instance), scope).into();
                                                        return_value_opt = Some(retv);
                                                    } else {
                                                        return_value_opt = Some(v8::null(scope).into());
                                                    }
                                                }
                                            }
                                            if return_value_opt.is_none() {
                                                if let Ok(return_type) = NativeType::try_from(return_sig.as_str()) {
                                                    let v = unsafe { read_value_from_ptr(result as *const c_void, scope, return_type) };
                                                    return_value_opt = Some(v);
                                                }
                                            }
                                            if let Some(rv) = return_value_opt {
                                                arr.set_index(scope, idx, rv);
                                                idx += 1;
                                            }
                                        }

                                        for outv in outs.into_iter() {
                                            arr.set_index(scope, idx, outv);
                                            idx += 1;
                                        }
                                        retval.set(arr.into());
                                        return;
                                    }

                                    if method.is_void() {
                                        retval.set_undefined();
                                        return;
                                    }

                                    let return_sig = method.return_type().to_string();
                                    if return_sig.contains('.') {
                                        if let Some(declaration) = MetadataReader::find_by_name(return_sig.as_str()) {
                                            let ret: Local<v8::Value> = if matches!(declaration.read().kind(), DeclarationKind::Struct) {
                                                create_struct_object_from_raw(declaration, result, scope).into()
                                            } else if result.is_null() {
                                                v8::null(scope).into()
                                            } else {
                                                let instance = unsafe { IUnknown::from_raw(result) };
                                                create_ns_ctor_instance_object(return_sig.as_str(), None, None, declaration, Some(instance), scope).into()
                                            };
                                            retval.set(ret);
                                            return;
                                        }
                                    } else if let Ok(return_type) = NativeType::try_from(return_sig.as_str()) {
                                        unsafe { set_ret_val(result, scope, retval, return_type); }
                                    }
                                })
                                    .data(ext.into())
                                    .build(scope);

                                if let Some(name) = name {
                                    proto.set(name.into(), func.into());
                                }
                            }
                        }
                        _ => {}
                    }
                }


                for method in clazz.methods().iter() {
                    let name = v8::String::new(scope, method.name());
                    let is_static = method.is_static();

                    let declaration = DeclarationFFI::new_with_instance(
                        Arc::new(
                            RwLock::new(
                                method.clone()
                            )
                        ),
                        if is_static { factory.clone() } else { instance.clone() },
                    );

                    let declaration = Box::into_raw(Box::new(declaration));

                    let ext = v8::External::new(scope, declaration as _);

                    let func = v8::FunctionTemplate::builder(|scope: &mut v8::PinScope<'_, '_>,
                                                              args: v8::FunctionCallbackArguments,
                                                              mut retval: v8::ReturnValue| {
                        let dec = unsafe { args.data().cast::<v8::External>() };

                        let dec = dec.value() as *mut DeclarationFFI;

                        let dec = unsafe { &*dec };

                        let lock = dec.read();

                        let Some(method) = lock.as_any().downcast_ref::<MethodDeclaration>() else { return; };

                        let Some(ns_instance) = dec.instance.clone() else { return; };
                        let mut method = MethodCall::new(
                            method, method.is_sealed(), ns_instance, false,
                        );

                        let (ret, result, outs) = method.call(scope, &args);

                        if ret.is_err() {
                            let detail = crate::error::format_hresult_message(ret);
                            let msg = v8::String::new(scope, &detail).unwrap();
                            let err = v8::Exception::error(scope, msg.into());
                            scope.throw_exception(err);
                            return;
                        }

                        if !outs.is_empty() {
                            let mut arr_len = outs.len();
                            if !method.is_void() { arr_len += 1; }
                            let arr = v8::Array::new(scope, arr_len as i32);
                            let mut idx = 0u32;

                            if !method.is_void() {
                                let return_sig = method.return_type().to_string();
                                let mut return_value_opt: Option<Local<v8::Value>> = None;
                                if return_sig.contains('.') {
                                    if let Some(declaration) = MetadataReader::find_by_name(return_sig.as_str()) {
                                        if matches!(declaration.read().kind(), DeclarationKind::Struct) {
                                            let obj = crate::create_struct_object_from_raw(declaration, result, scope).into();
                                            return_value_opt = Some(obj);
                                        } else if !result.is_null() {
                                            let instance = unsafe { IUnknown::from_raw(result) };
                                            let retv: Local<v8::Value> = create_ns_ctor_instance_object(&return_sig, None, None, declaration, Some(instance), scope).into();
                                            return_value_opt = Some(retv);
                                        } else {
                                            return_value_opt = Some(v8::null(scope).into());
                                        }
                                    }
                                }
                                if return_value_opt.is_none() {
                                    if let Ok(return_type) = NativeType::try_from(return_sig.as_str()) {
                                        let v = unsafe { read_value_from_ptr(result as *const c_void, scope, return_type) };
                                        return_value_opt = Some(v);
                                    }
                                }
                                if let Some(rv) = return_value_opt {
                                    arr.set_index(scope, idx, rv);
                                    idx += 1;
                                }
                            }

                            for outv in outs.into_iter() {
                                arr.set_index(scope, idx, outv);
                                idx += 1;
                            }
                            retval.set(arr.into());
                            return;
                        }

                        if method.is_void() {
                            retval.set_undefined();
                            return;
                        }

                        let return_sig = method.return_type().to_string();
                        if return_sig.contains('.') {
                            if result.is_null() {
                                retval.set(v8::null(scope).into());
                            } else {
                                let declaration = MetadataReader::find_by_name(return_sig.as_str())
                                    .unwrap_or_else(|| dec.inner.clone());
                                let instance = unsafe { IUnknown::from_raw(result) };
                                let ret_val: Local<v8::Value> = create_ns_ctor_instance_object(
                                    &return_sig, None, None, declaration, Some(instance), scope,
                                ).into();
                                retval.set(ret_val);
                            }
                        } else if let Ok(return_type) = NativeType::try_from(return_sig.as_str()) {
                            unsafe { set_ret_val(result, scope, retval, return_type); }
                        }
                    })
                        .data(ext.into())
                        .build(scope);


                    if is_static {
                        tmpl.set(name.unwrap().into(), func.into());
                    } else {
                        proto.set(name.unwrap().into(), func.into());
                    }
                }

                for property in clazz.properties().iter() {
                    let name = v8::String::new(scope, property.name());
                    let is_static = property.is_static();

                    let declaration = DeclarationFFI::new_with_instance(
                        Arc::new(
                            RwLock::new(
                                property.clone()
                            )
                        ),
                        if is_static { factory.clone() } else { instance.clone() },
                    );


                    let getter_declaration = declaration.clone();

                    let getter_declaration = Box::into_raw(Box::new(getter_declaration));

                    let getter_declaration_ext = v8::External::new(scope, getter_declaration as _);


                    let getter = FunctionTemplate::builder(|scope: &mut v8::PinScope<'_, '_>,
                                                            args: v8::FunctionCallbackArguments,
                                                            mut retval: v8::ReturnValue| {
                        let dec = unsafe { args.data().cast::<v8::External>() };

                        let dec = dec.value() as *mut DeclarationFFI;

                        let dec = unsafe { &*dec };

                        let lock = dec.read();

                        let _kind = lock.kind();

                        let Some(method) = lock.as_any().downcast_ref::<PropertyDeclaration>() else { return; };

                        let Some(ns_instance) = dec.instance.clone() else { return; };
                        let Some(mut method) = PropertyCall::new(
                            method, false, ns_instance, false,
                        ) else { return; };


                        let (ret, result, outs) = method.call(scope, &args);

                                    if ret.is_err() {
                                                let detail = crate::error::format_hresult_message(ret);
                                                let msg = v8::String::new(scope, &detail).unwrap();
                                                let err = v8::Exception::error(scope, msg.into());
                                                scope.throw_exception(err);
                                                return;
                                            }

                                    if !outs.is_empty() {
                                        let mut arr_len = outs.len();
                                        if !method.is_void() { arr_len += 1; }
                                        let arr = v8::Array::new(scope, arr_len as i32);
                                        let mut idx = 0u32;

                                        if !method.is_void() {
                                            let return_sig = method.return_type().to_string();
                                            let mut return_value_opt: Option<Local<v8::Value>> = None;
                                            if return_sig.contains('.') {
                                                if let Some(declaration) = MetadataReader::find_by_name(return_sig.as_str()) {
                                                    if matches!(declaration.read().kind(), DeclarationKind::Struct) {
                                                        let obj = crate::create_struct_object_from_raw(declaration, result, scope).into();
                                                        return_value_opt = Some(obj);
                                                    } else if !result.is_null() {
                                                        let instance = unsafe { IUnknown::from_raw(result) };
                                                        let retv: Local<v8::Value> = create_ns_ctor_instance_object(return_sig.as_str(), None, None, declaration, Some(instance), scope).into();
                                                        return_value_opt = Some(retv);
                                                    } else {
                                                        return_value_opt = Some(v8::null(scope).into());
                                                    }
                                                }
                                            }
                                            if return_value_opt.is_none() {
                                                if let Ok(return_type) = NativeType::try_from(return_sig.as_str()) {
                                                    let v = unsafe { read_value_from_ptr(result as *const c_void, scope, return_type) };
                                                    return_value_opt = Some(v);
                                                }
                                            }
                                            if let Some(rv) = return_value_opt {
                                                arr.set_index(scope, idx, rv);
                                                idx += 1;
                                            }
                                        }

                                        for outv in outs.into_iter() {
                                            arr.set_index(scope, idx, outv);
                                            idx += 1;
                                        }
                                        retval.set(arr.into());
                                        return;
                                    }

                                    if method.is_void() {
                                        retval.set_undefined();
                                        return;
                                    }

                                    match NativeType::try_from(method.return_type()) {
                                Ok(return_type) => {
                                    unsafe { set_ret_val(result, scope, retval, return_type); }
                                }
                                Err(_) => {}
                            }
                    })
                        .data(getter_declaration_ext.into())
                        .build(scope);


                    let mut setter: Option<Local<FunctionTemplate>> = None;


                    if property.setter().is_some() {
                        let setter_declaration = declaration;

                        let setter_declaration = Box::into_raw(Box::new(setter_declaration));

                        let setter_declaration_ext = v8::External::new(scope, setter_declaration as _);


                        setter = Some(FunctionTemplate::builder(|scope: &mut v8::PinScope<'_, '_>,
                                                                 args: v8::FunctionCallbackArguments,
                                                                 _retval: v8::ReturnValue| {
                            let dec = unsafe { args.data().cast::<v8::External>() };
                            let dec = dec.value() as *mut DeclarationFFI;
                            let dec = unsafe { &*dec };
                            let lock = dec.read();
                            let Some(prop) = lock.as_any().downcast_ref::<PropertyDeclaration>() else { return; };
                            let Some(setter) = prop.setter() else { return; };
                            let Some(ns_instance) = dec.instance.clone() else { return; };
                            let mut method = MethodCall::new(setter, false, ns_instance, false);
                            let (ret, _, _outs) = method.call(scope, &args);
                            if ret.is_err() {
                                let detail = crate::error::format_hresult_message(ret);
                                let msg = v8::String::new(scope, &detail).unwrap();
                                let err = v8::Exception::error(scope, msg);
                                scope.throw_exception(err);
                            }
                        })
                            .data(setter_declaration_ext.into())
                            .build(scope));
                    }


                    if property.is_static() {
                        let name = name.unwrap();
                        tmpl.set_accessor_property(name.into(), Some(getter), setter, v8::PropertyAttribute::DONT_DELETE);
                    } else {
                        let name = name.unwrap();
                        proto.set_accessor_property(name.into(), Some(getter), setter, v8::PropertyAttribute::READ_ONLY | v8::PropertyAttribute::DONT_DELETE);
                    }
                }
            }
            DeclarationKind::GenericInterface => {
                let Some(clazz) = lock.as_any().downcast_ref::<GenericInterfaceDeclaration>() else { return v8::undefined(scope).into(); };

                let return_types = helpers::get_generic_return_types(name);
                let type_args_str: String = return_types.names().join(",");

                for method in clazz.methods() {
                    let signature = method.return_type();

                    let Some(metadata) = method.metadata() else { continue; };
                    let return_type_str = Signature::to_string(metadata, &signature);

                    let return_type_index = match usize::from_str_radix(&*return_type_str.as_str().replace("Var!", ""), 10) {
                        Ok(idx) => idx,
                        Err(_) => continue,
                    };

                    let Some(&return_type) = return_types.names().get(return_type_index) else { continue; };

                    let name = v8::String::new(scope, method.name());

                    let is_static = method.is_static();

                    let parent = declaration.clone();
                    let mut declaration = DeclarationFFI::new_with_instance(
                        Arc::new(
                            RwLock::new(
                                method.clone()
                            )
                        ),
                        if is_static {
                            factory.clone()
                        } else {
                            instance.clone()
                        },
                    );
                    declaration.parent = Some(parent);

                    let declaration = Box::into_raw(Box::new(declaration));

                    let Some(return_type) = v8::String::new(scope, return_type) else { continue; };
                    let Some(type_args_v8) = v8::String::new(scope, &type_args_str) else { continue; };

                    let ext = v8::External::new(scope, declaration as _);

                    let data = v8::Array::new_with_elements(scope, &[ext.into(), return_type.into(), type_args_v8.into()]);

                    let func = FunctionTemplate::builder(|scope: &mut v8::PinScope<'_, '_>,
                                                          args: v8::FunctionCallbackArguments,
                                                          mut retval: v8::ReturnValue| {
                        let Ok(data) = v8::Local::<v8::Array>::try_from(args.data()) else { return; };

                        let Some(return_type_val) = data.get_index(scope, 1) else { return; };
                        let return_type = return_type_val.to_rust_string_lossy(scope);

                        let type_args_str = data.get_index(scope, 2).map(|v| v.to_rust_string_lossy(scope)).unwrap_or_default();
                        let type_args: Vec<String> = if type_args_str.is_empty() {
                            Vec::new()
                        } else {
                            type_args_str.split(',').map(|s| s.to_owned()).collect()
                        };

                        let Some(dec_val) = data.get_index(scope, 0) else { return; };
                        let dec = unsafe { dec_val.cast::<v8::External>() };

                        let dec = dec.value() as *mut DeclarationFFI;

                        let dec = unsafe { &*dec };

                        let lock = dec.read();

                        let Some(method) = lock.as_any().downcast_ref::<MethodDeclaration>() else { return; };

                        let Some(parent_arc) = dec.parent.as_ref() else { return; };
                        let parent = parent_arc.read();
                        let Some(parent) = parent.as_any().downcast_ref::<GenericInterfaceDeclaration>() else { return; };

                        let Some(ns_instance) = dec.instance.clone() else { return; };
                        let mut method = GenericMethodCall::new(
                            parent, method, method.is_sealed(), ns_instance, false, return_type, type_args,
                        );

                        let (ret, result, outs) = method.call(scope, &args);

                                if ret.is_err() {
                                    let detail = crate::error::format_hresult_message(ret);
                                    let msg = v8::String::new(scope, &detail).unwrap();
                                    let err = v8::Exception::error(scope, msg.into());
                                    scope.throw_exception(err);
                                    return;
                                }

                                if !outs.is_empty() {
                                    let mut arr_len = outs.len();
                                    if !method.is_void() { arr_len += 1; }
                                    let arr = v8::Array::new(scope, arr_len as i32);
                                    let mut idx = 0u32;

                                    if !method.is_void() {
                                        let return_sig = method.return_type();
                                        let mut return_value_opt: Option<Local<v8::Value>> = None;
                                        if return_sig.contains('.') {
                                            if let Some(declaration) = MetadataReader::find_by_name(return_sig) {
                                                if matches!(declaration.read().kind(), DeclarationKind::Struct) {
                                                    let obj = crate::create_struct_object_from_raw(declaration, result, scope).into();
                                                    return_value_opt = Some(obj);
                                                } else if !result.is_null() {
                                                    let instance = unsafe { IUnknown::from_raw(*(result as *mut *mut c_void)) };
                                                    let retv: Local<v8::Value> = create_ns_ctor_instance_object(return_sig, None, dec.parent.clone(), declaration, Some(instance), scope).into();
                                                    return_value_opt = Some(retv);
                                                } else {
                                                    return_value_opt = Some(v8::null(scope).into());
                                                }
                                            } else {
                                                let instance = unsafe { IUnknown::from_raw(*(result as *mut *mut c_void)) };
                                                let declaration = MetadataReader::find_by_name(return_sig)
                                                    .unwrap_or_else(|| dec.inner.clone());
                                                let retv: Local<v8::Value> = create_ns_ctor_instance_object(return_sig, None, dec.parent.clone(), declaration, Some(instance), scope).into();
                                                return_value_opt = Some(retv);
                                            }
                                        }
                                        if return_value_opt.is_none() {
                                            if let Ok(return_type) = NativeType::try_from(return_sig) {
                                                let v = unsafe { read_value_from_ptr(result as *const c_void, scope, return_type) };
                                                return_value_opt = Some(v);
                                            }
                                        }
                                        if let Some(rv) = return_value_opt {
                                            arr.set_index(scope, idx, rv);
                                            idx += 1;
                                        }
                                    }

                                    for outv in outs.into_iter() {
                                        arr.set_index(scope, idx, outv);
                                        idx += 1;
                                    }
                                    retval.set(arr.into());
                                    return;
                                }

                                if method.is_void() {
                                    retval.set_undefined();
                                    return;
                                }

                                let return_sig = method.return_type();
                                match NativeType::try_from(return_sig) {
                                    Ok(return_type) => {
                                        if return_sig.contains('.') {
                                            if let Some(declaration) = MetadataReader::find_by_name(return_sig) {
                                                let ret: Local<v8::Value> = if matches!(declaration.read().kind(), DeclarationKind::Struct) {
                                                    crate::create_struct_object_from_raw(declaration, result, scope).into()
                                                } else if result.is_null() {
                                                    v8::null(scope).into()
                                                } else {
                                                    let instance = unsafe { IUnknown::from_raw(*(result as *mut *mut c_void)) };
                                                    create_ns_ctor_instance_object(return_sig, None, dec.parent.clone(), declaration, Some(instance), scope).into()
                                                };
                                                retval.set(ret.into());
                                                return;
                                            } else {
                                                let instance = unsafe { IUnknown::from_raw(*(result as *mut *mut c_void)) };
                                                let declaration = MetadataReader::find_by_name(return_sig)
                                                    .unwrap_or_else(|| dec.inner.clone());
                                                let ret: Local<v8::Value> = create_ns_ctor_instance_object(return_sig, None, dec.parent.clone(), declaration, Some(instance), scope).into();
                                                retval.set(ret.into());
                                                return;
                                            }
                                        }
                                        unsafe { set_ret_val(result, scope, retval, return_type); }
                                    }
                                    Err(_) => {}
                                }


                        // todo
                    })
                        .data(data.into())
                        .build(scope);

                    if let Some(n) = name {
                        if is_static {
                            tmpl.set_with_attr(n.into(), func.into(), v8::PropertyAttribute::DONT_DELETE);
                        } else {
                            proto.set_with_attr(n.into(), func.into(), v8::PropertyAttribute::DONT_DELETE);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    let object = match object_tmpl.new_instance(scope) {
        Some(o) => o,
        None => {
            let msg = v8::String::new(scope, "Failed to create instance object").unwrap();
            let err = v8::Exception::error(scope, msg.into());
            scope.throw_exception(err);
            return v8::null(scope).into();
        }
    };

    object.set_internal_field(0, ext.into());

    // Per-instance side store for JS-assigned overrides and caching
    let object_store = v8::Map::new(scope);
    object.set_internal_field(1, object_store.into());

    if let Some(handle_key) = v8::String::new(scope, "handle") {
        let handle_value: Local<v8::Value> = if let Some(instance) = instance.as_ref() {
            v8::External::new(scope, instance.as_raw() as *mut c_void).into()
        } else {
            v8::null(scope).into()
        };
        object.set(scope, handle_key.into(), handle_value);
    }

    let ret = object;

    if let Some(key) = identity_key {
        let weak = v8::Weak::with_guaranteed_finalizer(
            scope.as_mut(),
            ret,
            Box::new(move || {
                INSTANCE_CACHE.with(|cache| {
                    cache.borrow_mut().remove(&key);
                });
            }),
        );
        let new_size = INSTANCE_CACHE.with(|cache| {
            let mut c = cache.borrow_mut();
            c.insert(key, weak);
            c.len()
        });
        maybe_request_gc_nudge(new_size, scope.as_mut());
    }

    ret.into()
}

/// Converts a raw WinRT out-parameter result pointer to a `Local<v8::Value>`.
/// Returns `None` for void returns, null COM pointers, or unrecognised types.
/// The `parent_decl` is forwarded to `create_ns_ctor_instance_object` for COM returns.
unsafe fn raw_result_to_local<'s>(
    result: *mut c_void,
    signature: &str,
    parent_decl: Option<Arc<RwLock<dyn Declaration>>>,
    scope: &mut v8::PinScope<'s, '_>,
) -> Option<Local<'s, v8::Value>> {
    let raw = result as usize;
    match signature {
        "Void" => None,
        "Guid" => Some(unsafe { guid_ptr_to_js_object(result, scope) }.into()),
        _ if !signature.contains('.') => {
            let native_type = NativeType::try_from(signature).ok()?;
            let v: Local<v8::Value> = match native_type {
                NativeType::Void => return None,
                NativeType::Bool  => v8::Boolean::new(scope, (raw as u8) != 0).into(),
                NativeType::U8    => v8::Number::new(scope, raw as u8 as f64).into(),
                NativeType::I8    => v8::Number::new(scope, (raw as u8 as i8) as f64).into(),
                NativeType::U16   => v8::Number::new(scope, raw as u16 as f64).into(),
                NativeType::I16   => v8::Number::new(scope, (raw as u16 as i16) as f64).into(),
                NativeType::U32   => v8::Number::new(scope, raw as u32 as f64).into(),
                NativeType::I32   => v8::Number::new(scope, (raw as u32 as i32) as f64).into(),
                NativeType::U64 => {
                    let v = raw as u64;
                    if v > MAX_SAFE_INTEGER as u64 { v8::BigInt::new_from_u64(scope, v).into() }
                    else { v8::Number::new(scope, v as f64).into() }
                }
                NativeType::I64 => {
                    let v = raw as u64 as i64;
                    if v > MAX_SAFE_INTEGER as i64 || v < MIN_SAFE_INTEGER as i64 {
                        v8::BigInt::new_from_i64(scope, v).into()
                    } else {
                        v8::Number::new(scope, v as f64).into()
                    }
                }
                NativeType::USize => {
                    if raw > MAX_SAFE_INTEGER as usize { v8::BigInt::new_from_u64(scope, raw as u64).into() }
                    else { v8::Number::new(scope, raw as f64).into() }
                }
                NativeType::ISize => {
                    let v = raw as isize;
                    if !(MIN_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&v) {
                        v8::BigInt::new_from_i64(scope, v as i64).into()
                    } else {
                        v8::Number::new(scope, v as f64).into()
                    }
                }
                NativeType::F32 => v8::Number::new(scope, f32::from_bits(raw as u32) as f64).into(),
                NativeType::F64 => v8::Number::new(scope, f64::from_bits(raw as u64)).into(),
                NativeType::String => {
                    let hstring: HSTRING = std::mem::transmute(result);
                    let s = hstring.to_string();
                    v8::String::new(scope, s.as_str())?.into()
                }
                // "Object" in WinRT metadata maps to NativeType::Pointer and means
                // IInspectable* — try to resolve the concrete runtime class name so
                // we can hand back a typed wrapper instead of an opaque External.
                NativeType::Pointer => {
                    if result.is_null() { return None; }
                    let unknown = IUnknown::from_raw(result);
                    if let Ok(inspectable) = unknown.cast::<IInspectable>() {
                        if let Ok(class_name) = inspectable.GetRuntimeClassName() {
                            let name_str = class_name.to_string();
                            if let Some(decl) = MetadataReader::find_by_name(&name_str) {
                                let instance = unknown.clone();
                                return Some(create_ns_ctor_instance_object(
                                    &name_str,
                                    None,
                                    parent_decl,
                                    decl,
                                    Some(instance),
                                    scope,
                                ).into());
                            }
                        }
                    }
                    // Could not resolve type — expose as opaque External so the
                    // property is at least present rather than silently missing.
                    v8::External::new(scope, result).into()
                }
                _ => return None,
            };
            Some(v)
        }
        _ => {
            if result.is_null() { return None; }
            let com_instance = IUnknown::from_raw(result);
            let decl = MetadataReader::find_by_name(signature)?;
            Some(create_ns_ctor_instance_object(
                signature,
                None,
                parent_decl,
                decl,
                Some(com_instance),
                scope,
            ).into())
        }
    }
}


fn create_ns_ctor_object<'a>(name: &str, parent: Option<Arc<RwLock<dyn Declaration>>>, declaration: Arc<RwLock<dyn Declaration>>, scope: &mut v8::PinScope<'a, '_>) -> Local<'a, v8::Value> {

    // Re-entrancy guard: if we're already building this constructor on this
    // thread, return a lightweight stub function to avoid mutating V8
    // templates/descriptors twice (which can trigger internal V8 assertions).
    let name_str = name;
    let already_building = CREATING_CTORS.with(|set| {
        let mut set = set.borrow_mut();
        if set.contains(name_str) {
            true
        } else {
            set.insert(name_str.to_string());
            false
        }
    });

    if already_building {
        let stub = v8::FunctionTemplate::builder(|_scope: &mut v8::PinScope<'_, '_>, _args: v8::FunctionCallbackArguments, mut _retval: v8::ReturnValue| {} ).build(scope);
        let Some(func) = stub.get_function(scope) else { return v8::undefined(scope).into(); };
        let key = v8::String::new(scope, "__typeName__").unwrap();
        let val = v8::String::new(scope, name_str).unwrap();
        func.set(scope, key.into(), val.into());
        return func.into();
    }

    let name = v8::String::new(scope, name).unwrap();

    let mut declaration_ffi = DeclarationFFI::new(Arc::clone(&declaration));

    declaration_ffi.parent = parent;

    let declaration_ptr = Box::into_raw(Box::new(declaration_ffi));

    let data_ext = v8::External::new(scope, declaration_ptr as _);

    let tmpl = v8::FunctionTemplate::builder(|scope: &mut v8::PinScope<'_, '_>,
                                              args: v8::FunctionCallbackArguments,
                                              mut retval: v8::ReturnValue| {
        let length = args.length();

        let dec = unsafe { args.data().cast::<v8::External>() };

        let dec = dec.value() as *mut DeclarationFFI;

        let dec = unsafe { &*dec };

        let lock = dec.read();

        let kind = lock.kind();

        let ext = args.data();

        match kind {
            DeclarationKind::Class => {
                // Collect all needed data while holding the lock, then release it so
                // create_ns_ctor_instance_object can re-acquire without deadlocking.
                let full_name;
                let is_sealed;
                let initializers: Vec<MethodDeclaration>;
                let parent;
                {
                    let Some(clazz) = lock.as_any().downcast_ref::<ClassDeclaration>() else { return; };
                    full_name = clazz.full_name().to_string();
                    is_sealed = clazz.is_sealed();
                    initializers = clazz.initializers().iter().cloned().collect();
                    parent = dec.parent.clone();
                }
                drop(lock);

                // Attempt activation using several candidate type names derived
                // from metadata (full name, stripped-generic, simple name).
                // This allows trying alternate activators when the default
                // `RoGetActivationFactory` lookup for `full_name` doesn't work
                // (observed with some XAML types such as FontFamily).
                let mut clazz_factory_opt: Option<IUnknown> = None;
                let mut last_err: Option<windows::core::Error> = None;
                let mut candidates: Vec<String> = Vec::new();
                candidates.push(full_name.clone());
                let stripped = crate::helpers::strip_generic_suffix(full_name.as_str()).to_string();
                if stripped != candidates[0] {
                    candidates.push(stripped);
                }
                let (_ns, simple_name) = split_type_name(full_name.as_str());
                if !simple_name.is_empty() && !candidates.contains(&simple_name) {
                    candidates.push(simple_name.clone());
                }

                for candidate in candidates.iter() {
                    match class_activation_factory(candidate.as_str()) {
                        Ok(factory) => { clazz_factory_opt = Some(factory); break; }
                        Err(e) => { last_err = Some(e); }
                    }
                }

                let clazz_factory = match clazz_factory_opt {
                    Some(f) => f,
                    None => {
                        if let Some(e) = last_err {
                            throw_js_error(
                                scope,
                                format!("Failed to activate WinRT class {}: {}", full_name, e.message()).as_str(),
                            );
                        } else {
                            throw_js_error(
                                scope,
                                format!("Failed to activate WinRT class {}", full_name).as_str(),
                            );
                        }
                        return;
                    }
                };

                if length == 0 {
                    match clazz_factory.cast::<IActivationFactory>() {
                        Ok(activation_factory) => {
                            match unsafe { activation_factory.ActivateInstance() } {
                                Ok(instance) => {
                                    let result = match instance.cast::<IUnknown>() {
                                        Ok(value) => value,
                                        Err(error) => {
                                            throw_js_error(
                                                scope,
                                                format!("Failed to cast activated instance for {}: {}", full_name, error.message()).as_str(),
                                            );
                                            return;
                                        }
                                    };

                                    if let Ok(init) = result.cast::<IInitializeWithWindow>() {
                                        let hwnd = unsafe { GetConsoleWindow() };
                                        if !hwnd.is_invalid() {
                                            let _ = unsafe { init.Initialize(hwnd) };
                                        }
                                    }

                                    if let Some(declaration) = MetadataReader::find_by_name(&full_name) {
                                        let instance_obj = create_ns_ctor_instance_object(
                                            &full_name, Some(clazz_factory.clone()), parent.clone(), declaration, Some(result), scope,
                                        );
                                        retval.set(instance_obj);
                                    }
                                    return;
                                }
                                Err(error) => {
                                    throw_js_error(
                                        scope,
                                        format!("ActivateInstance failed for WinRT class {}: {}", full_name, error.message()).as_str(),
                                    );
                                    return;
                                }
                            }
                        }
                        Err(_) => {
                            // Fall through to metadata-driven constructor dispatch when
                            // activation factory cannot be cast.
                        }
                    }
                }

                for ctor in &initializers {
                    let number_of_parameters = ctor.number_of_parameters();
                    if number_of_parameters != length as usize {
                        continue;
                    }
                    let mut method = MethodCall::new(ctor, is_sealed, clazz_factory.clone(), true);
                    let (ret, result, outs) = method.call(scope, &args);

                    if ret.is_ok() {
                        if result.is_null() {
                            retval.set(v8::null(scope).into());
                            return;
                        }
                        let result = unsafe { IUnknown::from_raw(result) };
                        let vtable = result.vtable();
                        let mut qi_ptr: *mut c_void = std::ptr::null_mut();
                        let res = unsafe {
                            ((*vtable).QueryInterface)(
                                result.as_raw(),
                                &IUnknown::IID,
                                std::mem::transmute(&mut qi_ptr),
                            )
                        };
                        if res.is_err() || qi_ptr.is_null() {
                            let message = res.message().to_string();
                            let message = v8::String::new(scope, message.as_str()).unwrap();
                            let error = v8::Exception::error(scope, message.into());
                            scope.throw_exception(error);
                            return;
                        }
                        let result = unsafe { IUnknown::from_raw(qi_ptr) };

                        if let Ok(init) = result.cast::<IInitializeWithWindow>() {
                            let hwnd = unsafe { GetConsoleWindow() };
                            if !hwnd.is_invalid() {
                                let _ = unsafe { init.Initialize(hwnd) };
                            }
                        }

                        if let Some(declaration) = MetadataReader::find_by_name(&full_name) {
                            let instance_obj = create_ns_ctor_instance_object(
                                &full_name, Some(clazz_factory.clone()), parent.clone(), declaration, Some(result), scope,
                            );
                            if !outs.is_empty() {
                                let mut arr = v8::Array::new(scope, (1 + outs.len()) as i32);
                                arr.set_index(scope, 0, instance_obj);
                                let mut idx = 1u32;
                                for outv in outs.into_iter() {
                                    arr.set_index(scope, idx, outv);
                                    idx += 1;
                                }
                                retval.set(arr.into());
                            } else {
                                retval.set(instance_obj);
                            }
                        }
                        return;
                    } else {
                        let detail = crate::error::format_hresult_message(ret);
                        let message = v8::String::new(scope, &detail).unwrap();
                        let error = v8::Exception::error(scope, message.into());
                        scope.throw_exception(error);
                        return;
                    }
                }
                return;
            }
            DeclarationKind::Struct => {}
            DeclarationKind::Delegate
            | DeclarationKind::GenericDelegate
            | DeclarationKind::GenericDelegateInstance => {
                // Accept a plain JS function or an Android-style { Invoke(){} } object.
                if length >= 1 {
                    let arg0 = args.get(0);
                    let maybe_func: Option<v8::Local<v8::Function>> = if arg0.is_function() {
                        v8::Local::<v8::Function>::try_from(arg0).ok()
                    } else if arg0.is_object() {
                        if let Some(obj) = arg0.to_object(scope) {
                            let mut found = None;
                            for name in &["Invoke", "invoke"] {
                                if let Some(key) = v8::String::new(scope, name) {
                                    if let Some(val) = obj.get(scope, key.into()) {
                                        if let Ok(f) = v8::Local::<v8::Function>::try_from(val) {
                                            found = Some(f);
                                            break;
                                        }
                                    }
                                }
                            }
                            found
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    if let Some(func) = maybe_func {
                        if let Some((guid, param_types)) =
                            js_delegate_params_from_declaration(&*lock, kind)
                        {
                            
                            let global_func = v8::Global::new(scope, func);
                            let data = Box::new(JsDelegateData {
                                js_func: global_func,
                                param_types,
                            });
                            let delegate = Box::new(JsDelegate {
                                vtable: &JS_DELEGATE_VTBL as *const _,
                                ref_count: AtomicU32::new(1),
                                guid,
                                data: Box::into_raw(data),
                            });
                            let raw = Box::into_raw(delegate) as *mut c_void;

                            let result_obj = v8::Object::new(scope);
                            if let Some(key) = v8::String::new(scope, "handle") {
                                result_obj.set(
                                    scope,
                                    key.into(),
                                    v8::External::new(scope, raw).into(),
                                );
                            }
                            retval.set(result_obj.into());
                            return;
                        } else {
                        }
                    } else {
                    }
                }
            }
            _ => {}
        }

        let object_tmpl = v8::ObjectTemplate::new(scope);
        object_tmpl.set_named_property_handler(
            v8::NamedPropertyHandlerConfiguration::new()
                .query(handle_named_property_query)
                .getter(handle_named_property_getter)
                .setter(handle_named_property_setter)
        );
        object_tmpl.set_indexed_property_handler(
            v8::IndexedPropertyHandlerConfiguration::new()
                .setter(handle_indexed_property_setter)
                .getter(handle_indexed_property_getter)
        );
        object_tmpl.set_internal_field_count(2);
        let Some(object) = object_tmpl.new_instance(scope) else { return; };

        object.set_internal_field(0, ext.into());

        let object_store = v8::Map::new(scope);

        if matches!(
            kind,
            DeclarationKind::Interface
                | DeclarationKind::GenericInterface
                | DeclarationKind::GenericInterfaceInstance
                | DeclarationKind::Delegate
                | DeclarationKind::GenericDelegate
                | DeclarationKind::GenericDelegateInstance
                | DeclarationKind::Event
        ) && length >= 1
        {
            let implementation = args.get(0);
            if implementation.is_object() || implementation.is_function() {
                if let Some(impl_key) = v8::String::new(scope, "__implementation__") {
                    object_store.set(scope, impl_key.into(), implementation);
                }
            }
        }

        object.set_internal_field(1, object_store.into());

        retval.set(object.into());
    })
        .data(data_ext.into()).build(scope);
    // Ensure instances created by this constructor use the same
    // named/indexed property handlers and internal fields as the
    // instance-wrapper path so prototype/instance members are visible.
    let instance_tmpl = tmpl.instance_template(scope);
    instance_tmpl.set_named_property_handler(
        v8::NamedPropertyHandlerConfiguration::new()
            .getter(|scope: &mut v8::PinScope<'_, '_>,
                     key: Local<v8::Name>,
                     args: v8::PropertyCallbackArguments,
                     mut rv: v8::ReturnValue<v8::Value>| -> v8::Intercepted {
                if !key.is_string() {
                    return v8::Intercepted::kNo;
                }

                let name = key.to_rust_string_lossy(scope);
                if name == "__probe__" {
                    let value = v8::String::new(scope, "instance-handler-active").unwrap();
                    rv.set(value.into());
                    return v8::Intercepted::kYes;
                }

                // Prefer the DeclarationFFI stored on the instance (holder internal field[0]).
                // Fall back to the callback data if the holder lacks the internal field.
                let holder = args.holder();
                let dec_field_opt = holder.get_internal_field(scope, 0);
                let dec = if let Some(dec_field) = dec_field_opt {
                    let dec_ext = unsafe { dec_field.cast::<v8::External>() };
                    let dec_ptr = dec_ext.value() as *mut DeclarationFFI;
                    unsafe { &*dec_ptr }
                } else {
                    let dec_ext = unsafe { args.data().cast::<v8::External>() };
                    let dec_ptr = dec_ext.value() as *mut DeclarationFFI;
                    unsafe { &*dec_ptr }
                };

                let lock = dec.read();

                let Some(clazz) = lock.as_any().downcast_ref::<ClassDeclaration>() else {
                    return v8::Intercepted::kNo;
                };

                // If a JS-assigned override exists in the per-instance store, return it.
                // Use the holder (where the property was found) to access the side-store map.
                let this = holder;
                if let Some(store_field) = this.get_internal_field(scope, 1) {
                    let store = unsafe { store_field.cast::<v8::Map>() };
                    if let Some(cache) = store.get(scope, key.into()) {
                        if !cache.is_null_or_undefined() {
                            rv.set(cache);
                            return v8::Intercepted::kYes;
                        }
                    }
                }

                if let Some(property) = find_class_property(clazz, &name) {
                    let Some(ns_instance) = dec.instance.clone() else {
                        return v8::Intercepted::kNo;
                    };
                    let Some(mut property_call) = PropertyCall::new(&property, false, ns_instance, false) else {
                        return v8::Intercepted::kNo;
                    };
                    let (ret, result, _outs) = property_call.call_with_values(scope, &[]);

                    if ret.is_err() {
                        let detail = format!("Property get '{}' failed: {} (0x{:08X})", name, ret.message(), ret.0 as u32);
                        let message = v8::String::new(scope, &detail).unwrap();
                        let error = v8::Exception::error(scope, message);
                        scope.throw_exception(error);
                        return v8::Intercepted::kYes;
                    }

                    if property_call.is_void() {
                        rv.set_undefined();
                        return v8::Intercepted::kYes;
                    }

                    let return_sig = property_call.return_type().to_string();
                    if return_sig.contains('.') {
                        if let Some(declaration) = MetadataReader::find_by_name(return_sig.as_str()) {
                            let ret: Local<v8::Value> = if matches!(declaration.read().kind(), DeclarationKind::Struct) {
                                create_struct_object_from_raw(declaration, result, scope).into()
                            } else if result.is_null() {
                                v8::null(scope).into()
                            } else {
                                let instance = unsafe { IUnknown::from_raw(result) };
                                create_ns_ctor_instance_object(return_sig.as_str(), None, None, declaration, Some(instance), scope).into()
                            };
                            rv.set(ret);
                            return v8::Intercepted::kYes;
                        }
                    }

                    if let Ok(return_type) = NativeType::try_from(return_sig.as_str()) {
                        unsafe { set_ret_val(result, scope, rv, return_type); }
                        return v8::Intercepted::kYes;
                    }

                    return v8::Intercepted::kNo;
                }

                if let Some(method) = find_class_method(clazz, &name) {
                    let declaration = Arc::new(RwLock::new(method.clone()));
                    let declaration = Box::into_raw(Box::new(DeclarationFFI::new_with_instance(declaration, dec.instance.clone())));
                    let ext = v8::External::new(scope, declaration as _);

                    let builder = v8::Function::builder(|scope: &mut v8::PinScope<'_, '_>,
                                                         args: v8::FunctionCallbackArguments,
                                                         mut retval: v8::ReturnValue| {
                        let dec = unsafe { args.data().cast::<v8::External>() };
                        let dec = dec.value() as *mut DeclarationFFI;
                        let dec = unsafe { &*dec };
                        let lock = dec.read();
                        let Some(method) = lock.as_any().downcast_ref::<MethodDeclaration>() else { return; };
                        let Some(ns_instance) = dec.instance.clone() else { return; };
                        let mut method = MethodCall::new(method, method.is_sealed(), ns_instance, false);
                        let (ret, result, outs) = method.call(scope, &args);

                        if ret.is_err() {
                            let detail = crate::error::format_hresult_message(ret);
                            let message = v8::String::new(scope, &detail).unwrap();
                            let error = v8::Exception::error(scope, message);
                            scope.throw_exception(error);
                            return;
                        }

                        if !outs.is_empty() {
                            let mut arr_len = outs.len();
                            if !method.is_void() { arr_len += 1; }
                            let arr = v8::Array::new(scope, arr_len as i32);
                            let mut idx = 0u32;

                            if !method.is_void() {
                                let return_sig = method.return_type().to_string();
                                let mut return_value_opt: Option<Local<v8::Value>> = None;
                                if return_sig.contains('.') {
                                    if let Some(declaration) = MetadataReader::find_by_name(return_sig.as_str()) {
                                        if matches!(declaration.read().kind(), DeclarationKind::Struct) {
                                            let obj = crate::create_struct_object_from_raw(declaration, result, scope).into();
                                            return_value_opt = Some(obj);
                                        } else if !result.is_null() {
                                            let instance = unsafe { IUnknown::from_raw(result) };
                                            let retv: Local<v8::Value> = create_ns_ctor_instance_object(return_sig.as_str(), None, None, declaration, Some(instance), scope).into();
                                            return_value_opt = Some(retv);
                                        } else {
                                            return_value_opt = Some(v8::null(scope).into());
                                        }
                                    }
                                }
                                if return_value_opt.is_none() {
                                    if let Ok(return_type) = NativeType::try_from(return_sig.as_str()) {
                                        let v = unsafe { read_value_from_ptr(result as *const c_void, scope, return_type) };
                                        return_value_opt = Some(v);
                                    }
                                }
                                if let Some(rv) = return_value_opt {
                                    arr.set_index(scope, idx, rv);
                                    idx += 1;
                                }
                            }

                            for outv in outs.into_iter() {
                                arr.set_index(scope, idx, outv);
                                idx += 1;
                            }
                            retval.set(arr.into());
                            return;
                        }

                        if method.is_void() {
                            retval.set_undefined();
                            return;
                        }

                        let return_sig = method.return_type().to_string();
                        if return_sig.contains('.') {
                            if let Some(declaration) = MetadataReader::find_by_name(return_sig.as_str()) {
                                let ret: Local<v8::Value> = if matches!(declaration.read().kind(), DeclarationKind::Struct) {
                                    create_struct_object_from_raw(declaration, result, scope).into()
                                } else if result.is_null() {
                                    v8::null(scope).into()
                                } else {
                                    let instance = unsafe { IUnknown::from_raw(result) };
                                    create_ns_ctor_instance_object(return_sig.as_str(), None, None, declaration, Some(instance), scope).into()
                                };
                                retval.set(ret.into());
                                return;
                            }
                        }

                        if let Ok(return_type) = NativeType::try_from(return_sig.as_str()) {
                            unsafe { set_ret_val(result, scope, retval, return_type); }
                        }
                    })
                    .data(ext.into())
                    .build(scope)
                    .unwrap();

                    let func: Local<v8::Value> = builder.into();
                    if let Some(store_field) = holder.get_internal_field(scope, 1) {
                        let store = unsafe { store_field.cast::<v8::Map>() };
                        store.set(scope, key.into(), func);
                    }
                    rv.set(func);
                    return v8::Intercepted::kYes;
                }

                v8::Intercepted::kNo
            })
                .data(v8::External::new(scope, declaration_ptr as _).into())
    );

    instance_tmpl.set_indexed_property_handler(
        v8::IndexedPropertyHandlerConfiguration::new()
            .setter(handle_indexed_property_setter)
            .getter(handle_indexed_property_getter)
            .data(v8::External::new(scope, declaration_ptr as _).into())
    );

    instance_tmpl.set_internal_field_count(2);
    tmpl.set_class_name(name);


    {
        let lock = declaration.read();

        if lock.kind() != DeclarationKind::Class {
            let Some(func) = tmpl.get_function(scope) else {
                CREATING_CTORS.with(|set| { set.borrow_mut().remove(name_str); });
                return v8::undefined(scope).into();
            };
            CREATING_CTORS.with(|set| { set.borrow_mut().remove(name_str); });
            return func.into();
        }

        let Some(clazz) = lock.as_any().downcast_ref::<ClassDeclaration>() else {
            CREATING_CTORS.with(|set| { set.borrow_mut().remove(name_str); });
            return v8::undefined(scope).into();
        };
        let mut added_names: AHashSet<String> = AHashSet::new();

        for method in clazz.methods().iter() {
            let name = v8::String::new(scope, method.name());
            let is_static = method.is_static();

            if !is_static {
                continue;
            }

            // Skip duplicate method/property names to avoid adding the same
            // descriptor multiple times to the FunctionTemplate which can
            // corrupt V8 internal descriptor arrays.
            let m_name = method.name();
            if added_names.contains(m_name) { continue; }
            added_names.insert(m_name.to_string());

            let parent = Arc::clone(&declaration);

            let mut declaration = DeclarationFFI::new_with_instance(
                Arc::new(
                    RwLock::new(
                        method.clone()
                    )
                ),
                None,
            );

            declaration.parent = Some(parent);

            let declaration = Box::into_raw(Box::new(declaration));

            let ext = v8::External::new(scope, declaration as _);

            let func = v8::FunctionTemplate::builder(|scope: &mut v8::PinScope<'_, '_>,
                                                      args: v8::FunctionCallbackArguments,
                                                      mut retval: v8::ReturnValue| {
                let dec = unsafe { args.data().cast::<v8::External>() };

                let dec = dec.value() as *mut DeclarationFFI;

                let dec = unsafe { &*dec };

                let lock = dec.read();

                let Some(method) = lock.as_any().downcast_ref::<MethodDeclaration>() else { return; };

                let return_type = method.return_type();

                let signature = method.metadata()
                    .map(|m| Signature::to_string(m, &return_type))
                    .unwrap_or_default();


                let factory = match resolve_class_factory_from_parent(dec) {
                    Ok(factory) => factory,
                    Err(error) => {
                        throw_js_error(
                            scope,
                            format!(
                                "Failed to resolve WinRT static method factory for {}: {}",
                                method.name(),
                                error.message()
                            )
                            .as_str(),
                        );
                        return;
                    }
                };

                let mut method = MethodCall::new(
                    method, method.is_sealed(), factory, false,
                );

                let (ret, result, outs) = method.call(scope, &args);


                if ret.is_ok() {
                    if !outs.is_empty() {
                        let mut arr_len = outs.len();
                        if !method.is_void() { arr_len += 1; }
                        let arr = v8::Array::new(scope, arr_len as i32);
                        let mut idx = 0u32;

                        if !method.is_void() {
                            let mut return_value_opt: Option<Local<v8::Value>> = None;
                            if signature.contains('.') {
                                if let Some(declaration) = MetadataReader::find_by_name(signature.as_str()) {
                                    if matches!(declaration.read().kind(), DeclarationKind::Struct) {
                                        return_value_opt = Some(create_struct_object_from_raw(declaration, result, scope).into());
                                    } else if !result.is_null() {
                                        let instance = unsafe { IUnknown::from_raw(result) };
                                        return_value_opt = Some(create_ns_ctor_instance_object(signature.as_str(), dec.instance.clone(), dec.parent.clone(), declaration, Some(instance), scope).into());
                                    } else {
                                        return_value_opt = Some(v8::null(scope).into());
                                    }
                                }
                            }

                            if return_value_opt.is_none() {
                                if signature == "Boolean" {
                                    return_value_opt = Some(v8::Boolean::new(scope, unsafe {*(result as *mut bool)}).into());
                                } else if signature == "Guid" {
                                    let obj = unsafe { guid_ptr_to_js_object(result, scope) };
                                    return_value_opt = Some(obj.into());
                                } else if !signature.contains('.') {
                                    if let Ok(return_type) = NativeType::try_from(signature.as_str()) {
                                        let v = unsafe { read_value_from_ptr(result as *const c_void, scope, return_type) };
                                        return_value_opt = Some(v);
                                    }
                                }
                            }

                            if let Some(rv) = return_value_opt {
                                arr.set_index(scope, idx, rv);
                                idx += 1;
                            }
                        }

                        for outv in outs.into_iter() {
                            arr.set_index(scope, idx, outv);
                            idx += 1;
                        }
                        retval.set(arr.into());
                        return;
                    }

                    unsafe {
                        match signature.as_str() {
                            "Boolean" => {
                                retval.set_bool(
                                    *(result as *mut bool)
                                )
                            }
                            "Guid" => {
                                let obj = unsafe { guid_ptr_to_js_object(result, scope) };
                                retval.set(obj.into());
                            }
                            _ if !signature.contains('.') => {
                                // Primitive / value-type return: use set_ret_val when possible.
                                match NativeType::try_from(signature.as_str()) {
                                    Ok(return_type) => {
                                        set_ret_val(result, scope, retval, return_type);
                                    }
                                    Err(_) => {
                                        retval.set_undefined();
                                    }
                                }
                            }
                            _ => {
                                if result.is_null() {
                                    retval.set(v8::null(scope).into());
                                    return;
                                }
                                if let Some(declaration) = MetadataReader::find_by_name(signature.as_str()) {
                                    let ret: Local<v8::Value> = if matches!(declaration.read().kind(), DeclarationKind::Struct) {
                                        create_struct_object_from_raw(declaration, result, scope).into()
                                    } else {
                                        let instance = IUnknown::from_raw(result);
                                        create_ns_ctor_instance_object(signature.as_str(), dec.instance.clone(), dec.parent.clone(), declaration, Some(instance), scope).into()
                                    };
                                    retval.set(ret.into());
                                } else {
                                    let instance = IUnknown::from_raw(result);
                                    let Some(declaration) = MetadataReader::find_by_name(signature.as_str()) else { return };
                                    let ret: Local<v8::Value> = create_ns_ctor_instance_object(signature.as_str(), dec.instance.clone(), dec.parent.clone(), declaration, Some(instance), scope).into();
                                    retval.set(ret.into());
                                }
                            } // end _ (COM object)
                        } // end match signature
                    } // end unsafe
                } else {
                    let detail = crate::error::format_hresult_message(ret);
                    let message = v8::String::new(scope, &detail).unwrap();
                    let error = v8::Exception::error(scope, message.into());
                    scope.throw_exception(error);
                }
            })
                .data(ext.into())
                .build(scope);

            tmpl.set(name.unwrap().into(), func.into());
        }

        // Register lazy accessor properties for each static property.
        // The WinRT getter is only invoked when JS actually reads the property,
        // avoiding eager FFI calls at class-lookup time that crash for types
        // with many static DependencyProperty members (e.g. ScrollViewer).
        for property in clazz.properties().iter() {
            if !property.is_static() {
                continue;
            }

            let prop_name_str = property.name();
            if added_names.contains(prop_name_str) { continue; }
            added_names.insert(prop_name_str.to_string());

            let Some(prop_name) = v8::String::new(scope, property.name()) else { continue };

            let parent = Arc::clone(&declaration);
            let mut prop_dec = DeclarationFFI::new_with_instance(
                Arc::new(RwLock::new(property.clone())),
                None,
            );
            prop_dec.parent = Some(parent);
            let prop_ext = Box::into_raw(Box::new(prop_dec));
            let prop_ext = v8::External::new(scope, prop_ext as _);

            let getter = v8::FunctionTemplate::builder(|scope: &mut v8::PinScope<'_, '_>,
                                                        args: v8::FunctionCallbackArguments,
                                                        mut retval: v8::ReturnValue| {
                let dec = unsafe { args.data().cast::<v8::External>() };
                let dec = dec.value() as *mut DeclarationFFI;
                let dec = unsafe { &*dec };

                let lock = dec.read();
                let Some(property) = lock.as_any().downcast_ref::<PropertyDeclaration>() else { return };

                let signature = {
                    let sig = property.getter().return_type();
                    match property.getter().metadata() {
                        Some(md) => Signature::to_string(md, &sig),
                        None => return,
                    }
                };

                let factory = match resolve_class_factory_from_parent(dec) {
                    Ok(f) => f,
                    Err(e) => {
                        throw_js_error(scope, &format!("Failed to resolve static property factory: {}", e.message()));
                        return;
                    }
                };

                let mut prop_call_opt = PropertyCall::new(property, false, factory, false);
                if prop_call_opt.is_none() {
                    return;
                }

                let mut prop_call = prop_call_opt.unwrap();

                let (hresult, result, _outs) = prop_call.call_with_values(scope, &[]);

                // If the call failed because the process is unpackaged, do not provide a runtime shim.
                // HRESULT 0x80073D54 = "The process has no package identity." — let callers/tests handle fallback.
                if !hresult.is_ok() && (hresult.0 as u32) == 0x80073D54u32 {

                    // If the static getter failed because the process is unpackaged,
                    // do not synthesize values here; let the caller/tests detect the
                    // missing package identity via the diagnostic marker on the
                    // constructor/object and provide any test-only fallbacks.

                    // Expose a small, non-invasive diagnostic on the JS constructor/object
                    // so code running in V8 can inspect why the value was absent.
                    if let Some(obj) = args.this().to_object(scope) {
                        let key = v8::String::new(scope, "__missingPackageIdentity__").unwrap();
                        let val = v8::String::new(scope, &hresult.message().to_string()).unwrap();
                        let _ = obj.set(scope, key.into(), val.into());
                    }

                    retval.set_undefined();
                    return;
                }

                if hresult.is_ok() {
                    unsafe {
                        match signature.as_str() {
                            "Boolean" => {
                                retval.set_bool(*(result as *mut bool));
                            }
                            "Guid" => {
                                let obj = unsafe { guid_ptr_to_js_object(result, scope) };
                                retval.set(obj.into());
                            }
                            _ if !signature.contains('.') => {
                                match NativeType::try_from(signature.as_str()) {
                                    Ok(return_type) => { set_ret_val(result, scope, retval, return_type); }
                                    Err(_) => { retval.set_undefined(); }
                                }
                            }
                            _ => {
                                if result.is_null() {
                                    retval.set(v8::null(scope).into());
                                    return;
                                }
                                if let Some(declaration) = MetadataReader::find_by_name(signature.as_str()) {
                                    let ret: Local<v8::Value> = if matches!(declaration.read().kind(), DeclarationKind::Struct) {
                                        create_struct_object_from_raw(declaration, result, scope).into()
                                    } else {
                                        let instance = IUnknown::from_raw(result);
                                        create_ns_ctor_instance_object(
                                            signature.as_str(),
                                            dec.instance.clone(),
                                            dec.parent.clone(),
                                            declaration,
                                            Some(instance),
                                            scope,
                                        ).into()
                                    };
                                    retval.set(ret.into());
                                } else {
                                    let instance = IUnknown::from_raw(result);
                                    let Some(ret_decl) = MetadataReader::find_by_name(signature.as_str()) else { return };
                                    let ret: Local<v8::Value> = create_ns_ctor_instance_object(
                                        signature.as_str(),
                                        dec.instance.clone(),
                                        dec.parent.clone(),
                                        ret_decl,
                                        Some(instance),
                                        scope,
                                    ).into();
                                    retval.set(ret.into());
                                }
                            }
                        }
                    }
                }
            })
                .data(prop_ext.into())
                .build(scope);

            tmpl.set_accessor_property(
                prop_name.into(),
                Some(getter),
                None,
                v8::PropertyAttribute::DONT_DELETE,
            );
        }

    }

    let Some(func) = tmpl.get_function(scope) else {
        CREATING_CTORS.with(|set| { set.borrow_mut().remove(name_str); });
        return v8::undefined(scope).into();
    };

    {
        let lock = declaration.read();
        if let Some(full_name) = match lock.kind() {
            DeclarationKind::Class => lock
                .as_any()
                .downcast_ref::<ClassDeclaration>()
                .map(|clazz| clazz.full_name().to_string()),
            _ => None,
        } {
            let key = v8::String::new(scope, "__typeName__").unwrap();
            let value = v8::String::new(scope, full_name.as_str()).unwrap();
            func.set(scope, key.into(), value.into());
        }
    }
    let ret = func;
    CREATING_CTORS.with(|set| { set.borrow_mut().remove(name_str); });
    ret.into()
}

fn create_ns_struct_ctor_object<'a>(name: &str, declaration: Arc<RwLock<dyn Declaration>>, scope: &mut v8::PinScope<'a, '_>) -> Local<'a, v8::Value> {
    

    let name = v8::String::new(scope, name).unwrap();

    let ext = DeclarationFFI::new(Arc::clone(&declaration));

    let ext = Box::into_raw(Box::new(ext));

    let ext = v8::External::new(scope, ext as _);

    let tmpl = FunctionTemplate::builder(|scope: &mut v8::PinScope<'_, '_>,
                                          args: v8::FunctionCallbackArguments,
                                          mut retval: v8::ReturnValue| {
        let dec = unsafe { args.data().cast::<v8::External>() };

        let dec = dec.value() as *mut DeclarationFFI;

        let dec = unsafe { &mut *dec };

        let lock = dec.write();

        let ext = args.data();

        let object_tmpl = v8::ObjectTemplate::new(scope);
        object_tmpl.set_internal_field_count(1);

        let mut field_args: Vec<NativeValue> = Vec::new();

        let mut field_types: Vec<NativeType> = Vec::new();

        let Some(struct_dec) = lock.as_any().downcast_ref::<StructDeclaration>() else { return; };

        // Support both positional args `new Thickness(5, 10, 15, 20)` and
        // named-field object `new Thickness({ Left: 5, Top: 10, Right: 15, Bottom: 20 })`.
        let use_positional = args.length() > 0 && !args.get(0).is_object();
        let named_object: Option<v8::Local<v8::Object>> = if !use_positional {
            match args.get(0).to_object(scope) {
                Some(obj) => Some(obj),
                None => {
                    throw_js_error(scope, "Expected object or positional arguments for struct constructor");
                    return;
                }
            }
        } else {
            None
        };

        for (field_idx, field) in struct_dec.fields().iter().enumerate() {
            let Some(metadata) = field.base().metadata() else { continue; };
            let field_type = Signature::to_string(metadata, &field.type_());

            let Ok(native_type) = NativeType::try_from(field_type.as_str()) else { continue; };

            field_types.push(native_type.clone());

            let field_value = if use_positional {
                Some(args.get(field_idx as i32))
            } else {
                let Some(name) = v8::String::new(scope, field.name()) else { continue; };
                named_object.unwrap().get(scope, name.into())
            };

            match field_value {
                None => {
                    let message = format!("Missing key {}", field.name());
                    let message = v8::String::new(scope, message.as_str()).unwrap();
                    let error = v8::Exception::error(scope, message.into());
                    scope.throw_exception(error);
                }
                Some(field) => {
                    let value = match native_type {
                        NativeType::Void => {
                            Err(error::type_error("Void is not a valid WinRT struct field type"))
                        }
                        NativeType::Bool => {
                            ffi_parse_bool_arg(field)
                        }
                        NativeType::U8 => {
                            ffi_parse_u8_arg(field)
                        }
                        NativeType::I8 => {
                            ffi_parse_i8_arg(field)
                        }
                        NativeType::U16 => {
                            ffi_parse_u16_arg(field)
                        }
                        NativeType::I16 => {
                            ffi_parse_i16_arg(field)
                        }
                        NativeType::U32 => {
                            ffi_parse_u32_arg(field)
                        }
                        NativeType::I32 => {
                            ffi_parse_i32_arg(field)
                        }
                        NativeType::U64 => {
                            ffi_parse_u64_arg(scope, field)
                        }
                        NativeType::I64 => {
                            ffi_parse_i64_arg(scope, field)
                        }
                        NativeType::USize => {
                            ffi_parse_usize_arg(scope, field)
                        }
                        NativeType::ISize => {
                            ffi_parse_isize_arg(scope, field)
                        }
                        NativeType::F32 => {
                            ffi_parse_f32_arg(field)
                        }
                        NativeType::F64 => {
                            ffi_parse_f64_arg(field)
                        }
                        NativeType::Pointer => {
                            ffi_parse_pointer_arg(scope, field)
                        }
                        NativeType::Buffer => {
                            ffi_parse_buffer_arg(scope, field)
                        }
                        NativeType::Function => {
                            ffi_parse_function_arg(scope, field)
                        }
                        NativeType::Struct(_) => {
                            ffi_parse_struct_arg(scope, field)
                        }
                        NativeType::String => {
                            ffi_parse_string_arg(scope, field)
                        }
                    };
                    match value {
                        Ok(value) => {
                            field_args.push(value);
                        }
                        Err(err) => {
                            let message = err.to_string();
                            let message = v8::String::new(scope, message.as_str()).unwrap();
                            let error = v8::Exception::error(scope, message.into());
                            scope.throw_exception(error);
                        }
                    }
                }
            }
        }

        let mut struct_size = 0_usize;

        let params =
            field_types
                .clone()
                .into_iter()
                .map(|item| {
                    struct_size = struct_size + item.size();
                    libffi::middle::Type::try_from(item)
                })
                .collect::<Result<Vec<libffi::middle::Type>, error::AnyError>>();

        if params.is_err() { return; }

        let mut struct_buf: Vec<u8> = unsafe { vec![0_u8; struct_size] };

        struct_buf.shrink_to_fit();

        let mut position = 0_isize;

        for (field_type, field_value) in field_types.iter().zip(field_args.iter()) {
            let size = field_type.size();

            unsafe {
                let buffer = struct_buf.as_mut_ptr();
                let buffer = buffer.offset(position);

                let value: *mut u8 = std::mem::transmute(field_value.as_arg(field_type));

                let slice = std::slice::from_raw_parts_mut(buffer, size);

                std::ptr::copy(value, slice.as_mut_ptr(), size);
            }

            position = position + size as isize;
        }

        let name = lock.name().to_string();

        drop(lock);

        dec.struct_instance = Some((struct_buf, field_types));

        let _name = v8::String::new(scope, name.as_str()).unwrap();

        let getter = |scope: &mut v8::PinScope<'_, '_>,
                      key: Local<v8::Name>,
                      args: v8::PropertyCallbackArguments,
                      mut rv: v8::ReturnValue<v8::Value>| -> v8::Intercepted {
            let key = key.to_rust_string_lossy(scope);

            let this = args.data();

            let dec = unsafe { this.cast::<v8::External>() };

            let dec = dec.value() as *mut DeclarationFFI;

            let dec = unsafe { &*dec };

            let lock = dec.read();

            if key == "toString" {
                let name = lock.name();

                let name = v8::String::new(scope, name).unwrap();
                let func = v8::Function::builder(|_scope: &mut v8::PinScope<'_, '_>,
                                                  args: v8::FunctionCallbackArguments,
                                                  mut retval: v8::ReturnValue| {
                    retval.set(args.data());
                }).data(name.into())
                    .build(scope);


                if let Some(f) = func { rv.set(f.into()); }
                return v8::Intercepted::kYes;
            }

            let Some(struct_dec) = lock.as_any().downcast_ref::<StructDeclaration>() else { return v8::Intercepted::kNo; };

            let mut offset = 0;
            let instance = dec.struct_instance.as_ref();
            let mut position = 0;
            for field in struct_dec.fields() {
                if field.name() == key.as_str() {
                    if let Some((buffer, types)) = instance {
                        let mut current_field_position = 0;
                        for field_type in types.iter() {
                            let size = field_type.size();

                            if position == current_field_position {
                                unsafe {
                                    let buffer = buffer.as_ptr();
                                    let buffer = buffer.offset(offset);

                                    let slice = std::slice::from_raw_parts(buffer, size);

                                    match field_type {
                                        NativeType::Void => {}
                                        NativeType::Bool => {
                                            let ret: &u8 = std::mem::transmute(slice.as_ptr() as *const u8);
                                            rv.set_bool(
                                                *ret == 1
                                            );
                                        }
                                        NativeType::U8 => {
                                            let ret: &u8 = std::mem::transmute(slice.as_ptr() as *const u8);
                                            rv.set_uint32(
                                                *ret as u32
                                            );
                                        }
                                        NativeType::I8 => {
                                            let ret: &i8 = std::mem::transmute(slice.as_ptr() as *const i8);
                                            rv.set_int32(
                                                *ret as i32
                                            );
                                        }
                                        NativeType::U16 => {
                                            let ret: &u16 = std::mem::transmute(slice.as_ptr() as *const u16);
                                            rv.set_uint32(
                                                *ret as u32
                                            );
                                        }
                                        NativeType::I16 => {
                                            let ret: &i16 = std::mem::transmute(slice.as_ptr() as *const i16);
                                            rv.set_int32(
                                                *ret as i32
                                            );
                                        }
                                        NativeType::U32 => {
                                            let ret: &u32 = std::mem::transmute(slice.as_ptr() as *const u32);
                                            rv.set_uint32(
                                                *ret
                                            );
                                        }
                                        NativeType::I32 => {
                                            let ret: &i32 = std::mem::transmute(slice.as_ptr() as *const i32);
                                            rv.set_int32(
                                                *ret
                                            );
                                        }
                                        NativeType::U64 => {
                                            let ret: u64 = *std::mem::transmute::<*const u64, &u64>(slice.as_ptr() as *const u64);

                                            let local_value: v8::Local<v8::Value> =
                                                if ret > MAX_SAFE_INTEGER as u64 {
                                                    v8::BigInt::new_from_u64(scope, ret).into()
                                                } else {
                                                    v8::Number::new(scope, ret as f64).into()
                                                };

                                            rv.set(local_value);
                                        }
                                        NativeType::I64 => {
                                            let ret: i64 = *std::mem::transmute::<*const i64, &i64>(slice.as_ptr() as *const i64);
                                            let local_value: v8::Local<v8::Value> =
                                                if ret > MAX_SAFE_INTEGER as i64 || ret < MIN_SAFE_INTEGER as i64
                                                {
                                                    v8::BigInt::new_from_i64(scope, ret).into()
                                                } else {
                                                    v8::Number::new(scope, ret as f64).into()
                                                };
                                            rv.set(local_value);
                                        }
                                        NativeType::USize => {}
                                        NativeType::ISize => {}
                                        NativeType::F32 => {
                                            //let ret: &f32 = std::mem::transmute(slice.as_ptr() as *const f32);

                                            let ret: f32 = if cfg!(target_endian = "big") {
                                                f32::from_be_bytes(<[u8; 4]>::try_from(slice).unwrap())
                                            } else {
                                                f32::from_le_bytes(<[u8; 4]>::try_from(slice).unwrap())
                                            };

                                            rv.set(
                                                v8::Number::new(scope, ret as f64).into()
                                            );
                                        }
                                        NativeType::F64 => {
                                            let ret: &f64 = std::mem::transmute(slice.as_ptr() as *const f64);
                                            rv.set(
                                                v8::Number::new(scope, *ret).into()
                                            );
                                        }
                                        NativeType::Pointer => {}
                                        NativeType::Buffer => {}
                                        NativeType::Function => {}
                                        NativeType::Struct(_) => {}
                                        NativeType::String => {
                                            // TODO
                                        }
                                    }
                                }
                            }

                            current_field_position = current_field_position + 1;

                            offset = offset + size as isize;
                        }
                    }
                    break;
                }
                position = position + 1;
            }
            v8::Intercepted::kYes
        };

        let setter = |scope: &mut v8::PinScope<'_, '_>,
                      key: Local<v8::Name>,
                      value: Local<v8::Value>,
                      args: v8::PropertyCallbackArguments,
                      _rv: v8::ReturnValue<()>| -> v8::Intercepted {
            let key = key.to_rust_string_lossy(scope);

            let this = args.data();

            let dec = unsafe { this.cast::<v8::External>() };

            let dec = dec.value() as *mut DeclarationFFI;

            let instance = unsafe { (&mut *dec).struct_instance.as_mut() };

            let dec = unsafe { &mut *dec };

            let lock = dec.write();

            let Some(struct_dec) = lock.as_any().downcast_ref::<StructDeclaration>() else { return v8::Intercepted::kNo; };

            let mut offset = 0;

            let mut position = 0;
            for field in struct_dec.fields() {
                if field.name() == key.as_str() {
                    if let Some((buffer, types)) = instance {
                        let field = value;

                        let mut current_field_position = 0;
                        for field_type in types.iter() {
                            let size = field_type.size();

                            if position == current_field_position {
                                let value = match field_type {
                                    NativeType::Void => {
                                        Err(error::type_error("Void is not a valid WinRT struct field type"))
                                    }
                                    NativeType::Bool => {
                                        ffi_parse_bool_arg(field)
                                    }
                                    NativeType::U8 => {
                                        ffi_parse_u8_arg(field)
                                    }
                                    NativeType::I8 => {
                                        ffi_parse_i8_arg(field)
                                    }
                                    NativeType::U16 => {
                                        ffi_parse_u16_arg(field)
                                    }
                                    NativeType::I16 => {
                                        ffi_parse_i16_arg(field)
                                    }
                                    NativeType::U32 => {
                                        ffi_parse_u32_arg(field)
                                    }
                                    NativeType::I32 => {
                                        ffi_parse_i32_arg(field)
                                    }
                                    NativeType::U64 => {
                                        ffi_parse_u64_arg(scope, field)
                                    }
                                    NativeType::I64 => {
                                        ffi_parse_i64_arg(scope, field)
                                    }
                                    NativeType::USize => {
                                        ffi_parse_usize_arg(scope, field)
                                    }
                                    NativeType::ISize => {
                                        ffi_parse_isize_arg(scope, field)
                                    }
                                    NativeType::F32 => {
                                        ffi_parse_f32_arg(field)
                                    }
                                    NativeType::F64 => {
                                        ffi_parse_f64_arg(field)
                                    }
                                    NativeType::Pointer => {
                                        ffi_parse_pointer_arg(scope, field)
                                    }
                                    NativeType::Buffer => {
                                        ffi_parse_buffer_arg(scope, field)
                                    }
                                    NativeType::Function => {
                                        ffi_parse_function_arg(scope, field)
                                    }
                                    NativeType::Struct(_) => {
                                        ffi_parse_struct_arg(scope, field)
                                    }
                                    NativeType::String => {
                                        ffi_parse_string_arg(scope, field)
                                    }
                                };
                                match value {
                                    Ok(value) => {
                                        unsafe {
                                            let buffer = buffer.as_mut_ptr();
                                            let buffer = buffer.offset(offset);

                                            let value: *mut u8 = std::mem::transmute(value.as_arg(field_type));

                                            let slice = std::slice::from_raw_parts_mut(buffer, size);

                                            std::ptr::copy(value, slice.as_mut_ptr(), size);
                                        }
                                    }
                                    Err(err) => {
                                        let message = err.to_string();
                                        let message = v8::String::new(scope, message.as_str()).unwrap();
                                        let error = v8::Exception::error(scope, message.into());
                                        scope.throw_exception(error);
                                    }
                                }
                            }

                            current_field_position = current_field_position + 1;

                            offset = offset + size as isize;
                        }
                    }
                    break;
                }
                position = position + 1;
            }
            v8::Intercepted::kYes
        };

        object_tmpl.set_named_property_handler(
            v8::NamedPropertyHandlerConfiguration::new()
                .getter(getter)
                .setter(setter)
                .data(ext)
        );


        let Some(object) = object_tmpl.new_instance(scope) else { return; };

        object.set_internal_field(0, ext.into());

        retval.set(object.into());
    })
        .data(ext.into()).build(scope);
    tmpl.set_class_name(name);


    let Some(func) = tmpl.get_function(scope) else { return v8::undefined(scope).into(); };
    let ret = func;

    ret.into()
}

pub(crate) fn create_struct_object_from_raw<'a>(
    declaration: Arc<RwLock<dyn Declaration>>,
    raw_data: *mut c_void,
    scope: &mut v8::PinScope<'a, '_>,
) -> Local<'a, v8::Object> {
    let fallback = v8::Object::new(scope);

    // Build the byte buffer from raw_data before setting up the template.
    let (struct_buf, field_types) = {
        let lock = declaration.read();
        let Some(struct_dec) = lock.as_any().downcast_ref::<StructDeclaration>() else {
            return fallback;
        };
        let mut buf: Vec<u8> = Vec::new();
        let mut types: Vec<NativeType> = Vec::new();
        let mut offset: isize = 0;
        for field in struct_dec.fields() {
            let Some(metadata) = field.base().metadata() else { continue; };
            let field_type_str = Signature::to_string(metadata, &field.type_());
            let Ok(native_type) = NativeType::try_from(field_type_str.as_str()) else { continue; };
            let size = native_type.size() as isize;
            let field_ptr = unsafe { (raw_data as *const u8).offset(offset) as *mut c_void };
            let buf_start = buf.len();
            buf.extend(std::iter::repeat(0u8).take(size as usize));
            unsafe {
                std::ptr::copy_nonoverlapping(
                    field_ptr as *const u8,
                    buf[buf_start..].as_mut_ptr(),
                    size as usize,
                );
            }
            types.push(native_type);
            offset += size;
        }
        (buf, types)
    };

    // Store the byte buffer in a DeclarationFFI so native code can read it back
    // via ffi_parse_pointer_arg → try_get_external_handle, AND so the property
    // interceptors can keep reads/writes in sync with the same buffer.
    let mut dec_ffi = DeclarationFFI::new(Arc::clone(&declaration));
    dec_ffi.struct_instance = Some((struct_buf, field_types));
    let dec_raw = Box::into_raw(Box::new(dec_ffi));
    let ext = v8::External::new(scope, dec_raw as *mut c_void);

    // Use property interceptors so that JS writes (e.g. `size.Width = 300`) go
    // through to the buffer, not just to a detached plain JS property.
    let tmpl = v8::ObjectTemplate::new(scope);
    tmpl.set_internal_field_count(1);
    tmpl.set_named_property_handler(
        v8::NamedPropertyHandlerConfiguration::new()
            .getter(crate::ns_proxy::ns_struct_field_getter)
            .setter(crate::ns_proxy::ns_struct_field_setter)
            .enumerator(crate::ns_proxy::ns_struct_field_enumerator)
            .data(ext.into())
    );
    let Some(object) = tmpl.new_instance(scope) else { return fallback; };
    object.set_internal_field(0, ext.into());
    object
}

fn init_meta(scope: &mut v8::ContextScope<v8::HandleScope<v8::Context>>, context: Local<v8::Context>) {
    let global = context.global(scope);
    let Some(global_metadata) = MetadataReader::find_by_name("") else { return; };
    let data = global_metadata.read();
    let ns = data.as_any().downcast_ref::<NamespaceDeclaration>();
    if let Some(global_namespaces) = ns {
        let full_name = global_namespaces.full_name();
        for ns in global_namespaces.children() {
            let full_name = if full_name.is_empty() {
                ns.to_string()
            } else {
                format!("{}.{}", full_name, ns)
            };

            let name: Local<v8::Name> = v8::String::new(scope, ns.as_str()).unwrap().into();
            if let Some(name_space) = MetadataReader::find_by_name(full_name.as_str()) {
                let object = create_ns_object(ns, name_space, scope);
                global.define_own_property(scope, name, object, v8::PropertyAttribute::READ_ONLY | v8::PropertyAttribute::DONT_DELETE | v8::PropertyAttribute::NONE);
            }
        }
    }
}

// Setter for the namespace/enum/struct *proxy* objects (the ones returned by
// `create_ns_object`). These are not instances — they're traversal handles like
// `Windows` or `Windows.UI.Popups`. The rule is:
//   - Names that resolve to real WinRT metadata are immutable (writes are ignored).
//   - Anything else is stored in the per-object side map so user code can stash
//     custom properties (e.g. `Windows.myShim = ...`) without breaking lookups.
fn handle_named_property_setter(scope: &mut v8::PinScope<'_, '_>,
                                key: Local<v8::Name>,
                                value: Local<v8::Value>,
                                args: v8::PropertyCallbackArguments,
                                mut _rv: v8::ReturnValue<()>) -> v8::Intercepted {
    let this = args.holder();
    let Some(dec_field) = this.get_internal_field(scope, 0) else { return v8::Intercepted::kNo };
    let dec = unsafe { dec_field.cast::<v8::External>() }.value() as *mut DeclarationFFI;
    let dec = unsafe { &*dec };
    let lock = dec.read();
    let kind = lock.kind();

    let Some(store_field) = this.get_internal_field(scope, 1) else { return v8::Intercepted::kNo };
    let store = unsafe { store_field.cast::<v8::Map>() };

    let name = key.to_rust_string_lossy(scope);

    // Returns true if `name` is a name reserved by the WinRT metadata for this
    // declaration kind. Reserved names are read-only.
    let is_reserved = match kind {
        DeclarationKind::Namespace => lock
            .as_any()
            .downcast_ref::<NamespaceDeclaration>()
            .map(|d| d.children().contains(&name))
            .unwrap_or(false),
        DeclarationKind::Enum => lock
            .as_any()
            .downcast_ref::<EnumDeclaration>()
            .map(|d| d.enum_for_name(&name).is_some())
            .unwrap_or(false),
        DeclarationKind::Class => lock
            .as_any()
            .downcast_ref::<ClassDeclaration>()
            .map(|d| class_has_member_named(d, &name))
            .unwrap_or(false),
        DeclarationKind::Interface => lock
            .as_any()
            .downcast_ref::<InterfaceDeclaration>()
            .map(|d| {
                d.methods().iter().any(|m| m.name() == name)
                    || d.properties().iter().any(|p| p.name() == name)
            })
            .unwrap_or(false),
        DeclarationKind::Struct => lock
            .as_any()
            .downcast_ref::<StructDeclaration>()
            .map(|d| d.fields().iter().any(|f| f.name() == name))
            .unwrap_or(false),
        // For everything else there's no metadata to clash with, so the
        // assignment can be stored verbatim.
        _ => false,
    };

    // For class instances: wire WinRT event handlers via the add/remove ABI methods.
    // `button.Click = delegate` calls add_Click; replacing removes the prior handler first.
    if kind == DeclarationKind::Class && !is_reserved {
        if let Some(class) = lock.as_any().downcast_ref::<ClassDeclaration>() {
            if let Some((add_method, remove_method)) = find_event_methods(class, &name) {
                let instance = dec.instance.clone();
                drop(lock);

                // Remove existing handler if there is one.
                let token_key = format!("__tok_{}__", name);
                if let Some(tok_key_str) = v8::String::new(scope, &token_key) {
                    if let Some(tok_val) = store.get(scope, tok_key_str.into()) {
                        if let Ok(tok_ext) = v8::Local::<v8::External>::try_from(tok_val) {
                            let token = tok_ext.value() as i64;
                            if let Some(instance) = instance.clone() {
                                let mut mc = MethodCall::new(
                                    &remove_method, remove_method.is_sealed(), instance, false,
                                );
                                mc.call_with_event_token(token);
                            }
                            // Clear the stored token.
                            let undef = v8::undefined(scope);
                            store.set(scope, tok_key_str.into(), undef.into());
                        }
                    }
                }

                // Register the new delegate.
                //
                // Path A — explicit `{ handle: External }` object from a delegate constructor
                //           or `__nsAsDelegate(typeName, fn)`: extract the raw pointer directly.
                // Path B — plain JS function: auto-derive the delegate type from the add_method's
                //           first parameter and wrap on the fly (correct parameterized IID).
                let effective_ptr: Option<*mut c_void> = if value.is_object() {
                    value.to_object(scope).and_then(|obj| {
                        let key = v8::String::new(scope, "handle")?;
                        let handle_val = obj.get(scope, key.into())?;
                        v8::Local::<v8::External>::try_from(handle_val).ok().map(|ext| ext.value())
                    })
                } else if let Ok(func) = v8::Local::<v8::Function>::try_from(value) {
                    delegate_info_from_add_method(&add_method).map(|(guid, param_types)| {
                        let data = Box::new(JsDelegateData {
                            js_func: v8::Global::new(scope, func),
                            param_types,
                        });
                        let delegate = Box::new(JsDelegate {
                            vtable:    &JS_DELEGATE_VTBL as *const _,
                            ref_count: AtomicU32::new(1),
                            guid,
                            data:      Box::into_raw(data),
                        });
                        Box::into_raw(delegate) as *mut c_void
                    })
                } else {
                    None
                };

                if let Some(delegate_ptr) = effective_ptr {
                    if let Some(instance) = instance {
                        let mut mc = MethodCall::new(
                            &add_method, add_method.is_sealed(), instance, false,
                        );
                        let (_, token) = mc.call_with_raw_ptr(delegate_ptr);
                        if let Some(tok_key_str) = v8::String::new(scope, &token_key) {
                            let tok_ptr = token as *mut c_void;
                            store.set(scope, tok_key_str.into(), v8::External::new(scope, tok_ptr).into());
                        }
                    }
                }

                store.set(scope, key.into(), value);
                return v8::Intercepted::kYes;
            }
        }
    }

    if !is_reserved {
        store.set(scope, key.into(), value);
        v8::Intercepted::kYes
    } else {
        v8::Intercepted::kNo
    }
}

fn handle_named_property_query(_scope: &mut v8::PinScope<'_, '_>,
                               _key: v8::Local<v8::Name>,
                               _args: v8::PropertyCallbackArguments,
                               mut rv: v8::ReturnValue<v8::Integer>) -> v8::Intercepted {
    // NONE
    rv.set_int32(0);
    v8::Intercepted::kNo
}

fn handle_named_property_getter(scope: &mut v8::PinScope<'_, '_>,
                                key: v8::Local<v8::Name>,
                                args: v8::PropertyCallbackArguments,
                                mut rv: v8::ReturnValue<v8::Value>) -> v8::Intercepted {
    let this = args.holder();
    let Some(dec) = this.get_internal_field(scope, 0) else { return v8::Intercepted::kNo; };
    let dec = unsafe { dec.cast::<v8::External>() };
    let dec = dec.value() as *mut DeclarationFFI;
    let dec = unsafe { &*dec };
    let lock = dec.read();
    let Some(store) = this.get_internal_field(scope, 1) else { return v8::Intercepted::kNo; };
    let store = unsafe { store.cast::<v8::Map>() };
    let kind = lock.kind();

    if key.is_string() {
        if let Some(cache) = store.get(scope, key.into()) {
            if !cache.is_null_or_undefined() {
                rv.set(cache);
                return v8::Intercepted::kYes;
            }
        }

        let name = match key.to_string(scope) {
            Some(s) => s.to_rust_string_lossy(scope),
            None => return v8::Intercepted::kNo,
        };
        match kind {
            DeclarationKind::Namespace => {
                let parent = dec.inner.clone();
                let dec = lock.as_any().downcast_ref::<NamespaceDeclaration>();
                if let Some(dec) = dec {
                    let full_name = format!("{}.{}", dec.full_name(), name.as_str());

                    if let Some(dec) = MetadataReader::find_by_name_or_generic(full_name.as_str()) {
                        let declaration = Arc::clone(&dec);
                        let lock = dec.read();

                        match lock.kind() {
                            DeclarationKind::Struct => {
                                let Some(struct_dec) = lock.as_any().downcast_ref::<StructDeclaration>() else { return v8::Intercepted::kNo; };
                                let name = struct_dec.name().to_string();
                                drop(lock);

                                let ret = create_ns_struct_ctor_object(name.as_str(), Arc::clone(&dec), scope);
                                let ret: Local<v8::Value> = ret.into();
                                store.set(scope, key.into(), ret);
                                rv.set(ret);
                            }
                            DeclarationKind::Class => {
                                let ret: Local<v8::Value> = create_ns_ctor_object(lock.name(), Some(parent), declaration, scope).into();
                                store.set(scope, key.into(), ret);
                                rv.set(ret);
                            }
                            DeclarationKind::Interface
                            | DeclarationKind::GenericInterface
                            | DeclarationKind::GenericInterfaceInstance
                            | DeclarationKind::Delegate
                            | DeclarationKind::GenericDelegate
                            | DeclarationKind::GenericDelegateInstance
                            | DeclarationKind::Event => {
                                let ret: Local<v8::Value> = create_ns_ctor_object(lock.name(), Some(parent), declaration, scope).into();
                                store.set(scope, key.into(), ret);
                                rv.set(ret);
                            }
                            _ => {
                                let ret: Local<v8::Value> = create_ns_object(name.as_str(), declaration, scope).into();
                                store.set(scope, key.into(), ret);
                                rv.set(ret);
                            }
                        }
                        return v8::Intercepted::kYes;
                    }

                    // Name isn't a real namespace child — let V8 fall back to
                    // Object.prototype defaults so toString/valueOf/etc. work.
                    return v8::Intercepted::kNo;
                }
            }
            DeclarationKind::Class => {
                let clazz_dec = lock.as_any().downcast_ref::<ClassDeclaration>();

                if let Some(clazz_dec) = clazz_dec {
                    if let Some(method) = find_class_method(clazz_dec, name.as_str()) {
                        let declaration = Arc::new(RwLock::new(method));

                        let declaration = Box::into_raw(Box::new(DeclarationFFI::new_with_instance(declaration, dec.instance.clone())));

                        let ext = v8::External::new(scope, declaration as _);

                        let builder = v8::Function::builder(|scope: &mut v8::PinScope<'_, '_>,
                                                             args: v8::FunctionCallbackArguments,
                                                             _retval: v8::ReturnValue| {
                            let _length = args.length();

                            let dec = unsafe { args.data().cast::<v8::External>() };

                            let dec = dec.value() as *mut DeclarationFFI;

                            let dec = unsafe { &*dec };

                            let lock = dec.read();

                            let Some(method) = lock.as_any().downcast_ref::<MethodDeclaration>() else { return; };

                            let Some(ns_instance) = dec.instance.clone() else { return; };

                            let mut method = MethodCall::new(
                                method, method.is_sealed(), ns_instance, false,
                            );

                            let (_ret, _result, _outs) = method.call(scope, &args);
                        })
                            .data(ext.into()).build(scope);


                        let Some(func) = builder else { return v8::Intercepted::kNo; };

                        let func: Local<v8::Value> = func.into();
                        store.set(scope, key.into(), func);
                        rv.set(func);
                        return v8::Intercepted::kYes;
                    }
                }
            }
            DeclarationKind::Interface
            | DeclarationKind::GenericInterface
            | DeclarationKind::GenericInterfaceInstance
            | DeclarationKind::Delegate
            | DeclarationKind::GenericDelegate
            | DeclarationKind::GenericDelegateInstance
            | DeclarationKind::Event => {
                if let Some(impl_key) = v8::String::new(scope, "__implementation__") {
                    if let Some(implementation) = store.get(scope, impl_key.into()) {
                        if let Some(implementation) = implementation.to_object(scope) {
                            if let Some(value) = implementation.get(scope, key.into()) {
                                if !value.is_null_or_undefined() {
                                    store.set(scope, key.into(), value);
                                    rv.set(value);
                                    return v8::Intercepted::kYes;
                                }
                            }
                        }
                    }
                }
            }
            DeclarationKind::Enum => {
                let dec = lock.as_any().downcast_ref::<EnumDeclaration>();
                if let Some(dec) = dec {
                    if let Some(value) = dec.enum_for_name(name.as_str()) {
                        match value.value() {
                            Value::Int32(value) => {
                                rv.set_int32(value);
                                // Store as v8::Integer (Smi) so the cached value passes
                                // v8::Local::<v8::Int32>::try_from when used as a setter arg.
                                let ret: Local<v8::Value> = v8::Integer::new(scope, value).into();
                                store.set(scope, key.into(), ret);
                                return v8::Intercepted::kYes;
                            }
                            Value::Uint32(value) => {
                                rv.set_uint32(value);
                                let ret: Local<v8::Value> = v8::Integer::new_from_unsigned(scope, value).into();
                                store.set(scope, key.into(), ret);
                                return v8::Intercepted::kYes;
                            }
                            _ => {}
                        }
                    }

                    // Name isn't an enum member (e.g. `toString`, `valueOf`).
                    // Don't intercept — let V8 fall back to Object.prototype
                    // so coercion-style operations (console.log on the enum,
                    // template-string interpolation, etc.) work.
                    return v8::Intercepted::kNo;
                }
            }
            DeclarationKind::EnumMember => {}
            DeclarationKind::Struct => {}
            DeclarationKind::StructField => {}
            DeclarationKind::Property => {}
            DeclarationKind::Method => {
                let dec = lock.as_any().downcast_ref::<ClassDeclaration>();

                if let Some(dec) = dec {
                    for method in dec.methods() {
                        let mut name = method.overload_name();
                        if name.is_empty() {
                            name = method.name();
                        }
                        // let cached_item = store.get(scope, key.into());
                        // if let Some(cache) = cached_item {
                        //     if !cache.is_null_or_undefined() {
                        //         rv.set(cache);
                        //         return;
                        //     }
                        // }

                        // let full_name = format!("{}.{}", dec.full_name(), name.as_str());
                        // if let Some(dec) = MetadataReader::find_by_name(full_name.as_str()) {
                        //     let declaration = Arc::clone(&dec);
                        //     let lock = dec.read();
                        //
                        //     match lock.kind() {
                        //         DeclarationKind::Class => {
                        //             let ret: Local<v8::Value> = create_ns_ctor_object(lock.name(), declaration, scope).into();
                        //             rv.set(ret.into());
                        //         }
                        //         _ => {
                        //             let ret: Local<v8::Value> = create_ns_object(name.as_str(), declaration, scope).into();
                        //             rv.set(ret.into());
                        //         }
                        //     }
                        //
                        //
                        //     //  store.set(scope, key.into(), ret.into());
                        //     return;
                        // }
                        //
                        // rv.set_undefined();
                        // return;
                    }
                }
            }
            DeclarationKind::Parameter => {}
        }
        // Fell through every arm without setting rv — let V8 do its default
        // lookup (returns `undefined` for missing names, which is what JS
        // expects for unknown WinRT properties).
        return v8::Intercepted::kNo;
    }

    v8::Intercepted::kNo
}


fn handle_indexed_property_setter(_scope: &mut v8::PinScope<'_, '_>,
                                  _index: u32,
                                  _value: v8::Local<v8::Value>,
                                  _args: v8::PropertyCallbackArguments,
                                  mut _rv: v8::ReturnValue<()>,
) -> v8::Intercepted {
    v8::Intercepted::kNo
}


fn handle_indexed_property_getter(_scope: &mut v8::PinScope<'_, '_>,
                                  _index: u32,
                                  _args: v8::PropertyCallbackArguments,
                                  mut _rv: v8::ReturnValue<v8::Value>) -> v8::Intercepted {
    v8::Intercepted::kNo
}


fn handle_ns_func(_scope: &mut v8::PinScope<'_, '_>,
                  _args: v8::FunctionCallbackArguments,
                  mut _retval: v8::ReturnValue) {
    // scope.throw_exception(v8::Exception::error(scope, v8::String::new("")))
}

// ── WinRT JS Delegate COM bridge ─────────────────────────────────────────────
//
// A JsDelegate wraps a `v8::Global<v8::Function>` inside a minimal COM object
// so it can be passed directly to WinRT event-add methods.  Every delegate
// type shares a single vtable; the per-instance GUID stored in the struct
// makes QueryInterface work correctly for each concrete type.

#[repr(C)]
struct JsDelegateVtbl {
    query_interface: unsafe extern "system" fn(*mut JsDelegate, *const GUID, *mut *mut c_void) -> HRESULT,
    add_ref:         unsafe extern "system" fn(*mut JsDelegate) -> u32,
    release:         unsafe extern "system" fn(*mut JsDelegate) -> u32,
    // Declared with 4 usize params so the same slot works for delegates with
    // 0–4 pointer-sized arguments.  Callers pass only what they need; extras
    // land in dead registers and are never read (guarded by param_types.len()).
    invoke:          unsafe extern "system" fn(*mut JsDelegate, usize, usize, usize, usize) -> HRESULT,
}

pub(crate) static JS_DELEGATE_VTBL: JsDelegateVtbl = JsDelegateVtbl {
    query_interface: js_delegate_query_interface,
    add_ref:         js_delegate_add_ref,
    release:         js_delegate_release,
    invoke:          js_delegate_invoke,
};

pub(crate) struct JsDelegateData {
    pub(crate) js_func:     v8::Global<v8::Function>,
    pub(crate) param_types: Vec<NativeType>,
}

#[repr(C)]
pub(crate) struct JsDelegate {
    pub(crate) vtable:    *const JsDelegateVtbl,
    pub(crate) ref_count: AtomicU32,
    pub(crate) guid:      GUID,
    pub(crate) data:      *mut JsDelegateData,
}

unsafe impl Send for JsDelegate {}
unsafe impl Sync for JsDelegate {}

unsafe extern "system" fn js_delegate_query_interface(
    this: *mut JsDelegate,
    iid:  *const GUID,
    out:  *mut *mut c_void,
) -> HRESULT {
    let d = &*this;
    if *iid == IUnknown::IID || *iid == d.guid {
        *out = this as *mut c_void;
        js_delegate_add_ref(this);
        HRESULT(0)
    } else {
        *out = std::ptr::null_mut();
        HRESULT(0x80004002u32 as i32) // E_NOINTERFACE
    }
}

unsafe extern "system" fn js_delegate_add_ref(this: *mut JsDelegate) -> u32 {
    (*this).ref_count.fetch_add(1, AtomicOrdering::Relaxed) + 1
}

unsafe extern "system" fn js_delegate_release(this: *mut JsDelegate) -> u32 {
    let prev = (*this).ref_count.fetch_sub(1, AtomicOrdering::Release);
    if prev == 1 {
        std::sync::atomic::fence(AtomicOrdering::Acquire);
        let b = Box::from_raw(this);
        drop(Box::from_raw(b.data));
        // b (JsDelegate) dropped here
    }
    prev - 1
}

unsafe extern "system" fn js_delegate_invoke(
    this: *mut JsDelegate,
    p0: usize, p1: usize, p2: usize, _p3: usize,
) -> HRESULT {
    // Wrap everything in catch_unwind so Rust panics cannot propagate through
    // the WinRT C++ caller stack (which would be UB and cause CLR FailFast).
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        js_delegate_invoke_inner(this, p0, p1, p2)
    }));
    match result {
        Ok(hr) => hr,
        Err(_) => HRESULT(0x80004005u32 as i32), // E_FAIL on panic
    }
}

fn js_delegate_invoke_inner(
    this: *mut JsDelegate,
    p0: usize, p1: usize, p2: usize,
) -> HRESULT {
    if this.is_null() { return HRESULT(0x80004005u32 as i32); }
    let data = unsafe {
        let data_ptr = (*this).data;
        if data_ptr.is_null() { return HRESULT(0x80004005u32 as i32); }
        &*data_ptr
    };

    let isolate_ptr = DELEGATE_ISOLATE_PTR.with(|c| c.get());
    if isolate_ptr.is_null() {
        return HRESULT(0x80004005u32 as i32);
    }

    let isolate: &mut v8::Isolate = unsafe { &mut *isolate_ptr };
    v8::scope!(scope, isolate);
    let ctx_global = {
        let Some(g) = scope.get_slot::<v8::Global<v8::Context>>() else {
            return HRESULT(0x80004005u32 as i32);
        };
        g.clone()
    };
    let context = v8::Local::new(scope, &ctx_global);
    let scope = &mut v8::ContextScope::new(scope, context);
    // TryCatch so JS exceptions don't escape into WinRT C++ frames.
    v8::tc_scope!(tc, scope);

    let func = v8::Local::new(tc, &data.js_func);
    let recv = v8::undefined(tc);

    let params_raw = [p0, p1, p2];
    let n = data.param_types.len().min(3);
    let mut js_args: Vec<v8::Local<v8::Value>> = Vec::with_capacity(n);

    for i in 0..n {
        let raw = params_raw[i] as *mut c_void;
        let val: v8::Local<v8::Value> = match data.param_types[i] {
            NativeType::Pointer => {
                if raw.is_null() {
                    v8::null(tc).into()
                } else {
                    // Delegate [in] parameters are borrowed COM pointers (the caller
                    // owns the ref for the duration of Invoke). Clone (AddRef) before
                    // wrapping so the proxy can outlive this stack frame.
                    let owned: IUnknown = unsafe {
                        let borrowed = std::mem::ManuallyDrop::new(IUnknown::from_raw(raw));
                        (*borrowed).clone()
                    };
                    // Try to resolve the concrete WinRT type so the JS callback
                    // receives a fully typed proxy (with property/method access)
                    // rather than an opaque plain object.
                    let proxy = (|| -> Option<v8::Local<v8::Value>> {
                        let inspectable = owned.cast::<IInspectable>().ok()?;
                        let class_name = inspectable.GetRuntimeClassName().ok()?;
                        let name_str = class_name.to_string();
                        let decl = MetadataReader::find_by_name(&name_str)?;
                        Some(create_ns_ctor_instance_object(
                            &name_str,
                            None,
                            None,
                            decl,
                            Some(owned.clone()),
                            tc,
                        ).into())
                    })();
                    proxy.unwrap_or_else(|| v8::External::new(tc, raw).into())
                }
            }
            NativeType::Bool  => v8::Boolean::new(tc, (raw as u8) != 0).into(),
            NativeType::U8    => v8::Integer::new_from_unsigned(tc, raw as u8 as u32).into(),
            NativeType::I8    => v8::Integer::new(tc, raw as i8 as i32).into(),
            NativeType::U16   => v8::Integer::new_from_unsigned(tc, raw as u16 as u32).into(),
            NativeType::I16   => v8::Integer::new(tc, raw as i16 as i32).into(),
            NativeType::U32   => v8::Integer::new_from_unsigned(tc, raw as u32).into(),
            NativeType::I32   => v8::Integer::new(tc, raw as i32).into(),
            NativeType::U64   => v8::Number::new(tc, raw as u64 as f64).into(),
            NativeType::I64   => v8::Number::new(tc, raw as i64 as f64).into(),
            _                 => v8::undefined(tc).into(),
        };
        js_args.push(val);
    }

    let _ = func.call(tc, recv.into(), &js_args);
    if tc.has_caught() {
        if let Some(ex) = tc.exception() {
            let msg = ex.to_rust_string_lossy(tc);
            store_last_js_error(msg);
        }
        tc.reset();
    }
    tc.perform_microtask_checkpoint();
    HRESULT(0)
}

/// Resolves the NativeType for a single delegate `Invoke` parameter signature.
///
/// Unlike `ffi_native_type_from_signature`, this also resolves named WinRT enum types
/// (e.g. `Windows.Foundation.AsyncStatus`) to `NativeType::U32` rather than Pointer,
/// so that `js_delegate_invoke_inner` receives them as plain integers instead of
/// trying to dereference them as COM vtable pointers.
fn ffi_type_for_delegate_param(sig: &str) -> NativeType {
    let base = crate::helpers::ffi_native_type_from_signature(sig);
    if matches!(base, NativeType::Pointer) && sig.contains('.') {
        let stripped = crate::helpers::strip_generic_suffix(sig);
        if let Some(decl) = MetadataReader::find_by_name(stripped) {
            if matches!(decl.read().kind(), DeclarationKind::Enum) {
                return NativeType::U32;
            }
        }
    }
    base
}

/// Extract the GUID and input-parameter NativeTypes from a delegate declaration.
pub(crate) fn js_delegate_params_from_declaration(
    lock: &dyn Declaration,
    kind: DeclarationKind,
) -> Option<(GUID, Vec<NativeType>)> {
    let build = |method: &MethodDeclaration, guid: GUID| -> (GUID, Vec<NativeType>) {
        let params = method
            .parameters()
            .iter()
            .filter(|p| !p.is_out())
            .filter_map(|p| {
                let sig = Signature::to_string(p.metadata()?, &p.type_());
                Some(ffi_type_for_delegate_param(&sig))
            })
            .collect();
        (guid, params)
    };

    Some(match kind {
        DeclarationKind::Delegate => {
            let d = lock.as_any().downcast_ref::<DelegateDeclaration>()?;
            build(d.invoke_method(), d.id())
        }
        DeclarationKind::GenericDelegate => {
            let d = lock.as_any().downcast_ref::<GenericDelegateDeclaration>()?;
            build(d.invoke_method(), d.id())
        }
        DeclarationKind::GenericDelegateInstance => {
            let d = lock.as_any().downcast_ref::<GenericDelegateInstanceDeclaration>()?;
            build(d.invoke_method(), d.id())
        }
        _ => return None,
    })
}

/// Resolves a delegate type's GUID and input-parameter NativeTypes from an IID-name string.
///
/// `iid_name` must be the full IID-form name, e.g.:
///   - `"RoutedEventHandler"` (non-generic)
///   - `"EventHandler\`1<SuspendingEventArgs>"` (closed generic instance)
///
/// Used by `delegate_info_from_add_method` and by property/method setters that
/// auto-wrap raw JS functions as `JsDelegate` COM objects.
pub(crate) fn delegate_info_from_type_sig(iid_name: &str) -> Option<(GUID, Vec<NativeType>)> {
    if let Some(open_name) = iid_name.split_once('<').map(|(prefix, _)| prefix) {
        // Generic delegate instance — compute the parameterized GUID.
        let guid = GenericInstanceIdBuilder::generate_id_from_name(iid_name);

        // Derive param_types from the open-generic delegate's Invoke signature.
        let open_decl = MetadataReader::find_by_name(open_name)?;
        let open_lock = open_decl.read();
        let open_delegate = open_lock.as_any().downcast_ref::<GenericDelegateDeclaration>()?;
        let invoke = open_delegate.invoke_method();
        let param_types = invoke
            .parameters()
            .iter()
            .filter(|p| !p.is_out())
            .filter_map(|p| {
                let sig_str = Signature::to_string(p.metadata()?, &p.type_());
                Some(ffi_type_for_delegate_param(&sig_str))
            })
            .collect();
        Some((guid, param_types))
    } else {
        // Non-generic delegate — look up by the exact type name.
        let decl = MetadataReader::find_by_name(iid_name)?;
        let lock = decl.read();
        let kind = lock.kind();
        js_delegate_params_from_declaration(&*lock, kind)
    }
}

/// Derives the delegate (GUID, param_types) expected by a WinRT event's `add_*`
/// method from the method's first parameter type.
pub(crate) fn delegate_info_from_add_method(add_method: &MethodDeclaration) -> Option<(GUID, Vec<NativeType>)> {
    let params = add_method.parameters();
    let param = params.first()?;
    let metadata = param.metadata()?;
    let sig = param.type_();

    // `to_iid_string` preserves the backtick+arity required by GenericInstanceIdBuilder
    // and by MetadataReader for open-generic lookup (e.g. "EventHandler`1<...>").
    let iid_name = Signature::to_iid_string(metadata, &sig);
    if iid_name.is_empty() {
        return None;
    }
    delegate_info_from_type_sig(&iid_name)
}

/// JS-callable handler for `__nsAsDelegate(typeName, fn)`.
///
/// Looks up `typeName` in the WinRT metadata, derives the delegate's GUID and
/// input-parameter NativeTypes via `js_delegate_params_from_declaration`, then
/// allocates a `JsDelegate` COM object and returns `{ handle: External }` —
/// the same shape that WinRT event-add methods expect.
pub(crate) fn handle_as_delegate(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 2 {
        throw_js_error(scope, "__nsAsDelegate(typeName, fn): expected 2 arguments");
        return;
    }
    let Some(name_v8) = args.get(0).to_string(scope) else {
        throw_js_error(scope, "__nsAsDelegate: first argument must be a string");
        return;
    };
    let type_name = name_v8.to_rust_string_lossy(scope);

    let Ok(func) = v8::Local::<v8::Function>::try_from(args.get(1)) else {
        throw_js_error(scope, "__nsAsDelegate: second argument must be a function");
        return;
    };

    let Some(declaration) = MetadataReader::find_by_name(&type_name) else {
        throw_js_error(scope, &format!("Type not found in WinRT metadata: {}", type_name));
        return;
    };
    let lock = declaration.read();
    let kind = lock.kind();

    let Some((guid, param_types)) = js_delegate_params_from_declaration(&*lock, kind) else {
        throw_js_error(scope, &format!("{} is not a WinRT delegate type", type_name));
        return;
    };

    let data = Box::new(JsDelegateData { js_func: v8::Global::new(scope, func), param_types });
    let delegate = Box::new(JsDelegate {
        vtable:    &JS_DELEGATE_VTBL as *const _,
        ref_count: AtomicU32::new(1),
        guid,
        data:      Box::into_raw(data),
    });
    let raw = Box::into_raw(delegate) as *mut c_void;

    let result_obj = v8::Object::new(scope);
    if let Some(key) = v8::String::new(scope, "handle") {
        result_obj.set(scope, key.into(), v8::External::new(scope, raw).into());
    }
    retval.set(result_obj.into());
}

impl Runtime {
    pub fn new(app_root: &str) -> Self {
        INIT.call_once(|| {
            // --expose-gc makes gc() available as a global JS function so callers
            // can trigger a full GC sweep (useful for debugging and test harnesses).
            v8::V8::set_flags_from_string("--expose-gc");
            let platform = v8::new_default_platform(0, false).make_shared();
            v8::V8::initialize_platform(platform);
            v8::V8::initialize();
        });

        let winrt_initialized = match unsafe { RoInitialize(RO_INIT_SINGLETHREADED) } {
            Ok(_) => true,
            // Any failure (including RPC_E_CHANGED_MODE=0x80010106 and the ASTA apartment
            // model used by UWP/XAML hosts) means WinRT was already initialized externally.
            // Skip RoUninitialize on drop rather than panicking.
            Err(_) => false,
        };

        // Create the message-only HWND for native UI-thread dispatch. Must run
        // on the UI thread (here, in Runtime::new) before any cross-thread posts.
        crate::ui_dispatcher::init_ui_dispatcher();

        let params = v8::CreateParams::default();
        let mut isolate = v8::Isolate::new(params);
        isolate.set_capture_stack_trace_for_uncaught_exceptions(true, 100);

        // Provide a host callback for dynamic `import()` so embedders and
        // tests that use `import(modulePath)` work. The callback compiles
        // the requested module (and its transitive graph), instantiates
        // and evaluates it, then resolves the returned Promise with the
        // module namespace object.
        isolate.set_host_import_module_dynamically_callback(
            |scope: &mut v8::PinScope<'_, '_>, _host_defined_options: v8::Local<v8::Data>, resource_name: v8::Local<v8::Value>, specifier: v8::Local<v8::String>, _import_attributes: v8::Local<v8::FixedArray>| -> Option<v8::Local<v8::Promise>> {
                // Create a promise resolver to return to JS.
                let resolver = match v8::PromiseResolver::new(scope) {
                    Some(r) => r,
                    None => return None,
                };

                let spec = specifier.to_rust_string_lossy(scope);
                let referrer_path = value_to_string(scope, resource_name);
                let resolved = resolve_esm_path(&spec, referrer_path.as_deref());

                match std::fs::read_to_string(&resolved) {
                    Ok(content) => compile_module_graph(scope, &content, &resolved),
                    Err(e) => {
                        if let Some(err_str) = v8::String::new(scope, &format!("ESM: cannot read {resolved}: {e}")) {
                            resolver.reject(scope, err_str.into());
                        }
                        return Some(resolver.get_promise(scope));
                    }
                }

                let root_global = ESM_MODULE_REGISTRY.with(|r| r.borrow().get(&resolved).cloned());
                let Some(root_global) = root_global else {
                    if let Some(err_str) = v8::String::new(scope, "ESM: root module was not compiled") {
                        resolver.reject(scope, err_str.into());
                    }
                    return Some(resolver.get_promise(scope));
                };

                let module = v8::Local::new(scope, &root_global);

                if module.instantiate_module(scope, resolve_module_callback).is_none() {
                    if let Some(err_str) = v8::String::new(scope, "ESM: module instantiation failed") {
                        resolver.reject(scope, err_str.into());
                    }
                    return Some(resolver.get_promise(scope));
                }

                if module.evaluate(scope).is_none() {
                    if let Some(err_str) = v8::String::new(scope, "ESM: module evaluation failed") {
                        resolver.reject(scope, err_str.into());
                    }
                    return Some(resolver.get_promise(scope));
                }

                scope.perform_microtask_checkpoint();

                let ns = module.get_module_namespace();
                resolver.resolve(scope, ns);
                Some(resolver.get_promise(scope))
            },
        );

        let global_context;
        {
            v8::scope!(scope, &mut isolate);

            let mut global_template = v8::ObjectTemplate::new(scope);

            globals::performance::init_performance(scope, &mut global_template);
            globals::time::init_time(scope, &mut global_template);

            let context = v8::Context::new(
                scope,
                v8::ContextOptions {
                    global_template: Some(global_template),
                    ..Default::default()
                },
            );
            {
                let scope = &mut v8::ContextScope::new(scope, context);
                preload_sbg_manifest();
                init_global(scope, context);
                globals::console::init_console(scope, context);
                init_meta(scope, context);
                crate::global_fns::init_async_helpers(scope, app_root);
                crate::win32::prewarm_known_fns();
                global_context = v8::Global::new(scope, context);
            }
        }

        // Store the global context in the isolate slot so the JS delegate
        // Invoke trampoline can enter V8 without an existing scope on the stack.
        {
            let ctx_clone = global_context.clone();
            isolate.set_slot(ctx_clone);
        }
        // Don't capture `&mut *isolate as *mut v8::Isolate` here: the wrapper's
        // address moves with the `OwnedIsolate` into `Self`, dangling our pointer.
        // The caller must invoke `register_delegate_isolate_ptr` after boxing.

        Self {
            isolate,
            global_context,
            app_root: app_root.to_string(),
            winrt_initialized,
        }
    }

    /// Must be called after the Runtime is at a stable address (e.g. boxed),
    /// so the captured isolate pointer doesn't dangle.
    pub fn register_delegate_isolate_ptr(&mut self) {
        let raw_isolate: *mut v8::Isolate = &mut *self.isolate as *mut v8::Isolate;
        DELEGATE_ISOLATE_PTR.with(|cell| cell.set(raw_isolate));
    }

    /// Provides mutable access to the underlying V8 isolate.
    /// Used by the devtools integration to attach a `V8Inspector`.
    pub fn isolate_mut(&mut self) -> &mut v8::Isolate {
        &mut self.isolate
    }

    /// Returns the persistent context handle.
    /// Used by the devtools integration to register the context with the inspector.
    pub fn global_context(&self) -> &v8::Global<v8::Context> {
        &self.global_context
    }

    pub fn run_module(&mut self, script: &str, filename: &str) {
        v8::scope!(scope, &mut self.isolate);
        let context = v8::Local::new(scope, &self.global_context);
        let scope = &mut v8::ContextScope::new(scope, context);
        v8::tc_scope!(tc, scope);

        let resolved_path = {
            let p = normalize_js_path(filename);
            let p = try_resolve_with_known_extensions(p);
            p.canonicalize().unwrap_or(p).to_string_lossy().into_owned()
        };

        macro_rules! check_exception {
            ($tc:ident) => {
                if $tc.has_caught() {
                    let mut error_report = String::new();
                    if let Some(msg) = $tc.message() {
                        let text = msg.get($tc).to_rust_string_lossy($tc);
                        let line = msg.get_line_number($tc).unwrap_or(0);
                        let file_name = msg.get_script_resource_name($tc)
                            .map(|v| v.to_rust_string_lossy($tc))
                            .unwrap_or_else(|| "<unknown>".to_string());
                        error_report.push_str(&format!("{} ({}:{})\n", text, file_name, line));
                        if let Some(stack) = msg.get_stack_trace($tc) {
                            for i in 0..stack.get_frame_count() {
                                if let Some(frame) = stack.get_frame($tc, i) {
                                    let fn_name = frame.get_function_name($tc)
                                        .map(|s| s.to_rust_string_lossy($tc))
                                        .unwrap_or_else(|| "<anonymous>".to_string());
                                    let file = frame.get_script_name($tc)
                                        .map(|s| s.to_rust_string_lossy($tc))
                                        .unwrap_or_else(|| "<unknown>".to_string());
                                    let line_str = format!("    at {} ({}:{}:{})\n", fn_name, file,
                                        frame.get_line_number(), frame.get_column());
                                    error_report.push_str(&line_str);
                                }
                            }
                        }
                    } else if let Some(exc) = $tc.exception() {
                        let text = exc.to_rust_string_lossy($tc);
                        error_report.push_str(&text);
                    }
                    if !error_report.is_empty() {
                        crate::store_last_js_error(error_report);
                    }
                    return;
                }
            };
        }

        compile_module_graph(tc, script, &resolved_path);
        check_exception!(tc);

        let root_global = ESM_MODULE_REGISTRY.with(|r| r.borrow().get(&resolved_path).cloned());
        let Some(root_global) = root_global else {
            crate::store_last_js_error("ESM: root module was not compiled".to_string());
            return;
        };
        let module = v8::Local::new(tc, &root_global);

        if module.instantiate_module(tc, resolve_module_callback).is_none() {
            check_exception!(tc);
            return;
        }

        if module.evaluate(tc).is_none() {
            check_exception!(tc);
            return;
        }

        check_exception!(tc);
        tc.perform_microtask_checkpoint();
    }

    pub fn run_script(&mut self, script: &str, filename: &str) {
        // Delegate ESM bundles to the native V8 module loader.
        let is_esm = filename.ends_with(".mjs")
            || {
                let trimmed = script.trim_start();
                trimmed.starts_with("import ") || trimmed.starts_with("export ")
            };
        if is_esm {
            self.run_module(script, filename);
            return;
        }

        v8::scope!(scope, &mut self.isolate);
        let context = v8::Local::new(scope, &self.global_context);
        let scope = &mut v8::ContextScope::new(scope, context);
        v8::tc_scope!(tc, scope);

        let Some(code) = v8::String::new(tc, script) else { return };
        let origin = v8::String::new(tc, filename).map(|name| {
            v8::ScriptOrigin::new(tc, name.into(), 0, 0, false, -1, None, false, false, false, None)
        });
        if let Some(compiled) = v8::Script::compile(tc, code, origin.as_ref()) {
            compiled.run(tc);
        }

        // Log any uncaught JS exception. Do NOT call tc.rethrow() here: run_script is
        // invoked from FFI (runtime_runscript) where there is no outer V8 TryCatch.
        // Rethrowing into an empty V8 scope causes V8 to call its fatal-error handler
        // → abort() → System.ExecutionEngineException in the CLR.
        if tc.has_caught() {
            let mut error_report = String::new();
            if let Some(msg) = tc.message() {
                let text = msg.get(tc).to_rust_string_lossy(tc);
                let line = msg.get_line_number(tc).unwrap_or(0);
                let file_name = msg.get_script_resource_name(tc)
                    .map(|v| v.to_rust_string_lossy(tc))
                    .unwrap_or_else(|| "<unknown>".to_string());
                error_report.push_str(&format!("{} ({}:{})\n", text, file_name, line));
                if let Some(stack) = msg.get_stack_trace(tc) {
                    for i in 0..stack.get_frame_count() {
                        if let Some(frame) = stack.get_frame(tc, i) {
                            let fn_name = frame.get_function_name(tc)
                                .map(|s| s.to_rust_string_lossy(tc))
                                .unwrap_or_else(|| "<anonymous>".to_string());
                            let file = frame.get_script_name(tc)
                                .map(|s| s.to_rust_string_lossy(tc))
                                .unwrap_or_else(|| "<unknown>".to_string());
                            let line_str = format!("    at {} ({}:{}:{})\n", fn_name, file,
                                frame.get_line_number(), frame.get_column());
                            error_report.push_str(&line_str);
                        }
                    }
                }
            } else if let Some(exc) = tc.exception() {
                let text = exc.to_rust_string_lossy(tc);
                error_report.push_str(&text);
            }
            if !error_report.is_empty() {
                store_last_js_error(error_report);
            }
            return;
        }

        // Drain any synchronous Promise microtasks that the script may have queued.
        tc.perform_microtask_checkpoint();
    }

    pub fn eval_script_to_string(&mut self, script: &str) -> Option<String> {
        v8::scope!(scope, &mut self.isolate);
        let context = v8::Local::new(scope, &self.global_context);
        let scope = &mut v8::ContextScope::new(scope, context);
        v8::tc_scope!(tc, scope);

        let code = v8::String::new(tc, script)?;
        let compiled = v8::Script::compile(tc, code, None)?;
        let value = compiled.run(tc)?;

        if tc.has_caught() {
            if let Some(msg) = tc.message() {
                let text = msg.get(tc).to_rust_string_lossy(tc);
                let line = msg.get_line_number(tc).unwrap_or(0);
                eprintln!("[NativeScript] Worker eval exception at line {}: {}", line, text);
            }
            tc.rethrow();
            return None;
        }

        tc.perform_microtask_checkpoint();

        value
            .to_string(tc)
            .map(|s| s.to_rust_string_lossy(tc))
    }

    pub fn dispose(&self) {}
}

impl Drop for Runtime {
    fn drop(&mut self) {
        INSTANCE_CACHE.with(|cache| cache.borrow_mut().clear());
        if self.winrt_initialized {
            unsafe { RoUninitialize() };
        }
    }
}

// ── Structured-clone helpers ─────────────────────────────────────────────────

struct WorkerValueSerializer;

impl v8::ValueSerializerImpl for WorkerValueSerializer {
    fn throw_data_clone_error<'s>(
        &self,
        scope: &mut v8::PinScope<'s, '_>,
        message: v8::Local<'s, v8::String>,
    ) {
        let error = v8::Exception::error(scope, message);
        scope.throw_exception(error);
    }
}

struct WorkerValueDeserializer;

impl v8::ValueDeserializerImpl for WorkerValueDeserializer {}

impl Runtime {
    /// Serialize a single V8 value to structured-clone bytes using V8's own
    /// `ValueSerializer`.  Returns `None` if the value is not cloneable (e.g.
    /// a function or a circular object); in that case an exception has already
    /// been thrown into `scope`.
    pub fn serialize_value<'s, 'v>(
        scope: &mut v8::PinScope<'s, '_>,
        value: v8::Local<'v, v8::Value>,
    ) -> Option<Vec<u8>> {
        use v8::ValueSerializerHelper;
        let context = scope.get_current_context();
        let ser = v8::ValueSerializer::new(scope, Box::new(WorkerValueSerializer));
        ser.write_header();
        if ser.write_value(context, value).unwrap_or(false) {
            Some(ser.release())
        } else {
            None
        }
    }

    /// Deserialize structured-clone bytes produced by `serialize_value` back
    /// into a V8 value in the current context.
    pub fn deserialize_value<'s>(
        scope: &mut v8::PinScope<'s, '_>,
        bytes: &[u8],
    ) -> Option<v8::Local<'s, v8::Value>> {
        use v8::ValueDeserializerHelper;
        let context = scope.get_current_context();
        let de = v8::ValueDeserializer::new(scope, Box::new(WorkerValueDeserializer), bytes);
        if !de.read_header(context).unwrap_or(false) {
            return None;
        }
        de.read_value(context)
    }

    /// Drain `globalThis.__nsWorkerOutbox`, serialize every item with V8's
    /// structured-clone algorithm, and return the resulting byte blobs.
    pub fn drain_outbox_bytes(&mut self) -> Vec<Result<Vec<u8>, String>> {
        v8::scope!(scope, &mut self.isolate);
        let context = v8::Local::new(scope, &self.global_context);
        let scope = &mut v8::ContextScope::new(scope, context);
        v8::tc_scope!(tc, scope);

        let script_src = "(function(){var o=globalThis.__nsWorkerOutbox||[];return o.splice(0);})()";
        let Some(src) = v8::String::new(tc, script_src) else { return Vec::new() };
        let Some(script) = v8::Script::compile(tc, src, None) else { return Vec::new() };
        let Some(result) = script.run(tc) else { return Vec::new() };
        let Ok(array) = v8::Local::<v8::Array>::try_from(result) else { return Vec::new() };

        let len = array.length();
        let mut out = Vec::with_capacity(len as usize);

        for i in 0..len {
            let Some(item) = array.get_index(tc, i) else { continue };
            match Self::serialize_value(tc, item) {
                Some(bytes) => out.push(Ok(bytes)),
                None => {
                    let msg = if tc.has_caught() {
                        let s = tc.message()
                            .and_then(|m| Some(m.get(tc).to_rust_string_lossy(tc)))
                            .unwrap_or_else(|| "DataCloneError".to_string());
                        tc.reset();
                        s
                    } else {
                        "DataCloneError: value could not be cloned".to_string()
                    };
                    out.push(Err(msg));
                }
            }
        }

        out
    }

    /// Deserialize `payload_bytes` and deliver them to the worker's
    /// `__nsDispatchToWorker` JS function.
    pub fn dispatch_to_worker(&mut self, payload_bytes: &[u8]) {
        v8::scope!(scope, &mut self.isolate);
        let context = v8::Local::new(scope, &self.global_context);
        let scope = &mut v8::ContextScope::new(scope, context);
        v8::tc_scope!(tc, scope);

        let data_value = match Self::deserialize_value(tc, payload_bytes) {
            Some(v) => v,
            None => {
                eprintln!("[NativeScript] Worker dispatch: failed to deserialize message");
                return;
            }
        };

        let ctx = tc.get_current_context();
        let global = ctx.global(tc);
        let Some(fn_name) = v8::String::new(tc, "__nsDispatchToWorker") else { return };
        let Some(fn_val) = global.get(tc, fn_name.into()) else { return };
        let Ok(dispatch_fn) = v8::Local::<v8::Function>::try_from(fn_val) else { return };
        let recv: v8::Local<v8::Value> = v8::undefined(tc).into();
        dispatch_fn.call(tc, recv, &[data_value]);

        tc.perform_microtask_checkpoint();
    }
}

#[cfg(test)]
mod color_test;

#[cfg(test)]
mod error_handling_test;

#[cfg(test)]
mod instance_cache_test;

#[cfg(test)]
mod interop_test;

#[cfg(test)]
mod js_delegate_tests {
    use super::{
        JsDelegate, JS_DELEGATE_VTBL,
        js_delegate_add_ref, js_delegate_release, js_delegate_query_interface,
    };
    use std::sync::atomic::AtomicU32;
    use windows::core::{GUID, IUnknown, Interface, HRESULT};
    use std::ffi::c_void;

    /// Build a JsDelegate with a null data pointer for reference-count-only tests.
    /// Callers must ensure the delegate's ref_count never reaches 0 (which would
    /// try to free the null data pointer).
    unsafe fn make_test_delegate(guid: GUID) -> *mut JsDelegate {
        Box::into_raw(Box::new(JsDelegate {
            vtable:    &JS_DELEGATE_VTBL as *const _,
            ref_count: AtomicU32::new(1),
            guid,
            data:      std::ptr::null_mut(),
        }))
    }

    #[test]
    fn js_delegate_add_ref_increments_count() {
        unsafe {
            let ptr = make_test_delegate(GUID::zeroed());
            assert_eq!(js_delegate_add_ref(ptr), 2);
            assert_eq!(js_delegate_add_ref(ptr), 3);
            // Release back to 1 so the destructor is not triggered.
            js_delegate_release(ptr);
            js_delegate_release(ptr);
            // At ref_count=1 we stop (one more release would trigger Box::from_raw on null data).
            // Leak intentionally to avoid UB in a test-only stub.
            let _ = Box::from_raw(ptr); // free only the JsDelegate; data is null but not accessed
        }
    }

    #[test]
    fn js_delegate_release_decrements_count() {
        unsafe {
            let ptr = make_test_delegate(GUID::zeroed());
            js_delegate_add_ref(ptr);  // -> 2
            js_delegate_add_ref(ptr);  // -> 3
            assert_eq!(js_delegate_release(ptr), 2);
            assert_eq!(js_delegate_release(ptr), 1);
            // Leave at 1 and free manually.
            let _ = Box::from_raw(ptr);
        }
    }

    #[test]
    fn js_delegate_query_interface_iunknown_succeeds() {
        unsafe {
            let guid = GUID::zeroed();
            let ptr = make_test_delegate(guid);
            let mut out: *mut c_void = std::ptr::null_mut();
            let hr = js_delegate_query_interface(ptr, &IUnknown::IID, &mut out);
            assert_eq!(hr, HRESULT(0), "QI for IUnknown should return S_OK");
            assert_eq!(out, ptr as *mut c_void);
            // QI called AddRef internally; balance it.
            js_delegate_release(ptr);
            // Now ref_count is back to 1; free manually.
            let _ = Box::from_raw(ptr);
        }
    }

    #[test]
    fn js_delegate_query_interface_matching_guid_succeeds() {
        let test_guid = GUID {
            data1: 0xDEAD_BEEF,
            data2: 0xCAFE,
            data3: 0xBABE,
            data4: [1, 2, 3, 4, 5, 6, 7, 8],
        };
        unsafe {
            let ptr = make_test_delegate(test_guid);
            let mut out: *mut c_void = std::ptr::null_mut();
            let hr = js_delegate_query_interface(ptr, &test_guid, &mut out);
            assert_eq!(hr, HRESULT(0), "QI for the delegate's own GUID should return S_OK");
            assert_eq!(out, ptr as *mut c_void);
            js_delegate_release(ptr);
            let _ = Box::from_raw(ptr);
        }
    }

    #[test]
    fn js_delegate_query_interface_unknown_guid_returns_e_nointerface() {
        let other_guid = GUID {
            data1: 0x1111_1111,
            data2: 0x2222,
            data3: 0x3333,
            data4: [0; 8],
        };
        unsafe {
            let ptr = make_test_delegate(GUID::zeroed());
            let mut out: *mut c_void = std::ptr::null_mut();
            let hr = js_delegate_query_interface(ptr, &other_guid, &mut out);
            assert_eq!(
                hr,
                HRESULT(0x80004002u32 as i32),
                "QI for unknown GUID should return E_NOINTERFACE"
            );
            assert!(out.is_null(), "out pointer should be null on E_NOINTERFACE");
            let _ = Box::from_raw(ptr);
        }
    }
}
