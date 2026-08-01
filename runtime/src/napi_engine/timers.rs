//! Engine-neutral timers for standalone hosts: `setTimeout` / `setInterval` / `clearTimeout` /
//! `clearInterval` / `setImmediate` / `clearImmediate`, fired by the standalone event loop
//! (`napi_engine::event_loop`).
//!
//! Install-if-missing like the other napi globals: Node/Bun/Deno already provide timers backed by
//! their own event loops, so ours only install on bare engines (QuickJS/Hermes/V8/JSC shims).
//! Callbacks and their extra arguments are pinned with napi references until the timer fires or
//! is cleared. Extra call arguments are stored inside a JS array behind a single reference —
//! plain-value references are not supported by every engine's Node-API implementation, arrays are.

use std::cell::RefCell;
use std::time::{Duration, Instant};

use napi::{sys, CallContext, Env, JsFunction, JsUnknown, NapiRaw, ValueType};

use crate::globals::console::write_console;

struct Timer {
    id: u64,
    due: Instant,
    /// `Some` for `setInterval` (reschedule after firing), `None` for `setTimeout`.
    interval: Option<Duration>,
    cb: sys::napi_ref,
    /// Reference to a JS array holding the extra call arguments; null when there are none.
    args: sys::napi_ref,
    env: sys::napi_env,
}

impl Timer {
    /// Drop the napi references pinning the callback (and args array). JS-thread only.
    unsafe fn release_refs(&self) {
        let _ = sys::napi_delete_reference(self.env, self.cb);
        if !self.args.is_null() {
            let _ = sys::napi_delete_reference(self.env, self.args);
        }
    }
}

struct Registry {
    timers: Vec<Timer>,
    /// FIFO `setImmediate` queue (reuses `Timer` with `due`/`interval` unused). Ids are appended
    /// in increasing order, which [`run_due_immediates`] relies on for its batch boundary.
    immediates: Vec<Timer>,
    next_id: u64,
    /// Timer currently being fired (already popped from `timers`).
    firing: Option<u64>,
    /// Set when `clearTimeout`/`clearInterval` targets the currently-firing timer, so an
    /// interval that clears itself from its own callback is not rescheduled.
    firing_cleared: bool,
}

thread_local! {
    static REGISTRY: RefCell<Registry> = RefCell::new(Registry {
        timers: Vec::new(),
        immediates: Vec::new(),
        next_id: 1,
        firing: None,
        firing_cleared: false,
    });
}

/// Install `setTimeout`/`setInterval`/`clearTimeout`/`clearInterval` on the global object if the
/// host does not already provide them (keyed on `setTimeout`, matching the all-or-nothing way
/// real hosts expose the group).
pub fn install_timers(env: &Env) -> napi::Result<()> {
    let mut global = env.get_global()?;
    let has_timers = matches!(
        global
            .get_named_property::<JsUnknown>("setTimeout")
            .and_then(|v| v.get_type()),
        Ok(ValueType::Function)
    );
    if has_timers {
        return Ok(());
    }

    let set_timeout =
        env.create_function_from_closure("setTimeout", |ctx: CallContext| schedule(&ctx, false))?;
    global.set_named_property("setTimeout", set_timeout)?;
    let set_interval =
        env.create_function_from_closure("setInterval", |ctx: CallContext| schedule(&ctx, true))?;
    global.set_named_property("setInterval", set_interval)?;
    let clear_timeout =
        env.create_function_from_closure("clearTimeout", |ctx: CallContext| clear(&ctx))?;
    global.set_named_property("clearTimeout", clear_timeout)?;
    let clear_interval =
        env.create_function_from_closure("clearInterval", |ctx: CallContext| clear(&ctx))?;
    global.set_named_property("clearInterval", clear_interval)?;
    let set_immediate = env
        .create_function_from_closure("setImmediate", |ctx: CallContext| schedule_immediate(&ctx))?;
    global.set_named_property("setImmediate", set_immediate)?;
    let clear_immediate =
        env.create_function_from_closure("clearImmediate", |ctx: CallContext| clear_immediate(&ctx))?;
    global.set_named_property("clearImmediate", clear_immediate)?;
    Ok(())
}

/// Pin the callback at arg 0 (+ extra call arguments from `args_from` onward, boxed into an
/// array) with napi references. Returns `(cb_ref, args_ref)`; `args_ref` is null when there are
/// no extra arguments.
fn pin_callback_and_args(
    ctx: &CallContext,
    args_from: usize,
) -> napi::Result<(sys::napi_ref, sys::napi_ref)> {
    let env = &ctx.env;
    let cb = ctx.get::<JsFunction>(0)?;

    let mut cb_ref: sys::napi_ref = std::ptr::null_mut();
    let status = unsafe { sys::napi_create_reference(env.raw(), cb.raw(), 1, &mut cb_ref) };
    if status != sys::Status::napi_ok || cb_ref.is_null() {
        return Err(napi::Error::from_reason("timer: failed to pin callback"));
    }

    let mut args_ref: sys::napi_ref = std::ptr::null_mut();
    if ctx.length > args_from {
        let mut arr = env.create_array_with_length(ctx.length - args_from)?;
        for i in args_from..ctx.length {
            arr.set_element((i - args_from) as u32, ctx.get::<JsUnknown>(i)?)?;
        }
        let status = unsafe { sys::napi_create_reference(env.raw(), arr.raw(), 1, &mut args_ref) };
        if status != sys::Status::napi_ok {
            args_ref = std::ptr::null_mut();
        }
    }
    Ok((cb_ref, args_ref))
}

/// setTimeout/setInterval body: pin the callback (+ extra args, boxed into an array), register
/// the timer, return its numeric id.
fn schedule(ctx: &CallContext, repeating: bool) -> napi::Result<f64> {
    let env = &ctx.env;

    let mut delay_ms = 0f64;
    if ctx.length > 1 {
        let raw = ctx.get::<JsUnknown>(1)?;
        delay_ms = raw
            .coerce_to_number()
            .and_then(|n| n.get_double())
            .unwrap_or(0.0);
        if !delay_ms.is_finite() || delay_ms < 0.0 {
            delay_ms = 0.0;
        }
    }
    // Intervals get a 1ms floor so a 0ms interval cannot monopolize the loop.
    let delay = Duration::from_micros((delay_ms * 1000.0) as u64);
    let interval = repeating.then(|| delay.max(Duration::from_millis(1)));

    let (cb_ref, args_ref) = pin_callback_and_args(ctx, 2)?;

    let id = REGISTRY.with(|r| {
        let mut reg = r.borrow_mut();
        let id = reg.next_id;
        reg.next_id += 1;
        reg.timers.push(Timer {
            id,
            due: Instant::now() + delay,
            interval,
            cb: cb_ref,
            args: args_ref,
            env: env.raw(),
        });
        id
    });
    Ok(id as f64)
}

/// clearTimeout/clearInterval body (one implementation serves both, as in real hosts).
fn clear(ctx: &CallContext) -> napi::Result<()> {
    if ctx.length == 0 {
        return Ok(());
    }
    let id = ctx
        .get::<JsUnknown>(0)?
        .coerce_to_number()
        .and_then(|n| n.get_double())
        .unwrap_or(f64::NAN);
    if !id.is_finite() || id < 0.0 {
        return Ok(());
    }
    let id = id as u64;
    REGISTRY.with(|r| {
        let mut reg = r.borrow_mut();
        if reg.firing == Some(id) {
            reg.firing_cleared = true;
            return;
        }
        if let Some(pos) = reg.timers.iter().position(|t| t.id == id) {
            let t = reg.timers.remove(pos);
            unsafe { t.release_refs() };
        }
    });
    Ok(())
}

/// setImmediate body: pin the callback (+ extra args) and append to the FIFO immediate queue.
fn schedule_immediate(ctx: &CallContext) -> napi::Result<f64> {
    let env = &ctx.env;
    let (cb_ref, args_ref) = pin_callback_and_args(ctx, 1)?;
    let id = REGISTRY.with(|r| {
        let mut reg = r.borrow_mut();
        let id = reg.next_id;
        reg.next_id += 1;
        reg.immediates.push(Timer {
            id,
            due: Instant::now(),
            interval: None,
            cb: cb_ref,
            args: args_ref,
            env: env.raw(),
        });
        id
    });
    Ok(id as f64)
}

/// clearImmediate body: drop a not-yet-fired immediate (fired ones are already popped, so a
/// callback clearing itself is a harmless no-op).
fn clear_immediate(ctx: &CallContext) -> napi::Result<()> {
    if ctx.length == 0 {
        return Ok(());
    }
    let id = ctx
        .get::<JsUnknown>(0)?
        .coerce_to_number()
        .and_then(|n| n.get_double())
        .unwrap_or(f64::NAN);
    if !id.is_finite() || id < 0.0 {
        return Ok(());
    }
    let id = id as u64;
    REGISTRY.with(|r| {
        let mut reg = r.borrow_mut();
        if let Some(pos) = reg.immediates.iter().position(|t| t.id == id) {
            let t = reg.immediates.remove(pos);
            unsafe { t.release_refs() };
        }
    });
    Ok(())
}

/// True while any timer or immediate is registered (keeps the standalone event loop alive).
pub fn has_pending() -> bool {
    REGISTRY.with(|r| {
        let reg = r.borrow();
        !reg.timers.is_empty() || !reg.immediates.is_empty()
    })
}

/// True while the immediate queue is non-empty (the event loop must not sleep).
pub fn has_immediates() -> bool {
    REGISTRY.with(|r| !r.borrow().immediates.is_empty())
}

/// Fire the immediates queued *before* this call, FIFO (Node check-phase semantics: an immediate
/// scheduled from an immediate callback runs on the next loop iteration, not this one). The
/// batch boundary is the id counter at entry — queue ids are monotonically increasing, so the
/// batch is exactly the front entries with `id < boundary`.
pub fn run_due_immediates(env: &Env) {
    let boundary = REGISTRY.with(|r| r.borrow().next_id);
    loop {
        let next = REGISTRY.with(|r| {
            let mut reg = r.borrow_mut();
            if reg.immediates.first().map(|t| t.id < boundary).unwrap_or(false) {
                Some(reg.immediates.remove(0))
            } else {
                None
            }
        });
        let Some(timer) = next else { break };
        fire(env, &timer);
        unsafe { timer.release_refs() };
    }
}

/// When the earliest registered timer is due, for the event loop's wait computation.
pub fn next_due() -> Option<Instant> {
    REGISTRY.with(|r| r.borrow().timers.iter().map(|t| t.due).min())
}

/// Fire every timer that is due, earliest first. Callbacks may register or clear timers freely
/// (the registry borrow is released around each call). Returns the next due instant, if any.
/// Uncaught callback exceptions are reported to the console and never escape into the loop.
pub fn run_due_timers(env: &Env) -> Option<Instant> {
    loop {
        let now = Instant::now();
        let due = REGISTRY.with(|r| {
            let mut reg = r.borrow_mut();
            let idx = reg
                .timers
                .iter()
                .enumerate()
                .filter(|(_, t)| t.due <= now)
                .min_by_key(|(_, t)| t.due)
                .map(|(i, _)| i);
            idx.map(|i| {
                let t = reg.timers.remove(i);
                reg.firing = Some(t.id);
                reg.firing_cleared = false;
                t
            })
        });
        let Some(mut timer) = due else { break };

        fire(env, &timer);

        REGISTRY.with(|r| {
            let mut reg = r.borrow_mut();
            let cleared = reg.firing_cleared;
            reg.firing = None;
            reg.firing_cleared = false;
            match timer.interval {
                Some(iv) if !cleared => {
                    timer.due = Instant::now() + iv;
                    reg.timers.push(timer);
                }
                _ => unsafe { timer.release_refs() },
            }
        });
    }
    next_due()
}

/// Invoke one timer callback with its pinned extra arguments.
fn fire(env: &Env, timer: &Timer) {
    unsafe {
        let raw = env.raw();
        let mut scope: sys::napi_handle_scope = std::ptr::null_mut();
        if sys::napi_open_handle_scope(raw, &mut scope) != sys::Status::napi_ok {
            return;
        }
        let mut func: sys::napi_value = std::ptr::null_mut();
        if sys::napi_get_reference_value(raw, timer.cb, &mut func) == sys::Status::napi_ok
            && !func.is_null()
        {
            let mut args: Vec<sys::napi_value> = Vec::new();
            if !timer.args.is_null() {
                let mut arr: sys::napi_value = std::ptr::null_mut();
                if sys::napi_get_reference_value(raw, timer.args, &mut arr)
                    == sys::Status::napi_ok
                    && !arr.is_null()
                {
                    let mut len = 0u32;
                    if sys::napi_get_array_length(raw, arr, &mut len) == sys::Status::napi_ok {
                        for i in 0..len {
                            let mut el: sys::napi_value = std::ptr::null_mut();
                            if sys::napi_get_element(raw, arr, i, &mut el)
                                == sys::Status::napi_ok
                            {
                                args.push(el);
                            }
                        }
                    }
                }
            }
            let mut recv: sys::napi_value = std::ptr::null_mut();
            let _ = sys::napi_get_undefined(raw, &mut recv);
            let mut result: sys::napi_value = std::ptr::null_mut();
            let status =
                sys::napi_call_function(raw, recv, func, args.len(), args.as_ptr(), &mut result);
            if status != sys::Status::napi_ok {
                // Match host behavior: report the uncaught exception, keep the loop running.
                let mut exc: sys::napi_value = std::ptr::null_mut();
                if sys::napi_get_and_clear_last_exception(raw, &mut exc) == sys::Status::napi_ok
                    && !exc.is_null()
                {
                    let msg = describe_exception(raw, exc);
                    write_console(&format!("[ERROR] Uncaught exception in timer callback: {msg}\n"));
                }
            }
        }
        let _ = sys::napi_close_handle_scope(raw, scope);
    }
}

unsafe fn describe_exception(env: sys::napi_env, exc: sys::napi_value) -> String {
    let mut coerced: sys::napi_value = std::ptr::null_mut();
    if sys::napi_coerce_to_string(env, exc, &mut coerced) != sys::Status::napi_ok {
        return "<unprintable exception>".into();
    }
    let mut len = 0usize;
    if sys::napi_get_value_string_utf8(env, coerced, std::ptr::null_mut(), 0, &mut len)
        != sys::Status::napi_ok
    {
        return "<unprintable exception>".into();
    }
    let mut buf = vec![0u8; len + 1];
    let mut written = 0usize;
    if sys::napi_get_value_string_utf8(env, coerced, buf.as_mut_ptr() as *mut _, buf.len(), &mut written)
        != sys::Status::napi_ok
    {
        return "<unprintable exception>".into();
    }
    buf.truncate(written);
    String::from_utf8_lossy(&buf).into_owned()
}
