// Simple integration test to exercise typed vtable calls for JsonValue
// Uses the high-level `windows` crate APIs which call the vtable directly.

#[test]
fn direct_typed_calls() {
    use windows::core::HSTRING;
    use windows::Data::Json::JsonValue;

    // Boolean
    let b = JsonValue::CreateBooleanValue(true).expect("CreateBooleanValue failed");
    println!("CreateBooleanValue succeeded: {:?}", b);

    // Number
    let n = JsonValue::CreateNumberValue(3.14).expect("CreateNumberValue failed");
    println!("CreateNumberValue succeeded: {:?}", n);

    // String
    let s =
        JsonValue::CreateStringValue(&HSTRING::from("hello")).expect("CreateStringValue failed");
    println!("CreateStringValue succeeded: {:?}", s);
}
