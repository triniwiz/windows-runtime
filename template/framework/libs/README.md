# Runtime DLLs

This directory contains the pre-built NativeScript Windows runtime DLLs.

Expected structure:

```
libs/
  x64/nativescript.dll          # Release build, 64-bit
  arm64/nativescript.dll        # Release build, ARM64
  devtools/
    x64/nativescript.dll        # Debug build with DevTools, 64-bit
    arm64/nativescript.dll      # Debug build with DevTools, ARM64
```

These are populated by the CI release pipeline (cargo build --release / --features devtools).
They are not included in the git repository — download a release package or build from source.
