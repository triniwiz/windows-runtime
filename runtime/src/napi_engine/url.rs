//! Native `URL` parsing helpers for the napi backend, over the `url` crate (WHATWG). These back the
//! JS `URL`/`URLSearchParams` polyfill that standalone engine hosts install when the host doesn't
//! already provide `URL` (Node/Bun/Deno do; QuickJS/Hermes/JSC/V8-standalone don't). Two functions:
//! `__urlParse(input, base?)` → a components object (throws on invalid), and
//! `__urlWith(href, key, value)` → the re-serialized href after setting one component.

use napi::{CallContext, Env, JsGlobal, JsObject, JsString, JsUnknown};
use url::Url;

fn opt_string(ctx: &CallContext, i: usize) -> Option<String> {
    let v = ctx.get::<JsUnknown>(i).ok()?;
    if !matches!(v.get_type(), Ok(napi::ValueType::String)) {
        return None;
    }
    let s = unsafe { v.cast::<JsString>() };
    s.into_utf8().ok()?.as_str().ok().map(|x| x.to_owned())
}

fn parse(input: &str, base: Option<&str>) -> Result<Url, url::ParseError> {
    match base {
        Some(b) => Url::options().base_url(Some(&Url::parse(b)?)).parse(input),
        None => Url::parse(input),
    }
}

/// WHATWG component values for a parsed URL (matches the classic v8 URL globals).
fn components(env: &Env, u: &Url) -> napi::Result<JsObject> {
    let mut o = env.create_object()?;
    let mut set = |k: &str, v: String| -> napi::Result<()> {
        o.set_named_property(k, env.create_string(&v)?)
    };
    set("href", u.as_str().to_owned())?;
    set("protocol", format!("{}:", u.scheme()))?;
    set("username", u.username().to_owned())?;
    set("password", u.password().unwrap_or("").to_owned())?;
    let hostname = u.host_str().unwrap_or("").to_owned();
    let port = u.port().map(|p| p.to_string()).unwrap_or_default();
    set("hostname", hostname.clone())?;
    set("port", port.clone())?;
    set("host", if port.is_empty() { hostname } else { format!("{hostname}:{port}") })?;
    set("pathname", u.path().to_owned())?;
    set("search", u.query().map(|q| format!("?{q}")).unwrap_or_default())?;
    set("hash", u.fragment().map(|f| format!("#{f}")).unwrap_or_default())?;
    // `url` gives "null" for opaque origins; WHATWG expects the literal string "null".
    set("origin", u.origin().ascii_serialization())?;
    Ok(o)
}

/// Install `__urlParse` and `__urlWith` on the global. Idempotent-safe; the JS polyfill decides
/// whether to expose `URL`/`URLSearchParams` (only when the host lacks them).
pub fn install_url_natives(env: &Env, global: &mut JsGlobal) -> napi::Result<()> {
    let parse_fn = env.create_function_from_closure("__urlParse", |ctx: CallContext| {
        let env = &ctx.env;
        let input = ctx
            .get::<JsString>(0)?
            .into_utf8()?
            .as_str()?
            .to_owned();
        let base = opt_string(&ctx, 1);
        let u = parse(&input, base.as_deref())
            .map_err(|e| napi::Error::from_reason(format!("Invalid URL: {e}")))?;
        components(env, &u)
    })?;
    global.set_named_property("__urlParse", parse_fn)?;

    let with_fn = env.create_function_from_closure("__urlWith", |ctx: CallContext| {
        let env = &ctx.env;
        let href = ctx.get::<JsString>(0)?.into_utf8()?.as_str()?.to_owned();
        let key = ctx.get::<JsString>(1)?.into_utf8()?.as_str()?.to_owned();
        let val = ctx.get::<JsString>(2)?.into_utf8()?.as_str()?.to_owned();
        let mut u = Url::parse(&href)
            .map_err(|e| napi::Error::from_reason(format!("Invalid URL: {e}")))?;
        let bad = || napi::Error::from_reason(format!("cannot set {key}"));
        match key.as_str() {
            "href" => u = Url::parse(&val).map_err(|e| napi::Error::from_reason(format!("Invalid URL: {e}")))?,
            "protocol" => u.set_scheme(val.trim_end_matches(':')).map_err(|_| bad())?,
            "username" => u.set_username(&val).map_err(|_| bad())?,
            "password" => u.set_password(if val.is_empty() { None } else { Some(&val) }).map_err(|_| bad())?,
            "hostname" => u.set_host(if val.is_empty() { None } else { Some(&val) }).map_err(|_| bad())?,
            "port" => u.set_port(val.parse().ok()).map_err(|_| bad())?,
            "pathname" => u.set_path(&val),
            "search" => u.set_query(if val.is_empty() { None } else { Some(val.trim_start_matches('?')) }),
            "hash" => u.set_fragment(if val.is_empty() { None } else { Some(val.trim_start_matches('#')) }),
            _ => return Err(bad()),
        }
        Ok(env.create_string(u.as_str())?)
    })?;
    global.set_named_property("__urlWith", with_fn)?;
    Ok(())
}
