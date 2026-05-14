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
        "globalThis.__jv = Windows.Data.Json.JsonValue.CreateStringValue('hello');",
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
