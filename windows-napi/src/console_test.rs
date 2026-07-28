//! Verification hooks for the ported console formatter. Test-only surface.

use napi::{Env, JsUnknown};
use napi_derive::napi;

/// Format one value exactly as `console.log` (rich=false) or `console.dir` (rich=true) would.
#[napi]
pub fn format_value(env: Env, value: JsUnknown, rich: bool) -> String {
    let mut out = String::new();
    runtime::napi_engine::console::format_item(&env, &value, &mut out, true, rich);
    out
}

/// Render `console.table` output for a value (optional column filter).
#[napi]
pub fn table_for(env: Env, value: JsUnknown, columns: Option<Vec<String>>) -> String {
    runtime::napi_engine::console::format_table(&env, &value, columns.as_deref())
}
