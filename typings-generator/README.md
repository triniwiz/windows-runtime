# typings-generator

WinRT and .NET typings generator for the Windows runtime workspace.

## What it does

- Traverses metadata namespaces from a root (default: `Windows`)
- Emits a `.d.ts` file with declarations for:
  - classes
  - interfaces
  - enums
  - structs
  - delegates

## WinRT typings

Generate TypeScript declarations from WinRT metadata (`.winmd`):

```powershell
# All Windows APIs (Windows.*)
cargo run -p typings-generator -- --root Windows --out windows.d.ts

# Specific namespaces
cargo run -p typings-generator -- --roots Windows.Foundation,Windows.UI --out windows.d.ts

# From a custom WinMD or DLL
cargo run -p typings-generator -- --input path\to\MyComponent.winmd --out my-component.d.ts
```

Options:

| Flag | Description |
|------|-------------|
| `--root <namespace>` | Single root namespace to traverse |
| `--roots <ns1,ns2,...>` | Comma-separated root namespaces |
| `--out <path>` | Output `.d.ts` file path (single file) |
| `--out-dir <path>` | Output directory — one `.d.ts` per top-level namespace |
| `--input <path>` | Discovery source (`.dll`, `.winmd`, `.cs`, `.csproj`) |

Split output writes one file per second-level namespace group, e.g. `Windows.Foundation.d.ts`, `Windows.Graphics.d.ts`, etc.

## .NET BCL typings

Generate TypeScript declarations directly from any .NET assembly or WinMD:

```powershell
cargo run -p typings-generator -- --input path\to\MyLibrary.dll --out my-library.d.ts
```

No bridge process required — the generator reads assembly metadata directly.

## Validation (WinRT only)

```powershell
pwsh typings-generator\scripts\validate-projection.ps1
```

## Notes

- Some WinRT generic interfaces project to TypeScript generics; advanced
  constructs may degrade to `any`.
- Generic-heavy namespaces (`Windows.Foundation.Collections`) fall back to
  WinMD payload scanning when namespace traversal alone is insufficient.
