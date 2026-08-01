// [windows port] A clean Windows bring-up for the napi-android V8 shim (v8-api.cpp), replacing its
// Android/JNI-coupled jsr.cpp. Creates a V8 platform + isolate + context and wraps them in the
// shim's `napi_env__`. Single-threaded host: the isolate/context/handle-scope are entered once and
// kept open for the process lifetime (like the Hermes host's env scope), so napi calls from Rust
// work without per-call scope wrapping. Built against the `v8` crate's V8 14.7 headers; our v8 crate
// uses the default config (NO pointer compression / NO sandbox), so there is no ABI-define matching.

#include "v8-api.h"        // napi_env__, plus napi types
#include "js_native_api.h" // napi_run_script_source
#include "libplatform/libplatform.h"
#include "SimpleAllocator.h"
#include <memory>

struct napi_runtime__ {
    tns::SimpleAllocator* allocator;
    v8::Isolate* isolate;
};

// Forwards V8's fatal/OOM errors (CHECK failures otherwise crash silently via __debugbreak) to
// the Rust trace log; defined in windows-v8's lib.rs.
extern "C" void ns_v8_fatal_error(const char* location, const char* message);

static void OnV8FatalError(const char* location, const char* message) {
    ns_v8_fatal_error(location, message);
}

static void OnV8OOMError(const char* location, const v8::OOMDetails& details) {
    ns_v8_fatal_error(location, details.detail ? details.detail : "out of memory");
}

extern "C" {

// The V8 platform + V8::Initialize are done from Rust via the `v8` crate (rusty_v8), because
// NewDefaultPlatform's std::unique_ptr<Platform> return can't be linked from this MSVC-STL shim
// (rusty_v8 uses V8's bundled libc++). V8 must be initialized before this is called.
napi_status js_create_runtime(napi_runtime__** runtime) {
    if (!runtime) return napi_invalid_arg;
    auto* rt = new napi_runtime__();
    rt->allocator = new tns::SimpleAllocator();
    v8::Isolate::CreateParams params;
    params.array_buffer_allocator = rt->allocator;
    rt->isolate = v8::Isolate::New(params);
    rt->isolate->SetFatalErrorHandler(&OnV8FatalError);
    rt->isolate->SetOOMErrorHandler(&OnV8OOMError);
    *runtime = rt;
    return napi_ok;
}

napi_status js_create_napi_env(napi_env* env, napi_runtime__* rt) {
    if (!env || !rt) return napi_invalid_arg;
    v8::Isolate* isolate = rt->isolate;
    isolate->Enter();
    {
        // Stack handle scope just to create the context; the context is then persisted inside
        // napi_env__ (context_persistent) and kept current via Enter(), so it outlives this scope.
        v8::HandleScope handle_scope(isolate);
        v8::Local<v8::Context> context = v8::Context::New(isolate);
        context->Enter();
        *env = new napi_env__(context, NAPI_VERSION_EXPERIMENTAL);
    }
    // Open a long-lived napi handle scope (heap-backed HandleScopeWrapper — HandleScope itself is
    // stack-only) so napi calls from the Rust host can allocate handles. Kept open for process life.
    napi_handle_scope scope = nullptr;
    napi_open_handle_scope(*env, &scope);
    return napi_ok;
}

napi_status js_execute_script(napi_env env,
                              napi_value script,
                              const char* file,
                              napi_value* result) {
    return napi_run_script_source(env, script, file, result);
}

napi_status js_execute_pending_jobs(napi_env env) {
    env->isolate->PerformMicrotaskCheckpoint();
    return napi_ok;
}

} // extern "C"
