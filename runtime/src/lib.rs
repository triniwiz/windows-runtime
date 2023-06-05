mod converter;
mod value;
mod interop;
mod method_call;
mod property_call;
mod error;
mod structure;
mod globals;
mod generic_method_call;
mod helpers;
mod class;
mod name_space;

use std::any::Any;
use std::cell::RefCell;
use std::ffi::{c_char, c_void, CString};
use std::mem::MaybeUninit;
use std::ops::Deref;
use std::ptr::{addr_of, addr_of_mut, NonNull};
use std::result;
use std::sync::{Arc, Once};
use libffi::high::arg;
use libffi::low::{CodePtr, ffi_type};
use libffi::middle::Cif;
use parking_lot::{Mutex, RawRwLock, RwLock};
use parking_lot::lock_api::{MappedRwLockReadGuard, MappedRwLockWriteGuard, RwLockReadGuard, RwLockWriteGuard};
use v8::{FunctionTemplate, Global, Local, Number, Object};
use windows::core::{HSTRING, IUnknown, GUID, HRESULT, Interface, IUnknown_Vtbl, ComInterface, PCWSTR, Type, IInspectable, Error};
use windows::Foundation::{AsyncStatus, GuidHelper, IAsyncInfo, IAsyncOperation};
use windows::Win32::Foundation::CO_E_INIT_ONLY_SINGLE_THREADED;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CLSCTX_LOCAL_SERVER, CLSIDFromProgID, CLSIDFromString, CoCreateInstance, CoGetClassObject, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, COINIT_MULTITHREADED, CoInitialize, CoInitializeEx, CoUninitialize, DISPATCH_METHOD, DISPPARAMS, EXCEPINFO, IClassFactory, IDispatch, IDispatch_Vtbl, ITypeLib, VARIANT, VT_UI2};
use windows::Win32::System::WinRT::{IActivationFactory, RoActivateInstance, RoGetActivationFactory};
use windows::Win32::System::WinRT::Metadata::ELEMENT_TYPE_CHAR;
use windows::Win32::UI::WindowsAndMessaging::LB_GETLOCALE;
use metadata::declarations::base_class_declaration::{BaseClassDeclaration, BaseClassDeclarationImpl};
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
use metadata::declarations::interface_declaration::generic_interface_declaration::GenericInterfaceDeclaration;
use metadata::declarations::method_declaration::MethodDeclaration;
use metadata::declarations::property_declaration::PropertyDeclaration;
use metadata::declarations::struct_declaration::StructDeclaration;
use metadata::declarations::struct_field_declaration::StructFieldDeclaration;
use metadata::signature::Signature;
use metadata::value::{Value, Variant};
use crate::value::{ffi_parse_bool_arg, ffi_parse_buffer_arg, ffi_parse_f32_arg, ffi_parse_f64_arg, ffi_parse_function_arg, ffi_parse_i16_arg, ffi_parse_i32_arg, ffi_parse_i8_arg, ffi_parse_isize_arg, ffi_parse_pointer_arg, ffi_parse_string_arg, ffi_parse_struct_arg, ffi_parse_u16_arg, ffi_parse_u32_arg, ffi_parse_u64_arg, ffi_parse_u8_arg, ffi_parse_usize_arg, MAX_SAFE_INTEGER, MIN_SAFE_INTEGER, NativeType, NativeValue, set_ret_val};

thread_local!(static ISOLATE: RefCell<Option<&'static mut v8::Isolate>> = RefCell::new(None));

pub struct Runtime {
    isolate: v8::OwnedIsolate,
    global_context: v8::Global<v8::Context>,
    app_root: String,
    pub(crate) runtime: Arc<RwLock<runtime::Runtime>>,
}

static INIT: Once = Once::new();

struct IUnknownSafeInner(IUnknown);

unsafe impl Send for IUnknownSafeInner {}

struct IUnknownSafe {
    inner: Mutex<IUnknownSafeInner>,
}

impl IUnknownSafe {
    pub fn new(value: IUnknown) -> Self {
        Self {
            inner: Mutex::new(IUnknownSafeInner(value))
        }
    }
}

struct FFIPromiseInner {
    promise: NonNull<v8::PromiseResolver>,
    context: NonNull<v8::Context>,
}

unsafe impl Send for FFIPromiseInner {}

struct FFIPromise(Arc<RwLock<FFIPromiseInner>>);

impl FFIPromise {
    pub fn new(
        context: NonNull<v8::Context>,
        promise: NonNull<v8::PromiseResolver>,
    ) -> Self {
        Self {
            0: Arc::new(RwLock::new(FFIPromiseInner {
                context,
                promise,
            }))
        }
    }
}

unsafe impl Send for FFIPromise {}

#[derive(Clone)]
struct DeclarationFFI {
    inner: Arc<RwLock<dyn Declaration>>,
    pub(crate) instance: Option<IUnknown>,
    parent: Option<Arc<RwLock<dyn Declaration>>>,
    pub(crate) struct_instance: Option<(Vec<u8>, Vec<NativeType>)>,
}

unsafe impl Sync for DeclarationFFI {}

unsafe impl Send for DeclarationFFI {}

impl DeclarationFFI {
    pub fn new(declaration: Arc<RwLock<dyn Declaration>>) -> Self {
        Self { inner: declaration, instance: None, parent: None, struct_instance: None }
    }

    pub fn new_with_instance(declaration: Arc<RwLock<dyn Declaration>>, instance: Option<IUnknown>) -> Self {
        Self { inner: declaration, instance, parent: None, struct_instance: None }
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

use regex::Regex;
use tokio::io::AsyncReadExt;
use tokio::runtime;
use metadata::declarations::interface_declaration::generic_interface_instance_declaration::GenericInterfaceInstanceDeclaration;
use crate::generic_method_call::GenericMethodCall;
use crate::method_call::MethodCall;
use crate::property_call::PropertyCall;

fn init_global(scope: &mut v8::ContextScope<v8::HandleScope<v8::Context>>, context: v8::Local<v8::Context>) {
    let mut global = context.global(scope);
    let value = v8::String::new(
        scope, "global",
    ).unwrap().into();
    global.define_own_property(scope, value, global.into(), v8::PropertyAttribute::READ_ONLY);
}

fn create_ns_object<'a>(name: &str, declaration: Arc<RwLock<dyn Declaration>>, scope: &mut v8::HandleScope<'a>) -> Local<'a, v8::Value> {
    let scope = &mut v8::EscapableHandleScope::new(scope);
    let name = v8::String::new(scope, name).unwrap();
    let tmpl = FunctionTemplate::new(scope, handle_ns_func);
    tmpl.set_class_name(name);
    let object_tmpl = tmpl.instance_template(scope);
    object_tmpl.set_named_property_handler(
        v8::NamedPropertyHandlerConfiguration::new()
            .query(handle_named_property_query)
            .getter(handle_named_property_getter)
            .setter(handle_named_property_setter)
    );
    object_tmpl.set_internal_field_count(1);

    let object = object_tmpl.new_instance(scope).unwrap();
    let declaration = Box::new(DeclarationFFI::new(declaration));
    let ext = v8::External::new(scope, Box::into_raw(declaration) as _);
    object.set_internal_field(0, ext.into());

    let object_store = v8::Map::new(scope);
    object.set_internal_field(1, object_store.into());

    let ret = scope.escape(object);

    ret.into()
}

fn create_ns_ctor_instance_object<'a>(name: &str, factory: Option<IUnknown>, parent: Option<Arc<RwLock<dyn Declaration>>>, declaration: Arc<RwLock<dyn Declaration>>, instance: Option<IUnknown>, scope: &mut v8::HandleScope<'a>) -> Local<'a, v8::Value> {
    let scope = &mut v8::EscapableHandleScope::new(scope);

    let class_name = v8::String::new(scope, name).unwrap();

    let tmpl = FunctionTemplate::new(scope, handle_ns_func);

    tmpl.set_class_name(class_name);

    let proto = tmpl.prototype_template(scope);

    {
        let lock = declaration.read();

        let kind = lock.kind();

        println!("name {} : kind {}", name, kind);

        match kind {
            DeclarationKind::Class => {
                let clazz = lock.as_any().downcast_ref::<ClassDeclaration>().unwrap();


                let to_string_func = FunctionTemplate::builder(|scope: &mut v8::HandleScope,
                                                                args: v8::FunctionCallbackArguments,
                                                                mut retval: v8::ReturnValue| {
                    retval.set(args.data());
                })
                    .data(class_name.into())
                    .build(scope);

                let to_string = v8::String::new(scope, "toString").unwrap();

                proto.set(to_string.into(), to_string_func.into());

                for method in clazz.methods().iter() {
                    let name = v8::String::new(scope, method.name());

                    let is_static = method.is_static();

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

                        let scope = unsafe { &mut v8::CallbackScope::new(info) };
                        let args = unsafe { v8::FunctionCallbackArguments::from_function_callback_info(info) };
                        let mut retval = v8::ReturnValue::from_function_callback_info(info);


                        let dec = unsafe { Local::<v8::External>::cast(args.data()) };

                        let dec = dec.value() as *mut DeclarationFFI;

                        let dec = unsafe { &*dec };

                        let lock = dec.read();

                        let method = lock.as_any().downcast_ref::<MethodDeclaration>().unwrap();

                        let nam = method.name();
                        let mut method = MethodCall::new(
                            method, method.is_sealed(), dec.instance.clone().unwrap(), false,
                        );

                        let (ret, result) = method.call(scope, &args);

                        if ret.is_err() {
                            println!(">>> {}", ret.message().to_string())
                        } else if !method.is_void() {
                            let return_sig = method.return_type().to_string();
                            match NativeType::try_from(return_sig.as_str()) {
                                Ok(return_type) => {
                                    if return_sig.contains(".") {
                                        let instance = unsafe { IUnknown::from_raw(result) };

                                        if return_sig.contains('`') {
                                            let mut name = return_sig.to_string();

                                            if let Some(backtick_index) = name.rfind('<') {
                                                name.truncate(backtick_index);
                                            }


                                            // use the generic name
                                            let declaration = MetadataReader::find_by_name(name.as_str()).unwrap();


                                            // todo async

                                            // check if async

                                            let is_async = helpers::is_async(&instance);

                                            if is_async {
                                                let promise = v8::PromiseResolver::new(scope).unwrap();

                                                retval.set(promise.get_promise(scope).into());

                                                let promise: Global<v8::PromiseResolver> = Global::new(scope, promise);

                                                let current_context = scope.get_current_context();
                                                let context = v8::Global::new(scope, current_context).into_raw();

                                                let callback = FFIPromise::new(context, promise.into_raw());

                                                let runtime = scope.get_slot::<Arc<RwLock<runtime::Runtime>>>();

                                                let runtime = runtime.unwrap();

                                                let runtime = runtime.read();

                                                //  let instance = IUnknownSafe::new(instance);

                                                let declaration = DeclarationFFI::new_with_instance(
                                                    declaration,
                                                    Some(instance),
                                                );

                                                // let declaration = Box::into_raw(Box::new(declaration));


                                                runtime.spawn(async {
                                                    let callback = callback;
                                                    let callback = callback.0.read();

                                                    let promise: NonNull<v8::PromiseResolver> = callback.promise;
                                                    let context: NonNull<v8::Context> = callback.context;

                                                    let context = unsafe {
                                                        std::mem::transmute::<
                                                            NonNull<v8::Context>,
                                                            v8::Local<v8::Context>,
                                                        >(context)
                                                    };

                                                    let mut cb_scope = unsafe { v8::CallbackScope::new(context) };
                                                    let scope = &mut v8::HandleScope::new(&mut cb_scope);
                                                    let promise = unsafe {
                                                        std::mem::transmute::<
                                                            NonNull<v8::PromiseResolver>,
                                                            v8::Local<v8::PromiseResolver>,
                                                        >(promise)
                                                    };


                                                    let declaration = declaration;

                                                    let instance = declaration.instance.unwrap();

                                                    let declaration = declaration.inner;

                                                    // let instance = instance.inner.lock().0.clone();;
                                                    let info = unsafe { IAsyncInfo::from_raw(instance.clone().into_raw()) };

                                                    let mut current_status = AsyncStatus::Started;

                                                    let mut is_running = info.Status()
                                                        .map(|status| {
                                                            current_status = status;
                                                            status == AsyncStatus::Started
                                                        })
                                                        .unwrap_or_else(|e| {
                                                            let message = e.message().to_string();
                                                            let message = v8::String::new(scope, message.as_str()).unwrap();
                                                            let error = v8::Exception::error(scope, message.into());
                                                            promise.reject(scope, error.into());
                                                            false
                                                        });

                                                    let return_sig = return_sig;

                                                    let ret = create_ns_ctor_instance_object(return_sig.as_str(), None, None, declaration, Some(instance), scope);

                                                    let mut did = false;
                                                    while is_running {
                                                        match info.Status() {
                                                            Ok(done) => {
                                                                match done {
                                                                    AsyncStatus::Completed => {
                                                                        is_running = false;
                                                                        println!("Completed");
                                                                    }
                                                                    AsyncStatus::Error => {
                                                                        is_running = false;
                                                                        println!("Error");
                                                                    }
                                                                    AsyncStatus::Canceled => {
                                                                        is_running = false;
                                                                        println!("Canceled");
                                                                    }
                                                                    AsyncStatus::Started => {
                                                                        if !did {
                                                                            promise.resolve(scope, ret.into());
                                                                            did = true;
                                                                        }
                                                                    }
                                                                    _ => {}
                                                                }
                                                            }
                                                            Err(error) => {
                                                                println!("error {}", error.message())
                                                            }
                                                        }
                                                    }

                                                    // IAsyncInfo

                                                    //
                                                });

                                                return;
                                            }
                                        }
                                        let ret: Local<v8::Value> = create_ns_ctor_instance_object(return_sig.as_str(), None, dec.parent.clone(), dec.inner.clone(), Some(instance), scope).into();
                                        retval.set(ret.into());
                                        return;
                                    }
                                    unsafe { set_ret_val(result, scope, retval, return_type); }
                                }
                                Err(_) => {}
                            }
                        } else {
                            retval.set_undefined();
                        }

                        // todo
                    }

                    let func = FunctionTemplate::builder_raw(callback)
                        .data(ext.into())
                        .build(scope);

                    if is_static {
                        tmpl.set_with_attr(name.unwrap().into(), func.into(), v8::PropertyAttribute::DONT_DELETE);
                    } else {
                        proto.set_with_attr(name.unwrap().into(), func.into(), v8::PropertyAttribute::DONT_DELETE);
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


                    let getter = FunctionTemplate::builder(|scope: &mut v8::HandleScope,
                                                            args: v8::FunctionCallbackArguments,
                                                            mut retval: v8::ReturnValue| {
                        let dec = unsafe { Local::<v8::External>::cast(args.data()) };

                        let dec = dec.value() as *mut DeclarationFFI;

                        let dec = unsafe { &*dec };

                        let lock = dec.read();

                        let method = lock.as_any().downcast_ref::<PropertyDeclaration>().unwrap();

                        let mut method = PropertyCall::new(
                            method, false, dec.instance.clone().unwrap(), false,
                        );

                        let (ret, result) = method.call(scope, &args);

                        if ret.is_err() {
                            println!(">>> {}", ret.message().to_string())
                        } else if !method.is_void() {
                            match NativeType::try_from(method.return_type()) {
                                Ok(return_type) => {
                                    unsafe { set_ret_val(result, scope, retval, return_type); }
                                }
                                Err(_) => {}
                            }
                        } else {
                            retval.set_undefined();
                        }
                    })
                        .data(getter_declaration_ext.into())
                        .build(scope);


                    let mut setter: Option<Local<FunctionTemplate>> = None;


                    if property.setter().is_some() {
                        let setter_declaration = declaration;

                        let setter_declaration = Box::into_raw(Box::new(setter_declaration));

                        let setter_declaration_ext = v8::External::new(scope, setter_declaration as _);


                        setter = Some(FunctionTemplate::builder(|scope: &mut v8::HandleScope,
                                                                 args: v8::FunctionCallbackArguments,
                                                                 mut retval: v8::ReturnValue| {})
                            .data(setter_declaration_ext.into())
                            .build(scope));
                    }


                    if property.is_static() {
                        // todo
                    } else {
                        let name = name.unwrap();
                        proto.set_accessor_property(name.into(), Some(getter), setter, v8::PropertyAttribute::NONE);
                    }
                }
            }
            DeclarationKind::Interface => {
                let clazz = lock.as_any().downcast_ref::<InterfaceDeclaration>().unwrap();


                let to_string_func = FunctionTemplate::builder(|scope: &mut v8::HandleScope,
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
                            let clazz = clazz.as_any().downcast_ref::<ClassDeclaration>().unwrap();

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

                                let func = v8::FunctionTemplate::builder(|scope: &mut v8::HandleScope,
                                                                          args: v8::FunctionCallbackArguments,
                                                                          mut retval: v8::ReturnValue| {
                                    let dec = unsafe { Local::<v8::External>::cast(args.data()) };

                                    let dec = dec.value() as *mut DeclarationFFI;

                                    let dec = unsafe { &*dec };

                                    let lock = dec.read();

                                    let method = lock.as_any().downcast_ref::<MethodDeclaration>().unwrap();

                                    let mut method = MethodCall::new(
                                        method, method.is_sealed(), dec.instance.clone().unwrap(), false,
                                    );

                                    let (ret, result) = method.call(scope, &args);

                                    if ret.is_err() {
                                        println!(">>> {}", ret.message().to_string())
                                    } else if !method.is_void() {
                                        match NativeType::try_from(method.return_type()) {
                                            Ok(return_type) => {
                                                unsafe { set_ret_val(result, scope, retval, return_type); }
                                            }
                                            Err(_) => {}
                                        }
                                    } else {
                                        retval.set_undefined();
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

                                let getter = FunctionTemplate::builder(|scope: &mut v8::HandleScope,
                                                                        args: v8::FunctionCallbackArguments,
                                                                        mut retval: v8::ReturnValue| {
                                    let dec = unsafe { Local::<v8::External>::cast(args.data()) };

                                    let dec = dec.value() as *mut DeclarationFFI;

                                    let dec = unsafe { &*dec };

                                    let lock = dec.read();

                                    let kind = lock.kind();

                                    let method = lock.as_any().downcast_ref::<PropertyDeclaration>().unwrap();

                                    let mut method = PropertyCall::new(
                                        method, false, dec.instance.clone().unwrap(), false,
                                    );


                                    let (ret, result) = method.call(scope, &args);

                                    if ret.is_err() {
                                        println!(">>> {}", ret.message().to_string())
                                    } else if !method.is_void() {
                                        match NativeType::try_from(method.return_type()) {
                                            Ok(return_type) => {
                                                unsafe { set_ret_val(result, scope, retval, return_type); }
                                            }
                                            Err(_) => {}
                                        }
                                    } else {
                                        retval.set_undefined();
                                    }
                                })
                                    .data(getter_declaration_ext.into())
                                    .build(scope);


                                let mut setter: Option<Local<FunctionTemplate>> = None;


                                if property.setter().is_some() {
                                    let setter_declaration = declaration;

                                    let setter_declaration = Box::into_raw(Box::new(setter_declaration));

                                    let setter_declaration_ext = v8::External::new(scope, setter_declaration as _);


                                    setter = Some(FunctionTemplate::builder(|scope: &mut v8::HandleScope,
                                                                             args: v8::FunctionCallbackArguments,
                                                                             mut retval: v8::ReturnValue| {})
                                        .data(setter_declaration_ext.into())
                                        .build(scope));
                                }


                                if property.is_static() {
                                    // todo
                                } else {
                                    let name = name.unwrap();
                                    proto.set_accessor_property(name.into(), Some(getter), setter, v8::PropertyAttribute::READ_ONLY | v8::PropertyAttribute::DONT_DELETE);
                                }
                            }
                        }
                        DeclarationKind::Interface => {
                            let clazz = clazz.as_any().downcast_ref::<InterfaceDeclaration>().unwrap();

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

                                let func = v8::FunctionTemplate::builder(|scope: &mut v8::HandleScope,
                                                                          args: v8::FunctionCallbackArguments,
                                                                          mut retval: v8::ReturnValue| {
                                    let dec = unsafe { Local::<v8::External>::cast(args.data()) };

                                    let dec = dec.value() as *mut DeclarationFFI;

                                    let dec = unsafe { &*dec };

                                    let lock = dec.read();

                                    let method = lock.as_any().downcast_ref::<MethodDeclaration>().unwrap();

                                    let mut method = MethodCall::new(
                                        method, method.is_sealed(), dec.instance.clone().unwrap(), false,
                                    );

                                    let (ret, result) = method.call(scope, &args);

                                    if ret.is_err() {
                                        println!(">>> {}", ret.message().to_string())
                                    } else if !method.is_void() {
                                        match NativeType::try_from(method.return_type()) {
                                            Ok(return_type) => {
                                                unsafe { set_ret_val(result, scope, retval, return_type); }
                                            }
                                            Err(_) => {}
                                        }
                                    } else {
                                        retval.set_undefined();
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


                                let getter = FunctionTemplate::builder(|scope: &mut v8::HandleScope,
                                                                        args: v8::FunctionCallbackArguments,
                                                                        mut retval: v8::ReturnValue| {
                                    let dec = unsafe { Local::<v8::External>::cast(args.data()) };

                                    let dec = dec.value() as *mut DeclarationFFI;

                                    let dec = unsafe { &*dec };

                                    let lock = dec.read();

                                    let kind = lock.kind();

                                    let method = lock.as_any().downcast_ref::<PropertyDeclaration>().unwrap();

                                    let mut method = PropertyCall::new(
                                        method, false, dec.instance.clone().unwrap(), false,
                                    );


                                    let (ret, result) = method.call(scope, &args);

                                    if ret.is_err() {
                                        println!(">>> {}", ret.message().to_string())
                                    } else if !method.is_void() {
                                        match NativeType::try_from(method.return_type()) {
                                            Ok(return_type) => {
                                                unsafe { set_ret_val(result, scope, retval, return_type); }
                                            }
                                            Err(_) => {}
                                        }
                                    } else {
                                        retval.set_undefined();
                                    }
                                })
                                    .data(getter_declaration_ext.into())
                                    .build(scope);


                                let mut setter: Option<Local<FunctionTemplate>> = None;


                                if property.setter().is_some() {
                                    let setter_declaration = declaration;

                                    let setter_declaration = Box::into_raw(Box::new(setter_declaration));

                                    let setter_declaration_ext = v8::External::new(scope, setter_declaration as _);


                                    setter = Some(FunctionTemplate::builder(|scope: &mut v8::HandleScope,
                                                                             args: v8::FunctionCallbackArguments,
                                                                             mut retval: v8::ReturnValue| {})
                                        .data(setter_declaration_ext.into())
                                        .build(scope));
                                }


                                if property.is_static() {
                                    // todo
                                } else {
                                    let name = name.unwrap();
                                    proto.set_accessor_property(name.into(), Some(getter), setter, v8::PropertyAttribute::READ_ONLY | v8::PropertyAttribute::DONT_DELETE);
                                }
                            }
                        }
                        DeclarationKind::GenericInterface => {}
                        DeclarationKind::GenericInterfaceInstance => {}
                        DeclarationKind::Delegate => {}
                        DeclarationKind::GenericDelegate => {}
                        DeclarationKind::GenericDelegateInstance => {}
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

                    let func = v8::FunctionTemplate::builder(|scope: &mut v8::HandleScope,
                                                              args: v8::FunctionCallbackArguments,
                                                              mut retval: v8::ReturnValue| {
                        let dec = unsafe { Local::<v8::External>::cast(args.data()) };

                        let dec = dec.value() as *mut DeclarationFFI;

                        let dec = unsafe { &*dec };

                        let lock = dec.read();

                        let method = lock.as_any().downcast_ref::<MethodDeclaration>().unwrap();

                        let mut method = MethodCall::new(
                            method, method.is_sealed(), dec.instance.clone().unwrap(), false,
                        );

                        let (ret, result) = method.call(scope, &args);

                        if ret.is_err() {
                            println!(">>> {}", ret.message().to_string())
                        } else if !method.is_void() {
                            match NativeType::try_from(method.return_type()) {
                                Ok(return_type) => {
                                    unsafe { set_ret_val(result, scope, retval, return_type); }
                                }
                                Err(_) => {}
                            }
                        } else {
                            retval.set_undefined();
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


                    let getter = FunctionTemplate::builder(|scope: &mut v8::HandleScope,
                                                            args: v8::FunctionCallbackArguments,
                                                            mut retval: v8::ReturnValue| {
                        let dec = unsafe { Local::<v8::External>::cast(args.data()) };

                        let dec = dec.value() as *mut DeclarationFFI;

                        let dec = unsafe { &*dec };

                        let lock = dec.read();

                        let kind = lock.kind();

                        let method = lock.as_any().downcast_ref::<PropertyDeclaration>().unwrap();

                        let mut method = PropertyCall::new(
                            method, false, dec.instance.clone().unwrap(), false,
                        );


                        let (ret, result) = method.call(scope, &args);

                        if ret.is_err() {
                            println!(">>> {}", ret.message().to_string())
                        } else if !method.is_void() {
                            match NativeType::try_from(method.return_type()) {
                                Ok(return_type) => {
                                    unsafe { set_ret_val(result, scope, retval, return_type); }
                                }
                                Err(_) => {}
                            }
                        } else {
                            retval.set_undefined();
                        }
                    })
                        .data(getter_declaration_ext.into())
                        .build(scope);


                    let mut setter: Option<Local<FunctionTemplate>> = None;


                    if property.setter().is_some() {
                        let setter_declaration = declaration;

                        let setter_declaration = Box::into_raw(Box::new(setter_declaration));

                        let setter_declaration_ext = v8::External::new(scope, setter_declaration as _);


                        setter = Some(FunctionTemplate::builder(|scope: &mut v8::HandleScope,
                                                                 args: v8::FunctionCallbackArguments,
                                                                 mut retval: v8::ReturnValue| {})
                            .data(setter_declaration_ext.into())
                            .build(scope));
                    }


                    if property.is_static() {
                        // todo
                    } else {
                        let name = name.unwrap();
                        proto.set_accessor_property(name.into(), Some(getter), setter, v8::PropertyAttribute::READ_ONLY | v8::PropertyAttribute::DONT_DELETE);
                    }
                }
            }
            DeclarationKind::GenericInterface => {
                let clazz = lock.as_any().downcast_ref::<GenericInterfaceDeclaration>().unwrap();

                let return_types = helpers::get_generic_return_types(name);

                for method in clazz.methods() {
                    let signature = method.return_type();

                    let return_type = Signature::to_string(method.metadata().unwrap(), &signature);

                    let return_type_index = usize::from_str_radix(&*return_type.as_str().replace("Var!", ""), 10).unwrap();

                    let return_type = *return_types.names().get(return_type_index).unwrap();

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

                    let return_type = v8::String::new(scope, return_type).unwrap();

                    let ext = v8::External::new(scope, declaration as _);

                    let data = v8::Array::new_with_elements(scope, &[ext.into(), return_type.into()]);

                    let func = FunctionTemplate::builder(|scope: &mut v8::HandleScope,
                                                          args: v8::FunctionCallbackArguments,
                                                          mut retval: v8::ReturnValue| {
                        let data = v8::Local::<v8::Array>::try_from(args.data()).unwrap();

                        let return_type = data.get_index(scope, 1).unwrap().to_rust_string_lossy(scope);

                        let dec = unsafe { Local::<v8::External>::cast(data.get_index(scope, 0).unwrap()) };

                        let dec = dec.value() as *mut DeclarationFFI;

                        let dec = unsafe { &*dec };

                        let lock = dec.read();

                        let method = lock.as_any().downcast_ref::<MethodDeclaration>().unwrap();

                        let parent = dec.parent.as_ref().unwrap();
                        let parent = parent.read();
                        let parent = parent.as_any().downcast_ref::<GenericInterfaceDeclaration>().unwrap();


                        let mut method = GenericMethodCall::new(
                            parent, method, method.is_sealed(), dec.instance.clone().unwrap(), false, return_type,
                        );

                        let (ret, result) = method.call(scope, &args);

                        if ret.is_err() {
                            println!(">>> {}", ret.message().to_string())
                        } else if !method.is_void() {
                            let return_sig = method.return_type();
                            match NativeType::try_from(return_sig) {
                                Ok(return_type) => {
                                    if return_sig.contains(".") {
                                        let instance = unsafe { IUnknown::from_raw(*(result as *mut *mut c_void)) };

                                        if return_sig.contains('`') {
                                            let mut name = return_sig.to_string();

                                            if let Some(backtick_index) = name.rfind('<') {
                                                name.truncate(backtick_index);
                                            }

                                            // use the generic name
                                            let declaration = MetadataReader::find_by_name(name.as_str()).unwrap();


                                            let ret: Local<v8::Value> = create_ns_ctor_instance_object(return_sig, None, dec.parent.clone(), declaration, Some(instance), scope).into();
                                            retval.set(ret.into());

                                            return;
                                        }
                                        let ret: Local<v8::Value> = create_ns_ctor_instance_object(return_sig, None, dec.parent.clone(), dec.inner.clone(), Some(instance), scope).into();
                                        retval.set(ret.into());
                                        return;
                                    }
                                    unsafe { set_ret_val(result, scope, retval, return_type); }
                                }
                                Err(_) => {}
                            }
                        } else {
                            retval.set_undefined();
                        }


                        // todo
                    })
                        .data(data.into())
                        .build(scope);

                    if is_static {
                        tmpl.set_with_attr(name.unwrap().into(), func.into(), v8::PropertyAttribute::DONT_DELETE);
                    } else {
                        proto.set_with_attr(name.unwrap().into(), func.into(), v8::PropertyAttribute::DONT_DELETE);
                    }
                }
            }
            _ => {}
        }
    }

    let object_tmpl = tmpl.instance_template(scope);

    object_tmpl.set_internal_field_count(1);

    let object = object_tmpl.new_instance(scope).unwrap();

    let declaration = Box::new(DeclarationFFI::new_with_instance(declaration, instance));
    let ext = v8::External::new(scope, Box::into_raw(declaration) as _);
    object.set_internal_field(0, ext.into());

    let ret = scope.escape(object);

    ret.into()
}

fn create_ns_ctor_object<'a>(name: &str, parent: Option<Arc<RwLock<dyn Declaration>>>, declaration: Arc<RwLock<dyn Declaration>>, scope: &mut v8::HandleScope<'a>) -> Local<'a, v8::Value> {
    let scope = &mut v8::EscapableHandleScope::new(scope);

    let name = v8::String::new(scope, name).unwrap();

    let mut ext = DeclarationFFI::new(Arc::clone(&declaration));

    ext.parent = parent;

    let ext = Box::into_raw(Box::new(ext));

    let ext = v8::External::new(scope, ext as _);

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
                        let number_of_parameters = ctor.number_of_parameters();
                        if number_of_parameters != length as usize {
                            continue;
                        }
                        let mut method = MethodCall::new(
                            ctor, is_sealed, clazz_factory.clone(), true,
                        );

                        let (ret, result) = method.call(scope, &args);

                        if ret.is_ok() {
                            let result = unsafe { IUnknown::from_raw(result) };

                            let vtable = result.vtable();

                            let mut ret: *mut c_void = std::ptr::null_mut();

                            let res = unsafe {
                                ((*vtable).QueryInterface)(
                                    result.as_raw(),
                                    &IUnknown::IID,
                                    std::mem::transmute(&mut ret),
                                )
                            };

                            assert!(res.is_ok());
                            assert!(!ret.is_null());

                            let result = IUnknown::from_raw(ret);

                            let instance = create_ns_ctor_instance_object(clazz.name(), Some(clazz_factory), None, dec.inner.clone(), Some(result), scope);
                            retval.set(instance);

                            return;
                        } else {
                            let message = ret.message().to_string();
                            let message = v8::String::new(scope, message.as_str()).unwrap();
                            let error = v8::Exception::error(scope, message.into());
                            scope.throw_exception(error);
                        }
                    }
                }
            }
            DeclarationKind::Struct => {}
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
        object_tmpl.set_internal_field_count(1);
        let object = object_tmpl.new_instance(scope).unwrap();

        object.set_internal_field(0, ext);

        let object_store = v8::Map::new(scope);
        object.set_internal_field(1, object_store.into());

        retval.set(object.into());
    })
        .data(ext.into()).build(scope);
    tmpl.set_class_name(name);


    {
        let lock = declaration.read();

        let clazz = lock.as_any().downcast_ref::<ClassDeclaration>().unwrap();

        let clazz_name = HSTRING::from(clazz.full_name());

        let clazz_factory = unsafe { RoGetActivationFactory::<IUnknown>(&clazz_name) };

        let clazz_factory = clazz_factory.unwrap();

        for method in clazz.methods().iter() {
            let name = v8::String::new(scope, method.name());
            let is_static = method.is_static();

            if !is_static {
                continue;
            }

            let parent = Arc::clone(&declaration);

            let mut declaration = DeclarationFFI::new_with_instance(
                Arc::new(
                    RwLock::new(
                        method.clone()
                    )
                ),
                Some(clazz_factory.clone()),
            );

            declaration.parent = Some(parent);

            let declaration = Box::into_raw(Box::new(declaration));

            let ext = v8::External::new(scope, declaration as _);

            let func = v8::FunctionTemplate::builder(|scope: &mut v8::HandleScope,
                                                      args: v8::FunctionCallbackArguments,
                                                      mut retval: v8::ReturnValue| {
                let dec = unsafe { Local::<v8::External>::cast(args.data()) };

                let dec = dec.value() as *mut DeclarationFFI;

                let dec = unsafe { &*dec };

                let lock = dec.read();

                let method = lock.as_any().downcast_ref::<MethodDeclaration>().unwrap();

                let return_type = method.return_type();

                let signature = Signature::to_string(method.metadata().unwrap(), &return_type);


                let mut method = MethodCall::new(
                    method, method.is_sealed(), dec.instance.clone().unwrap(), false,
                );

                let (ret, result) = method.call(scope, &args);


                if ret.is_ok() {
                    unsafe {
                        match signature.as_str() {
                            "Boolean" => {
                                retval.set_bool(
                                    *(result as *mut bool)
                                )
                            }
                            _ => {
                                let instance = IUnknown::from_raw(result);

                                let declaration = method.declaration.clone().unwrap();

                                let lock = declaration.read();

                                let declaration: Arc<RwLock<dyn Declaration>>;

                                {
                                    let lock = lock;

                                    match lock.base().kind() {
                                        DeclarationKind::Interface => {
                                            let dec = lock.as_declaration().as_any().downcast_ref::<InterfaceDeclaration>();


                                            declaration = Arc::new(
                                                RwLock::new(dec.unwrap().clone())
                                            )
                                        }
                                        DeclarationKind::Class => {
                                            let dec = lock.as_declaration().as_any().downcast_ref::<ClassDeclaration>();
                                            declaration = Arc::new(
                                                RwLock::new(dec.unwrap().clone())
                                            )
                                        }
                                        _ => {
                                            // todo
                                            unimplemented!()
                                        }
                                    }
                                }


                                let ret: Local<v8::Value> = create_ns_ctor_instance_object(signature.as_str(), dec.instance.clone(), dec.parent.clone(), declaration, Some(instance), scope).into();
                                retval.set(ret.into());
                            }
                        }
                    }
                } else {
                    let message = ret.message().to_string();
                    let message = v8::String::new(scope, message.as_str()).unwrap();
                    let error = v8::Exception::error(scope, message.into());
                    scope.throw_exception(error);
                }
            })
                .data(ext.into())
                .build(scope);

            tmpl.set(name.unwrap().into(), func.into());
        }
    }

    let func = tmpl.get_function(scope).unwrap();
    let ret = scope.escape(func);
    ret.into()
}

fn create_ns_struct_ctor_object<'a>(name: &str, declaration: Arc<RwLock<dyn Declaration>>, scope: &mut v8::HandleScope<'a>) -> Local<'a, v8::Value> {
    let scope = &mut v8::EscapableHandleScope::new(scope);

    let name = v8::String::new(scope, name).unwrap();

    let mut ext = DeclarationFFI::new(Arc::clone(&declaration));

    let ext = Box::into_raw(Box::new(ext));

    let ext = v8::External::new(scope, ext as _);

    let tmpl = FunctionTemplate::builder(|scope: &mut v8::HandleScope,
                                          args: v8::FunctionCallbackArguments,
                                          mut retval: v8::ReturnValue| {
        let dec = unsafe { Local::<v8::External>::cast(args.data()) };

        let dec = dec.value() as *mut DeclarationFFI;

        let dec = unsafe { &mut *dec };

        let lock = dec.write();

        let ext = args.data();

        let object_tmpl = v8::ObjectTemplate::new(scope);

        let mut field_args: Vec<NativeValue> = Vec::new();

        let mut field_types: Vec<NativeType> = Vec::new();

        let struct_dec = lock.as_any().downcast_ref::<StructDeclaration>().unwrap();

        let object = args.get(0).to_object(scope).unwrap();

        for field in struct_dec.fields() {
            let field_type = Signature::to_string(field.base().metadata().unwrap(), &field.type_());

            let native_type = NativeType::try_from(field_type.as_str()).unwrap();

            field_types.push(native_type.clone());

            let name = v8::String::new(scope, field.name()).unwrap();

            let field_value = object.get(scope, name.into());

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
                            // todo
                            unreachable!()
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
                            ffi_parse_i16_arg(field)
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

        assert!(params.is_ok());

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

        let name = v8::String::new(scope, name.as_str()).unwrap();

        let getter = |scope: &mut v8::HandleScope,
                      key: Local<v8::Name>,
                      args: v8::PropertyCallbackArguments,
                      mut rv: v8::ReturnValue| {
            let key = key.to_rust_string_lossy(scope);

            let this = args.data();

            let dec = unsafe { Local::<v8::External>::cast(this) };

            let dec = dec.value() as *mut DeclarationFFI;

            let dec = unsafe { &*dec };

            let lock = dec.read();

            if key == "toString" {
                let name = lock.name();

                let name = v8::String::new(scope, name).unwrap();
                let func = v8::Function::builder(|scope: &mut v8::HandleScope,
                                                  args: v8::FunctionCallbackArguments,
                                                  mut retval: v8::ReturnValue| {
                    retval.set(args.data());
                }).data(name.into())
                    .build(scope);


                rv.set(func.unwrap().into());
                return;
            }

            let struct_dec = lock.as_any().downcast_ref::<StructDeclaration>().unwrap();

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
        };

        let setter = |scope: &mut v8::HandleScope,
                      key: Local<v8::Name>,
                      value: Local<v8::Value>,
                      args: v8::PropertyCallbackArguments,
                      mut rv: v8::ReturnValue| {
            let key = key.to_rust_string_lossy(scope);

            let this = args.data();

            let dec = unsafe { Local::<v8::External>::cast(this) };

            let dec = dec.value() as *mut DeclarationFFI;

            let instance = unsafe { (&mut *dec).struct_instance.as_mut() };

            let mut dec = unsafe { &mut *dec };

            let mut lock = dec.write();

            let struct_dec = lock.as_any().downcast_ref::<StructDeclaration>().unwrap();

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
                                        // todo
                                        unreachable!()
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
                                        ffi_parse_i16_arg(field)
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
        };

        object_tmpl.set_named_property_handler(
            v8::NamedPropertyHandlerConfiguration::new()
                .getter(getter)
                .setter(setter)
                .data(ext)
        );


        let object = object_tmpl.new_instance(scope).unwrap();

        // object.set_internal_field(0, ext);

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
                global.define_own_property(scope, name, object, v8::PropertyAttribute::READ_ONLY | v8::PropertyAttribute::DONT_DELETE | v8::PropertyAttribute::NONE);
            }
        }
    }
}

fn handle_named_property_setter(scope: &mut v8::HandleScope,
                                key: Local<v8::Name>,
                                value: Local<v8::Value>,
                                args: v8::PropertyCallbackArguments,
                                mut rv: v8::ReturnValue) {
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

fn handle_named_property_query(_scope: &mut v8::HandleScope,
                               _key: v8::Local<v8::Name>,
                               _args: v8::PropertyCallbackArguments,
                               mut rv: v8::ReturnValue) {
    // NONE
    rv.set_int32(0);
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
                let parent = dec.inner.clone();
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
                            DeclarationKind::Struct => {
                                let struct_dec = lock.as_any().downcast_ref::<StructDeclaration>().unwrap();
                                let name = struct_dec.name().to_string();
                                drop(lock);

                                let ret = create_ns_struct_ctor_object(name.as_str(), Arc::clone(&dec), scope);
                                rv.set(ret.into());
                            }
                            DeclarationKind::Class => {
                                let ret: Local<v8::Value> = create_ns_ctor_object(lock.name(), Some(parent), declaration, scope).into();
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
                                  args: v8::PropertyCallbackArguments,
                                  mut rv: v8::ReturnValue,
) {}


fn handle_indexed_property_getter(scope: &mut v8::HandleScope,
                                  index: u32,
                                  args: v8::PropertyCallbackArguments,
                                  mut rv: v8::ReturnValue) {}


fn handle_ns_func(scope: &mut v8::HandleScope,
                  _args: v8::FunctionCallbackArguments,
                  mut _retval: v8::ReturnValue) {
    // scope.throw_exception(v8::Exception::error(scope, v8::String::new("")))
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
            let global = v8::FunctionTemplate::builder(|_scope: &mut v8::HandleScope,
                                                               _args: v8::FunctionCallbackArguments,
                                                               mut _retval: v8::ReturnValue|{})
                .constructor_behavior(v8::ConstructorBehavior::Throw)
                .build(scope);

            let class_name = v8::String::new(scope, "NativeScriptGlobalObject").unwrap();
            global.set_class_name(class_name);

            let mut global_template = v8::ObjectTemplate::new_from_template(scope, global);

            global_template.set_internal_field_count(1);

            {
                let template = &mut global_template;

                globals::performance::init_performance(scope, template);

                globals::time::init_time(scope, template);

                let context = v8::Context::new_from_template(scope, global_template);
                {
                    let scope = &mut v8::ContextScope::new(scope, context);

                    init_global(scope, context);
                    globals::console::init_console(scope, context);
                    init_meta(scope, context);
                    global_context = Global::new(scope, context);
                }
            }
        }

        let runtime = runtime::Builder::new_current_thread()
            .enable_all()
            .build().unwrap();

        let runtime = Arc::new(RwLock::new(runtime));

        isolate.set_slot(Arc::clone(&runtime));

        Self {
            isolate,
            global_context,
            app_root: app_root.to_string(),
            runtime,
        }
    }

    pub fn run_script(&mut self, script: &str) {
        let runtime = self.runtime.read();
        runtime.block_on(async {
            let isolate = &mut self.isolate;
            let scope = &mut v8::HandleScope::new(isolate);
            let context = v8::Local::new(scope, &self.global_context);
            let scope = &mut v8::ContextScope::new(scope, context);
            let code = v8::String::new(scope, script).unwrap();
            let script = v8::Script::compile(scope, code, None).unwrap();
            let _ = script.run(scope);
            tokio::task::yield_now().await
        });
    }

    pub fn dispose(&self) {
        /* unsafe {
             CoUninitialize();
         }*/
    }
}