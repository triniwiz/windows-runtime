use std::env;

use std::path::PathBuf;

fn main() {
   let mut build = cc::Build::default() ;
    build.file("src/bindings.cpp")
        .cpp(true)
        .include("C:\\Program Files (x86)\\Windows Kits\\NETFXSDK\\4.8\\Include\\um")
        .static_crt(true)
        .compile("libcorebindings");

    println!("cargo:rerun-if-changed=src/bindings.cpp");


/*
    let bindgen = bindgen::builder()
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
        .expect("Couldn't write bindings!");*/
}