use std::env;

use std::path::PathBuf;
use bindgen::EnumVariation;


fn main() {
    windows::build!(
        Windows::Win32::System::Console::WriteConsoleA,
        Windows::Win32::System::WindowsProgramming::STD_OUTPUT_HANDLE,
        Windows::Win32::System::WindowsProgramming::GetStdHandle,
        Windows::Win32::System::SystemServices::GetProcAddress,
        Windows::Win32::System::SystemServices::LoadLibraryA,
        Windows::Win32::System::SystemServices::FARPROC
    );

    let mut build = cc::Build::default() ;
    build.file("src/bindings.cpp")
        .cpp(true)
        .include("C:\\Program Files (x86)\\Windows Kits\\NETFXSDK\\4.8\\Include\\um")
        .static_crt(true)
        .compile("libcorebindings");

    println!("cargo:rerun-if-changed=src/bindings.cpp");



    let bindgen = bindgen::builder()
        .generate_comments(false)
        .default_enum_style(
            EnumVariation::Rust {
                non_exhaustive: false
            }
        )
        .size_t_is_usize(true)
        .use_core()
        .clang_arg("-std=c++17")
        .clang_args(&["-x", "c++"])
        .clang_arg("-v")
        .whitelist_function("Rometadataresolution_.*")
        .whitelist_function("IMetaDataImport2_.*")
        .whitelist_function("Enums_.*")
        .whitelist_function("Helpers_.*")
        .whitelist_var("Helpers_.*")
        .blacklist_type("HSTRING")
        .raw_line("pub type HSTRING = *mut windows::HSTRING;")
        .clang_args(&["-I", "C:\\Program Files (x86)\\Windows Kits\\NETFXSDK\\4.8\\Include\\um"])
        .header("src/bindings.cpp")
        // Tell cargo to invalidate the built crate whenever any of the
        // included header files changed.
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        // Finish the builder and generate the bindings.
        .generate()
        // Unwrap the Result and panic on failure.
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindgen
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}