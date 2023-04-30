mod console;

use std::any::Any;
use std::ffi::c_void;
use std::ops::Deref;
use std::sync::{Arc, Once};
use parking_lot::{RawRwLock, RwLock};
use parking_lot::lock_api::{MappedRwLockReadGuard, MappedRwLockWriteGuard, RwLockReadGuard, RwLockWriteGuard};
use v8::{Local, Object};
use metadata::declarations::declaration;
use metadata::declarations::declaration::{
    DeclarationKind,
    Declaration,
};
use metadata::declarations::enum_declaration::EnumDeclaration;
use metadata::declarations::namespace_declaration::NamespaceDeclaration;
use metadata::declarations::type_declaration::TypeDeclaration;
use metadata::meta_data_reader::MetadataReader;
use metadata::value::Value;

pub struct Runtime {
    isolate: v8::OwnedIsolate,
    global_context: v8::Global<v8::Context>,
    app_root: String,
}

static INIT: Once = Once::new();

struct DeclarationFFI {
    inner: Arc<RwLock<dyn Declaration>>
}

impl DeclarationFFI {
    pub fn new (declaration: Arc<RwLock<dyn Declaration>>) -> Self{
        Self {inner: declaration}
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

fn handle_global(scope: &mut v8::HandleScope,
                 args: v8::FunctionCallbackArguments,
                 mut _retval: v8::ReturnValue) {}

fn init_global(scope: &mut v8::ContextScope<v8::HandleScope<v8::Context>>, context: v8::Local<v8::Context>) {
    let mut global = context.global(scope);
    let value = v8::String::new(
        scope, "global",
    ).unwrap().into();
    global.define_own_property(scope, value, global.into(), v8::READ_ONLY);
    // let interop = Box::new(COMInterop::new());
    // let interop = Box::into_raw(interop);
    // let value = v8::External::new(scope, interop as *mut c_void);
    // global.set_internal_field(0, value.into());
}

fn init_console(scope: &mut v8::ContextScope<v8::HandleScope<v8::Context>>, context: v8::Local<v8::Context>) {
    let console = v8::Object::new(scope);
    let log = v8::Function::new(scope, console::handle_console_log).unwrap();
    let dir = v8::Function::new(scope, console::handle_console_dir).unwrap();

    let name = v8::String::new(scope, "log").unwrap().into();
    console.set(
        scope,
        name,
        log.into(),
    );

    let name = v8::String::new(scope, "dir").unwrap().into();
    console.set(
        scope,
        name,
        dir.into(),
    );

    let mut global = context.global(scope);
    let value = v8::String::new(
        scope, "console",
    ).unwrap().into();
    global.define_own_property(scope, value, console.into(), v8::READ_ONLY);
}

fn handle_time(scope: &mut v8::HandleScope,
               _args: v8::FunctionCallbackArguments,
               mut retval: v8::ReturnValue) {
    let now = chrono::Utc::now();
    retval.set_double((now.timestamp_millis() / 1000000) as f64);
}

fn init_time(scope: &mut v8::HandleScope<()>, global: &mut v8::Local<v8::ObjectTemplate>) {
    let time = v8::FunctionTemplate::new(scope, handle_time);
    global.set(
        v8::String::new(scope, "time").unwrap().into(), time.into(),
    );
}

fn init_performance(scope: &mut v8::HandleScope<()>, global: &mut v8::Local<v8::ObjectTemplate>) {
    let performance = v8::ObjectTemplate::new(scope);
    let now = v8::FunctionTemplate::new(scope, handle_now);
    performance.set(
        v8::String::new(scope, "now").unwrap().into(),
        now.into(),
    );
    global.set(
        v8::String::new(scope, "performance").unwrap().into(),
        performance.into(),
    );
}

fn handle_now(scope: &mut v8::HandleScope,
              _args: v8::FunctionCallbackArguments,
              mut retval: v8::ReturnValue) {
    let now = chrono::Utc::now();
    retval.set_double(
        now.timestamp_nanos() as f64
    )
}

fn create_ns_object<'a>(name:&str, declaration: Arc<RwLock<dyn Declaration>>, scope: &mut v8::HandleScope<'a>) -> Local<'a, v8::Value> {
    let scope = &mut v8::EscapableHandleScope::new(scope);
    let name = v8::String::new(scope, name).unwrap();
    let tmpl = v8::FunctionTemplate::new(scope, handle_ns_func);
    tmpl.set_class_name(name);
    let object_tmpl = tmpl.instance_template(scope);
    object_tmpl.set_named_property_handler(
        v8::NamedPropertyHandlerConfiguration::new()
            .getter(handle_ns_meta_getter)
            .setter(handle_ns_meta_setter)
    );
    object_tmpl.set_internal_field_count(2);
    let object = object_tmpl.new_instance(scope).unwrap();
    let declaration = Box::new(DeclarationFFI::new(declaration));
    let ext = v8::External::new(scope, Box::into_raw(declaration) as _);
    object.set_internal_field(0, ext.into());

    let object_store = v8::Map::new(scope);
    object.set_internal_field(1, object_store.into());

    let ret = scope.escape(object);

    ret.into()
}

fn init_meta(scope: &mut v8::ContextScope<v8::HandleScope<v8::Context>>, context: Local<v8::Context>) {
    let mut global = context.global(scope);
    let global_metadata = MetadataReader::find_by_name("").unwrap();
    let data = global_metadata.read();
    let ns = data.as_any().downcast_ref::<NamespaceDeclaration>();
    if let Some(global_namespaces) = ns {
        let full_name = global_namespaces.full_name();
        for ns in global_namespaces.children() {
            let full_name = if full_name.is_empty() {
                ns.to_string()
            }else {
                format!("{}.{}", full_name, ns)
            };

            let name: Local<v8::Name> = v8::String::new(scope, ns.as_str()).unwrap().into();
            if let Some(name_space) = MetadataReader::find_by_name(full_name.as_str()){

                let object = create_ns_object(ns, name_space, scope);

                global.define_own_property(scope, name, object, v8::READ_ONLY | v8::DONT_DELETE);
            }
        }
    }
}

fn handle_ns_meta_setter(scope: &mut v8::HandleScope,
                         key: v8::Local<v8::Name>,
                         value: v8::Local<v8::Value>,
                         args: v8::PropertyCallbackArguments) {
    let this = args.holder();
    let dec = this.get_internal_field(scope, 0).unwrap();
    let dec = unsafe { Local::<v8::External>::cast(dec) };
    let dec = dec.value() as *mut Box<dyn Declaration>;
    let dec = unsafe { &mut *dec };
    let kind = dec.kind();
    let store = this.get_internal_field(scope, 1).unwrap();
    let store = unsafe { Local::<v8::Map>::cast(store) };
    let name = key.to_rust_string_lossy(scope);
    match kind {
        DeclarationKind::Namespace => {
            let dec = unsafe { (*dec).as_any().downcast_ref::<NamespaceDeclaration>() };
            if let Some(dec) = dec {
                if !dec.children().contains(&name) {
                    store.set(scope, key.into(), value);
                }
            }
        }
        DeclarationKind::Class => {}
        DeclarationKind::Interface => {}
        DeclarationKind::GenericInterface => {}
        DeclarationKind::GenericInterfaceInstance => {}
        DeclarationKind::Enum => {
            let dec = unsafe { (*dec).as_any().downcast_ref::<EnumDeclaration>() };
            if let Some(dec) = dec {
                if dec.enum_for_name(&name).is_none() {
                    store.set(scope, key.into(), value);
                }
            }
        }
        DeclarationKind::EnumMember => {}
        DeclarationKind::Struct => {}
        DeclarationKind::StructField => {}
        DeclarationKind::Delegate => {}
        DeclarationKind::GenericDelegate => {}
        DeclarationKind::GenericDelegateInstance => {}
        DeclarationKind::Event => {}
        DeclarationKind::Property => {}
        DeclarationKind::Method => {}
        DeclarationKind::Parameter => {}
    }
}

fn handle_ns_meta_getter(scope: &mut v8::HandleScope,
                         key: v8::Local<v8::Name>,
                         args: v8::PropertyCallbackArguments,
                         mut rv: v8::ReturnValue) {
    let this = args.this();
    let dec = this.get_internal_field(scope, 0).unwrap();
    let dec = unsafe { Local::<v8::External>::cast(dec) };
    let dec = dec.value() as *mut DeclarationFFI;
    let dec = unsafe{&*dec};
    let lock = dec.read();
    let store = this.get_internal_field(scope, 1).unwrap();
    let store = unsafe { Local::<v8::Map>::cast(store) };
    let kind = lock.kind();
    if key.is_string() {
        let name = key.to_string(scope).unwrap().to_rust_string_lossy(scope);
        match kind {
            DeclarationKind::Namespace => {
                let dec = lock.as_any().downcast_ref::<NamespaceDeclaration>();
                if let Some(dec) = dec {
                    let cached_item = store.get(scope, key.into());
                    if let Some(cache) = cached_item {
                        if !cache.is_null_or_undefined() {
                            rv.set(cache);
                            return
                        }
                    }

                    let full_name = format!("{}.{}", dec.full_name(), name.as_str());
                    if let Some(declaration) = MetadataReader::find_by_name(full_name.as_str()){
                        let ret: Local<v8::Value> = create_ns_object(name.as_str(), declaration, scope).into();
                        rv.set(ret.into());
                        store.set(scope, key.into(), ret.into());
                        return;
                    }

                    rv.set_undefined();
                    return;
                }
            }
            DeclarationKind::Class => {}
            DeclarationKind::Interface => {}
            DeclarationKind::GenericInterface => {}
            DeclarationKind::GenericInterfaceInstance => {}
            DeclarationKind::Enum => {
                let dec = lock.as_any().downcast_ref::<EnumDeclaration>();
                if let Some(dec) = dec {
                    let cached_item = store.get(scope, key.into());
                    if let Some(cache) = cached_item {
                        if !cache.is_null_or_undefined() {
                            rv.set(cache);
                            return
                        }
                    }

                    if let Some(value) = dec.enum_for_name(name.as_str()) {
                        match value.value() {
                            Value::Int32(value) => {
                                rv.set_int32(value);
                                let ret = v8::Number::new(scope, value as f64).into();
                                store.set(scope, key.into(), ret);
                                return;
                            }
                            Value::Uint32(value) => {
                                rv.set_uint32(value);
                                let ret = v8::Number::new(scope, value as f64).into();
                                store.set(scope, key.into(), ret);
                                return;
                            }
                            _ =>{}
                        }
                    }

                    rv.set_undefined();
                    return
                }
            }
            DeclarationKind::EnumMember => {}
            DeclarationKind::Struct => {}
            DeclarationKind::StructField => {}
            DeclarationKind::Delegate => {}
            DeclarationKind::GenericDelegate => {}
            DeclarationKind::GenericDelegateInstance => {}
            DeclarationKind::Event => {}
            DeclarationKind::Property => {}
            DeclarationKind::Method => {}
            DeclarationKind::Parameter => {}
        }
        return;
    }


    rv.set(args.holder().into());
}


fn handle_ns_func(scope: &mut v8::HandleScope,
                  _args: v8::FunctionCallbackArguments,
                  mut _retval: v8::ReturnValue) {
    // scope.throw_exception(v8::Exception::error(scope, v8::String::new("")))
}

fn handle_meta(scope: &mut v8::HandleScope,
               args: v8::FunctionCallbackArguments,
               mut retval: v8::ReturnValue) {
    // let isolate = scope.get_isolate_mut();
    // let class_name = args.get(0).to_rust_string_lossy(scope);
    // let global = scope.get_current_context().global(scope);
    // match global.get_internal_field(scope, 0) {
    //     None => {}
    //     Some(value) => {
    //         let interop: v8::External = value.try_into().unwrap();
    //         let interop = interop.value() as *mut COMInterop;
    //     }
    // }
}

impl Runtime {
    pub fn new(app_root: &str) -> Self {
        INIT.call_once(|| {
            let platform = v8::Platform::new(0, false).make_shared();
            v8::V8::initialize_platform(platform);
            v8::V8::initialize();
        });
        let params = v8::CreateParams::default();
        let mut isolate = v8::Isolate::new(params);
        isolate.set_capture_stack_trace_for_uncaught_exceptions(true, 100);

        let global_context;

        {
            let scope = &mut v8::HandleScope::new(&mut isolate);
            let global = v8::FunctionTemplate::new(scope, handle_global);

            let class_name = v8::String::new(scope, "NativeScriptGlobalObject").unwrap();
            global.set_class_name(class_name);

            let mut global_template = v8::ObjectTemplate::new_from_template(scope, global);

            global_template.set_internal_field_count(1);

            {
                let template = &mut global_template;

                init_performance(scope, template);

                init_time(scope, template);

                let context = v8::Context::new_from_template(scope, global_template);
                {
                    let scope = &mut v8::ContextScope::new(scope, context);

                    init_global(scope, context);
                    init_console(scope, context);
                    init_meta(scope, context);
                    global_context = v8::Global::new(scope, context);
                }
            }
        }

        Self {
            isolate,
            global_context,
            app_root: app_root.to_string(),
        }
    }

    pub fn run_script(&mut self, script: &str) {
        let isolate = &mut self.isolate;
        let scope = &mut v8::HandleScope::new(isolate);
        let context = v8::Local::new(scope, &self.global_context);
        let scope = &mut v8::ContextScope::new(scope, context);
        let code = v8::String::new(scope, script).unwrap();
        let script = v8::Script::compile(scope, code, None).unwrap();
        let _ = script.run(scope);
    }
}