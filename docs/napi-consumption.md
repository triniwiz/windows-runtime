# Consuming `@nativescript/windows` from Node, Bun, and Deno

The package is a napi-rs native addon (`windows.<triple>.node`) plus a small JS layer
(`index.js` from napi-rs, and `nswinrt.js` with the WinRT ergonomics: `toPromise`, auto-pump,
`interop.*`). Because it targets the **Node-API** ABI, any runtime that implements Node-API can
load it — the engine underneath differs, but the interop code does not.

## Loading

### Node.js (V8)
```js
const { Windows, toPromise, enableAutoPump } = require('@nativescript/windows/nswinrt.js');
enableAutoPump();
```
Or ESM via `createRequire`. No flags needed. This is the primary, fully-tested path.

### Bun (JavaScriptCore)
Bun implements Node-API and loads `.node` addons through the same `require`/`import`:
```js
import { Windows, toPromise } from '@nativescript/windows/nswinrt.js';
```
This is the **JSC-on-Windows** route — no WebKit embedding needed; Bun provides JSC + Node-API.

### Deno (V8)
Deno implements Node-API behind its Node-compat layer. Import via the `npm:` specifier (or
`createRequire`), and grant the permissions the addon needs:
```
deno run --allow-ffi --allow-read --allow-env --allow-write app.js
```
```js
import { Windows, toPromise } from "npm:@nativescript/windows/nswinrt.js";
```
`--allow-ffi` is required for native addons; `--allow-read` is needed for winmd discovery
(below).

> The native `.node` is identical across all three — only the host engine changes. `nswinrt.js`
> is plain CommonJS and uses only `require('./index.js')` + timers, so it runs unmodified on all
> three runtimes.

## winmd discovery (the important part)

WinRT type metadata comes from two sources:

1. **System / OS types** (`Windows.*`) — resolved by the OS via `RoGetMetaDataFile`. **No files
   ship with the app**; these always resolve on any Windows machine, in any runtime. Everything
   in the test suite (JsonObject, Uri, Calendar, ThreadPool, CryptographicBuffer, …) is system
   metadata and needs zero configuration.

2. **Third-party / app `.winmd`** (WebView2, your own WinRT components) — **not** discoverable by
   the OS; they must be registered explicitly.

### How the napi package finds them

The rusty_v8 runtime auto-scans for `.winmd` in `Runtime::new` (exe dir + app root). **The napi
path never constructs a `Runtime`** — it drives interop directly on the host's `napi_env` — so
that scan is reproduced on the napi side:

- **Automatic:** the first `getNamespace(...)` (or any interop call) runs a one-time scan of the
  **current working directory** and the **host executable's directory**
  (`scan_default_winmd_dirs`). Drop your `.winmd` next to where you launch and it's picked up.
- **Explicit (recommended for apps):** point the runtime at your metadata directly —
  ```js
  const { interop } = require('@nativescript/windows/nswinrt.js');
  interop.registerWinmd('C:/app/metadata/MyComponent.winmd');   // one file
  interop.scanWinmdDir('C:/app/metadata');                       // whole folder → count
  ```
  Call these **before** touching the corresponding namespaces.

### Why auto-scan differs from the standalone runtime

Under the standalone (embedded-V8) runtime, `current_exe` is *your app's* exe, so the exe-dir
scan finds app-bundled winmd automatically. Under Node/Bun/Deno, `current_exe` is
`node.exe`/`bun.exe`/`deno.exe` — so the exe-dir scan covers the runtime's own directory, and the
**cwd scan + explicit `registerWinmd`/`scanWinmdDir`** are how app metadata gets loaded. A CLI
that launches apps should either `chdir` to the metadata location or call `scanWinmdDir` on
startup. (This matches the NativeScript CLI's existing responsibility to deploy the `.winmd`.)

## The message loop / async

WinRT async completions (`IAsyncAction`/`IAsyncOperation`) are delivered on the STA thread's
message queue, so that queue must be pumped for a `Promise` to settle. `nswinrt.js` handles this:

- `enableAutoPump()` — installs a ref-counted pump on the host timer loop; it's ref'd only while
  a WinRT `Promise` is outstanding (so the process still exits normally). After this, plain
  `await toPromise(op)` just works.
- `awaitWithPump(promise, timeoutMs)` — explicit alternative that pumps inline until settle.

This is the same on all three runtimes (all expose `setInterval`/`setImmediate`).

## Natural-syntax extras

Beyond method/property/event access, the napi backend supports:

- **Keyed maps (`IMap`/`IMapView`/`IPropertySet`)** — classes whose default interface is a map
  (`StringMap`, `PropertySet`, `ValueSet`, `ApplicationDataContainerSettings`, …) and
  interface-typed map returns get keyed sugar: `m['key']` → `Lookup`, `m['key'] = v` → `Insert`,
  `'key' in m` → `HasKey`, plus `m.length` → `Size`. WinRT member names always win over map
  keys (use `m.Lookup('Size')` for a shadowed key). `PropertySet`/`ValueSet` values box on
  insert and unbox (`IPropertyValue` primitives) on lookup, so `ps['n'] = 42; ps['n'] === 42`.
  Classes that merely *also* implement `IMap` alongside a richer identity (e.g. `JsonObject`)
  stay host objects — call `Lookup`/`Insert` explicitly there.
- **Subclassing** — `class Sub extends WinRTClass` works on both object models (host objects
  and Proxy-path instances): constructor args flow through `super(...)`, overrides can call
  `super.Method()`, instances satisfy `instanceof` for both the subclass and the WinRT class,
  and subclass instances marshal as WinRT arguments (identity-cached, so the same JS object
  comes back out).
- **Composable (non-sealed) constructors** — the composition (null-outer) ABI is wired; note
  every activatable non-sealed WinRT class lives in `Windows.UI.Xaml`, whose activation
  requires a XAML-initialized thread. Headless those ctors surface the factory's
  `RPC_E_WRONG_THREAD` as a normal catchable JS error (identical to the classic runtime).
  `Windows.UI.Composition` object trees (non-sealed bases) work headless — the backend creates
  a `DispatcherQueue` for the JS thread at init.
- **`setImmediate`/`clearImmediate`** — provided by the standalone engine hosts' event loop
  (Node/Bun/Deno already have their own).
