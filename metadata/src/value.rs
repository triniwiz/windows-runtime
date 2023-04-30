use std::ffi::c_void;

#[derive(Debug)]
pub enum Value{
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
    SZArray(Vec<Value>)
}