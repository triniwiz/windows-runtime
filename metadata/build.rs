fn main() {
    cxx_build::bridge("src/lib.rs")
        .include("src")
        .include("C:\\Program Files (x86)\\Windows Kits\\NETFXSDK\\4.8\\Include\\um")
        .file("src/bindings.cpp")
        .flag_if_supported("-std=c++17")
        .compile("libbindings");

    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=src/bindings.cpp");
    println!("cargo:rerun-if-changed=src/bindings.h");
}