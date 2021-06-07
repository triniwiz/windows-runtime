#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

windows::include_bindings!();


include!(concat!(env!("OUT_DIR"), "/bindings.rs"));


impl GUID {
	pub fn new() -> Self {
		Self {
			Data1: 0,
			Data2: 0,
			Data3: 0,
			Data4: [0_u8;8]
		}
	}
}

impl Default for GUID {
	fn default() -> Self {
		Self::new()
	}
}