use crate::globals::time::PROCESS_START;
use std::time::Instant;
use v8::fast_api::{CFunction, CFunctionInfo, CTypeInfo, Flags, Int64Representation, Type as FastType};

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

/// Fast API variant invoked directly from TurboFan-optimized code;
/// `handle_now` stays the callback for unoptimized frames.
extern "C" fn fast_now(_receiver: v8::Local<v8::Value>) -> f64 {
    let start = PROCESS_START.get_or_init(Instant::now);
    start.elapsed().as_nanos() as f64 / 1_000_000.0
}

// The receiver slot is mandatory in V8's fast-call signatures.
static FAST_NOW_ARGS: [CTypeInfo; 1] = [CTypeInfo::new(FastType::V8Value, Flags::empty())];

/// `CFunctionInfo` embeds a raw pointer to the arg array, which blocks `Sync`.
/// The pointee is a `'static` immutable array and V8 only reads it, so sharing
/// is sound.
struct SyncCFunctionInfo(CFunctionInfo);
unsafe impl Sync for SyncCFunctionInfo {}

static FAST_NOW_INFO: SyncCFunctionInfo = SyncCFunctionInfo(CFunctionInfo::new(
    CTypeInfo::new(FastType::Float64, Flags::empty()),
    &FAST_NOW_ARGS,
    Int64Representation::Number,
));

/// Must run after context creation: `build_fast` requires a context-ful scope.
pub fn install_fast_now(
    scope: &mut v8::ContextScope<v8::HandleScope<v8::Context>>,
    context: v8::Local<v8::Context>,
) {
    let fast = CFunction::new(fast_now as *const std::ffi::c_void, &FAST_NOW_INFO.0);
    let tmpl = v8::FunctionTemplate::builder(handle_now).build_fast(scope, &[fast]);
    let Some(func) = tmpl.get_function(scope) else { return };
    let global = context.global(scope);
    let Some(perf_key) = v8::String::new(scope, "performance") else { return };
    let Some(perf_val) = global.get(scope, perf_key.into()) else { return };
    let Some(perf_obj) = perf_val.to_object(scope) else { return };
    if let Some(now_key) = v8::String::new(scope, "now") {
        perf_obj.set(scope, now_key.into(), func.into());
    }
}
