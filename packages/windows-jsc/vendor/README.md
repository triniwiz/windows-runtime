# Vendored JavaScriptCore engine (prebuilt)

Committed prebuilt binaries (no git-LFS), same model as `windows-hermes/vendor`.

## Contents (`x64/`)
| File | ~Size | Purpose |
|---|---|---|
| `JavaScriptCore.dll` | 33 MB | JSC engine (exports the public C API) |
| `icudt77.dll` | 32 MB | ICU data (JSC dep) |
| `icuin77.dll` / `icuuc77.dll` | 3 / 1.8 MB | ICU i18n / common (JSC deps) |
| `JavaScriptCore.lib` | ~45 KB | Import lib generated from the DLL's exports |
| `JavaScriptCore.def` | — | Export list used to generate the .lib |

`include/JavaScriptCore/*.h` = the public JSC C API headers (from napi-android). `shim/` = the
napi-over-JSC provider (napi-android `jsc-api.cpp` + `jsr.cpp`), with Windows fixes noted in the
package README.

## Provenance / refresh
Source: **Playwright's `webkit-win64` build** (Playwright rebuilds WebKit continuously; the official
WebKit WinCairo buildbot has been dead since Sept 2024).

```sh
REV=$(curl -s https://raw.githubusercontent.com/microsoft/playwright/main/packages/playwright-core/browsers.json \
  | tr ',' '\n' | grep -A2 '"name": *"webkit"' | grep revision | grep -oE '[0-9]+')
curl -sL -o wk.zip "https://playwright.download.prss.microsoft.com/dbazure/download/playwright/builds/webkit/$REV/webkit-win64.zip"
unzip -o wk.zip JavaScriptCore.dll icuin77.dll icuuc77.dll icudt77.dll -d x64/
# regenerate the import lib (no .lib ships in the zip):
llvm-objdump -p x64/JavaScriptCore.dll | awk '/Export Table/{f=1;next} f&&/0x/{print $NF}' \
  | grep -E '^(JS|WK|k)' | awk '{print ($0 ~ /^k/) ? $0" DATA" : $0}' > exp
printf 'EXPORTS\n' > x64/JavaScriptCore.def; cat exp >> x64/JavaScriptCore.def
lib /def:x64/JavaScriptCore.def /machine:x64 /out:x64/JavaScriptCore.lib   # MSVC lib.exe
```
`k*` exports (e.g. `kJSClassDefinitionEmpty`) MUST be marked `DATA` in the .def or linking fails with
`__imp_kJSClassDefinitionEmpty` unresolved.

## Size note
~70 MB committed (dominated by `JavaScriptCore.dll` + `icudt77.dll`). WebKit is BSD/LGPL (open source);
redistribution is fine. If repo size matters, swap to a download-on-build step using the recipe above.
