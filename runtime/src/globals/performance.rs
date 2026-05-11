use std::time::Instant;
use crate::globals::time::PROCESS_START;

pub fn init_performance(
    scope: &mut v8::PinScope<'_, '_, ()>,
    global: &mut v8::Local<v8::ObjectTemplate>,
) {
    let performance = v8::ObjectTemplate::new(scope);
    let now = v8::FunctionTemplate::new(scope, handle_now);
    performance.set(
        v8::String::new(scope, "now").unwrap().into(),
        now.into(),
    );
    global.set(
        v8::String::new(scope, "performance").unwrap().into(),
        performance.into(),
    );
}

/// Returns milliseconds since process start as a f64, matching the
/// browser / NativeScript `performance.now()` contract.
fn handle_now(
    _scope: &mut v8::PinScope<'_, '_>,
    _args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let start = PROCESS_START.get_or_init(Instant::now);
    retval.set_double(start.elapsed().as_nanos() as f64 / 1_000_000.0);
}
