use std::{ffi::c_void, mem, ptr, io::Write};
use libffi::middle::{Cif, Type, Arg, CodePtr};
use windows::core::HSTRING;
use windows::core::Interface;
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED, RoGetActivationFactory};
use windows::Data::Json::{IJsonValueStatics, IJsonValue, JsonValue};
use windows::core::IUnknown;

#[test]
fn hstring_libffi_roundtrip() {
    unsafe {
        // Write a tiny diagnostic log to temp so we can trace progress even
        // if the process crashes before stdout is flushed.
        let log_path = std::env::temp_dir().join("hstring_test_diag.log");
        let mut log_file = std::fs::OpenOptions::new().create(true).append(true).open(&log_path).ok();
        let mut write_log = |m: &str| {
            if let Some(f) = log_file.as_mut() {
                let _ = f.write_all(m.as_bytes());
                let _ = f.write_all(b"\n");
            }
        };

        write_log("[DIAG TEST] starting hstring_libffi_roundtrip test");
        let _ = RoInitialize(RO_INIT_MULTITHREADED);

        let class_name: HSTRING = HSTRING::from("Windows.Data.Json.JsonValue");
        let statics = RoGetActivationFactory::<IJsonValueStatics>(&class_name).expect("RoGetActivationFactory failed");
        let statics_ptr: *mut c_void = statics.as_raw() as *mut c_void;
        let vtable_ptr_ptr: *mut *mut c_void = mem::transmute(statics_ptr);
        let vtable_ptr = *vtable_ptr_ptr as *mut *mut c_void;

        let create_str_off = 10isize;
        let func_ptr = *vtable_ptr.offset(create_str_off) as *const c_void;

        // Debug: print vtable and pointers before calling anything
        println!("[DIAG TEST] statics_ptr={:p} vtable_ptr_ptr={:p} vtable_ptr={:p}", statics_ptr, vtable_ptr_ptr, vtable_ptr);
        write_log("[DIAG TEST] got statics and vtable");

        // Typed direct vtable call for baseline frame comparison
        type CreateStringValueFn = unsafe extern "system" fn(this: *mut c_void, value: HSTRING, result: *mut *mut c_void) -> i32;
        let func_typed: CreateStringValueFn = mem::transmute(func_ptr);

        let typed_h = HSTRING::from("hello-roundtrip");
        let typed_h_clone = typed_h.clone();
        let typed_h_raw: usize = unsafe { mem::transmute(typed_h_clone) };
        let mut typed_result: *mut c_void = ptr::null_mut();
        let typed_hr: i32 = func_typed(statics_ptr as *mut c_void, typed_h.clone(), &mut typed_result);
        assert_eq!(typed_hr, 0, "typed CreateStringValue failed hr={:#x}", typed_hr);
        assert!(!typed_result.is_null(), "typed CreateStringValue returned null");

        // Baseline contiguous frame for the typed call (for comparison)
        let ptr_size = std::mem::size_of::<usize>();
        let mut typed_frame: Vec<u8> = Vec::new();
        typed_frame.extend_from_slice(&(statics_ptr as usize).to_le_bytes()[0..ptr_size]);
        typed_frame.extend_from_slice(&typed_h_raw.to_le_bytes()[0..ptr_size]);
        typed_frame.extend_from_slice(&(typed_result as usize).to_le_bytes()[0..ptr_size]);
        let mut typed_hex = String::new();
        for (i, b) in typed_frame.iter().enumerate() {
            typed_hex.push_str(&format!("{:02x}", b));
            if i % 4 == 3 { typed_hex.push(' '); }
        }
        println!("[DIAG TEST] typed contiguous frame (bytes={})", typed_hex);
        write_log(&format!("[DIAG TFRAME] bytes={}", typed_hex).as_str());

        // Convert typed result pointer to IUnknown and read string for comparison
        let unknown_typed = IUnknown::from_raw(typed_result);
        let typed_ijv: IJsonValue = unknown_typed.cast::<IJsonValue>().expect("cast to IJsonValue failed");
        let typed_h_res = typed_ijv.GetString().expect("typed GetString failed");
        let typed_s = typed_h_res.to_string();

        // Reconstruct the typed clone we extracted earlier so it is dropped.
        let _recon_typed: HSTRING = unsafe { mem::transmute(typed_h_raw) };

        write_log("[DIAG TEST] before runtime wrapper call");
        let libffi_s = runtime::diag_libffi_create_string_value_via_runtime("hello-roundtrip").expect("runtime libffi wrapper failed");
        write_log("[DIAG TEST] runtime wrapper returned");
        assert_eq!(typed_s, libffi_s, "typed vs libffi string mismatch");
    }
}
