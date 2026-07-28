//! Compiles the napi-android V8 shim (`v8-api.cpp`) + our Windows bring-up (`csrc/win_jsr.cpp`) +
//! `SimpleAllocator.cpp` against the `v8` crate's V8 14.7 headers. Our v8 crate uses the DEFAULT
//! config (no pointer compression, no sandbox), so there are NO ABI-matching defines — the shim
//! just compiles against v8.h. The rusty_v8 static archive (pulled in via the `runtime` dep) carries
//! the `v8::` symbols the shim calls, so the final link resolves them.
use std::path::{Path, PathBuf};

fn find_v8_include() -> PathBuf {
    // The v8 crate ships the full C++ headers in its registry source: .../v8-147.x/v8/include.
    let cargo_home = std::env::var("CARGO_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).unwrap();
            Path::new(&home).join(".cargo")
        });
    let src = cargo_home.join("registry").join("src");
    if let Ok(indexes) = std::fs::read_dir(&src) {
        for idx in indexes.flatten() {
            if let Ok(crates) = std::fs::read_dir(idx.path()) {
                let mut hits: Vec<PathBuf> = crates
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        p.file_name()
                            .and_then(|n| n.to_str())
                            .map(|n| n.starts_with("v8-147."))
                            .unwrap_or(false)
                            && p.join("v8/include/v8.h").exists()
                    })
                    .collect();
                hits.sort();
                if let Some(p) = hits.pop() {
                    return p.join("v8/include");
                }
            }
        }
    }
    panic!("could not locate the v8 crate's include dir (v8-147.x/v8/include) under {src:?}");
}

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let vendor = manifest.join("vendor");
    let v8_include = find_v8_include();

    let mut b = cc::Build::new();
    b.cpp(true)
        .include(&v8_include) // V8 14.7 C++ headers
        .include(vendor.join("shim")) // v8-api.h, v8-api-internals.h, SimpleAllocator.h
        .include(vendor.join("napi")) // napi headers
        .include(vendor.join("compat")) // <android/log.h> stub
        .file(vendor.join("shim/v8-api.cpp"))
        .file(vendor.join("shim/SimpleAllocator.cpp"))
        .file(manifest.join("csrc/win_jsr.cpp"))
        .define("NAPI_VERSION", "8")
        // [windows port] our V8 is 14.7 (>13): take the shim's modern code paths
        // (SetAccessorProperty instead of the removed Object::SetAccessor, etc.)
        .define("__V8_13__", None)
        .std("c++20") // V8 14.7 headers require C++20
        .warnings(false);
    if b.get_compiler().is_like_msvc() {
        b.flag("/EHsc")
            // MSVC reports __cplusplus=199711 even under /std:c++20 without this; v8config.h checks it.
            .flag("/Zc:__cplusplus")
            .define("NOMINMAX", None) // v8 uses std::min/max; avoid <windows.h> min/max macros
            .define("WIN32_LEAN_AND_MEAN", None)
            .define("_SILENCE_CXX17_CODECVT_HEADER_DEPRECATION_WARNING", None)
            .define("_CRT_SECURE_NO_WARNINGS", None);
    }
    b.compile("v8_napi_shim");

    println!("cargo:rerun-if-changed=csrc/win_jsr.cpp");
    println!("cargo:rerun-if-changed=vendor/shim/v8-api.cpp");
}
