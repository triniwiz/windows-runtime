mod global;
mod ffi;
mod console;

use rusty_v8 as v8;
use std::ffi::{CString, CStr, c_void, OsStr, OsString};
use std::os::raw::{c_char, c_uint, c_long};
use std::io::Read;
use std::os::windows::prelude::*;
use std::path::{Path, PathBuf};
use crate::runtime::metadata::com_helpers::{get_type_name, IMetaDataImport2};
use windows::HSTRING;

pub struct Runtime {
    isolate: v8::OwnedIsolate,
    global_context: v8::Global<v8::Context>,
    app_root: String,
}


const MAX_IDENTIFIER_LENGTH: usize = 511;


impl Runtime {
    pub fn new(app_root: &str) -> Self {
        let platform = v8::new_default_platform().unwrap();
        v8::V8::initialize_platform(platform);
        v8::V8::initialize();
        let mut isolate = v8::Isolate::new(Default::default());

        isolate.set_capture_stack_trace_for_uncaught_exceptions(true, 100);

        let global_context;
        {
            let mut scope = &mut v8::HandleScope::new(&mut isolate);
            let mut global = v8::FunctionTemplate::new(scope, Runtime::handle_global);

            {
                let class_name = v8::String::new(scope, "NativeScriptGlobalObject").unwrap();
                global.set_class_name(class_name);
            }

            let mut global_template = v8::ObjectTemplate::new_from_template(scope, global);

            {
                {
                    Runtime::init_performance(scope, &mut global_template);
                }

                {
                    Runtime::init_time(scope, &mut global_template);
                }


                let mut context = v8::Context::new_from_template(scope, global_template);
                let mut local_scope = v8::ContextScope::new(scope, context);

                {

                    Runtime::init_global(&mut local_scope, context);
                }

                {
                    Runtime::init_console(&mut local_scope, context);
                }

                {
                    Runtime::init_meta(&mut local_scope, context);
                }

                {
                    global_context = v8::Global::new(&mut local_scope, context);
                }

            }
        }

        Self {
            isolate,
            global_context,
            app_root: app_root.to_string(),
        }
    }

    fn run_script(&mut self, script: &str) {
        /*
        let path = PathBuf::from(script);
        println!("??? {:?}", path.to_string_lossy());
        let mut file = std::fs::File::open(std::fs::canonicalize(&path).unwrap()).unwrap();
        let mut data = Vec::new();
        file.read_to_end(&mut data);
        let script = String::from_utf8_lossy(data.as_slice());
        */

        let mut isolate = &mut self.isolate;
        let mut scope = &mut v8::HandleScope::new(isolate);
        let mut local = v8::Local::new(scope, &self.global_context);
        let mut scope = &mut v8::ContextScope::new(scope, local);
        let code = v8::String::new(scope, script).unwrap();
        let script = v8::Script::compile(&mut scope, code, None).unwrap();
        let result = script.run(&mut scope).unwrap();
        let result = result.to_string(&mut scope).unwrap();
    }

    fn handle_global(_scope: &mut v8::HandleScope, _args: v8::FunctionCallbackArguments, _retval: v8::ReturnValue) {}
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

        {
            let name = v8::String::new(scope, "log").unwrap().into();
            console.set(
                scope,
                name,
                log.into(),
            );
        }

        {
            let name = v8::String::new(scope, "dir").unwrap().into();
            console.set(
                scope,
                name,
                dir.into(),
            );
        }

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
        retval.set(
            v8::Number::new(scope, (now.timestamp_millis() / 1000000) as f64).into()
        )
    }

    fn init_time(scope: &mut v8::HandleScope<()>, global: &mut v8::Local<v8::ObjectTemplate>) {
        let time = v8::FunctionTemplate::new(scope, Runtime::handle_time);
        global.set(
            v8::String::new(scope, "time").unwrap().into(), time.into(),
        );
    }

    fn init_performance(scope: &mut v8::HandleScope<()>, global: &mut v8::Local<v8::ObjectTemplate>) {
        let performance = v8::ObjectTemplate::new(scope);
        let now = v8::FunctionTemplate::new(scope, Runtime::handle_now);
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
        retval.set(
            v8::Number::new(scope, now.timestamp_nanos() as f64).into()
        )
    }

    fn init_meta(scope: &mut v8::ContextScope<v8::HandleScope<v8::Context>>, context: v8::Local<v8::Context>) {
        let meta = v8::Function::new(scope, Runtime::handle_meta).unwrap();
        let mut global = context.global(scope);
        let value = v8::String::new(
            scope, "$",
        ).unwrap().into();
        global.define_own_property(scope, value, meta.into(), v8::READ_ONLY);
    }

    fn handle_meta(scope: &mut v8::HandleScope,
                  args: v8::FunctionCallbackArguments,
                  mut retval: v8::ReturnValue) {
        println!("handle meta {:?}", args.get(0).to_rust_string_lossy(scope));
        let typeres_dll = CString::new("api-ms-win-ro-typeresolution-l1-1-0.dll").unwrap();
        let method = CString::new("RoGetMetaDataFile").unwrap();


        let ro_get_meta_data_file: extern "stdcall" fn(windows::HSTRING,
                                             *const c_void,
                                             *const c_void,
                                             *mut *mut c_void,
                                            *mut u32
        ) -> windows::HRESULT;



        let proc = unsafe {
            bindings::Windows::Win32::System::SystemServices::GetProcAddress(
                bindings::Windows::Win32::System::SystemServices::LoadLibraryA(
                    "api-ms-win-ro-typeresolution-l1-1-0.dll"
                ),
                "RoGetMetaDataFile"
            )
        };

        ro_get_meta_data_file = unsafe { std::mem::transmute(proc) };

        println!("RoGetMetaDataFile Address is {}", ro_get_meta_data_file as i64);
        let class_name = args.get(0).to_rust_string_lossy(scope);
        let mut metadata = std::ptr::null_mut() as *mut c_void;
        let mut token = 0_u32;
        let mut metadata_ptr: *mut *mut c_void = &mut metadata;
        // let result = ro_get_meta_data_file(
        //     class_name.into(),
        //     std::ptr::null(),
        //     std::ptr::null(),
        //     metadata,
        //     &mut token
        // );

        let result = unsafe {
            Rometadataresolution_RoGetMetaDataFile(
                class_name.into(),
                std::ptr::null(),
                std::ptr::null(),
                metadata_ptr,
                &mut token
            )
        };


        let COR_CTOR_METHOD_NAME =".ctor";


        println!("metadata is {:?}", metadata);

        println!("token is {:?}", token);
        println!("result is {:?}", result);





        let mut md: *mut IMetaDataImport2 = unsafe {std::mem::transmute(metadata)};

        unsafe {
            let mut name = vec![0_u16;MAX_IDENTIFIER_LENGTH];
            let length = Helpers_Get_Type_Name(md as *const c_void, token, name.as_mut_ptr(), name.len());
            name.resize(length as usize, 0);
            let name = OsString::from_wide(name.as_slice());
             println!("nameaa {:?}", name);
             println!("val {:?}", name.to_string_lossy());
        }
       // let mut md = unsafe { &mut *md };

        let mut parent_token = 0_u32;
        let mut string = String::new();
        let mut name_length = 0;

        unsafe {
         //   IMetaDataImport2_EnumInterfaceImpls(md as _, token);
       //     IMetaDataImport2_GetTypeDefPropsNameSize(md as _, token, &mut name_length);
        }

        println!("name_length {}", name_length);

       // unsafe { IMetaDataImport2_GetTypeDefProps(md as _, token, string.as_mut_ptr() as *mut c_void, string.len() as i64, &mut name_length, 0 as _, &mut parent_token); }

       // md.GetTypeDefProps(token, &mut string, &mut name_length, 0, &mut parent_token);

        println!("parent token {:?}", parent_token);

        let type_name = get_type_name(metadata as *mut c_void, parent_token);

        println!("typename is {:?}", type_name);
    }
}


#[no_mangle]
pub extern fn runtime_init(main_entry: *const c_char) -> i64 {
    let string = unsafe { CStr::from_ptr(main_entry) }.to_string_lossy();
    Box::into_raw(Box::new(Runtime::new(string.as_ref()))) as i64
}

#[no_mangle]
pub extern fn runtime_runscript(runtime: i64, script: *const c_char) {
    if runtime != 0 {
        let mut runtime: *mut Runtime = runtime as _;
        let mut runtime = unsafe { &mut *runtime };
        let script = unsafe { CStr::from_ptr(script) }.to_string_lossy();
        runtime.run_script(script.as_ref());
    }
}