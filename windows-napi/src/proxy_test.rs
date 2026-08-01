//! Regression test for the native-backed JS `Proxy` mechanism that `ns_proxy.rs` relies on.
//!
//! `ns_proxy.rs` gives WinRT objects their "native feel" via a JS `Proxy` whose `get`/`set`/
//! `has` traps are napi callbacks into native code (Node-API has no V8-style named-property
//! interceptor). This module exercises that mechanism in isolation — a Proxy over a plain
//! native map — so the trap plumbing stays covered independently of live WinRT.
//!
//! In the real port, the native store becomes the COM instance + metadata, and the trap
//! bodies call `handle_named_property_getter/setter/query`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use napi::bindgen_prelude::*;
use napi::{CallContext, Env, JsFunction, JsNumber, JsObject, JsString, JsUnknown, ValueType};
use napi_derive::napi;

/// Coerce a trap's property argument to a string key, or `None` for symbols / non-strings
/// (which a WinRT proxy would ignore, e.g. `Symbol.iterator` probes during `console.log`).
fn prop_key(prop: JsUnknown) -> Result<Option<String>> {
    if prop.get_type()? != ValueType::String {
        return Ok(None);
    }
    let s = unsafe { prop.cast::<JsString>() };
    Ok(Some(s.into_utf8()?.as_str()?.to_owned()))
}

/// Build a native-backed JS `Proxy`. Reads/writes flow through Rust trap callbacks into a
/// shared native map — the exact shape `ns_proxy.rs` will take under Node-API.
#[napi]
pub fn make_native_proxy(env: Env) -> Result<JsObject> {
    // Native backing store standing in for a WinRT instance's dynamic members.
    let store: Rc<RefCell<HashMap<String, f64>>> = Rc::new(RefCell::new(HashMap::new()));

    let mut handler = env.create_object()?;

    // get(target, prop, receiver) -> number | undefined
    let get_store = store.clone();
    let get_fn = env.create_function_from_closure(
        "get",
        move |ctx: CallContext| -> Result<Either<f64, ()>> {
            let prop = ctx.get::<JsUnknown>(1)?;
            match prop_key(prop)? {
                Some(key) => match get_store.borrow().get(&key) {
                    Some(v) => Ok(Either::A(*v)),
                    None => Ok(Either::B(())),
                },
                None => Ok(Either::B(())),
            }
        },
    )?;
    handler.set_named_property("get", get_fn)?;

    // set(target, prop, value, receiver) -> boolean
    let set_store = store.clone();
    let set_fn = env.create_function_from_closure(
        "set",
        move |ctx: CallContext| -> Result<bool> {
            let prop = ctx.get::<JsUnknown>(1)?;
            let value = ctx.get::<JsNumber>(2)?.get_double()?;
            if let Some(key) = prop_key(prop)? {
                set_store.borrow_mut().insert(key, value);
            }
            Ok(true)
        },
    )?;
    handler.set_named_property("set", set_fn)?;

    // has(target, prop) -> boolean
    let has_store = store.clone();
    let has_fn = env.create_function_from_closure(
        "has",
        move |ctx: CallContext| -> Result<bool> {
            let prop = ctx.get::<JsUnknown>(1)?;
            Ok(match prop_key(prop)? {
                Some(key) => has_store.borrow().contains_key(&key),
                None => false,
            })
        },
    )?;
    handler.set_named_property("has", has_fn)?;

    // new Proxy(target, handler)
    let target = env.create_object()?;
    let global = env.get_global()?;
    let proxy_ctor: JsFunction = global.get_named_property("Proxy")?;
    let instance = proxy_ctor.new_instance(&[target, handler])?;
    Ok(instance)
}
