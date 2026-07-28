//! Self-test/demo support for the NativeScript Windows standalone-engine `nativescript-windows`
//! bin targets (`windows-quickjs`, `windows-hermes`, …): a native crash reporter and the small
//! demo harness that runs a list of JS stages against whatever engine the package embeds.
//!
//! This crate is only a dependency of those `[[bin]]` targets, never of the packages' `[lib]`
//! (the cdylib/rlib that ships as the napi addon or `nativescript.dll`) — see `ns-windows-common`
//! for the pieces (prelude, URL polyfill) that both the demo bin and the shipped addon need.

pub mod bench;
pub mod crash;
pub mod harness;
