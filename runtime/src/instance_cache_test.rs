use crate::Runtime;
use std::time::Instant;

/// Helper: run a script and return the last JS error if any was stored.
fn run_asserting(runtime: &mut Runtime, script: &str) {
    runtime.run_script(script, "instance_cache_test.js");
    if let Some(err) = crate::get_last_js_error() {
        panic!("JS assertion failed:\n{}", err);
    }
}

/// Helper: evaluate a JS expression and return its string representation.
fn eval(runtime: &mut Runtime, expr: &str) -> String {
    runtime
        .eval_script_to_string(expr)
        .unwrap_or_else(|| "<no result>".to_string())
}

/// Two JS references obtained by accessing the same WinRT object via different
/// code paths must satisfy `===` when the instance cache is working.
/// Strategy: parse a JSON array, retrieve element 0 twice, compare.
#[test]
fn test_json_array_element_same_object() {
    let mut runtime = Runtime::new(".");
    let result = eval(
        &mut runtime,
        r#"
        const arr = Windows.Data.Json.JsonArray.Parse('[{}]');
        const a = arr.GetObjectAt(0);
        const b = arr.GetObjectAt(0);
        String(a === b)
        "#,
    );
    assert_eq!(result, "true", "Same COM object from GetObjectAt(0) twice must be ===");
}

/// Accessing a nested JsonObject via GetNamedObject twice must yield the same proxy.
#[test]
fn test_json_nested_object_same_identity() {
    let mut runtime = Runtime::new(".");
    let result = eval(
        &mut runtime,
        r#"
        const root = Windows.Data.Json.JsonObject.Parse('{"child":{}}');
        const c1 = root.GetNamedObject('child');
        const c2 = root.GetNamedObject('child');
        String(c1 === c2)
        "#,
    );
    assert_eq!(result, "true", "Nested object retrieved twice must be ===");
}

/// A freshly constructed WinRT object inserted into a JsonObject as a value
/// and then retrieved must be the same JS proxy (cache round-trip).
#[test]
fn test_constructed_object_roundtrip_via_collection() {
    let mut runtime = Runtime::new(".");
    let result = eval(
        &mut runtime,
        r#"
        const JsonValue = Windows.Data.Json.JsonValue;
        const JsonObject = Windows.Data.Json.JsonObject;

        // Create a JsonValue and insert it into a container.
        const val = JsonValue.CreateStringValue('hello');
        const obj = new JsonObject();
        obj.SetNamedValue('key', val);

        // Retrieve the stored value — should be the identical COM object.
        const retrieved = obj.GetNamedValue('key');
        String(val === retrieved)
        "#,
    );
    assert_eq!(result, "true", "Round-tripped WinRT value must be === to the original");
}

/// Creating the same WinRT type via two separate `new` calls must NOT be ===
/// (different COM objects, different cache entries).
#[test]
fn test_distinct_constructed_objects_are_not_equal() {
    let mut runtime = Runtime::new(".");
    let result = eval(
        &mut runtime,
        r#"
        const JsonObject = Windows.Data.Json.JsonObject;
        const a = new JsonObject();
        const b = new JsonObject();
        String(a === b)
        "#,
    );
    assert_eq!(result, "false", "Two distinct WinRT instances must NOT be ===");
}

/// The cache must not confuse two different objects even if one was GC'd and
/// the slot was reused (the finalizer removes the stale entry before reuse).
#[test]
fn test_cache_does_not_confuse_distinct_objects() {
    let mut runtime = Runtime::new(".");
    run_asserting(
        &mut runtime,
        r#"
        const JsonValue = Windows.Data.Json.JsonValue;
        const v1 = JsonValue.CreateStringValue('alpha');
        const v2 = JsonValue.CreateStringValue('beta');
        if (v1 === v2) throw new Error('Different COM objects reported as ===');
        "#,
    );
}

/// Cache hit must be significantly faster than a cache miss (template construction).
/// This is not a hard latency assertion — it just prints the ratio so regressions are visible.
#[test]
fn test_cache_hit_is_faster_than_miss() {
    let mut runtime = Runtime::new(".");

    // Warm up: parse the object and get the child once (cache miss — builds template).
    eval(&mut runtime, "const _root = Windows.Data.Json.JsonObject.Parse('{\"c\":{}}')");

    // Time a cold access (cache miss on a fresh parse).
    let t0 = Instant::now();
    for _ in 0..100 {
        eval(&mut runtime, "Windows.Data.Json.JsonObject.Parse('{\"c\":{}}').GetNamedObject('c')");
    }
    let miss_avg_ns = t0.elapsed().as_nanos() / 100;

    // Time a hot access on the same object (cache hit every time).
    eval(&mut runtime, "globalThis.__obj = Windows.Data.Json.JsonObject.Parse('{\"c\":{}}')");
    let t1 = Instant::now();
    for _ in 0..100 {
        eval(&mut runtime, "globalThis.__obj.GetNamedObject('c')");
    }
    let hit_avg_ns = t1.elapsed().as_nanos() / 100;

    println!("cache miss avg: {} ns, cache hit avg: {} ns, speedup: {:.1}x",
        miss_avg_ns, hit_avg_ns,
        miss_avg_ns as f64 / hit_avg_ns.max(1) as f64);

    // Cache hit should be at least 2x faster than building a fresh template.
    assert!(
        hit_avg_ns * 2 <= miss_avg_ns,
        "Expected cache hit ({hit_avg_ns} ns) to be at least 2x faster than miss ({miss_avg_ns} ns)"
    );
}

/// Creating objects beyond the GC threshold must raise GC_NUDGE_NEXT_AT past
/// its base value, signalling that an incremental GC was requested and the
/// adaptive threshold advanced.
#[test]
fn test_gc_nudge_set_when_threshold_exceeded() {
    let mut runtime = Runtime::new(".");

    // Reset the adaptive threshold in case a prior test on this thread moved it.
    crate::GC_NUDGE_NEXT_AT.with(|f| f.set(crate::INSTANCE_CACHE_GC_THRESHOLD));

    // Create enough unique JsonValue objects to exceed the soft threshold.
    let count = crate::INSTANCE_CACHE_GC_THRESHOLD + 50;
    let script = format!(
        r#"
        const JsonValue = Windows.Data.Json.JsonValue;
        for (let i = 0; i < {}; i++) {{
            globalThis['__val' + i] = JsonValue.CreateStringValue('item' + i);
        }}
        "#,
        count
    );
    runtime.run_script(&script, "gc_nudge_test.js");

    let size = crate::INSTANCE_CACHE.with(|c| c.borrow().len());
    assert!(
        size > 0,
        "Expected at least some objects in the cache after creation"
    );
    // All objects are strongly held by globals, so the cache exceeded the base
    // threshold and the adaptive nudge threshold must have advanced beyond it.
    let next_at = crate::GC_NUDGE_NEXT_AT.with(|f| f.get());
    assert!(
        next_at > crate::INSTANCE_CACHE_GC_THRESHOLD,
        "Expected the adaptive GC nudge threshold to advance (got {next_at})"
    );
}

/// Accessing a JsonArray element at the same index through two different array
/// references (same underlying array COM object) must yield the same element proxy.
#[test]
fn test_element_identity_via_two_array_refs() {
    let mut runtime = Runtime::new(".");
    let result = eval(
        &mut runtime,
        r#"
        const JsonObject = Windows.Data.Json.JsonObject;
        const root = JsonObject.Parse('{"arr":[{"x":1}]}');
        const arr1 = root.GetNamedArray('arr');
        const arr2 = root.GetNamedArray('arr');
        const elem1 = arr1.GetObjectAt(0);
        const elem2 = arr2.GetObjectAt(0);
        String(elem1 === elem2)
        "#,
    );
    assert_eq!(result, "true", "Same element from two array refs must be ===");
}
