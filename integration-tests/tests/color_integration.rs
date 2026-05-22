use runtime::Runtime;

/// Integration test: ensure `Windows.UI.Color` values can be logged from JS
/// without causing runtime panics or unhandled exceptions.
#[test]
fn test_logging_windows_ui_color_integration() {
    let mut runtime = Runtime::new(".");

    runtime.run_script(
        "console.log('red', Windows.UI.ColorHelper.FromArgb(255, 255, 0, 0));",
        "color_integration_fromargb.js",
    );

    runtime.run_script(
        "console.log('green', Windows.UI.Colors.Green);",
        "color_integration_static_green.js",
    );

    if let Some(err) = runtime::get_last_js_error() {
        panic!("JS error during integration color test: {}", err);
    }
}
