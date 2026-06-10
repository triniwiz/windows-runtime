use libffi::middle::{Arg, Cif, CodePtr, Type};
use std::{ffi::c_void, mem, ptr};
use windows::core::Interface;
use windows::core::HSTRING;
use windows::Data::Json::IJsonValueStatics;
use windows::Win32::System::WinRT::{RoGetActivationFactory, RoInitialize, RO_INIT_MULTITHREADED};

#[test]
fn libffi_create_values() {
    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);

        let class_name: HSTRING = HSTRING::from("Windows.Data.Json.JsonValue");
        let statics = RoGetActivationFactory::<IJsonValueStatics>(&class_name)
            .expect("RoGetActivationFactory failed");
        let statics_ptr: *mut c_void = statics.as_raw() as *mut c_void;
        let vtable_ptr_ptr: *mut *mut c_void = mem::transmute(statics_ptr);
        let vtable_ptr = *vtable_ptr_ptr as *mut *mut c_void;

        eprintln!(
            "[LIBFFI] statics_ptr={:p} vtable_ptr={:p}",
            statics_ptr, vtable_ptr
        );
        for i in 0..12isize {
            let fp = *vtable_ptr.offset(i);
            eprintln!("[LIBFFI] vtable[{}] = {:p}", i, fp);
        }

        // Offsets: IInspectable base (6 slots) then Parse(6), TryParse(7), CreateBooleanValue(8), CreateNumberValue(9)
        let create_bool_off = 8isize;
        let create_num_off = 9isize;

        // CreateBooleanValue(this, bool, out **obj) -> HRESULT
        {
            let func_ptr = *vtable_ptr.offset(create_bool_off) as *const c_void;
            eprintln!("[LIBFFI] calling CreateBooleanValue at {:p}", func_ptr);

            type CreateBoolFn = unsafe extern "system" fn(
                this: *mut c_void,
                value: u8,
                result: *mut *mut c_void,
            ) -> i32;
            let func_typed: CreateBoolFn = mem::transmute(func_ptr);
            let mut direct_result: *mut c_void = ptr::null_mut();
            let direct_hr = unsafe { func_typed(statics_ptr, 1u8, &mut direct_result) };
            eprintln!(
                "[LIBFFI] direct CreateBooleanValue hr={:#x} result={:p}",
                direct_hr, direct_result
            );
            assert_eq!(
                direct_hr, 0,
                "direct CreateBooleanValue failed hr={:#x}",
                direct_hr
            );

            let cif = Cif::new(vec![Type::usize(), Type::u32(), Type::usize()], Type::i32());
            let this_ptr_usize: usize = statics_ptr as usize;
            let arg_bool_u32: u32 = 1;
            let mut result: *mut c_void = ptr::null_mut();
            let mut result_usize: usize = &mut result as *mut _ as usize;

            // Diagnostic frame build
            let mut frame_i32: Vec<u8> = Vec::new();
            let ptr_size = std::mem::size_of::<usize>();
            frame_i32.extend_from_slice(&this_ptr_usize.to_le_bytes()[0..ptr_size]);
            frame_i32.extend_from_slice(&arg_bool_u32.to_le_bytes());
            frame_i32
                .extend_from_slice(&((&mut result as *mut _ as usize).to_le_bytes())[0..ptr_size]);
            let mut hex = String::new();
            for (i, b) in frame_i32.iter().enumerate() {
                hex.push_str(&format!("{:02x}", b));
                if i % 4 == 3 {
                    hex.push(' ');
                }
            }
            eprintln!(
                "[LIBFFI] libffi contiguous frame (bool as u8) total_size={} bytes={}",
                frame_i32.len(),
                hex
            );

            let hr: i32 = cif.call(
                CodePtr::from_ptr(func_ptr),
                &[
                    Arg::new(&this_ptr_usize),
                    Arg::new(&arg_bool_u32),
                    Arg::new(&mut result_usize),
                ],
            );
            if result_usize != 0 {
                result = result_usize as *mut c_void;
            }
            eprintln!(
                "[LIBFFI] CreateBooleanValue returned hr={:#x} result={:p}",
                hr, result
            );
            assert_eq!(hr, 0, "CreateBooleanValue failed hr={:#x}", hr);
            assert!(!result.is_null(), "CreateBooleanValue returned null");
        }

        // CreateNumberValue(this, f64, out **obj) -> HRESULT
        {
            let func_ptr = *vtable_ptr.offset(create_num_off) as *const c_void;
            eprintln!("[LIBFFI] calling CreateNumberValue at {:p}", func_ptr);
            let cif = Cif::new(vec![Type::usize(), Type::f64(), Type::usize()], Type::i32());
            let this_ptr_usize: usize = statics_ptr as usize;
            let arg_num: f64 = 3.14159;
            let mut result: *mut c_void = ptr::null_mut();
            let mut result_usize: usize = &mut result as *mut _ as usize;
            let hr: i32 = cif.call(
                CodePtr::from_ptr(func_ptr),
                &[
                    Arg::new(&this_ptr_usize),
                    Arg::new(&arg_num),
                    Arg::new(&mut result_usize),
                ],
            );
            if result_usize != 0 {
                result = result_usize as *mut c_void;
            }
            eprintln!(
                "[LIBFFI] CreateNumberValue returned hr={:#x} result={:p}",
                hr, result
            );
            assert_eq!(hr, 0, "CreateNumberValue failed hr={:#x}", hr);
            assert!(!result.is_null(), "CreateNumberValue returned null");
        }
    }
}

#[test]
fn libffi_create_string_value() {
    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);

        let class_name: HSTRING = HSTRING::from("Windows.Data.Json.JsonValue");
        let statics = RoGetActivationFactory::<IJsonValueStatics>(&class_name)
            .expect("RoGetActivationFactory failed");
        let statics_ptr: *mut c_void = statics.as_raw() as *mut c_void;
        let vtable_ptr_ptr: *mut *mut c_void = mem::transmute(statics_ptr);
        let vtable_ptr = *vtable_ptr_ptr as *mut *mut c_void;

        let create_str_off = 10isize;
        let func_ptr = *vtable_ptr.offset(create_str_off) as *const c_void;

        // Validate with a typed direct call first.
        type CreateStringFn = unsafe extern "system" fn(
            this: *mut c_void,
            value: windows::core::HSTRING,
            result: *mut *mut c_void,
        ) -> i32;
        let func_typed: CreateStringFn = mem::transmute(func_ptr);
        let typed_h = HSTRING::from("hello libffi");
        let typed_h_clone = typed_h.clone();
        let typed_h_raw: usize = unsafe { mem::transmute(typed_h_clone) };
        let mut typed_result: *mut c_void = ptr::null_mut();
        let typed_hr = unsafe { func_typed(statics_ptr, typed_h, &mut typed_result) };
        eprintln!(
            "[LIBFFI] typed CreateStringValue hr={:#x} result={:p} typed_h_raw={:#x}",
            typed_hr, typed_result, typed_h_raw
        );
        assert_eq!(typed_hr, 0, "typed CreateStringValue failed");

        // libffi: pass `this`, `HSTRING` handle, and `out` pointer as `usize`
        // per WinRT x64 ABI (handle-sized values).
        let cif = Cif::new(
            vec![Type::usize(), Type::usize(), Type::usize()],
            Type::i32(),
        );
        let this_ptr_usize: usize = statics_ptr as usize;

        let h = HSTRING::from("hello libffi");
        // Extract the internal handle pointer from a clone by transmuting
        // the cloned `HSTRING` into a `usize` (repr(transparent) over a pointer).
        let h_clone = h.clone();
        let h_raw: usize = unsafe { mem::transmute(h_clone) };
        let mut result: *mut c_void = ptr::null_mut();
        let mut result_usize: usize = &mut result as *mut _ as usize;

        let hr: i32 = cif.call(
            CodePtr::from_ptr(func_ptr),
            &[
                Arg::new(&this_ptr_usize),
                Arg::new(&h_raw),
                Arg::new(&mut result_usize),
            ],
        );
        if result_usize != 0 {
            result = result_usize as *mut c_void;
        }
        eprintln!(
            "[LIBFFI] libffi CreateStringValue hr={:#x} result={:p} libffi_h_raw={:#x}",
            hr, result, h_raw
        );

        eprintln!("[LIBFFI] typed_raw==libffi_raw? {}", typed_h_raw == h_raw);

        // Reconstruct the cloned HSTRING from the raw usize so it is dropped.
        let _recon: HSTRING = unsafe { mem::transmute(h_raw) };
        // Reconstruct the typed clone we extracted earlier so it is dropped.
        let _recon_typed: HSTRING = unsafe { mem::transmute(typed_h_raw) };

        assert_eq!(hr, 0, "libffi CreateStringValue failed hr={:#x}", hr);
        assert!(!result.is_null(), "libffi CreateStringValue returned null");
    }
}
