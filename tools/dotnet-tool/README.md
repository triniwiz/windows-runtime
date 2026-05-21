dotnet-tool
============

Small Rust CLI to scan JS/TS sources for .NET usage and help publish/copy `DotNetBridge` artifacts into an app's output.

Quick start

Build (from repo root):

```powershell
cargo build -p dotnet-tool --release
```

Run (scan `app` directory by default):

```powershell
cargo run -p dotnet-tool -- --app-root . --dir app
```

Output is JSON similar to the Node scanner (`tools/scan_js_for_dotnet.js`).

Next steps

- Add `dotnet publish` + copy logic to publish and copy `dotnet-bridge/publish` into the app output when matches are found.
- Replace the simple regex-based scanner with `swc` parsing for more accurate detection.
- Integrate into the template's build hooks for Debug publishes.
