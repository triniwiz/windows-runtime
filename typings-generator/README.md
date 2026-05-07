# typings-generator

Experimental WinRT typings generator for the Windows runtime workspace.

## What it does

- Traverses metadata namespaces from a root (default: `Windows`)
- Emits a `.d.ts` file with initial declarations for:
  - classes
  - interfaces
  - enums
  - structs
  - delegates

## Usage

From workspace root:

- `cargo run -p typings-generator -- --root Windows --out windows-runtime.generated.d.ts`
- `cargo run -p typings-generator -- --roots Windows,Windows.Foundation --out windows-runtime.generated.d.ts`
- `cargo run -p typings-generator -- --input path\\to\\MyComponent.winmd --out windows-runtime.generated.d.ts`

Options:

- `--root <namespace>`: root namespace to traverse
- `--roots <ns1,ns2,...>`: comma-separated root namespaces
- `--out <path>`: output file path
- `--input <path>`: optional discovery source (`.dll`, `.winmd`, `.cs`, `.csproj`)

Output format:

- Declarations are grouped by namespace as `declare namespace ...` blocks.

Validation:

- `pwsh typings-generator\scripts\validate-projection.ps1`

## Notes

- This is an initial generator and not yet feature-complete.
- Some common WinRT generic interfaces now project to TypeScript generics, but many advanced WinRT metadata constructs still degrade to simplified types or `any`.
- Generic-heavy namespaces such as `Windows.Foundation.Collections` and parts of `Windows.Foundation` use metadata type-definition enumeration as a fallback when namespace traversal alone does not surface concrete types.
- Intended as a base package to evolve into full NativeScript runtime typings generation.
- If namespace traversal does not enumerate concrete types, the generator falls back to scanning WinMD payload strings (for example `Windows.winmd`) and validates candidates through `MetadataReader` before emitting declarations.
