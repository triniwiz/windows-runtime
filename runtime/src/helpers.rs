use std::ffi::c_void;
use regex::{Error, Regex};
use windows::core::{ComInterface, Interface, IUnknown};
use windows::Foundation::IAsyncInfo;

pub struct GenericReturnTypes <'s> {
    names: Vec<&'s str>,
    types: usize,
}

impl GenericReturnTypes<'_> {
    pub fn names(&self) -> &[&str] {
        self.names.as_slice()
    }

    pub fn types(&self) -> usize {
        self.types
    }
}

pub fn get_generic_return_types(name: &str) -> GenericReturnTypes {
    let types = match Regex::new(r"`(\d+)") {
        Ok(types) => {
            if let Some(captures) = types.captures(name) {
                captures.get(1).unwrap().as_str().parse::<usize>().unwrap()
            } else {
                0
            }
        }
        Err(_) => 0
    };

    let names = match Regex::new(r"<(.*?)>") {
        Ok(names) => {
            if let Some(captures) = names.captures(name) {
                captures.get(1).unwrap().as_str().split(", ").collect::<Vec<_>>()
            } else {
                vec![]
            }
        }
        Err(_) => vec![]
    };

    GenericReturnTypes {
        names,
        types,
    }
}

pub fn is_async(interface: &IUnknown) -> bool {
    let vtable = interface.vtable();

    let mut interface_ptr: *mut c_void = std::ptr::null_mut();

    let _ = unsafe {
        ((*vtable).QueryInterface)(
            interface.as_raw(),
            &IAsyncInfo::IID,
            &mut interface_ptr as *mut _ as *mut *const c_void,
        )
    };

    if !interface_ptr.is_null() {
        let _ = unsafe { IUnknown::from_raw(interface_ptr) };
        return true;
    }

    false
}