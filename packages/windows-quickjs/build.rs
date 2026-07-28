// Compiles quickjs-ng (4 core C files) + the embedding shim into a static lib via the `cc`
// crate (finds MSVC cl.exe through the Rust toolchain even when not on PATH). Mirrors the
// quickjs-ng CMake's core `qjs` target: dtoa/libregexp/libunicode/quickjs, C11 + c11atomics.
use std::path::PathBuf;

fn main() {
    // napi-android's quickjs-ng fork (source_ng) — carries the extra APIs the node_api shim
    // needs (JS_WeakRef_Deref, JS_NewString16, JS_IsArrayBuffer2). Matched pair with the shim.
    let vendor = PathBuf::from("vendor/quickjs-ng-ns");
    let mut build = cc::Build::new();
    build
        .include(&vendor)
        .file(vendor.join("quickjs.c"))
        .file(vendor.join("libregexp.c"))
        .file(vendor.join("libunicode.c"))
        .file(vendor.join("dtoa.c"))
        .file(vendor.join("cutils.c")) // fork splits dbuf_*/utf8_*/js__* utilities out here
        .file("csrc/qjs_smoke.c")
        .define("_GNU_SOURCE", None)
        .warnings(false);

    if build.get_compiler().is_like_msvc() {
        // quickjs-ng requires C11 + MSVC's experimental C11 atomics; Win32 lean headers.
        build
            .flag("/std:c11")
            .flag("/experimental:c11atomics")
            .define("WIN32_LEAN_AND_MEAN", None)
            .define("_WIN32_WINNT", "0x0601")
            .define("_CRT_SECURE_NO_WARNINGS", None);
    } else {
        build.std("c11");
    }

    build.compile("quickjs_ng");
    println!("cargo:rerun-if-changed=csrc/qjs_smoke.c");
    println!("cargo:rerun-if-changed=vendor/quickjs-ng/quickjs.c");

    // Optional: the napi-android node_api shim over quickjs-ng (quickjs-api.c = the napi_*
    // provider, jsr.cpp = the C++ runtime helper). Gated behind the `napi_shim` feature.
    if std::env::var("CARGO_FEATURE_NAPI_SHIM").is_ok() {
        let napi = PathBuf::from("vendor/napi");
        let compat = PathBuf::from("vendor/compat"); // portable <sys/queue.h> for MSVC

        // C: the napi implementation. __QJS_NG__ selects the quickjs-ng code paths (and skips
        // the classic cutils.h include). USE_MIMALLOC left undefined → stdlib malloc.
        // The shim uses clang/GCC C extensions (VLAs, compound-literal initializers,
        // non-constant case labels) that MSVC `cl` rejects — compile it with clang-cl, which
        // is MSVC-ABI-compatible (links with the cl-built engine + Rust) but accepts them.
        let clang_cl = "C:/Program Files/LLVM/bin/clang-cl.exe";
        let use_clang = std::path::Path::new(clang_cl).exists();

        let mut api = cc::Build::new();
        if use_clang {
            api.compiler(clang_cl);
        }
        api.include(&vendor)
            .include(napi.join("common"))
            .include(napi.join("quickjs"))
            .include(&compat)
            .file(napi.join("quickjs/quickjs-api.c"))
            .define("__QJS_NG__", None)
            .define("_GNU_SOURCE", None)
            .warnings(false);
        if api.get_compiler().is_like_msvc() {
            api.flag("/std:c11")
                .flag("/experimental:c11atomics")
                .define("WIN32_LEAN_AND_MEAN", None)
                .define("_WIN32_WINNT", "0x0601")
                .define("_CRT_SECURE_NO_WARNINGS", None);
        } else {
            api.std("c11");
        }
        api.compile("qjs_napi_api");

        // C++: the JSR runtime helper. Compiled with MSVC `cl` (NOT clang-cl): it's portable
        // C++ with no clang-only extensions, and the installed LLVM is older than the VS STL's
        // required Clang version (STL1000), so clang-cl can't consume <map>/<mutex>/<tuple>.
        let mut jsr = cc::Build::new();
        jsr.cpp(true)
            .include(&vendor)
            .include(napi.join("common"))
            .include(napi.join("quickjs"))
            .include(&compat)
            .file(napi.join("quickjs/jsr.cpp"))
            .define("__QJS_NG__", None)
            .warnings(false);
        if jsr.get_compiler().is_like_msvc() {
            jsr.flag("/std:c++17")
                .define("WIN32_LEAN_AND_MEAN", None)
                .define("_WIN32_WINNT", "0x0601")
                .define("_CRT_SECURE_NO_WARNINGS", None);
        } else {
            jsr.std("c++17");
        }
        jsr.compile("qjs_napi_jsr");

        println!("cargo:rerun-if-changed=vendor/napi/quickjs/quickjs-api.c");
        println!("cargo:rerun-if-changed=vendor/napi/quickjs/jsr.cpp");
        println!("cargo:rerun-if-changed=vendor/napi/common/js_native_api_types.h");
    }
}
