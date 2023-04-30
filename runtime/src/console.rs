use windows::Win32::System::{Console};
use windows::Win32::System::Console::{GetStdHandle, STD_OUTPUT_HANDLE};

pub(crate) fn handle_console_log(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, _retval: v8::ReturnValue) {
    let mut value = String::new();
    for i in 0..args.length() {
        let item = args.get(i).to_rust_string_lossy(scope);
        if i == args.length() - 1 {
            value.push_str(&item)
        } else {
            value.push_str(&item);
            value.push_str(",")
        }
        if i == args.length() - 1 {
            value.push_str("\n");
        }
    }

    let handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };

    let result = unsafe {
        Console::WriteConsoleA(handle.unwrap(), value.as_bytes(), None, None)
    };

    // try using println
    if !result.as_bool() {
        println!("{value}");
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