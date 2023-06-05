use libffi::low::*;
use libffi::raw::ffi_type_sint32;
//
// pub fn to_native(scope: &mut v8::HandleScope, value: v8::Value) -> Type {
//     if value.is_null() {
//         // is null a ptr ?
//         return Type::pointer()
//     }
//
//     if value.is_string() {
//         return Type::pointer()
//     }
//
//     if value.is_boolean() {
//         return Type::u8()
//     }
//
//     if value.is_number() {
//         return ffi_type_sint32
//     }
//
//     Type::pointer()
// }