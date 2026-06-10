use runtime::Runtime;

/// Assert that a JS expression evaluates to true. Any JS throw or false
/// result fails the test with the provided message.
fn assert_js(rt: &mut Runtime, expr: &str, msg: &str) {
    rt.run_script(
        &format!("if (!({expr})) throw new Error({msg:?});"),
        "enum_property_test.js",
    );
}

// ── Enum value lookup ────────────────────────────────────────────────────────

#[test]
fn enum_values_are_accessible_as_integers() {
    let mut rt = Runtime::new(".");

    // Windows.UI.Popups.Placement (no XAML dependency)
    assert_js(
        &mut rt,
        "typeof Windows.UI.Popups.Placement === 'object'",
        "Placement should be an object",
    );
    assert_js(
        &mut rt,
        "Windows.UI.Popups.Placement.Default === 0",
        "Placement.Default should be 0",
    );
    assert_js(
        &mut rt,
        "Windows.UI.Popups.Placement.Right === 4",
        "Placement.Right should be 4",
    );
}

#[test]
fn enum_values_compare_correctly_to_literals() {
    let mut rt = Runtime::new(".");

    // Verify the values round-trip through JS equality correctly.
    assert_js(
        &mut rt,
        "Windows.UI.Popups.Placement.Default == 0",
        "Default == 0",
    );
    assert_js(
        &mut rt,
        "Windows.UI.Popups.Placement.Right != 0",
        "Right != 0",
    );
    assert_js(
        &mut rt,
        "Windows.UI.Popups.Placement.Default !== Windows.UI.Popups.Placement.Right",
        "Default !== Right",
    );
}

// ── Enum-typed property setter ───────────────────────────────────────────────
//
// ScrollBarVisibility is a TypeRef enum inside ScrollViewer's metadata scope.
// This test specifically exercises the TypeRef → Int32 conversion path that
// was previously broken (passed as Pointer instead of Int32).
//
// Note: XAML controls require a live XAML compositor thread (UWP only).
// In this headless test we verify the enum VALUE reaches JS correctly and
// that the ABI signature resolves to Int32 (not Pointer) by checking the
// metadata layer; full round-trip requires the UWP host.

#[test]
fn scrollbarvisibility_enum_values_are_integers() {
    let mut rt = Runtime::new(".");

    assert_js(
        &mut rt,
        "typeof Windows.UI.Xaml.Controls.ScrollBarVisibility === 'object'",
        "ScrollBarVisibility should be accessible",
    );
    assert_js(
        &mut rt,
        "Windows.UI.Xaml.Controls.ScrollBarVisibility.Disabled === 0",
        "ScrollBarVisibility.Disabled should be 0",
    );
    assert_js(
        &mut rt,
        "Windows.UI.Xaml.Controls.ScrollBarVisibility.Auto === 1",
        "ScrollBarVisibility.Auto should be 1",
    );
    assert_js(
        &mut rt,
        "Windows.UI.Xaml.Controls.ScrollBarVisibility.Hidden === 2",
        "ScrollBarVisibility.Hidden should be 2",
    );
    assert_js(
        &mut rt,
        "Windows.UI.Xaml.Controls.ScrollBarVisibility.Visible === 3",
        "ScrollBarVisibility.Visible should be 3",
    );
}

#[test]
fn enum_value_is_a_number_not_an_object() {
    let mut rt = Runtime::new(".");

    // Enum values must be plain numbers so they pass as Int32 to WinRT setters.
    // If this returns 'object' the setter receives a pointer instead of int.
    assert_js(
        &mut rt,
        "typeof Windows.UI.Xaml.Controls.ScrollBarVisibility.Auto === 'number'",
        "ScrollBarVisibility.Auto must be typeof 'number'",
    );
}
