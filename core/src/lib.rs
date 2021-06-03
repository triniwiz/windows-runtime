pub mod runtime;
pub mod metadata;
pub mod bindings;

#[no_mangle]
pub extern fn hello(){
    println!("Hello from rust")
}

