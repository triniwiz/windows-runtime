fn handle_time(_scope: &mut v8::PinScope<'_, '_>,
               _args: v8::FunctionCallbackArguments,
               mut retval: v8::ReturnValue) {
    let now = chrono::Utc::now();
    retval.set_double((now.timestamp_millis() / 1000000) as f64);
}

pub fn init_time(scope: &mut v8::PinScope<'_, '_, ()>, global: &mut v8::Local<v8::ObjectTemplate>) {
    let time = v8::FunctionTemplate::new(scope, handle_time);
    global.set(
        v8::String::new(scope, "__time").unwrap().into(), time.into(),
    );
}