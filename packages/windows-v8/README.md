# @nativescript/windows-v8

NativeScript Windows runtime on **V8** — the same engine our classic runtime is built on. No Node.

## As an app runtime (drop-in for `@nativescript/windows`)
Publishes as `@nativescript/windows-v8`, the **same** WinUI 3 framework as `@nativescript/windows`
with a napi-backed V8 runtime DLL — swap the dependency and an app runs unchanged. Build the
framework with the engine flag:
```sh
pwsh -File ../../template/build.ps1 -Engine v8   # or: npm run build
```
The engine → runtime-DLL adapter (the C ABI the WinUI 3 host P/Invokes) follows the reference
implementation in `../windows-quickjs/src/abi.rs`. See `../README.md` for the full contract.

## Standalone host
Reuses napi-android's `v8-api.cpp` (napi over V8's C++ API), **ported to V8 14.7**, compiled against
the `v8` crate's bundled headers and linked against its prebuilt rusty_v8 (V8 14.7). The Android
bring-up (`jsr.cpp`) is replaced by `csrc/win_jsr.cpp`. Full WinRT demo passes (`exit 0`):
static-method resolution, typed WinRT calls, a 20-call stress loop, and a `JsonObject` round-trip.

```sh
cargo build --release --manifest-path packages/windows-v8/Cargo.toml
target/release/nativescript-windows.exe
```

## Why this was tractable (no ABI nightmare)
The `v8` crate's default config has **pointer compression and the sandbox OFF** (they're opt-in
cargo features we don't enable), so V8 uses plain/uncompressed pointers — there are **no ABI-matching
defines** and none of the corruption risk that config would bring. The port was therefore a bounded
API migration, not an ABI hunt.

## What the port involved (V8 13 → 14.7)
- Build recipe: C++20 + `/Zc:__cplusplus` (MSVC reports the real `__cplusplus`), locate the crate's
  `v8/include`, stub `<android/log.h>`, `NAPI_EXTERN` → `__declspec(dllexport)`, define `__V8_13__`
  (take the shim's modern `SetAccessorProperty` etc. paths). No pointer-compression/sandbox defines.
- API migrations: `String::Write/WriteUtf8/WriteOneByte` → `*V2` (+ `WriteFlags`); mandatory pointer
  **type tags** on internal fields / `External` (safe defaults `kEmbedderDataTypeTagDefault` /
  `kExternalPointerTypeTagDefault`); `Context::GetIsolate()` removed → `Isolate::GetCurrent()`;
  interceptor setters return `v8::Intercepted` (`ReturnValue<void>`).
- Platform init done from **Rust** (`v8::new_default_platform` → `V8::initialize`) because
  `NewDefaultPlatform`'s `std::unique_ptr<Platform>` return can't be linked from the MSVC-STL shim
  (rusty_v8 uses V8's bundled libc++). See below.

## The libc++ boundary — solved, including zero-copy
rusty_v8 is built with V8's bundled libc++, so V8 API methods whose signatures involve
`std::shared_ptr`/`unique_ptr<BackingStore>` can't be linked directly from this MSVC-STL shim.
Handled without any copy:
- Reads use `ArrayBuffer::Data()` (the direct `void*` accessor — no `BackingStore`).
- `napi_create_external_arraybuffer` is **true zero-copy**: it goes through rusty_v8's own
  `extern "C"` bindings (`v8__ArrayBuffer__NewBackingStore__with_data`, the unique→shared converter,
  `v8__ArrayBuffer__New__with_backing_store`) which pass the backing store as raw pointers / an
  opaque 16-byte `shared_ptr`, then bridges the returned pointer back to a `Local` (a `Local` is one
  pointer). The buffer aliases the source memory; the napi finalizer runs from the BackingStore
  deleter on GC. **Verified:** `interop.arrayBufferFromBuffer(CryptographicBuffer.GenerateRandom(8))`
  → `byteLength=8`, `instanceof ArrayBuffer`, no copy. So the IBuffer→ArrayBuffer perf path is intact.
