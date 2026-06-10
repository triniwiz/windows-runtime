use crate::Runtime;

/// Ensure logging WinRT `Windows.UI.Color` values does not crash the runtime.
#[test]
fn test_logging_windows_ui_color_does_not_panic() {
    let mut runtime = Runtime::new(".");

    // Instance-returning struct via factory helper
    runtime.run_script(
        "console.log('red', Windows.UI.ColorHelper.FromArgb(255, 255, 0, 0));",
        "color_fromargb_test.js",
    );

    // Static color property (predefined struct value)
    runtime.run_script(
        "console.log('green', Windows.UI.Colors.Green);",
        "color_static_green_test.js",
    );

    if let Some(err) = crate::get_last_js_error() {
        panic!("JS error during color logging test: {}", err);
    }
}
