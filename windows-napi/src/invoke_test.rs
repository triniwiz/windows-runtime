//! E2E hooks for the napi invoke seed: real WinRT static/instance method calls through
//! `MethodCall::call_napi`. Test-only surface until the full ns_proxy port lands.

use napi::{Env, JsUnknown};
use napi_derive::napi;

use runtime::napi_engine::invoke::{invoke_instance, invoke_static};
use runtime::napi_engine::value::ptr_from_external;

fn reason(e: impl ToString) -> napi::Error {
    napi::Error::from_reason(e.to_string())
}

/// Call a static WinRT method: `callStaticMethod('Windows.Data.Json.JsonValue',
/// 'CreateStringValue', ['hello'])`.
#[napi]
pub fn call_static_method(
    env: Env,
    class_name: String,
    method_name: String,
    args: Vec<JsUnknown>,
) -> napi::Result<JsUnknown> {
    invoke_static(&env, &class_name, &method_name, &args).map_err(reason)
}

/// Call an instance WinRT method on a pointer external returned by a previous call.
#[napi]
pub fn call_instance_method(
    env: Env,
    instance: JsUnknown,
    class_name: String,
    method_name: String,
    args: Vec<JsUnknown>,
) -> napi::Result<JsUnknown> {
    // Accept a raw pointer external or an instance proxy (whose get trap answers `handle`).
    let ptr = ptr_from_external(&env, &instance)
        .or_else(|| {
            use napi::{JsObject, JsUnknown, ValueType};
            if !matches!(instance.get_type(), Ok(ValueType::Object)) {
                return None;
            }
            let obj: JsObject = unsafe { instance.cast() };
            let handle = obj.get_named_property::<JsUnknown>("handle").ok()?;
            ptr_from_external(&env, &handle)
        })
        .ok_or_else(|| reason("expected a WinRT instance external or proxy"))?;
    invoke_instance(&env, ptr, &class_name, &method_name, &args).map_err(reason)
}
