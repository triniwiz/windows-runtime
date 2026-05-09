use runtime::Runtime;

#[test]
fn runtime_executes_simple_script() {
    let mut runtime = Runtime::new(".");
    runtime.run_script("const answer = 40 + 2;", "test.js");
}

#[test]
fn runtime_handles_js_throw_without_host_panic() {
    let mut runtime = Runtime::new(".");
    runtime.run_script("throw new Error('integration test throw');", "test.js");
}
