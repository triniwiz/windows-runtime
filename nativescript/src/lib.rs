use std::ffi::{c_char, CStr};
use runtime::Runtime;

#[no_mangle]
pub extern fn runtime_init(app_root: *const c_char) -> i64 {
    if app_root.is_null() {
        return Box::into_raw(Box::new(Runtime::new(""))) as i64
    }
    let string = unsafe { CStr::from_ptr(app_root) }.to_string_lossy();
    Box::into_raw(Box::new(Runtime::new(string.as_ref()))) as i64
}

#[no_mangle]
pub extern fn runtime_deinit(runtime: i64) {
    if runtime != 0 {
        let runtime: *mut Runtime = runtime as _;
        let _ = unsafe { Box::from_raw(runtime) };
    }
}

#[no_mangle]
pub extern fn runtime_runscript(runtime: i64, script: *const c_char) {
    if runtime != 0 {
        let runtime: *mut Runtime = runtime as _;
        let runtime = unsafe { &mut *runtime };
        let script = unsafe { CStr::from_ptr(script) }.to_string_lossy();
        runtime.run_script(script.as_ref());
    }
}