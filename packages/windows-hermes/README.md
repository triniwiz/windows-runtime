# @nativescript/windows-hermes

NativeScript Windows runtime on **Microsoft's prebuilt Hermes** (`hermes.dll`, which exports both
`napi_*` and the JSR C API), with no Node/Bun/Deno host. The engine-neutral
`runtime::napi_engine` WinRT interop runs unchanged over Hermes's `napi_env`.

## As an app runtime (drop-in for `@nativescript/windows`)
Publishes as `@nativescript/windows-hermes`, the **same** WinUI 3 framework as
`@nativescript/windows` with the Hermes runtime DLL — swap the dependency and an app runs
unchanged. Build the framework with the engine flag:
```sh
pwsh -File ../../template/build.ps1 -Engine hermes   # or: npm run build
```
The engine → runtime-DLL adapter (the C ABI the WinUI 3 host P/Invokes) follows the reference
implementation in `../windows-quickjs/src/abi.rs`. See `../README.md` for the full contract.

## Standalone host (dev/bench)
```sh
cargo build --release --manifest-path packages/windows-hermes/Cargo.toml
target/release/nativescript-windows.exe   # runs the WinRT demo, exit 0
```
build.rs links `vendor/x64/hermes.lib`, forward-exports Hermes's `napi_*` from the exe (so napi-sys
resolves them), and copies `hermes.dll`/`hermes-icu.dll` next to the binary.

## Notes
- Prebuilt engine binaries are committed under `vendor/` — see `vendor/README.md` for provenance
  (Microsoft.JavaScript.Hermes NuGet) and the refresh recipe.
