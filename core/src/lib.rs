pub mod runtime;
pub mod metadata;
pub mod bindings;
pub mod prelude;
pub mod enums;

#[no_mangle]
pub extern fn hello(){
    println!("Hello from rust")
}

