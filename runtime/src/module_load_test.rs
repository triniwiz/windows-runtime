//! Regression tests for the module-loading path that backs `global.loadModule`.
//!
//! `global.loadModule` (from @nativescript/core) routes through the runtime's
//! `require` shim and the native `__nsResolveModulePath` / `__nsReadTextFile`
//! host functions. When the module resolver can't find a registered module for a
//! path (e.g. an unregistered CSS file like `app.css`), it produces a null module
//! name. Passing that null down the chain must surface as a *catchable JS error* —
//! it must never panic / abort the Rust runtime.
//!
//! Each test runs a self-contained JS try/catch that returns a sentinel string.
//! If the native layer panicked instead of throwing, the panic cannot be caught by
//! the JS `catch` and would abort the test binary — so a passing test proves the
//! path throws rather than panics.

use crate::Runtime;

/// Evaluate a JS expression and return its string result, or `<no result>` if the
/// outer eval itself threw (it shouldn't — each snippet catches internally).
fn eval(runtime: &mut Runtime, expr: &str) -> String {
    runtime
        .eval_script_to_string(expr)
        .unwrap_or_else(|| "<no result>".to_string())
}

/// Wrap a call so a thrown error becomes `THREW:<message>` and a normal return
/// becomes `NO_THROW:<value>`. A Rust panic in the call would abort the process.
fn caught(call: &str) -> String {
    format!(
        r#"(function() {{
            try {{
                var r = {call};
                return 'NO_THROW:' + String(r);
            }} catch (e) {{
                return 'THREW:' + (e && e.message ? e.message : String(e));
            }}
        }})()"#
    )
}

#[test]
fn require_null_throws_not_panics() {
    let mut runtime = Runtime::new(".");
    let result = eval(&mut runtime, &caught("globalThis.require(null)"));
    assert!(
        result.starts_with("THREW:"),
        "require(null) must throw, got: {result}"
    );
}

#[test]
fn require_undefined_throws_not_panics() {
    let mut runtime = Runtime::new(".");
    let result = eval(&mut runtime, &caught("globalThis.require(undefined)"));
    assert!(
        result.starts_with("THREW:"),
        "require(undefined) must throw, got: {result}"
    );
}

#[test]
fn require_empty_string_throws_not_panics() {
    let mut runtime = Runtime::new(".");
    let result = eval(&mut runtime, &caught("globalThis.require('')"));
    assert!(
        result.starts_with("THREW:"),
        "require('') must throw, got: {result}"
    );
}

/// Mirrors how @nativescript/core calls `global.loadModule(resolvedModuleName)`
/// where `resolvedModuleName` is null because no registered webpack module matched
/// the CSS path. The runtime-owned `loadModule` (aliased to `require` here) must
/// throw cleanly.
#[test]
fn load_module_null_resolved_name_throws_not_panics() {
    let mut runtime = Runtime::new(".");
    let result = eval(
        &mut runtime,
        &caught(
            "(function() { \
                var loadModule = globalThis.loadModule || globalThis.require; \
                return loadModule(null); \
            })()",
        ),
    );
    assert!(
        result.starts_with("THREW:"),
        "loadModule(null) must throw, got: {result}"
    );
}

#[test]
fn resolve_module_path_null_throws_not_panics() {
    let mut runtime = Runtime::new(".");
    let result = eval(&mut runtime, &caught("globalThis.__nsResolveModulePath(null)"));
    assert!(
        result.starts_with("THREW:"),
        "__nsResolveModulePath(null) must throw, got: {result}"
    );
}

#[test]
fn resolve_module_path_empty_throws_not_panics() {
    let mut runtime = Runtime::new(".");
    let result = eval(&mut runtime, &caught("globalThis.__nsResolveModulePath('')"));
    assert!(
        result.starts_with("THREW:"),
        "__nsResolveModulePath('') must throw, got: {result}"
    );
}

#[test]
fn read_text_file_null_throws_not_panics() {
    let mut runtime = Runtime::new(".");
    let result = eval(&mut runtime, &caught("globalThis.__nsReadTextFile(null)"));
    assert!(
        result.starts_with("THREW:"),
        "__nsReadTextFile(null) must throw, got: {result}"
    );
}

#[test]
fn read_text_file_empty_throws_not_panics() {
    let mut runtime = Runtime::new(".");
    let result = eval(&mut runtime, &caught("globalThis.__nsReadTextFile('')"));
    assert!(
        result.starts_with("THREW:"),
        "__nsReadTextFile('') must throw, got: {result}"
    );
}

/// Hammer the bad-input path repeatedly: a latent panic / abort or memory-safety
/// issue in the native handlers would surface as a crash rather than 50 clean throws.
#[test]
fn repeated_bad_module_loads_are_stable() {
    let mut runtime = Runtime::new(".");
    for _ in 0..50 {
        let result = eval(&mut runtime, &caught("globalThis.require(null)"));
        assert!(result.starts_with("THREW:"), "unexpected: {result}");
    }
}
