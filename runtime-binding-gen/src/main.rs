//! Runtime Binding Generator - Standalone tool
//!
//! Can be invoked at development time to capture extension metadata
//! and generate JS dispatch tables

use runtime_binding_gen::RuntimeExtensionRegistry;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage(&args[0]);
        return;
    }

    match args[1].as_str() {
        "list" => {
            // List registered extensions from a manifest file
            if args.len() < 3 {
                eprintln!("Usage: runtime-binding-gen list <manifest.json>");
                return;
            }
            list_extensions(&args[2]);
        }
        "generate" => {
            // Generate dispatch tables from extensions
            println!("[Runtime Binding Generator] Ready for runtime-phase binding");
            println!("This tool is designed to be embedded in the runtime library");
        }
        _ => {
            eprintln!("Unknown command: {}", args[1]);
            print_usage(&args[0]);
        }
    }
}

fn print_usage(prog_name: &str) {
    eprintln!("Usage:");
    eprintln!(
        "  {} list <manifest.json>    - List extensions from manifest",
        prog_name
    );
    eprintln!(
        "  {} generate                 - Generate dispatch metadata",
        prog_name
    );
}

fn list_extensions(manifest_path: &str) {
    match std::fs::read_to_string(manifest_path) {
        Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
            Ok(manifest) => {
                println!("[Runtime Binding Generator] Extensions in manifest:");
                if let Some(classes) = manifest.get("proxy_classes").and_then(|c| c.as_array()) {
                    for class in classes {
                        if let Some(name) = class.get("name").and_then(|n| n.as_str()) {
                            println!("  - {}", name);
                        }
                    }
                }
            }
            Err(e) => eprintln!("Failed to parse manifest: {}", e),
        },
        Err(e) => eprintln!("Failed to read manifest: {}", e),
    }
}
