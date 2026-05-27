//! Proxy manifest for SBG output
//!
//! Creates a manifest that the runtime can use to load SBG-generated proxy classes
//! This manifest is consumed at app startup to wire up the pre-compiled proxies

use crate::metadata_reader::ExtensionMetadata;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Manifest for SBG-generated proxy assembly
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProxyManifest {
    /// Version of the manifest format
    pub version: String,

    /// Path to the compiled proxy assembly
    pub assembly_path: Option<String>,

    /// List of proxy classes available in the assembly
    pub proxy_classes: Vec<ProxyClass>,

    /// Timestamp of generation
    pub generated_at: String,
}

/// Information about a proxy class
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyClass {
    /// Class name
    pub name: String,

    /// Full JS-visible type name (if provided by the extension metadata)
    pub type_name: Option<String>,

    /// Whether the JS-visible type name was synthesized automatically
    pub is_auto_generated_name: bool,

    /// Base class (if any)
    pub base_class: Option<String>,

    /// Fully qualified namespace
    pub namespace: String,

    /// Available methods
    pub methods: Vec<ProxyMethod>,

    /// Available properties
    pub properties: Vec<ProxyProperty>,

    /// Interfaces implemented
    pub interfaces: Vec<String>,
}

/// Method information in proxy class
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyMethod {
    /// Method name
    pub name: String,

    /// Return type
    pub return_type: String,

    /// Parameter list
    pub parameters: Vec<(String, String)>,
}

/// Property information in proxy class
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyProperty {
    /// Property name
    pub name: String,

    /// Property type
    pub prop_type: String,

    /// Can be read
    pub readable: bool,

    /// Can be written
    pub writable: bool,
}

impl ProxyManifest {
    /// Create a manifest from extension metadata and assembly path
    pub fn from_extensions(
        extensions: Vec<ExtensionMetadata>,
        assembly_path: &Path,
    ) -> Result<Self> {
        let stored_assembly_path = assembly_path
            .ancestors()
            .nth(4)
            .and_then(|generation_root| assembly_path.strip_prefix(generation_root).ok())
            .map(|relative| relative.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|| assembly_path.to_string_lossy().replace('\\', "/"));

        let proxy_classes = extensions
            .into_iter()
            .map(|ext| ProxyClass {
                name: ext.class_name.clone(),
                type_name: ext.type_name.clone(),
                is_auto_generated_name: ext.is_auto_generated_name,
                base_class: ext.base_class,
                namespace: ext
                    .namespace
                    .unwrap_or_else(|| "NSWinRTProxies".to_string()),
                methods: ext
                    .methods
                    .into_iter()
                    .map(|m| ProxyMethod {
                        name: m.name,
                        return_type: m.return_type,
                        parameters: m.parameters,
                    })
                    .collect(),
                properties: ext
                    .properties
                    .into_iter()
                    .map(|p| ProxyProperty {
                        name: p.name,
                        prop_type: p.prop_type,
                        readable: p.is_readable,
                        writable: p.is_writable,
                    })
                    .collect(),
                interfaces: ext.interfaces,
            })
            .collect();

        Ok(Self {
            version: "1.0".to_string(),
            assembly_path: Some(stored_assembly_path),
            proxy_classes,
            generated_at: chrono::Utc::now().to_rfc3339(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_serialization() {
        let manifest = ProxyManifest {
            version: "1.0".to_string(),
            assembly_path: Some("/path/to/proxies.dll".to_string()),
            proxy_classes: vec![ProxyClass {
                name: "TestClass".to_string(),
                type_name: Some("com.example.TestClass".to_string()),
                is_auto_generated_name: false,
                base_class: None,
                namespace: "NSWinRTProxies".to_string(),
                methods: vec![],
                properties: vec![],
                interfaces: vec![],
            }],
            generated_at: "2026-05-07T00:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&manifest).unwrap();
        let _deserialized: ProxyManifest = serde_json::from_str(&json).unwrap();
    }
}
