# @nativescript/windows-jsc

NativeScript Windows runtime on **JavaScriptCore**, no Node. **Working** — the full WinRT demo
passes (`exit 0`): typed WinRT calls, a 20-call stress loop, and a `JsonObject` round-trip.

## As an app runtime (drop-in for `@nativescript/windows`)
Publishes as `@nativescript/windows-jsc`, the **same** WinUI 3 framework as `@nativescript/windows`
with the JavaScriptCore runtime DLL — swap the dependency and an app runs unchanged. Build the
framework with the engine flag (needs a real `JavaScriptCore.lib` in `vendor/x64`):
```sh
pwsh -File ../../template/build.ps1 -Engine jsc   # or: npm run build
```
The engine → runtime-DLL adapter (the C ABI the WinUI 3 host P/Invokes) follows the reference
implementation in `../windows-quickjs/src/abi.rs`. See `../README.md` for the full contract.

## The engine binary — Playwright's WebKit (current)
The official WebKit WinCairo buildbot is dead (last Windows build Sept 2024), and there's no NuGet/npm
prebuilt. The **current** source of a Windows `JavaScriptCore.dll` is **Playwright**, which rebuilds
WebKit constantly:

```
https://playwright.download.prss.microsoft.com/dbazure/download/playwright/builds/webkit/<rev>/webkit-win64.zip
```
(`<rev>` from `packages/playwright-core/browsers.json` in microsoft/playwright — 2331 at time of
writing; the vendored `JavaScriptCore.dll` is dated 2026-07-14). It exports the full JSC C API and
depends only on `icuin77.dll` / `icuuc77.dll` (+ `icudt77.dll` data).

`vendor/x64/` holds (committed, no LFS): `JavaScriptCore.dll` (33 MB), `icudt77.dll` (32 MB),
`icuin77.dll`, `icuuc77.dll`, and `JavaScriptCore.lib` — an **import lib generated from the DLL's
exports** (no `.lib` ships in the zip):
```sh
llvm-objdump -p JavaScriptCore.dll | awk '/Export Table/{f=1;next} f&&/0x/{print $NF}' \
  | grep -E '^(JS|WK|k)' | awk '{print ($0 ~ /^k/) ? $0" DATA" : $0}' > exports   # k* are DATA symbols
printf 'EXPORTS\n' > JavaScriptCore.def; cat exports >> JavaScriptCore.def
lib /def:JavaScriptCore.def /machine:x64 /out:JavaScriptCore.lib
```

## Build & run
```sh
cargo build --release --features jsc_link --manifest-path packages/windows-jsc/Cargo.toml
target/release/nativescript-windows.exe
```
`build.rs` links `JavaScriptCore.lib` and copies the DLLs beside the exe.

## Shim = napi-android's JSC provider (public C API), + two Windows fixes
`jsc-api.cpp` is over JSC's stable public C API (no WebKit internals), so it's version-robust. Two
bugs surfaced on Windows and are fixed in the vendored shim:
- **UTF-16 read truncation**: `CopyTo` did `memcpy(buf, chars, size)` where `size` is a count of
  2-byte `JSChar` → copied half the bytes (`'hi'`→`'h'`). Fixed to `size * sizeof(JSChar)`.
- **napi functions weren't constructors**: `FunctionInfo`'s `JSClassDefinition` set `callAsFunction`
  but not `callAsConstructor`, so a JS `Proxy` over them wasn't a constructor (`new
  Windows.Data.Json.JsonObject()` → "not a constructor"). Added a `CallAsConstructor` callback (the
  JSC analog of the QuickJS `JS_SetConstructorBit` fix; V8's napi functions are constructable by
  default).
Also `EXTERN_C` on the JSR bring-up decls (`jsr_common.h`) so the Rust host's `extern "C"` resolves.
