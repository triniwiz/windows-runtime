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

fn handle_item_log(scope: &mut v8::PinScope<'_, '_>, item: v8::Local<v8::Value>, output: &mut String, is_last: bool) {
    if item.is_array() {
        let item = v8::Local::<v8::Array>::try_from(item).unwrap();
        let length = item.length() as usize;
        for i in 0..length {
            let inner_is_last = is_last && i == length.saturating_sub(1);
            if let Some(child) = item.get_index(scope, i as u32) {
                handle_item_log(scope, child, output, inner_is_last);
            }
        }
    } else {
        output.push_str(&item.to_rust_string_lossy(scope));
        if !is_last {
            // Standard JS console.log separator: a single space.
            output.push(' ');
        }
    }
}

fn write_console(value: &str) {
    let handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    match handle {
        Ok(handle) => {
            let result = unsafe { Console::WriteConsoleA(handle, value.as_bytes(), None, None) };
            if result.is_err() {
                print!("{value}");
            }
        }
        Err(_) => {
            if let Ok(c) = CString::new(value) {
                unsafe { OutputDebugStringA(PCSTR::from_raw(c.as_ptr() as _)) }
            }
        }
    }
}

pub(crate) fn handle_console_log(scope: &mut v8::PinScope<'_, '_>, args: v8::FunctionCallbackArguments, _retval: v8::ReturnValue) {
    let mut value = String::new();
    let length = args.length() as usize;
    for i in 0..length {
        let is_last = i == length.saturating_sub(1);
        handle_item_log(scope, args.get(i as c_int), &mut value, is_last);
    }
    value.push('\n');
    write_console(&value);
}

pub(crate) fn handle_console_dir(scope: &mut v8::PinScope<'_, '_>, args: v8::FunctionCallbackArguments, _retval: v8::ReturnValue) {
    let mut value = String::new();
    let length = args.length() as usize;
    for i in 0..length {
        let is_last = i == length.saturating_sub(1);
        handle_item_log(scope, args.get(i as c_int), &mut value, is_last);
    }
    value.push('\n');
    write_console(&value);
}