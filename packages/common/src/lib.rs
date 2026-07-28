//! Shared runtime support for the NativeScript Windows standalone-engine packages
//! (`windows-quickjs`, `windows-hermes`, …): the JS prelude (queueMicrotask + NSWinRT) and the
//! URL/URLSearchParams polyfill, injected by both the shipped napi addon/cdylib and the
//! `nativescript-windows` demo bin. See `ns-windows-demo` for demo-only, bin-only support
//! (crash reporter, self-test harness) that the shipped addon never touches.

pub mod prelude;
pub mod url_polyfill;
