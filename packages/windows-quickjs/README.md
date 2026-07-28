# @nativescript/windows-quickjs

NativeScript Windows runtime on **embedded QuickJS** (quickjs-ng + the napi-android node_api shim),
with no Node/Bun/Deno host. The engine-neutral `runtime::napi_engine` WinRT interop runs unchanged
over the shim's `napi_env`.

## As an app runtime (drop-in for `@nativescript/windows`)
Publishes as `@nativescript/windows-quickjs`, the **same** WinUI 3 framework as
`@nativescript/windows` with the QuickJS runtime DLL — swap the dependency and an app runs
unchanged. Build the framework with the engine flag:
```sh
pwsh -File ../../template/build.ps1 -Engine quickjs   # or: npm run build
```
`src/abi.rs` (the `host_dll` feature) is the reference implementation of the engine → runtime-DLL
adapter (the C ABI the WinUI 3 host P/Invokes). See `../README.md` for the full contract.

## Standalone host (dev/bench)
```sh
cargo build --release --manifest-path packages/windows-quickjs/Cargo.toml
target/release/nativescript-windows.exe   # runs the WinRT demo, exit 0
```
`--no-default-features` builds only the engine smoke lib (no napi/v8).

## Notes
- The engine is compiled from `vendor/` (quickjs-ng fork + shim); see `vendor` sources.
- Carries a fix for a double-free in the shim's finalizer (`quickjs-api.c`
  `JSFinalizeValueCallback`) — worth upstreaming to napi-android.
