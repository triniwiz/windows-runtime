use std::ffi::{c_int, CString};
use std::sync::OnceLock;
use windows::core::PCSTR;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::{Console};
use windows::Win32::System::Console::{CONSOLE_MODE, GetConsoleMode, GetStdHandle, STD_OUTPUT_HANDLE};
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

    let global = context.global(scope);
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

/// Detect once whether stdout is attached to a real console. Packaged AppX
/// apps (and most GUI apps) have no console — `WriteConsoleA` fails silently
/// every call, and `print!` writes into a buffer nobody reads. Probing once
/// at startup lets the hot path go straight to `OutputDebugStringA`, which
/// is what the user actually sees in the VS Output window.
fn console_handle() -> Option<HANDLE> {
    static PROBED: OnceLock<Option<isize>> = OnceLock::new();
    let raw = *PROBED.get_or_init(|| unsafe {
        let h = GetStdHandle(STD_OUTPUT_HANDLE).ok()?;
        if h.is_invalid() {
            return None;
        }
        // GetConsoleMode succeeds only if `h` points at a real console
        // screen buffer — perfect detector for "is there actually a console
        // to write to". Returns false for the null handle UWP apps get.
        let mut mode = CONSOLE_MODE::default();
        if GetConsoleMode(h, &mut mode).is_ok() {
            Some(h.0 as isize)
        } else {
            None
        }
    });
    raw.map(|p| HANDLE(p as *mut _))
}

fn write_console(value: &str) {
    if let Some(handle) = console_handle() {
        let _ = unsafe { Console::WriteConsoleA(handle, value.as_bytes(), None, None) };
        return;
    }
    if let Ok(c) = CString::new(value) {
        unsafe { OutputDebugStringA(PCSTR::from_raw(c.as_ptr() as _)) }
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