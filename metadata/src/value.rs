use std::ffi::c_void;
use std::mem::ManuallyDrop;
use windows::core::{BSTR, Interface};
use windows::Win32::Foundation::VARIANT_BOOL;
use windows::Win32::System::Com::{IDispatch, SAFEARRAY, VARENUM, VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_ARRAY, VT_BOOL, VT_BSTR, VT_I1, VT_I2, VT_I4, VT_I8, VT_NULL, VT_PTR, VT_R4, VT_R8, VT_UI2, VT_UI4, VT_UI8, VT_VARIANT};

#[derive(Debug)]
pub enum Value {
    Void(*mut c_void),
    Boolean(bool),
    Char16(char),
    Int8(i8),
    Uint8(u8),
    Int16(i16),
    Uint16(u16),
    Int32(i32),
    Uint32(u32),
    Int64(i64),
    Uint64(u64),
    Single(f32),
    Double(f64),
    String(String),
    Object(*mut c_void),
    SZArray(Vec<Value>),
}

pub struct Variant(VARIANT);

impl Variant {
    pub fn new(venum: VARENUM, contents: VARIANT_0_0_0) -> Self {
        Self {
            0: VARIANT {
                Anonymous: VARIANT_0 {
                    Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                        vt: venum,
                        wReserved1: 0,
                        wReserved2: 0,
                        wReserved3: 0,
                        Anonymous: contents,
                    })
                }
            }
        }
    }

    pub fn null() -> Self {
        Self::new(VT_NULL, VARIANT_0_0_0::default())
    }

    pub fn as_abi(&self) -> &VARIANT {
        &self.0
    }
}

impl From<bool> for Variant {
    fn from(value: bool) -> Self {
        Variant::new(
            VT_BOOL, VARIANT_0_0_0 { boolVal: value.into() },
        )
    }
}

impl From<String> for Variant {
    fn from(value: String) -> Self {
        Variant::new(
            VT_BSTR, VARIANT_0_0_0 { bstrVal: ManuallyDrop::new(BSTR::from(value)) },
        )
    }
}

impl From<&str> for Variant {
    fn from(value: &str) -> Self {
        Variant::from(value.to_string())
    }
}

impl From<char> for Variant {
    fn from(value: char) -> Self {
        Variant::new(
            VT_UI2, VARIANT_0_0_0 { uiVal: value as u16 },
        )
    }
}

impl From<i8> for Variant {
    fn from(value: i8) -> Self {
        Variant::new(
            VT_I1, VARIANT_0_0_0 { cVal: value as u8 },
        )
    }
}

impl From<u8> for Variant {
    fn from(value: u8) -> Self {
        Variant::new(
            VT_UI4, VARIANT_0_0_0 { bVal: value },
        )
    }
}

impl From<i16> for Variant {
    fn from(value: i16) -> Self {
        Variant::new(
            VT_I2, VARIANT_0_0_0 { iVal: value },
        )
    }
}

impl From<u16> for Variant {
    fn from(value: u16) -> Self {
        Variant::new(
            VT_UI2, VARIANT_0_0_0 { uiVal: value },
        )
    }
}

impl From<i32> for Variant {
    fn from(value: i32) -> Self {
        Variant::new(
            VT_I4, VARIANT_0_0_0 { lVal: value },
        )
    }
}

impl From<u32> for Variant {
    fn from(value: u32) -> Self {
        Variant::new(
            VT_UI4, VARIANT_0_0_0 { ulVal: value },
        )
    }
}

impl From<i64> for Variant {
    fn from(value: i64) -> Self {
        Variant::new(
            VT_I8, VARIANT_0_0_0 { llVal: value },
        )
    }
}

impl From<u64> for Variant {
    fn from(value: u64) -> Self {
        Variant::new(
            VT_UI8, VARIANT_0_0_0 { ullVal: value },
        )
    }
}

impl From<f32> for Variant {
    fn from(value: f32) -> Self {
        Variant::new(
            VT_R4, VARIANT_0_0_0 { fltVal: value },
        )
    }
}

impl From<f64> for Variant {
    fn from(value: f64) -> Self {
        Variant::new(
            VT_R8, VARIANT_0_0_0 { dblVal: value },
        )
    }
}

impl From<*mut c_void> for Variant {
    fn from(value: *mut c_void) -> Self {
        let ret = if value.is_null() {
            None
        } else {
            Some(unsafe { IDispatch::from_raw(value) })
        };

        Variant::new(
            VT_PTR, VARIANT_0_0_0 {
                pdispVal: ManuallyDrop::new(
                    ret
                )
            },
        )
    }
}

impl Drop for Variant {
    fn drop(&mut self) {
        unsafe {
            match VARENUM(self.0.Anonymous.Anonymous.vt.0) {
                VT_BSTR => {
                    drop(&mut &self.0.Anonymous.Anonymous.Anonymous.bstrVal)
                }
                _ => {}
            }
            drop(&mut self.0.Anonymous.Anonymous)
        }
    }
}

impl Into<Variant> for Value {
    fn into(self) -> Variant {
        match self {
            Value::Void(value) => {
                Variant::from(value)
            }
            Value::Boolean(value) => {
                Variant::from(value)
            }
            Value::Char16(value) => {
                Variant::from(value)
            }
            Value::Int8(value) => {
                Variant::from(value)
            }
            Value::Uint8(value) => {
                Variant::from(value)
            }
            Value::Int16(value) => {
                Variant::from(value)
            }
            Value::Uint16(value) => {
                Variant::from(value)
            }
            Value::Int32(value) => {
                Variant::from(value)
            }
            Value::Uint32(value) => {
                Variant::from(value)
            }
            Value::Int64(value) => {
                Variant::from(value)
            }
            Value::Uint64(value) => {
                Variant::from(value)
            }
            Value::Single(value) => {
                Variant::from(value)
            }
            Value::Double(value) => {
                Variant::from(value)
            }
            Value::String(value) => {
                Variant::from(value.as_str())
            }
            Value::Object(value) => {
                Variant::from(value)
            }
            Value::SZArray(_) => {
                // todo
                Variant::null()
            }
        }
    }
}
