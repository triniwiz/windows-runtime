//! Node-API port of classic's native `NSWinRT.*ItemsSource` surface (required by core's Windows
//! ListView). Reuses `crate::js_observable_vector` as-is (pure `windows_core` COM, no V8 dep);
//! only the JS <-> napi glue below is new.

use std::ffi::c_void;
use std::mem::ManuallyDrop;

use napi::{CallContext, Env, JsFunction, JsNumber, JsObject, JsUnknown, NapiRaw, NapiValue, ValueType};
use windows_core::{IInspectable, Interface};

use crate::napi_engine::value::{as_unknown, external_from_ptr, ptr_from_external};

/// Coerce argument `index` to a non-negative u32, defaulting to 0 (missing/non-numeric). The JS
/// wrapper already truncates via `>>> 0`; this is only a defensive fallback for direct calls.
fn arg_u32(ctx: &CallContext, index: usize) -> u32 {
    if ctx.length <= index {
        return 0;
    }
    match ctx.get::<JsUnknown>(index) {
        Ok(v) => match v.coerce_to_number().and_then(|n: JsNumber| n.get_double()) {
            Ok(n) if n.is_finite() => n.max(0.0) as u32,
            _ => 0,
        },
        Err(_) => 0,
    }
}

/// Borrow (without releasing) the IInspectable that a `__nsMakeItemsSource` `{ handle }`
/// External owns. The External keeps ownership, so the returned value must not be dropped.
fn source_inspectable(ctx: &CallContext, index: usize) -> Option<ManuallyDrop<IInspectable>> {
    let arg = ctx.get::<JsUnknown>(index).ok()?;
    if !matches!(arg.get_type(), Ok(ValueType::Object)) {
        return None;
    }
    let obj: JsObject = unsafe { arg.cast() };
    let handle = obj.get_named_property::<JsUnknown>("handle").ok()?;
    let raw = ptr_from_external(&ctx.env, &handle)?;
    if raw.is_null() {
        return None;
    }
    Some(ManuallyDrop::new(unsafe { IInspectable::from_raw(raw) }))
}

/// Installs the `__ns*ItemsSource` natives + the `NSWinRT.*ItemsSource` JS surface (mirrors the
/// classic runtime's `global_fns.rs` block of the same name).
pub fn install_items_source(env: &Env) -> napi::Result<()> {
    let mut global = env.get_global()?;

    let make_fn = env.create_function_from_closure("__nsMakeItemsSource", |ctx: CallContext| {
        let count = arg_u32(&ctx, 0);
        match crate::js_observable_vector::make_index_vector(count) {
            Ok(inspectable) => {
                let raw = Interface::into_raw(inspectable) as *mut c_void;
                let handle = external_from_ptr(&ctx.env, raw)
                    .map_err(|e| napi::Error::from_reason(e.to_string()))?;
                let mut result = ctx.env.create_object()?;
                result.set_named_property("handle", handle)?;
                Ok(as_unknown(&ctx.env, result))
            }
            Err(_) => Err(napi::Error::from_reason(
                "NSWinRT.makeItemsSource: failed to build native vector".to_string(),
            )),
        }
    })?;
    global.set_named_property("__nsMakeItemsSource", make_fn)?;

    let extend_fn =
        env.create_function_from_closure("__nsExtendItemsSource", |ctx: CallContext| {
            if let Some(inspectable) = source_inspectable(&ctx, 0) {
                let new_count = arg_u32(&ctx, 1);
                let _ = crate::js_observable_vector::extend_index_vector(&inspectable, new_count);
            }
            Ok(())
        })?;
    global.set_named_property("__nsExtendItemsSource", extend_fn)?;

    let insert_fn =
        env.create_function_from_closure("__nsInsertItemsSource", |ctx: CallContext| {
            if let Some(inspectable) = source_inspectable(&ctx, 0) {
                let index = arg_u32(&ctx, 1);
                let count = arg_u32(&ctx, 2);
                let _ =
                    crate::js_observable_vector::insert_index_vector(&inspectable, index, count);
            }
            Ok(())
        })?;
    global.set_named_property("__nsInsertItemsSource", insert_fn)?;

    let remove_fn =
        env.create_function_from_closure("__nsRemoveItemsSource", |ctx: CallContext| {
            if let Some(inspectable) = source_inspectable(&ctx, 0) {
                let index = arg_u32(&ctx, 1);
                let count = arg_u32(&ctx, 2);
                let _ =
                    crate::js_observable_vector::remove_index_vector(&inspectable, index, count);
            }
            Ok(())
        })?;
    global.set_named_property("__nsRemoveItemsSource", remove_fn)?;

    let update_fn =
        env.create_function_from_closure("__nsUpdateItemsSource", |ctx: CallContext| {
            if let Some(inspectable) = source_inspectable(&ctx, 0) {
                let index = arg_u32(&ctx, 1);
                let count = arg_u32(&ctx, 2);
                let _ =
                    crate::js_observable_vector::update_index_vector(&inspectable, index, count);
            }
            Ok(())
        })?;
    global.set_named_property("__nsUpdateItemsSource", update_fn)?;

    let reset_fn =
        env.create_function_from_closure("__nsResetItemsSource", |ctx: CallContext| {
            if let Some(inspectable) = source_inspectable(&ctx, 0) {
                let new_count = arg_u32(&ctx, 1);
                let _ = crate::js_observable_vector::reset_index_vector(&inspectable, new_count);
            }
            Ok(())
        })?;
    global.set_named_property("__nsResetItemsSource", reset_fn)?;

    let func_ctor: JsFunction = global.get_named_property("Function")?;
    let body = env.create_string(ITEMS_SOURCE_HELPERS_JS)?;
    let installer_obj = func_ctor.new_instance(&[body])?;
    let installer: JsFunction = unsafe { JsFunction::from_raw(env.raw(), installer_obj.raw()) }?;
    installer.call_without_args(None)?;
    Ok(())
}

/// JS half — identical to the classic runtime's `NSWinRT.*ItemsSource` block in `global_fns.rs`.
const ITEMS_SOURCE_HELPERS_JS: &str = r#"
'use strict';
(function (g) {
    g.NSWinRT = g.NSWinRT || {};
    g.NSWinRT.makeItemsSource = function(count) {
        if (typeof g.__nsMakeItemsSource === 'function')
            return g.__nsMakeItemsSource(count >>> 0);
        return null;
    };
    g.NSWinRT.extendItemsSource = function(source, newCount) {
        if (source && typeof g.__nsExtendItemsSource === 'function')
            g.__nsExtendItemsSource(source, newCount >>> 0);
    };
    g.NSWinRT.insertItemsSource = function(source, index, count) {
        if (source && typeof g.__nsInsertItemsSource === 'function')
            g.__nsInsertItemsSource(source, index >>> 0, count >>> 0);
    };
    g.NSWinRT.removeItemsSource = function(source, index, count) {
        if (source && typeof g.__nsRemoveItemsSource === 'function')
            g.__nsRemoveItemsSource(source, index >>> 0, count >>> 0);
    };
    g.NSWinRT.updateItemsSource = function(source, index, count) {
        if (source && typeof g.__nsUpdateItemsSource === 'function')
            g.__nsUpdateItemsSource(source, index >>> 0, count >>> 0);
    };
    g.NSWinRT.resetItemsSource = function(source, count) {
        if (source && typeof g.__nsResetItemsSource === 'function')
            g.__nsResetItemsSource(source, count >>> 0);
    };
})(globalThis);
'items-source-ok'
"#;
