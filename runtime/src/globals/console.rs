use std::env::var;
use std::ffi::{c_int, CString};
use v8::{Local, Value};
use windows::core::{HSTRING, PCSTR, PCWSTR};
use windows::Win32::System::{Console};
use windows::Win32::System::Console::{GetStdHandle, STD_OUTPUT_HANDLE};
use windows::Win32::System::Diagnostics::Debug::OutputDebugStringA;

pub fn init_console(scope: &mut v8::ContextScope<v8::HandleScope<v8::Context>>, context: v8::Local<v8::Context>) {
    let console = v8::Object::new(scope);
    let log = v8::Function::new(scope, handle_console_log).unwrap();
    let dir = v8::Function::new(scope, handle_console_dir).unwrap();

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
    global.define_own_property(scope, value, console.into(), v8::PropertyAttribute::READ_ONLY);
}

fn handle_item_log(scope: &mut v8::HandleScope, item: v8::Local<v8::Value>, output: &mut String, is_last: bool){

    if item.is_array() {
        let item = v8::Local::<v8::Array>::try_from(item).unwrap();
        let length = item.length() as usize;
        for i in 0..length {
            let item = item.get_index(scope, i as u32);
            let is_last = i == length.saturating_sub(1);
            match item {
                None => {}
                Some(item) => {
                    handle_item_log(scope, item, output, is_last);
                }
            }
        }
    }else {
        let item = item.to_rust_string_lossy(scope);
        if is_last {
            output.push_str(&item)
        } else {
            output.push_str(&item);
            output.push_str(", ")
        }
        if is_last {
            output.push_str("\n");
        }
    }
}

pub(crate) fn handle_console_log(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, _retval: v8::ReturnValue) {
    let mut value = String::new();
    let length = args.length() as usize;
    for i in 0..length {
        let item = args.get(i as c_int);
        let is_last = i == length.saturating_sub(1);
        handle_item_log(scope, item, &mut value, is_last);
    }

    let handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };

    match handle {
        Ok(handle) => {
            let result = unsafe {
                Console::WriteConsoleA(handle, value.as_bytes(), None, None)
            };

            // try using println
            if !result.as_bool() {
                println!("{value}");
            }
        }
        Err(_) => {
            let value = CString::new(value).unwrap();
            unsafe {
                OutputDebugStringA(
                    PCSTR::from_raw(value.as_ptr() as _)
                )
            }
        }
    }


}

pub(crate) fn handle_console_dir(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, _retval: v8::ReturnValue) {
    let mut value = String::new();
    for i in 0..args.length() {
        let item = args.get(i).to_rust_string_lossy(scope);
        if i == args.length() - 1 {
            value.push_str(&item)
        } else {
            value.push_str(&item);
            value.push_str(",")
        }
    }
    let handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };

    unsafe {
        let result = Console::WriteConsoleA(handle.unwrap(), value.as_bytes(), None, None);

        // try using println
        if !result.as_bool() {
            println!("{value}");
        }
    }
}