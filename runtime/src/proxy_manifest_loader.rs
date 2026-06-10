//! SBG Manifest Loader
//!
//! Loads and integrates SBG-generated proxy manifests into the runtime
//! Enables the runtime to use pre-compiled proxy classes from the build phase

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Loaded SBG manifest data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadedManifest {
    /// Assembly path
    pub assembly_path: Option<String>,

    /// Proxy classes available
    pub proxy_classes: HashMap<String, ProxyClassInfo>,

    /// Manifest version
    pub version: String,
}

/// Proxy class information from manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyClassInfo {
    /// Class name
    pub name: String,

    /// Full JS-visible type name
    pub type_name: Option<String>,

    /// Whether the runtime synthesized the JS-visible type name
    pub is_auto_generated_name: bool,

    /// Full namespace path
    pub namespace: String,

    /// Base class
    pub base_class: Option<String>,

    /// Methods available
    pub methods: Vec<ProxyMethodInfo>,

    /// Properties available
    pub properties: Vec<ProxyPropertyInfo>,
}

/// Method information from manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyMethodInfo {
    pub name: String,
    pub return_type: String,
    pub param_count: usize,
}

/// Property information from manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyPropertyInfo {
    pub name: String,
    pub prop_type: String,
    pub readable: bool,
    pub writable: bool,
}

/// Loads and manages SBG manifests
pub struct SbgManifestLoader {
    manifests: Vec<LoadedManifest>,
}

impl SbgManifestLoader {
    /// Create a new loader
    pub fn new() -> Self {
        Self {
            manifests: Vec::new(),
        }
    }

    /// Load a manifest from a file
    pub fn load_manifest_file(&mut self, path: &Path) -> Result<()> {
        let content =
            fs::read_to_string(path).map_err(|e| anyhow!("Failed to read manifest file: {}", e))?;

        let raw_manifest: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| anyhow!("Failed to parse manifest JSON: {}", e))?;

        self.load_from_json_value(raw_manifest)?;
        Ok(())
    }

    /// Load a manifest from a JSON string
    pub fn load_manifest_json(&mut self, json: &str) -> Result<()> {
        let raw_manifest: serde_json::Value = serde_json::from_str(json)
            .map_err(|e| anyhow!("Failed to parse manifest JSON: {}", e))?;

        self.load_from_json_value(raw_manifest)?;
        Ok(())
    }

    /// Internal: Load from parsed JSON value
    fn load_from_json_value(&mut self, manifest: serde_json::Value) -> Result<()> {
        let mut classes = HashMap::new();

        // Extract proxy classes
        if let Some(proxy_classes) = manifest.get("proxy_classes").and_then(|c| c.as_array()) {
            for class_val in proxy_classes {
                if let Some(class_name) = class_val.get("name").and_then(|n| n.as_str()) {
                    let namespace = class_val
                        .get("namespace")
                        .and_then(|n| n.as_str())
                        .unwrap_or("NSWinRTProxies")
                        .to_string();

                    let base_class = class_val
                        .get("base_class")
                        .and_then(|b| b.as_str())
                        .map(|b| b.to_string());

                    let mut methods = Vec::new();
                    if let Some(methods_arr) = class_val.get("methods").and_then(|m| m.as_array()) {
                        for method_val in methods_arr {
                            if let Some(method_name) =
                                method_val.get("name").and_then(|n| n.as_str())
                            {
                                let return_type = method_val
                                    .get("return_type")
                                    .and_then(|t| t.as_str())
                                    .unwrap_or("void")
                                    .to_string();

                                let param_count = method_val
                                    .get("parameters")
                                    .and_then(|p| p.as_array())
                                    .map(|p| p.len())
                                    .unwrap_or(0);

                                methods.push(ProxyMethodInfo {
                                    name: method_name.to_string(),
                                    return_type,
                                    param_count,
                                });
                            }
                        }
                    }

                    let mut properties = Vec::new();
                    if let Some(props_arr) = class_val.get("properties").and_then(|p| p.as_array())
                    {
                        for prop_val in props_arr {
                            if let Some(prop_name) = prop_val.get("name").and_then(|n| n.as_str()) {
                                let prop_type = prop_val
                                    .get("prop_type")
                                    .and_then(|t| t.as_str())
                                    .unwrap_or("object")
                                    .to_string();

                                let readable = prop_val
                                    .get("readable")
                                    .and_then(|r| r.as_bool())
                                    .unwrap_or(true);

                                let writable = prop_val
                                    .get("writable")
                                    .and_then(|w| w.as_bool())
                                    .unwrap_or(true);

                                properties.push(ProxyPropertyInfo {
                                    name: prop_name.to_string(),
                                    prop_type,
                                    readable,
                                    writable,
                                });
                            }
                        }
                    }

                    classes.insert(
                        class_name.to_string(),
                        ProxyClassInfo {
                            name: class_name.to_string(),
                            type_name: class_val
                                .get("type_name")
                                .and_then(|t| t.as_str())
                                .map(|t| t.to_string()),
                            is_auto_generated_name: class_val
                                .get("is_auto_generated_name")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false),
                            namespace,
                            base_class,
                            methods,
                            properties,
                        },
                    );
                }
            }
        }

        let assembly_path = manifest
            .get("assembly_path")
            .and_then(|p| p.as_str())
            .map(|p| p.to_string());

        let version = manifest
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("1.0")
            .to_string();

        let loaded = LoadedManifest {
            assembly_path,
            proxy_classes: classes,
            version,
        };

        self.manifests.push(loaded);
        Ok(())
    }

    /// Get a proxy class by name
    pub fn get_proxy_class(&self, class_name: &str) -> Option<&ProxyClassInfo> {
        for manifest in &self.manifests {
            if let Some(class) = manifest.proxy_classes.get(class_name) {
                return Some(class);
            }
        }
        None
    }

    /// Get all proxy classes
    pub fn all_proxy_classes(&self) -> Vec<&ProxyClassInfo> {
        self.manifests
            .iter()
            .flat_map(|m| m.proxy_classes.values())
            .collect()
    }

    /// Get assembly path from first manifest
    pub fn get_assembly_path(&self) -> Option<&str> {
        self.manifests
            .first()
            .and_then(|m| m.assembly_path.as_deref())
    }

    /// Check if any manifests are loaded
    pub fn has_manifests(&self) -> bool {
        !self.manifests.is_empty()
    }
}

impl Default for SbgManifestLoader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_manifest_json() {
        let json = r#"{
            "version": "1.0",
            "assembly_path": "/path/to/proxies.dll",
            "proxy_classes": [
                {
                    "name": "TestClass",
                    "type_name": "com.example.TestClass",
                    "is_auto_generated_name": false,
                    "namespace": "NSWinRTProxies",
                    "base_class": null,
                    "methods": [
                        {
                            "name": "TestMethod",
                            "return_type": "void",
                            "parameters": []
                        }
                    ],
                    "properties": []
                }
            ]
        }"#;

        let mut loader = SbgManifestLoader::new();
        assert!(loader.load_manifest_json(json).is_ok());
        assert!(loader.has_manifests());
        assert!(loader.get_proxy_class("TestClass").is_some());
    }
}
