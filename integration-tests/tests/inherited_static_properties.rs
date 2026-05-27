/// Tests for inherited static properties on WinRT class constructors.
///
/// Static properties declared on a base class (e.g. UIElement.PointerPressedEvent)
/// must be accessible on the derived class constructor (e.g. Panel, Button) and
/// must return a valid WinRT object, not undefined.
///
/// The root cause of prior failures: create_ns_ctor_object only iterated
/// clazz.properties() (own statics), and always used the current class's
/// activation factory — which doesn't implement base-class statics interfaces.
///
/// NOTE: Tests are serialized via a global Mutex because each test creates its own
/// V8 isolate + COM apartment; concurrent teardown of multiple isolates on Windows
/// triggers intermittent STATUS_ACCESS_VIOLATION in the test output-capture infra.
use runtime::Runtime;
use std::sync::Mutex;

static TEST_SERIAL: Mutex<()> = Mutex::new(());

fn eval(rt: &mut Runtime, expr: &str) -> String {
    rt.eval_script_to_string(expr)
        .unwrap_or_else(|| "<eval failed>".to_string())
}

fn assert_js(rt: &mut Runtime, expr: &str, msg: &str) {
    match rt.eval_script_to_string(expr) {
        Some(ref v) if v.trim() == "true" => {}
        Some(v) => panic!("{msg}: expression evaluated to {v:?} (expected \"true\")"),
        None => panic!("{msg}: JS exception thrown"),
    }
}

/// UIElement.PointerPressedEvent is a static RoutedEvent declared on UIElement.
/// It must be accessible via Panel (a subclass) constructor, not undefined.
#[test]
fn panel_inherits_pointer_pressed_event_static() {
    let _g = TEST_SERIAL.lock().unwrap();
    let mut rt = Runtime::new(".");
    assert_js(
        &mut rt,
        "Windows.UI.Xaml.Controls.Panel.PointerPressedEvent !== undefined && \
         Windows.UI.Xaml.Controls.Panel.PointerPressedEvent !== null",
        "Panel.PointerPressedEvent should not be undefined",
    );
}

/// The PointerPressedEvent value should be a WinRT object (not a primitive).
#[test]
fn pointer_pressed_event_is_object() {
    let _g = TEST_SERIAL.lock().unwrap();
    let mut rt = Runtime::new(".");
    assert_js(
        &mut rt,
        "typeof Windows.UI.Xaml.Controls.Panel.PointerPressedEvent === 'object'",
        "Panel.PointerPressedEvent should be typeof object",
    );
}

/// PointerReleasedEvent and PointerMovedEvent are also UIElement statics.
#[test]
fn panel_inherits_pointer_released_and_moved_event_statics() {
    let _g = TEST_SERIAL.lock().unwrap();
    let mut rt = Runtime::new(".");
    assert_js(
        &mut rt,
        "Windows.UI.Xaml.Controls.Panel.PointerReleasedEvent !== undefined",
        "Panel.PointerReleasedEvent should not be undefined",
    );
    assert_js(
        &mut rt,
        "Windows.UI.Xaml.Controls.Panel.PointerMovedEvent !== undefined",
        "Panel.PointerMovedEvent should not be undefined",
    );
}

/// Same inherited static accessible via a deeper subclass (Button extends ContentControl
/// extends ButtonBase extends ... extends UIElement).
#[test]
fn button_inherits_pointer_pressed_event_static() {
    let _g = TEST_SERIAL.lock().unwrap();
    let mut rt = Runtime::new(".");
    assert_js(
        &mut rt,
        "Windows.UI.Xaml.Controls.Button.PointerPressedEvent !== undefined && \
         Windows.UI.Xaml.Controls.Button.PointerPressedEvent !== null",
        "Button.PointerPressedEvent should not be undefined",
    );
}

/// PointerPressedEvent accessed on an instance should match the one on the constructor.
/// Constructor access is the primary coverage; instance access requires a live XAML element
/// which may not be activatable in an unpackaged test process — that path is skipped gracefully.
#[test]
fn instance_pointer_pressed_event_matches_constructor_static() {
    let _g = TEST_SERIAL.lock().unwrap();
    let mut rt = Runtime::new(".");
    rt.run_script(
        "globalThis.__panel = (function(){ try { return new Windows.UI.Xaml.Controls.StackPanel(); } catch(_){} return null; })()",
        "setup.js",
    );
    // Constructor-level access is always expected to work after the fix.
    assert_js(
        &mut rt,
        "Windows.UI.Xaml.Controls.StackPanel.PointerPressedEvent !== undefined",
        "PointerPressedEvent via constructor should be defined",
    );
    // Instance-level access only verified when element creation succeeded.
    assert_js(
        &mut rt,
        "(function() { \
            if (!globalThis.__panel) return true; \
            var ppe_inst = globalThis.__panel.PointerPressedEvent; \
            return ppe_inst !== undefined; \
        })()",
        "PointerPressedEvent via instance should be defined when element is available",
    );
}

/// AddHandler should succeed when a valid RoutedEvent is passed: E_INVALIDARG was the
/// symptom before the declaring-class factory fix.  If the element cannot be activated
/// in this process (unpackaged, no XAML hosting) the instance call is skipped and we
/// only assert the RoutedEvent descriptor itself is a non-null object.
#[test]
fn add_handler_with_inherited_routed_event_does_not_throw() {
    let _g = TEST_SERIAL.lock().unwrap();
    let mut rt = Runtime::new(".");
    rt.run_script(
        "globalThis.__panel = (function(){ try { return new Windows.UI.Xaml.Controls.StackPanel(); } catch(_){} return null; })()",
        "setup.js",
    );
    let result = eval(
        &mut rt,
        r#"(function() {
            try {
                var ppe = Windows.UI.Xaml.Controls.StackPanel.PointerPressedEvent;
                if (!ppe) return 'RoutedEvent was null';
                if (!globalThis.__panel) return 'ok-no-instance';
                var handler = new Windows.UI.Xaml.Input.PointerEventHandler(function(s, e) {});
                globalThis.__panel.AddHandler(ppe, handler, true);
                return 'ok';
            } catch(e) {
                return 'threw: ' + (e && e.message ? e.message : String(e));
            }
        })()"#,
    );
    let r = result.trim();
    assert!(
        r == "ok" || r == "ok-no-instance",
        "AddHandler should succeed or be skipped (no instance); got: {r:?}"
    );
}
