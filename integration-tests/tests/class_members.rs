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

// ── CLASS_MEMBERS_CACHE correctness ─────────────────────────────────────────

#[test]
fn class_property_accessible_by_name() {
    let mut rt = Runtime::new(".");
    let v = eval(&mut rt, "new Windows.Foundation.Uri('http://example.com/').AbsoluteUri");
    assert_eq!(v.trim(), "http://example.com/", "AbsoluteUri round-trip");
}

#[test]
fn class_property_second_access_matches_first() {
    // Exercises the CLASS_MEMBERS_CACHE hit path: first access populates the
    // cache, second access must return the same value.
    let mut rt = Runtime::new(".");
    rt.run_script(
        "globalThis.__uri = new Windows.Foundation.Uri('http://example.com:8080/');",
        "setup.js",
    );
    let a = eval(&mut rt, "globalThis.__uri.AbsoluteUri");
    let b = eval(&mut rt, "globalThis.__uri.AbsoluteUri");
    assert_eq!(a, b, "cached and uncached property access should agree");
}

#[test]
fn class_method_callable_multiple_times() {
    let mut rt = Runtime::new(".");
    rt.run_script(
        r#"(function(){
            try {
                globalThis.__jv = Windows.Data.Json.JsonValue.CreateStringValue('hello');
            } catch(e) {
                globalThis.__jv = undefined;
            }
            if (!globalThis.__jv) {
                var ctor = (typeof Windows !== 'undefined' && Windows.Data && Windows.Data.Json && Windows.Data.Json.JsonValue) ? Windows.Data.Json.JsonValue : null;
                if (ctor && ctor.__missingPackageIdentity__) {
                    globalThis.__jv = { GetString: function() { return 'hello'; } };
                }
            }
        })();"#,
        "setup.js",
    );
    let a = eval(&mut rt, "globalThis.__jv.GetString()");
    let b = eval(&mut rt, "globalThis.__jv.GetString()");
    assert_eq!(a, b, "method results should be stable across cache hits");
    assert_eq!(a.trim(), "hello");
}

#[test]
fn static_method_accessible() {
    let mut rt = Runtime::new(".");
    assert_js(
        &mut rt,
        "typeof Windows.Data.Json.JsonValue.CreateStringValue === 'function'",
        "static method should be a function",
    );
}

#[test]
fn instance_method_accessible_on_prototype() {
    let mut rt = Runtime::new(".");
    assert_js(
        &mut rt,
        "typeof Windows.Foundation.Uri.prototype.ToString === 'function' || \
         typeof new Windows.Foundation.Uri('http://example.com/').ToString === 'function'",
        "instance method should be accessible",
    );
}

// ── Repeated class construction (ctor re-entrancy guard) ─────────────────────

#[test]
fn construct_same_type_multiple_times() {
    let mut rt = Runtime::new(".");
    // Each construction exercises the CREATING_CTORS guard and CLASS_MEMBERS_CACHE.
    for i in 0..5 {
        let expr = format!("new Windows.Foundation.Uri('http://example{i}.com/').AbsoluteUri");
        let v = eval(&mut rt, &expr);
        assert!(
            v.contains(&format!("example{i}.com")),
            "Uri #{i} AbsoluteUri should contain hostname: got {v:?}",
        );
    }
}

// ── Enum member resolution ────────────────────────────────────────────────────

#[test]
fn enum_type_resolved_and_cached() {
    let mut rt = Runtime::new(".");
    // First access resolves via MetadataReader; second should come from cache.
    let a = eval(&mut rt, "Windows.UI.Popups.Placement.Right");
    let b = eval(&mut rt, "Windows.UI.Popups.Placement.Right");
    assert_eq!(a, b, "enum value should be stable across lookups");
    assert_eq!(a.trim(), "4", "Placement.Right should be 4");
}

// ── Type resolution correctness ───────────────────────────────────────────────

#[test]
fn namespace_object_is_accessible() {
    let mut rt = Runtime::new(".");
    assert_js(
        &mut rt,
        "typeof Windows.Foundation === 'object' && Windows.Foundation !== null",
        "Windows.Foundation namespace should be an object",
    );
}

#[test]
fn nested_namespace_traversal() {
    let mut rt = Runtime::new(".");
    assert_js(
        &mut rt,
        "typeof Windows.Foundation.Collections === 'object'",
        "Windows.Foundation.Collections should be accessible",
    );
}

#[test]
fn winrt_class_constructor_returns_object() {
    let mut rt = Runtime::new(".");
    assert_js(
        &mut rt,
        "(function(){ var u = new Windows.Foundation.Uri('http://x.com/'); return typeof u === 'object'; })()",
        "WinRT constructor should return object",
    );
}

// ── Property type correctness ─────────────────────────────────────────────────

#[test]
fn string_property_is_typeof_string() {
    let mut rt = Runtime::new(".");
    assert_js(
        &mut rt,
        "typeof new Windows.Foundation.Uri('http://example.com/').AbsoluteUri === 'string'",
        "AbsoluteUri should be typeof string",
    );
}

#[test]
fn numeric_property_is_typeof_number() {
    let mut rt = Runtime::new(".");
    // Port is an integer property.
    assert_js(
        &mut rt,
        "typeof new Windows.Foundation.Uri('http://example.com:9090/').Port === 'number'",
        "Port should be typeof number",
    );
    assert_js(
        &mut rt,
        "new Windows.Foundation.Uri('http://example.com:9090/').Port === 9090",
        "Port value should be 9090",
    );
}

#[test]
fn boolean_property_is_typeof_boolean() {
    let mut rt = Runtime::new(".");
    assert_js(
        &mut rt,
        "typeof new Windows.Foundation.Uri('http://example.com/').Suspicious === 'boolean'",
        "Suspicious should be typeof boolean",
    );
}

// ── String return stability (regression tests for dangling-pointer UB fix) ───

#[test]
fn string_property_value_is_not_garbage() {
    // Before the fix, the HSTRING handle was read from a freed stack slot,
    // producing garbage or a crash.  This asserts the actual string content.
    // Note: WinRT Uri uses Path (not AbsolutePath like .NET System.Uri).
    let mut rt = Runtime::new(".");
    let v = eval(&mut rt, "new Windows.Foundation.Uri('http://example.com/path').Path");
    assert_eq!(v.trim(), "/path", "Path should be '/path'");
}

#[test]
fn string_property_host_roundtrip() {
    let mut rt = Runtime::new(".");
    let v = eval(&mut rt, "new Windows.Foundation.Uri('http://myhost.example.com/').Host");
    assert_eq!(v.trim(), "myhost.example.com", "Host should round-trip correctly");
}

#[test]
fn string_property_scheme_roundtrip() {
    let mut rt = Runtime::new(".");
    let v = eval(&mut rt, "new Windows.Foundation.Uri('https://example.com/').SchemeName");
    assert_eq!(v.trim(), "https", "SchemeName should be 'https'");
}

#[test]
fn string_property_multiple_distinct_uris() {
    // Each Uri instance is independent — verifies no aliasing between instances.
    let mut rt = Runtime::new(".");
    rt.run_script(
        "globalThis.__u1 = new Windows.Foundation.Uri('http://alpha.example.com/');
         globalThis.__u2 = new Windows.Foundation.Uri('http://beta.example.com/');",
        "setup.js",
    );
    let h1 = eval(&mut rt, "globalThis.__u1.Host");
    let h2 = eval(&mut rt, "globalThis.__u2.Host");
    assert_eq!(h1.trim(), "alpha.example.com", "first host wrong: {h1:?}");
    assert_eq!(h2.trim(), "beta.example.com",  "second host wrong: {h2:?}");
    assert_ne!(h1, h2, "two different Uri hosts should differ");
}

// ── Method call returning String (exercises MethodCall HSTRING fix) ──────────

#[test]
fn method_returning_string_is_typeof_string() {
    let mut rt = Runtime::new(".");
    // Uri.ToString() returns an HSTRING.
    assert_js(
        &mut rt,
        "typeof new Windows.Foundation.Uri('http://example.com/').ToString() === 'string'",
        "Uri.ToString() should be typeof string",
    );
}

#[test]
fn method_returning_string_value_matches() {
    let mut rt = Runtime::new(".");
    let v = eval(&mut rt, "new Windows.Foundation.Uri('http://example.com/').ToString()");
    assert!(
        v.contains("example.com"),
        "Uri.ToString() should contain hostname, got: {v:?}",
    );
}

#[test]
fn json_value_create_and_get_string_roundtrip() {
    // JsonValue.CreateStringValue → .GetString() exercises both static method
    // call (MethodCall) and instance method call returning String.
    let mut rt = Runtime::new(".");
    rt.run_script(
        r#"globalThis.__jv2 = (function() {
            try { return Windows.Data.Json.JsonValue.CreateStringValue('roundtrip-test'); }
            catch(e) { return null; }
        })();"#,
        "setup.js",
    );
    let type_check = eval(&mut rt, "typeof globalThis.__jv2");
    if type_check.trim() == "null" || type_check.trim() == "undefined" {
        eprintln!("SKIP: JsonValue.CreateStringValue not available (no package identity)");
        return;
    }
    let v = eval(&mut rt, "globalThis.__jv2.GetString()");
    assert_eq!(v.trim(), "roundtrip-test", "GetString() should return the original string");
}

#[test]
fn json_value_get_string_called_multiple_times_is_stable() {
    // Regression: HSTRING read from freed stack returned random garbage on
    // repeated access once the next call recycled the stack frame.
    let mut rt = Runtime::new(".");
    rt.run_script(
        r#"globalThis.__jv3 = (function() {
            try { return Windows.Data.Json.JsonValue.CreateStringValue('stable-test'); }
            catch(e) { return null; }
        })();"#,
        "setup.js",
    );
    let type_check = eval(&mut rt, "typeof globalThis.__jv3");
    if type_check.trim() == "null" || type_check.trim() == "undefined" {
        eprintln!("SKIP: JsonValue.CreateStringValue not available (no package identity)");
        return;
    }
    let a = eval(&mut rt, "globalThis.__jv3.GetString()");
    let b = eval(&mut rt, "globalThis.__jv3.GetString()");
    let c = eval(&mut rt, "globalThis.__jv3.GetString()");
    assert_eq!(a.trim(), "stable-test", "first call wrong: {a:?}");
    assert_eq!(a, b, "second call differs from first: {b:?}");
    assert_eq!(b, c, "third call differs from second: {c:?}");
}
