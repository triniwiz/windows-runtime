use runtime::Runtime;

/// Ensure constructing `Windows.UI.Xaml.Media.FontFamily` with
/// a plain name and a CSS-like comma-separated list does not crash
/// the runtime or produce an uncaught JS error.
#[test]
fn test_font_family_constructor_handles_css_list() {
    let mut runtime = Runtime::new(".");

    // Plain family name — run on UI thread to avoid RPC_E_WRONG_THREAD
    runtime.run_script(
        "__nsRunOnUIThread(function(){ new Windows.UI.Xaml.Media.FontFamily('Arial'); });",
        "font_family_plain.js",
    );

    // CSS-style fallback list — run on UI thread as well
    runtime.run_script(
        "__nsRunOnUIThread(function(){ new Windows.UI.Xaml.Media.FontFamily('Arial, Helvetica, sans-serif'); });",
        "font_family_css.js",
    );

    if let Some(err) = runtime::get_last_js_error() {
        panic!("JS error during FontFamily integration test: {}", err);
    }
}
