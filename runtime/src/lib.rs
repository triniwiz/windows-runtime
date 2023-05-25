mod console;
mod converter;
mod value;
mod interop;

use std::any::Any;
use std::ffi::{c_char, c_void, CString};
use std::mem::MaybeUninit;
use std::ops::Deref;
use std::ptr::{addr_of, addr_of_mut};
use std::result;
use std::sync::{Arc, Once};
use libffi::high::arg;
use libffi::low::ffi_type;
use libffi::middle::Cif;
use parking_lot::{RawRwLock, RwLock};
use parking_lot::lock_api::{MappedRwLockReadGuard, MappedRwLockWriteGuard, RwLockReadGuard, RwLockWriteGuard};
use v8::{Global, Local, Object};
use windows::core::{HSTRING, IUnknown, GUID, HRESULT, Interface, IUnknown_Vtbl, ComInterface, PCWSTR, Type, IInspectable, Error};
use windows::Foundation::{GuidHelper, IAsyncOperation};
use windows::Win32::Foundation::CO_E_INIT_ONLY_SINGLE_THREADED;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CLSCTX_LOCAL_SERVER, CLSIDFromProgID, CLSIDFromString, CoCreateInstance, CoGetClassObject, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, COINIT_MULTITHREADED, CoInitialize, CoInitializeEx, CoUninitialize, DISPATCH_METHOD, DISPPARAMS, EXCEPINFO, IClassFactory, IDispatch, IDispatch_Vtbl, ITypeLib, VARIANT, VT_UI2};
use windows::Win32::System::WinRT::{IActivationFactory, RoActivateInstance, RoGetActivationFactory};
use windows::Win32::UI::WindowsAndMessaging::LB_GETLOCALE;
use metadata::declarations::base_class_declaration::BaseClassDeclarationImpl;
use metadata::declarations::class_declaration::ClassDeclaration;
use metadata::declarations::declaration;
use metadata::declarations::declaration::{
    DeclarationKind,
    Declaration,
};
use metadata::declarations::enum_declaration::EnumDeclaration;
use metadata::declarations::interface_declaration::InterfaceDeclaration;
use metadata::declarations::namespace_declaration::NamespaceDeclaration;
use metadata::declarations::type_declaration::TypeDeclaration;
use metadata::declaring_interface_for_method::Metadata;
use metadata::meta_data_reader::MetadataReader;
use metadata::prelude::{get_guid_attribute_value, get_string_value_from_blob, get_type_name, LOCALE_SYSTEM_DEFAULT};
use metadata::{guid_to_string, query_interface, signature};
use metadata::declarations::method_declaration::MethodDeclaration;
use metadata::signature::Signature;
use metadata::value::{Value, Variant};
use crate::value::MethodCall;

pub struct Runtime {
    isolate: v8::OwnedIsolate,
    global_context: v8::Global<v8::Context>,
    app_root: String,
}

static INIT: Once = Once::new();

#[derive(Clone)]
struct DeclarationFFI {
    inner: Arc<RwLock<dyn Declaration>>,
    pub(crate) instance: Option<IUnknown>,
}

unsafe impl Sync for DeclarationFFI {}

unsafe impl Send for DeclarationFFI {}

impl DeclarationFFI {
    pub fn new(declaration: Arc<RwLock<dyn Declaration>>) -> Self {
        Self { inner: declaration, instance: None }
    }

    pub fn new_with_instance(declaration: Arc<RwLock<dyn Declaration>>, instance: Option<IUnknown>) -> Self {
        Self { inner: declaration, instance }
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

fn create_ns_object<'a>(name: &str, declaration: Arc<RwLock<dyn Declaration>>, scope: &mut v8::HandleScope<'a>) -> Local<'a, v8::Value> {
    let scope = &mut v8::EscapableHandleScope::new(scope);
    let name = v8::String::new(scope, name).unwrap();
    let tmpl = v8::FunctionTemplate::new(scope, handle_ns_func);
    tmpl.set_class_name(name);
    let object_tmpl = tmpl.instance_template(scope);
    object_tmpl.set_named_property_handler(
        v8::NamedPropertyHandlerConfiguration::new()
            .getter(handle_named_property_getter)
            .setter(handle_named_property_setter)
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

fn create_ns_ctor_instance_object<'a>(name: &str, factory: IUnknown, declaration: Arc<RwLock<dyn Declaration>>, instance: Option<IUnknown>, scope: &mut v8::HandleScope<'a>) -> Local<'a, v8::Value> {
    let scope = &mut v8::EscapableHandleScope::new(scope);

    let class_name = v8::String::new(scope, name).unwrap();

    let name = v8::String::new(scope, name).unwrap();
    let tmpl = v8::FunctionTemplate::new(scope, handle_ns_func);
    tmpl.set_class_name(name);

    let proto = tmpl.prototype_template(scope);


    {

        let lock = declaration.read();

        let clazz = lock.as_any().downcast_ref::<ClassDeclaration>().unwrap();


        let to_string_func = v8::FunctionTemplate::builder(|scope: &mut v8::HandleScope,
                                                                        args: v8::FunctionCallbackArguments,
                                                                        mut retval: v8::ReturnValue| {
            retval.set(args.data());
    })
        .data(class_name.into())
        .build(scope);

        let to_string = v8::String::new(scope, "toString").unwrap();
        proto.set(to_string.into(), to_string_func.into());

        println!("clazz {}", clazz.name());

        for method in clazz.methods().iter() {
            let name = v8::String::new(scope, method.name());
            let is_static = method.is_static();

            let declaration = DeclarationFFI::new_with_instance(
                Arc::new(
                    RwLock::new(
                        method.clone()
                    )
                ),
                if is_static { Some(factory.clone()) } else { instance.clone() },
            );

            let declaration = Box::into_raw(Box::new(declaration));


            let ext = v8::External::new(scope, declaration as _);

            let func = v8::FunctionTemplate::builder(|scope: &mut v8::HandleScope,
                                                      args: v8::FunctionCallbackArguments,
                                                      mut retval: v8::ReturnValue| {
                let dec = unsafe { Local::<v8::External>::cast(args.data()) };

                let dec = dec.value() as *mut DeclarationFFI;

                let dec = unsafe { &*dec };

                let lock = dec.read();

                let kind = lock.kind();

                println!("{} {}", kind, lock.name());

                let method = lock.as_any().downcast_ref::<MethodDeclaration>().unwrap();

                let mut method = MethodCall::new(
                    method, method.is_sealed(), dec.instance.clone().unwrap(), false,
                );

                let (ret, result) = method.call(scope, &args);

                println!("{}", ret);
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
            println!("prop: {} {:?} {:?}", property.name(), property.setter(), property.getter());
        }

    }


    let object_tmpl = tmpl.instance_template(scope);
    // object_tmpl.set_named_property_handler(
    //     v8::NamedPropertyHandlerConfiguration::new()
    //         .getter(handle_named_property_getter)
    //         .setter(handle_named_property_setter)
    // );
    object_tmpl.set_internal_field_count(2);
    let object = object_tmpl.new_instance(scope).unwrap();
    let declaration = Box::new(DeclarationFFI::new_with_instance(declaration, instance));
    let ext = v8::External::new(scope, Box::into_raw(declaration) as _);
    object.set_internal_field(0, ext.into());

    let object_store = v8::Map::new(scope);
    object.set_internal_field(1, object_store.into());

    let ret = scope.escape(object);

    ret.into()
}

fn create_ns_ctor_object<'a>(name: &str, declaration: Arc<RwLock<dyn Declaration>>, scope: &mut v8::HandleScope<'a>) -> Local<'a, v8::Value> {
    let scope = &mut v8::EscapableHandleScope::new(scope);

    let name = v8::String::new(scope, name).unwrap();

    let declaration = Box::into_raw(Box::new(DeclarationFFI::new(declaration)));

    let ext = v8::External::new(scope, declaration as _);

    let tmpl = v8::FunctionTemplate::builder(|scope: &mut v8::HandleScope,
                                              args: v8::FunctionCallbackArguments,
                                              mut retval: v8::ReturnValue| {
        let length = args.length();

        let dec = unsafe { Local::<v8::External>::cast(args.data()) };

        let dec = dec.value() as *mut DeclarationFFI;

        let dec = unsafe { &*dec };

        let lock = dec.read();

        let kind = lock.kind();

        let ext = args.data();

        match kind {
            DeclarationKind::Class => {
                let clazz = lock.as_any().downcast_ref::<ClassDeclaration>().unwrap();

                let clazz_name = HSTRING::from(clazz.full_name());

                let clazz_factory = unsafe { RoGetActivationFactory::<IUnknown>(&clazz_name) };

                assert!(clazz_factory.is_ok());

                let clazz_factory = clazz_factory.unwrap();

                unsafe {
                    let is_sealed = clazz.is_sealed();
                    for ctor in clazz.initializers() {
                        println!("ctor {}", clazz.is_sealed());

                        let mut method = MethodCall::new(
                            ctor, is_sealed, clazz_factory.clone(), true,
                        );

                        /*let number_of_parameters = ctor.number_of_parameters();

                        let mut index = 0 as usize;

                        let iid = match Metadata::find_declaring_interface_for_method(ctor, &mut index) {
                            None => {
                                index = 0;
                                IActivationFactory::IID
                            }
                            Some(interface) => {
                                let ii_lock = interface.read();
                                let ii = ii_lock.as_declaration().as_any().downcast_ref::<InterfaceDeclaration>();
                                let ii = ii.unwrap();
                                ii.id()
                            }
                        };

                        index = index.saturating_add(6); // account for IInspectable vtable overhead

                        let mut interface_ptr: *mut c_void = std::ptr::null_mut(); // IActivationFactory

                        let vtable = clazz_factory.vtable();

                        let interface_ptr_ptr = addr_of_mut!(interface_ptr);

                        let result = ((*vtable).QueryInterface)(clazz_factory.as_raw(), &iid, interface_ptr_ptr as *mut *const c_void);

                        assert!(result.is_ok());

                        let is_composition = !clazz.is_sealed();

                        let number_of_abi_parameters = number_of_parameters + if clazz.is_sealed() { 2 } else { 4 };

                        let mut parameter_types: Vec<*mut ffi_type> = Vec::new();

                        parameter_types.reserve(number_of_abi_parameters);

                        unsafe { parameter_types.push(&mut libffi::low::types::pointer); }

                        let mut arguments: Vec<*mut c_void> = Vec::new();

                        arguments.reserve(number_of_abi_parameters);

                        unsafe { arguments.push(interface_ptr) };

                        let mut string_buf = Vec::new();

                        for (i, parameter) in ctor.parameters().iter().enumerate() {
                            let type_ = parameter.type_();
                            let metadata = parameter.metadata().unwrap();

                            let signature = Signature::to_string(&*metadata, &type_);
                            match signature.as_str() {
                                "String" => {
                                    unsafe { parameter_types.push(&mut libffi::low::types::pointer); }
                                    let string = args.get(i as i32).to_string(scope).unwrap();

                                    let string = HSTRING::from(string.to_rust_string_lossy(scope));


                                    arguments.push(
                                        addr_of!(string) as *mut c_void
                                    );

                                    string_buf.push(string);
                                }
                                _ => {}
                            }
                        }

                        if is_composition {
                            unsafe { parameter_types.push(&mut libffi::low::types::pointer); }
                            unsafe { parameter_types.push(&mut libffi::low::types::pointer); }
                        }

                        unsafe { parameter_types.push(&mut libffi::low::types::pointer); }

                        let mut result: MaybeUninit<IUnknown> = MaybeUninit::zeroed();

                        arguments.push(&mut result.as_mut_ptr() as *mut _ as *mut c_void);

                        let interface = unsafe { IUnknown::from_raw(interface_ptr as *mut c_void) };

                        let mut vtable = interface.vtable();

                        let mut vtable: *mut *mut c_void = std::mem::transmute(vtable);

                        let func = unsafe {
                            *vtable.offset(index as isize)
                        };

                        let mut cif: libffi::low::ffi_cif = Default::default();

                        let prep_result = unsafe {
                            libffi::low::prep_cif(&mut cif,
                                                  libffi::low::ffi_abi_FFI_DEFAULT_ABI,
                                                  arguments.len(),
                                                  &mut libffi::low::types::sint32,
                                                  parameter_types.as_mut_ptr(),
                            )
                        };

                        let ret = unsafe {
                            libffi::low::call::<i32>(&mut cif, libffi::low::CodePtr::from_ptr(func), arguments.as_mut_ptr())
                        };

                        let ret = HRESULT(ret);

                        */

                        let (ret, result) = method.call(scope, &args);
                        if ret.is_ok() {
                            let result = unsafe { IUnknown::from_raw(result) };
                            let ctor = Arc::clone(&dec.inner);
                            let instance = create_ns_ctor_instance_object(clazz.name(), clazz_factory, ctor, Some(result), scope);
                            retval.set(instance);
                            return;
                        } else {
                            let error = Error::from(ret);
                            // let error = v8::Exception::
                            //   scope.throw_exception()

                            println!("ret {:?} {:?}", ret.to_string(), result);
                        }

                        // let mut cif = libffi::middle::Cif::new(
                        //     parameter_types,
                        //     libffi::middle::Type::pointer(),
                        // );


                        // let af = unsafe { IActivationFactory::from_raw(interface_ptr as _)};

                        // let vt = addr_of_mut!(interface_ptr);
                        //
                        // let func = unsafe {
                        //     *(vt.offset(index as isize))
                        // };

                        // let a = unsafe { af.ActivateInstance().unwrap()};
                        //
                        //
                        // let func =  a.as_raw();

                        // let result: *mut c_void = unsafe {
                        //     cif.call(
                        //         libffi::middle::CodePtr::from_ptr(*func),
                        //         arguments.as_slice(),
                        //     )
                        // };



                        /*
                           let class_id = HSTRING::from(clazz.full_name());

                                //  let class_id = HSTRING::from(ii.full_name());

                                println!("class_id {}", class_id);


                                let result = unsafe { CoInitialize(None) };


                                let ret = unsafe { RoGetActivationFactory::<IUnknown>(&class_id)};


                                let instance: windows::core::Result<IDispatch> = unsafe { CoCreateInstance(&id, None, CLSCTX_INPROC_SERVER)};

                                println!("instaaance {:?}", instance);

                                let ret = ret.unwrap();

                                let mut interface_ptr = std::ptr::null_mut() as *const c_void;

                                let raw = ret.vtable();

                                let result = unsafe { ((*raw).QueryInterface)(ret.as_raw(), &id, &mut interface_ptr)};

                                let name = HSTRING::from(ii.full_name());

                                let GUID_NULL = GUID::default();

                                let mut dispid = 0_i32;
                                let ds = unsafe { IDispatch::from_raw(interface_ptr as *mut c_void)};
                                let result = unsafe {ds.GetIDsOfNames(
                                    &GUID_NULL,
                                    &PCWSTR(name.as_ptr()),
                                    1,
                                    LOCALE_SYSTEM_DEFAULT,
                                    &mut dispid
                                )};

                                println!("name {} {} {:?} {:?} {}", ctor.name(), dispid, result, ctor.is_initializer(), ii.full_name());

                                let val: Variant = Value::String("Hello".to_string()).into();
                                let mut rgvarg = vec![val.as_abi()];


                                let mut disparams = DISPPARAMS {
                                    rgvarg: unsafe { rgvarg.as_mut_ptr() as *mut VARIANT},
                                    rgdispidNamedArgs: std::ptr::null_mut(),
                                    cArgs: 1,
                                    cNamedArgs: 0,
                                };


                                let mut pvarresult = VARIANT::default();
                                let mut excep = EXCEPINFO::default();
                                let mut err = 0_u32;
                                let result = unsafe {
                                    ds.Invoke(
                                        dispid,
                                        &GUID_NULL,
                                        LOCALE_SYSTEM_DEFAULT,
                                        DISPATCH_METHOD,
                                        &mut disparams,
                                        Some(&mut pvarresult),
                                        Some(&mut excep),
                                        Some(&mut err)
                                    )
                                };

                                // let instance: windows::core::Result<IDispatch> = unsafe { CoCreateInstance(&id, None, CLSCTX_INPROC_SERVER)};


                                // println!("instance {:?} {:?}", instance, ret);
                                //println!("result {:?} {:?}", result.is_ok(), interface_ptr);

                                // TODO handling ctor

                                let mut index = 0;
                                if param_count == length as usize {

                                    for param in ctor.parameters().iter() {
                                        let type_ = param.type_();
                                        let metadata = param.metadata();
                                        if let Some(metadata) = metadata {
                                            println!("{:?}", Signature::to_string(&*metadata, &type_));
                                        }

                                    }


                                    break;
                                }
                            }
                         */
                    }
                }
            }
            _ => {}
        }

        let object_tmpl = v8::ObjectTemplate::new(scope);
        object_tmpl.set_named_property_handler(
            v8::NamedPropertyHandlerConfiguration::new()
                .getter(handle_named_property_getter)
                .setter(handle_named_property_setter)
        );
        object_tmpl.set_indexed_property_handler(
            v8::IndexedPropertyHandlerConfiguration::new()
                .setter(handle_indexed_property_setter)
                .getter(handle_indexed_property_getter)
        );
        object_tmpl.set_internal_field_count(2);
        let object = object_tmpl.new_instance(scope).unwrap();

        object.set_internal_field(0, ext);

        let object_store = v8::Map::new(scope);
        object.set_internal_field(1, object_store.into());

        retval.set(object.into());
    })
        .data(ext.into()).build(scope);
    tmpl.set_class_name(name);

    let func = tmpl.get_function(scope).unwrap();
    let ret = scope.escape(func);

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
            } else {
                format!("{}.{}", full_name, ns)
            };

            let name: Local<v8::Name> = v8::String::new(scope, ns.as_str()).unwrap().into();
            if let Some(name_space) = MetadataReader::find_by_name(full_name.as_str()) {
                let object = create_ns_object(ns, name_space, scope);
                global.define_own_property(scope, name, object, v8::READ_ONLY | v8::DONT_DELETE);
            }
        }
    }
}


fn handle_named_property_setter(scope: &mut v8::HandleScope,
                                key: Local<v8::Name>,
                                value: Local<v8::Value>,
                                args: v8::PropertyCallbackArguments) {
    let this = args.holder();
    let dec = this.get_internal_field(scope, 0).unwrap();
    let dec = unsafe { Local::<v8::External>::cast(dec) };
    let dec = dec.value() as *mut DeclarationFFI;
    let dec = unsafe { &*dec };
    let lock = dec.read();
    let kind = lock.kind();
    let store = this.get_internal_field(scope, 1).unwrap();
    let store = unsafe { Local::<v8::Map>::cast(store) };
    let name = key.to_rust_string_lossy(scope);
    match kind {
        DeclarationKind::Namespace => {
            let dec = unsafe { lock.as_any().downcast_ref::<NamespaceDeclaration>() };
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
            let dec = unsafe { lock.as_any().downcast_ref::<EnumDeclaration>() };
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
        DeclarationKind::Method => {


            // let length = args.length();

            println!("setter {}", name);



            /* let json = windows::Data::Json::JsonObject::from_raw(result.into_raw());

             let runtime = JsonValue::CreateStringValue(&HSTRING::from("NativeScript")).unwrap();
             json.SetNamedValue(&HSTRING::from("runtime"), &runtime);

             println!("runtime key: {:?}", json.GetNamedValue(&HSTRING::from("runtime")).unwrap().GetString().unwrap());


             // todo
             for method in clazz.methods() {
                 let param_count = method.number_of_parameters();
                 //  println!("count {param_count}");
                 if param_count == length as usize {
                     println!("{:?}", method.parameters());
                 }
             }
             */
        }
        DeclarationKind::Parameter => {}
    }
}

fn handle_named_property_getter(scope: &mut v8::HandleScope,
                                key: v8::Local<v8::Name>,
                                args: v8::PropertyCallbackArguments,
                                mut rv: v8::ReturnValue) {
    let this = args.this();
    let dec = this.get_internal_field(scope, 0).unwrap();
    let dec = unsafe { Local::<v8::External>::cast(dec) };
    let dec = dec.value() as *mut DeclarationFFI;
    let dec = unsafe { &*dec };
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
                    // let cached_item = store.get(scope, key.into());
                    // if let Some(cache) = cached_item {
                    //     if !cache.is_null_or_undefined() {
                    //         rv.set(cache);
                    //         return;
                    //     }
                    // }

                    let full_name = format!("{}.{}", dec.full_name(), name.as_str());
                    if let Some(dec) = MetadataReader::find_by_name(full_name.as_str()) {
                        let declaration = Arc::clone(&dec);
                        let lock = dec.read();

                        match lock.kind() {
                            DeclarationKind::Class => {
                                let ret: Local<v8::Value> = create_ns_ctor_object(lock.name(), declaration, scope).into();
                                rv.set(ret.into());
                            }
                            _ => {
                                let ret: Local<v8::Value> = create_ns_object(name.as_str(), declaration, scope).into();
                                rv.set(ret.into());
                            }
                        }


                        //  store.set(scope, key.into(), ret.into());
                        return;
                    }

                    rv.set_undefined();
                    return;
                }
            }
            DeclarationKind::Class => {
                let clazz_dec = lock.as_any().downcast_ref::<ClassDeclaration>();

                if let Some(clazz_dec) = clazz_dec {
                    for method in clazz_dec.methods() {
                        let mut method_name = method.overload_name();
                        if method_name.is_empty() {
                            method_name = method.name();
                        }

                        if method_name == name {
                            let mut declaration = Arc::new(RwLock::new(method.clone()));

                            let declaration = Box::into_raw(Box::new(DeclarationFFI::new_with_instance(declaration, dec.instance.clone())));

                            let ext = v8::External::new(scope, declaration as _);

                            let builder = v8::Function::builder(|scope: &mut v8::HandleScope,
                                                                 args: v8::FunctionCallbackArguments,
                                                                 mut retval: v8::ReturnValue| {
                                let length = args.length();

                                let dec = unsafe { Local::<v8::External>::cast(args.data()) };

                                let dec = dec.value() as *mut DeclarationFFI;

                                let dec = unsafe { &*dec };

                                let lock = dec.read();

                                let method = lock.as_any().downcast_ref::<MethodDeclaration>();

                                let method = method.unwrap();

                                let instance = dec.instance.clone().unwrap();

                                let mut method = MethodCall::new(
                                    method, method.is_sealed(), instance, false,
                                );

                                let (ret, result) = method.call(scope, &args);

                                /*
                                let mut index = 0_usize;

                                if let Some(interface) = Metadata::find_declaring_interface_for_method(method, &mut index) {
                                    /*let interface = interface.read();
                                    let method_interface = interface.as_declaration().as_any().downcast_ref::<InterfaceDeclaration>();
                                    let method_interface = method_interface.unwrap();
                                    let iid = method_interface.id();

                                    let is_void = method.is_void();

                                    let number_of_parameters = method.number_of_parameters();
                                    let number_of_abi_parameters = number_of_parameters + if is_void {1} else {2};

                                    index = index.saturating_add(6);

                                    let instance = dec.instance.clone().unwrap();

                                    let mut interface_ptr: *mut c_void = std::ptr::null_mut();

                                    let vtable = instance.vtable();

                                    let interface_ptr_ptr = addr_of_mut!(interface_ptr);

                                    let result = unsafe { ((*vtable).QueryInterface)(instance.as_raw(), &iid, interface_ptr_ptr as *mut *const c_void)};

                                    assert!(result.is_ok());

                                    let mut parameter_types: Vec<*mut ffi_type> = Vec::new();

                                    parameter_types.reserve(number_of_abi_parameters);

                                    unsafe { parameter_types.push(&mut libffi::low::types::pointer); }

                                    let mut arguments: Vec<*mut c_void> = Vec::new();

                                    arguments.reserve(number_of_abi_parameters);

                                    unsafe { arguments.push(interface_ptr) };

                                    let mut string_buf = Vec::new();

                                    for (i, parameter) in method.parameters().iter().enumerate() {
                                        let type_ = parameter.type_();
                                        let metadata = parameter.metadata().unwrap();

                                        let signature = Signature::to_string(&*metadata, &type_);

                                        println!("signature {}", signature.as_str());

                                        match signature.as_str() {
                                            "String" => {
                                                unsafe { parameter_types.push(&mut libffi::low::types::pointer); }
                                                let string = args.get(i as i32).to_string(scope).unwrap();

                                                let string = HSTRING::from(string.to_rust_string_lossy(scope));


                                                arguments.push(
                                                    addr_of!(string) as *mut c_void
                                                );

                                                string_buf.push(string);
                                            }
                                            _ => {}
                                        }
                                    }

                                  //  let mut result: *mut c_void = std::ptr::null_mut();


                                    let mut result: MaybeUninit<IUnknown> = MaybeUninit::zeroed();

                                    if !is_void {
                                        unsafe { parameter_types.push(&mut libffi::low::types::pointer); }

                                       // arguments.push(addr_of_mut!(result) as *mut _);

                                        arguments.push(&mut result.as_mut_ptr() as *mut _ as *mut c_void);
                                    }


                                    let interface = unsafe { IUnknown::from_raw(interface_ptr as *mut c_void) };

                                    let mut vtable = interface.vtable();

                                    let mut vtable: *mut *mut c_void = unsafe { std::mem::transmute(vtable)};

                                    let func = unsafe {
                                        *vtable.offset(index as isize)
                                    };

                                    std::mem::forget(interface);

                                    let mut cif: libffi::low::ffi_cif = Default::default();

                                    let prep_result = unsafe {
                                        libffi::low::prep_cif(&mut cif,
                                                              libffi::low::ffi_abi_FFI_DEFAULT_ABI,
                                                              arguments.len(),
                                                              &mut libffi::low::types::sint32,
                                                              parameter_types.as_mut_ptr(),
                                        )
                                    };

                                    let ret = unsafe {
                                        libffi::low::call::<i32>(&mut cif, libffi::low::CodePtr::from_ptr(func), arguments.as_mut_ptr())
                                    };

                                    let ret = HRESULT(ret);

                                    */


                                    println!("ret {}", ret);

                                }

                                */
                            })
                                .data(ext.into()).build(scope);


                            let func = builder.unwrap();

                            rv.set(func.into());
                            return;
                        }
                    }
                }
            }
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
                            return;
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
                            _ => {}
                        }
                    }

                    rv.set_undefined();
                    return;
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
            DeclarationKind::Method => {
                println!("getter method {}", name);
                let dec = lock.as_any().downcast_ref::<ClassDeclaration>();

                if let Some(dec) = dec {
                    println!("dec {}", dec.name());
                    for method in dec.methods() {
                        let mut name = method.overload_name();
                        if name.is_empty() {
                            name = method.name();
                        }

                        println!("method name {}", name);
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
        return;
    }

    rv.set(args.holder().into());
}


fn handle_indexed_property_setter(_scope: &mut v8::HandleScope,
                                  index: u32,
                                  value: v8::Local<v8::Value>,
                                  args: v8::PropertyCallbackArguments) {}


fn handle_indexed_property_getter(scope: &mut v8::HandleScope,
                                  index: u32,
                                  args: v8::PropertyCallbackArguments,
                                  mut rv: v8::ReturnValue) {}


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
            /* let _ = unsafe {
                 // CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE)
                 CoInitialize(None)
             };
             */
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
                    global_context = Global::new(scope, context);
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

    pub fn dispose(&self) {
        /* unsafe {
             CoUninitialize();
         }*/
    }
}