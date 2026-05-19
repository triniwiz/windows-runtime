# NativeScript Windows Runtime

Windows support for NativeScript, with the same goal as the iOS and Android runtimes:
first-class access to Windows Runtime APIs and .NET APIs from JavaScript.

The workspace is split into the main runtime pieces:

- `metadata` - WinRT/.NET metadata reading and declaration modeling.
- `metadata-generator` - reads `.winmd`, `.dll`, `.exe`, `.nupkg`, or directories and emits runtime metadata bundles.
- `runtime` - V8-based JavaScript runtime and WinRT interop.
- `runtime-binding-gen` - runtime-phase binding metadata support.
- `nativescript` - native library entry point used by template/demo apps.
- `dotnet-bridge` - .NET/C# dispatch bridge.
- `sbg` - static binding generator, intended to move toward the NativeScript Android static binding generator model.
- `devtools` / `runtime-devtools` - debugging and developer tooling.

## Prerequisites

Install these before building the full workspace:

- Windows 10 or Windows 11.
- Visual Studio 2022 or Build Tools for Visual Studio 2022 with:
  - Desktop development with C++.
  - MSVC v143 toolset.
  - Windows 10/11 SDK.
  - C++ CMake tools are useful for native dependency troubleshooting.
- Rust stable via `rustup`.
- Rust MSVC targets used by the native library build:
  - `rustup target add x86_64-pc-windows-msvc`
  - `rustup target add i686-pc-windows-msvc`
  - `rustup target add aarch64-pc-windows-msvc`
- .NET SDK, currently needed for the C# bridge, tests, demo apps, and template projects.
- PowerShell 7+ (`pwsh`) for the build scripts.
- Node.js/npm when working on NativeScript app templates, package scripts, or JavaScript demo apps.

Open a Visual Studio Developer PowerShell when building native libraries so `cl.exe`
and the Windows SDK tools are on `PATH`.

The Rust `metadata` crate no longer builds the old `bindings.cpp` shim, so it does
not require the hardcoded `.NET Framework NETFXSDK` C++ include path just to compile.
The full repo still needs .NET and Visual Studio tooling for the bridge, app hosts,
native `nativescript.dll`, and tests.

## Build

Build the main workspace crates:

```powershell
cargo build --workspace
```

Build native `nativescript.dll` libraries and the runtime pipeline:

```powershell
.\build.ps1 -Profile debug -NativeArch x64
```

Build all native targets:

```powershell
.\build.ps1 -Profile release -NativeArch all
```

Run the SBG phase when you want generated static binding output:

```powershell
.\build.ps1 -RunSBG
```

## Tests

Run Rust tests:

```powershell
cargo test --workspace
```

Run .NET bridge tests:

```powershell
dotnet test dotnet-bridge-tests\DotNetBridgeTests.csproj
```

## DevTools

DevTools are part of the runtime story, not an optional afterthought. Use the
`release-with-devtools` profile when you need debugger/devtools symbols and runtime
tooling enabled:

```powershell
.\build.ps1 -Profile release-with-devtools -NativeArch x64
```
