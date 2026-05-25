use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use serde_json::Value as JsonValue;

use crate::throw_js_error;

#[cfg(feature = "devtools")]
use runtime_devtools::{DevtoolsServer, DevtoolsServerConfig};

#[cfg(feature = "devtools")]
thread_local!(static DEVTOOLS_SERVER: RefCell<Option<DevtoolsServer>> = RefCell::new(None));

thread_local!(static INSPECTOR_DOMAIN_DISPATCHERS: RefCell<HashMap<String, v8::Global<v8::Value>>> = RefCell::new(HashMap::new()));

const INSPECTOR_DISPATCHERS_GLOBAL: &str = "__nsInspectorDomainDispatchers";

pub(crate) fn handle_register_domain_dispatcher(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    if args.length() < 2 {
        throw_js_error(scope, "__registerDomainDispatcher(domain, dispatcher) expects 2 arguments");
        return;
    }

    let Some(domain) = crate::global_fns::value_to_string(scope, args.get(0)) else {
        throw_js_error(scope, "Unable to convert domain argument to string");
        return;
    };

    let dispatcher_val = args.get(1);
    if !dispatcher_val.is_object() && !dispatcher_val.is_function() {
        throw_js_error(scope, "dispatcher must be an object or constructor function");
        return;
    }

    // If a constructor was provided, attempt to instantiate it. Keep this
    // resilient: constructor failures should not crash the runtime.
    let mut instance: v8::Local<v8::Value> = dispatcher_val;
    if dispatcher_val.is_function() {
        if let Ok(ctor) = v8::Local::<v8::Function>::try_from(dispatcher_val) {
            // Build a tiny function expression that constructs the ctor and
            // returns the instance (or null on error).
            if let Some(src) = v8::String::new(scope, "(function(ctor){ try { return new ctor(); } catch (e) { return null; } })") {
                if let Some(script) = v8::Script::compile(scope, src, None) {
                    if let Some(factory) = script.run(scope) {
                        if let Ok(factory_fn) = v8::Local::<v8::Function>::try_from(factory) {
                            let recv: v8::Local<v8::Value> = v8::undefined(scope).into();
                            let args = [dispatcher_val];
                            let _ = factory_fn.call(scope, recv, &args).map(|res| {
                                if !res.is_null_or_undefined() {
                                    instance = res;
                                }
                            });
                        }
                    }
                }
            }
        }
    }

    // Persist the instance as a v8::Global so it survives across calls.
    let global = v8::Global::new(scope, instance);
    INSPECTOR_DOMAIN_DISPATCHERS.with(|m| {
        m.borrow_mut().insert(domain, global);
    });
}

pub(crate) fn handle_inspector_send_event(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    if args.length() < 1 {
        throw_js_error(scope, "__inspectorSendEvent(jsonStr) expects 1 argument");
        return;
    }

    let Some(sv) = args.get(0).to_string(scope) else {
        throw_js_error(scope, "__inspectorSendEvent: argument must be a string");
        return;
    };
    let json = sv.to_rust_string_lossy(scope);

    #[cfg(feature = "devtools")]
    {
        DEVTOOLS_SERVER.with(|d| {
            if let Ok(mut cell) = d.try_borrow_mut() {
                if let Some(srv) = cell.as_mut() {
                    let _ = srv.send(&json);
                }
            }
        });
    }

    #[cfg(not(feature = "devtools"))]
    {
        throw_js_error(scope, "DevTools support not enabled in this build");
    }
}

pub(crate) fn handle_inspector_timestamp(
    _scope: &mut v8::PinScope<'_, '_>,
    _args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let ms = now.as_secs() as f64 * 1000.0 + (now.subsec_millis() as f64);
    retval.set_double(ms);
}

/// Attempt to dispatch an incoming DevTools protocol message to a registered
/// JS domain handler. Returns true if the message was handled by JS (so the
/// caller should not fall back to V8 inspector dispatch).
pub fn try_dispatch_inspector_message_to_js(msg: &str) -> bool {
    // Parse method name quickly in Rust first to avoid creating V8 scopes
    // when unnecessary.
    let method = match serde_json::from_str::<JsonValue>(msg)
        .ok()
        .and_then(|v| v.get("method").and_then(|m| m.as_str()).map(|s| s.to_string()))
    {
        Some(m) => m,
        None => return false,
    };

    let dot = match method.find('.') { Some(i) => i, None => return false };
    let domain = &method[..dot];
    let member = &method[dot + 1..];

    let dispatcher_global = INSPECTOR_DOMAIN_DISPATCHERS.with(|m| m.borrow().get(domain).cloned());
    let Some(dg) = dispatcher_global else { return false };

    // Create a short-lived V8 scope and call the method if present.
    let isolate_ptr = crate::DELEGATE_ISOLATE_PTR.with(|c| c.get());
    if isolate_ptr.is_null() { return false; }
    let isolate: &mut v8::Isolate = unsafe { &mut *isolate_ptr };
    v8::scope!(scope, isolate);

    let ctx_global = match scope.get_slot::<v8::Global<v8::Context>>() {
        Some(g) => g.clone(),
        None => return false,
    };
    let context = v8::Local::new(scope, &ctx_global);
    let scope = &mut v8::ContextScope::new(scope, context);
    v8::tc_scope!(tc, scope);

    // Convert the incoming JSON message into a V8 value so we can pass `params`.
    let Some(msg_str) = v8::String::new(tc, msg) else { return false };
    let Some(msg_val) = v8::json::parse(tc, msg_str) else { return false };
    let msg_obj = match v8::Local::<v8::Object>::try_from(msg_val) {
        Ok(o) => o,
        Err(_) => return false,
    };

    let params_key = v8::String::new(tc, "params").unwrap();
    let params_val = msg_obj.get(tc, params_key.into()).unwrap_or_else(|| v8::undefined(tc).into());

    let recv = v8::Local::new(tc, &dg);
    let obj = if let Ok(o) = v8::Local::<v8::Object>::try_from(recv) { o } else { return false };

    let method_key = v8::String::new(tc, member).unwrap();
    let method_val = obj.get(tc, method_key.into());
    if method_val.is_none() || method_val.unwrap().is_undefined() { return false; }
    let method_val = method_val.unwrap();

    if let Ok(func) = v8::Local::<v8::Function>::try_from(method_val) {
        let _ = func.call(tc, obj.into(), &[params_val, msg_obj.into()]);
        if tc.has_caught() {
            if let Some(ex) = tc.exception() {
                let msg = ex.to_rust_string_lossy(tc);
                crate::store_last_js_error(msg);
            }
            tc.reset();
        }
        return true;
    }

    false
}

#[cfg(feature = "devtools")]
pub(crate) fn maybe_attach_devtools(scope: &mut v8::ContextScope<v8::HandleScope<v8::Context>>) {
    use runtime_devtools::DevtoolsServerConfig;

    // Create a Global<Context> for the DevTools server registration.
    let ctx = scope.get_current_context();
    let global_context = v8::Global::new(scope, ctx);

    let config = DevtoolsServerConfig::default();
    let forwarder = Some(Arc::new(|s: &str| crate::debug_output(s)) as Arc<dyn Fn(&str) + Send + Sync>);
    let dispatcher = Some(Arc::new(|msg: &str| try_dispatch_inspector_message_to_js(msg)) as Arc<dyn Fn(&str) -> bool + Send + Sync>);

    let isolate_ptr = crate::DELEGATE_ISOLATE_PTR.with(|c| c.get());
    if isolate_ptr.is_null() { return; }
    let isolate: &mut v8::Isolate = unsafe { &mut *isolate_ptr };

    match runtime_devtools::DevtoolsServer::attach(&config, isolate, &global_context, forwarder, dispatcher) {
        Ok(server) => {
            DEVTOOLS_SERVER.with(|d| { *d.borrow_mut() = Some(server); });
            // Wire pump hook so host wait loops will call into DevTools pump.
            crate::ASYNC_PUMP_HOOK.with(|hook| {
                if let Ok(mut guard) = hook.try_borrow_mut() {
                    *guard = Some(Box::new(|| {
                        DEVTOOLS_SERVER.with(|d| {
                            if let Ok(mut cell) = d.try_borrow_mut() {
                                if let Some(srv) = cell.as_mut() {
                                    srv.pump_messages();
                                }
                            }
                        })
                    }));
                }
            });
        }
        Err(_) => {}
    }
}

#[cfg(not(feature = "devtools"))]
pub(crate) fn maybe_attach_devtools(_scope: &mut v8::ContextScope<v8::HandleScope<v8::Context>>) {}
