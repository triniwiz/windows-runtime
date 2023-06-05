pub fn init_performance(scope: &mut v8::HandleScope<()>, global: &mut v8::Local<v8::ObjectTemplate>) {
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

fn handle_now(scope: &mut v8::HandleScope,
              _args: v8::FunctionCallbackArguments,
              mut retval: v8::ReturnValue) {
    let now = chrono::Utc::now();
    retval.set_double(
        now.timestamp_nanos() as f64
    )
}