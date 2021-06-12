use rusty_v8 as v8;
use core_bindings::{Windows::Win32::System::Console, Windows::Win32::System::WindowsProgramming};
use std::ffi::{CString, c_void};

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

	let length = value.chars().count() as u32;
	let handle = unsafe { WindowsProgramming::GetStdHandle(WindowsProgramming::STD_OUTPUT_HANDLE) };
	let string = CString::new(value).unwrap();
	let mut written = 0_u32;
	unsafe {
		Console::WriteConsoleA(handle, string.as_ptr() as *const c_void, length, &mut written, std::ptr::null() as _);
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
	let length = value.chars().count() as u32;
	let handle = unsafe { WindowsProgramming::GetStdHandle(WindowsProgramming::STD_OUTPUT_HANDLE) };

	let string = CString::new(value).unwrap();
	let mut written = 0_u32;
	unsafe {
		Console::WriteConsoleA(handle, string.as_ptr() as *const c_void, length, &mut written, std::ptr::null());
	}
}