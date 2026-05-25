use runtime::Runtime;

fn assert_js(rt: &mut Runtime, expr: &str, msg: &str) {
    match rt.eval_script_to_string(expr) {
        Some(ref v) if v.trim() == "true" => {}
        Some(v) => panic!("{msg}: expression evaluated to {v:?} (expected \"true\")"),
        None => panic!("{msg}: JS exception thrown"),
    }
}

fn eval(rt: &mut Runtime, expr: &str) -> String {
    rt.eval_script_to_string(expr).unwrap_or_else(|| "<eval failed>".to_string())
}

// ── class instanceof ─────────────────────────────────────────────────────────

/// `obj instanceof ClassName` must be true for objects whose declared WinRT type
/// name matches — this covers instances returned from APIs that bypass the normal
/// constructor prototype chain (created via ObjectTemplate, not FunctionTemplate).
#[test]
fn class_instance_is_instanceof_its_own_constructor() {
    let mut rt = Runtime::new(".");
    assert_js(
        &mut rt,
        "new Windows.Foundation.Uri('http://example.com/') instanceof Windows.Foundation.Uri",
        "Uri instance should be instanceof Uri",
    );
}

/// `instanceof` must be false when the object is a different WinRT class.
#[test]
fn class_instance_is_not_instanceof_different_class() {
    let mut rt = Runtime::new(".");
    // Uri is definitely not a Thickness struct or any other class.
    assert_js(
        &mut rt,
        "!(new Windows.Foundation.Uri('http://example.com/') instanceof Windows.Foundation.WwwFormUrlDecoder)",
        "Uri should not be instanceof WwwFormUrlDecoder",
    );
}

/// A plain JS object must not pass an instanceof check against a WinRT class.
#[test]
fn plain_object_is_not_instanceof_winrt_class() {
    let mut rt = Runtime::new(".");
    assert_js(
        &mut rt,
        "!({} instanceof Windows.Foundation.Uri)",
        "plain object should not be instanceof Uri",
    );
}

/// A primitive must not pass an instanceof check against a WinRT class.
#[test]
fn primitive_is_not_instanceof_winrt_class() {
    let mut rt = Runtime::new(".");
    // 'instanceof' on a non-object never throws; it just returns false.
    assert_js(
        &mut rt,
        "!('hello' instanceof Windows.Foundation.Uri)",
        "string should not be instanceof Uri",
    );
}

// ── interface instanceof (COM QueryInterface) ─────────────────────────────────

/// `Windows.Foundation.Uri` implements `Windows.Foundation.IStringable`.
/// `uri instanceof Windows.Foundation.IStringable` must be `true`.
#[test]
fn class_instance_is_instanceof_implemented_interface() {
    let mut rt = Runtime::new(".");
    assert_js(
        &mut rt,
        "new Windows.Foundation.Uri('http://example.com/') instanceof Windows.Foundation.IStringable",
        "Uri should be instanceof IStringable (it implements it)",
    );
}

/// `uri instanceof SomeOtherInterface` must be `false` when the WinRT object
/// does not implement that interface (QI returns E_NOINTERFACE).
#[test]
fn class_instance_is_not_instanceof_unimplemented_interface() {
    let mut rt = Runtime::new(".");
    // IClosable (Windows.Foundation.IClosable) is not implemented by Uri.
    assert_js(
        &mut rt,
        "!(new Windows.Foundation.Uri('http://example.com/') instanceof Windows.Foundation.IClosable)",
        "Uri should not be instanceof IClosable",
    );
}

/// A plain JS object must not pass an interface instanceof check.
#[test]
fn plain_object_is_not_instanceof_winrt_interface() {
    let mut rt = Runtime::new(".");
    assert_js(
        &mut rt,
        "!({} instanceof Windows.Foundation.IStringable)",
        "plain object should not be instanceof IStringable",
    );
}

// ── the user's primary use case: IVector / IVectorView ───────────────────────

/// Validates the pattern described in the feature request:
///   `nativeData instanceof Windows.Foundation.Collections.IVector ||
///    nativeData instanceof Windows.Foundation.Collections.IVectorView`
///
/// A plain Uri does NOT implement IVector or IVectorView, so both checks must
/// return false (proving the negative path doesn't accidentally return true).
#[test]
fn uri_is_not_instanceof_ivector_or_ivectorview() {
    let mut rt = Runtime::new(".");
    assert_js(
        &mut rt,
        r#"(function() {
            var uri = new Windows.Foundation.Uri('http://example.com/');
            var isVec = uri instanceof Windows.Foundation.Collections.IVector;
            var isVecView = uri instanceof Windows.Foundation.Collections.IVectorView;
            return !isVec && !isVecView;
        })()"#,
        "Uri should not be instanceof IVector or IVectorView",
    );
}

/// `Symbol.hasInstance` must be a function on interface constructors.
#[test]
fn interface_constructor_has_symbol_has_instance() {
    let mut rt = Runtime::new(".");
    assert_js(
        &mut rt,
        "typeof Windows.Foundation.IStringable[Symbol.hasInstance] === 'function'",
        "IStringable constructor should expose Symbol.hasInstance",
    );
}

/// `Symbol.hasInstance` must be a function on class constructors.
#[test]
fn class_constructor_has_symbol_has_instance() {
    let mut rt = Runtime::new(".");
    assert_js(
        &mut rt,
        "typeof Windows.Foundation.Uri[Symbol.hasInstance] === 'function'",
        "Uri constructor should expose Symbol.hasInstance",
    );
}
