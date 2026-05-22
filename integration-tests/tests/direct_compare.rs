#[test]
fn direct_create_string_value_diag() {
    let ptr = runtime::diag_direct_create_string_value("hello");
    assert!(!ptr.is_null(), "direct CreateStringValue returned null");
}
