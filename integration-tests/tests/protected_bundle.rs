//! Proves the sealed `app.nsbundle` container (`runtime::source_protect`) actually serves as the
//! JS source for a real `Runtime` run — not just round-tripping in isolation (see
//! `source_protect`'s own unit tests for that). The plaintext staging directory is deleted right
//! after packing, before `Runtime::new` ever runs, so there is no possible filesystem fallback:
//! if the ESM import below resolves at all, it can only have come from the decrypted in-memory
//! table.

use runtime::source_protect;
use runtime::Runtime;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn scratch_dir(name: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "nsbundle_integration_{name}_{}_{n}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn esm_import_resolves_from_sealed_bundle_with_no_plaintext_on_disk() {
    // Stage the two-file ESM "app", pack it, then destroy the plaintext — everything downstream
    // must come from the decrypted table or this test fails.
    let staging = scratch_dir("stage");
    fs::write(
        staging.join("entry.mjs"),
        b"import { value } from './dep.mjs';\n\
          if (value !== 42) { throw new Error('wrong value: ' + value); }\n",
    )
    .unwrap();
    fs::write(staging.join("dep.mjs"), b"export const value = 42;\n").unwrap();

    let app_root = scratch_dir("approot");
    let bundle_path = app_root.join("app.nsbundle");
    source_protect::pack_directory(
        &staging,
        &bundle_path,
        source_protect::KEY_MODE_DEFAULT,
        source_protect::default_key(),
    )
    .unwrap();

    fs::remove_dir_all(&staging).unwrap();
    assert!(!staging.exists(), "plaintext staging dir must be gone");

    // Runtime::new -> Runtime::source_protect::init_from_app_root locates app_root/app.nsbundle
    // and decrypts it into the in-memory table before anything else runs.
    let mut rt = Runtime::new(app_root.to_str().unwrap());
    assert!(
        source_protect::has_bundle(),
        "app.nsbundle should have been found and loaded from {}",
        app_root.display()
    );

    // Fetch the entry's own source from the protected table too (mirrors what a real host does
    // via runtime_read_protected_file instead of File.ReadAllText).
    let entry_source =
        source_protect::read_text("entry.mjs").expect("entry.mjs should be served from the bundle");

    rt.run_script(&entry_source, "entry.mjs");

    assert_eq!(
        runtime::get_last_js_error(),
        None,
        "ESM import of the bundle-only dep.mjs should have resolved cleanly"
    );
}
