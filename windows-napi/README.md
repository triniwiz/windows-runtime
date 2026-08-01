# @nativescript/windows-napi

Node-API WinRT interop for Windows.

## How this relates to the other packages

- `@nativescript/windows` — the classic V8-based app runtime.
- `@nativescript/windows-<engine>` (`windows-v8`, `windows-quickjs`, `windows-hermes`, `windows-jsc`) — the Node-API app-runtime variants, each embedding a standalone engine.
- `@nativescript/windows-napi` (this package) — the same Node-API WinRT bindings as a native addon, for use inside an existing Node, Bun, or Deno process rather than as a standalone app runtime.

## Usage

```js
const winrt = require('@nativescript/windows-napi');
```

Requires Windows (x64 or arm64) and Node >= 18 (or a Node-API-compatible runtime such as Bun or Deno).

## Development

```sh
npm run build        # release build via @napi-rs/cli
npm run build:debug  # debug build
npm test             # run the test suite (run-tests.js)
```
