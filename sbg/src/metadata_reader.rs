//! Metadata reader for SBG
//!
//! Reads extension metadata from various sources:
//! - Extension JSON files
//! - WinRT metadata files
//! - Runtime captured extensions

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionMetadata {
    #[serde(default)]
    pub type_name: Option<String>,
    pub class_name: String,
    #[serde(default)]
    pub namespace: Option<String>,
    pub base_class: Option<String>,
    pub methods: Vec<MethodMetadata>,
    pub properties: Vec<PropertyMetadata>,
    pub interfaces: Vec<String>,
    #[serde(default)]
    pub is_auto_generated_name: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodMetadata {
    pub name: String,
    pub return_type: String,
    pub parameters: Vec<(String, String)>, // (name, type)
    #[serde(default = "default_method_modifier")]
    pub modifier: String,
}

fn default_method_modifier() -> String {
    "public override".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyMetadata {
    pub name: String,
    pub prop_type: String,
    pub is_readable: bool,
    pub is_writable: bool,
}

/// Metadata reader for SBG pipeline
pub struct MetadataReader {
    source: std::path::PathBuf,
}

impl MetadataReader {
    /// Create a new metadata reader
    pub fn new(source: &Path) -> Self {
        Self {
            source: source.to_path_buf(),
        }
    }

    /// Read extension metadata from source
    pub fn read(&self) -> Result<Vec<ExtensionMetadata>> {
        // Try to read from JSON file first (extension metadata export)
        if self.source.extension().map_or(false, |e| e == "json") {
            return self.read_json_metadata();
        }

        // Try to read from directory (multiple extension files)
        if self.source.is_dir() {
            return self.read_from_directory();
        }

        // Default: empty metadata (no extensions defined yet)
        Ok(Vec::new())
    }

    /// Read from a JSON metadata file
    fn read_json_metadata(&self) -> Result<Vec<ExtensionMetadata>> {
        let content = fs::read_to_string(&self.source)
            .map_err(|e| anyhow!("Failed to read metadata file: {}", e))?;

        parse_metadata_content(&content)
    }

    /// Read all JSON files from directory
    fn read_from_directory(&self) -> Result<Vec<ExtensionMetadata>> {
        let mut all_metadata = Vec::new();

        if !self.source.exists() {
            return Ok(Vec::new());
        }

        for entry in
            fs::read_dir(&self.source).map_err(|e| anyhow!("Failed to read directory: {}", e))?
        {
            let entry = entry?;
            let path = entry.path();

            if path.extension().map_or(false, |e| e == "json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(metadata) = parse_metadata_content(&content) {
                        all_metadata.extend(metadata);
                    }
                }
            }
        }

        Ok(all_metadata)
    }
}

fn parse_metadata_content(content: &str) -> Result<Vec<ExtensionMetadata>> {
    if let Ok(metadata) = serde_json::from_str::<Vec<ExtensionMetadata>>(content) {
        return Ok(metadata);
    }

    // Runtime auto-capture format emitted by NSWinRT.extend pipeline.
    // Example element:
    // {
    //   "typeName": "com.example.CustomButton",
    //   "baseType": "Button",
    //   "methods": ["OnClick", "init"],
    //   "properties": ["state"],
    //   "interfaces": []
    // }
    let value: Value = serde_json::from_str(content)
        .map_err(|e| anyhow!("Failed to parse metadata JSON: {}", e))?;

    let Some(items) = value.as_array() else {
        return Err(anyhow!("Metadata JSON must be an array"));
    };

    let mut converted = Vec::with_capacity(items.len());
    for item in items {
        if let Some(explicit) = parse_explicit_entry(item) {
            converted.push(explicit);
            continue;
        }

        if let Some(runtime) = parse_runtime_entry(item) {
            converted.push(runtime);
        }
    }

    Ok(converted)
}

fn parse_explicit_entry(item: &Value) -> Option<ExtensionMetadata> {
    serde_json::from_value(item.clone()).ok()
}

fn parse_runtime_entry(item: &Value) -> Option<ExtensionMetadata> {
    let obj = item.as_object()?;
    let type_name = obj.get("typeName")?.as_str()?.to_string();

    let namespace = type_name
        .rfind('.')
        .map(|index| type_name[..index].to_string());

    let class_name = type_name
        .split('.')
        .next_back()
        .unwrap_or(type_name.as_str())
        .to_string();

    let base_class = obj
        .get("baseType")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let methods = obj
        .get("methods")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(parse_runtime_method)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let properties = obj
        .get("properties")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(parse_runtime_property)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let interfaces = obj
        .get("interfaces")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|entry| entry.as_str())
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Some(ExtensionMetadata {
        type_name: Some(type_name),
        class_name,
        namespace,
        base_class,
        methods,
        properties,
        interfaces,
        is_auto_generated_name: obj
            .get("isAutoGeneratedName")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    })
}

fn parse_runtime_method(entry: &Value) -> Option<MethodMetadata> {
    if let Some(name) = entry.as_str() {
        return Some(MethodMetadata {
            name: name.to_string(),
            return_type: "void".to_string(),
            parameters: Vec::new(),
            modifier: default_method_modifier(),
        });
    }

    let obj = entry.as_object()?;
    let name = obj.get("name")?.as_str()?.to_string();
    let return_type = obj
        .get("returnType")
        .or_else(|| obj.get("return_type"))
        .and_then(|value| value.as_str())
        .unwrap_or("void")
        .to_string();
    let parameters = obj
        .get("parameters")
        .and_then(|value| value.as_array())
        .map(|entries| entries.iter().filter_map(parse_runtime_parameter).collect())
        .unwrap_or_default();
    let modifier = obj
        .get("modifier")
        .and_then(|value| value.as_str())
        .unwrap_or("public override")
        .to_string();

    Some(MethodMetadata {
        name,
        return_type,
        parameters,
        modifier,
    })
}

fn parse_runtime_parameter(entry: &Value) -> Option<(String, String)> {
    if let Some(array) = entry.as_array() {
        let name = array.get(0)?.as_str()?.to_string();
        let type_name = array.get(1)?.as_str()?.to_string();
        return Some((name, type_name));
    }

    let obj = entry.as_object()?;
    let name = obj.get("name")?.as_str()?.to_string();
    let type_name = obj
        .get("type")
        .or_else(|| obj.get("typeName"))
        .or_else(|| obj.get("type_name"))
        .and_then(|value| value.as_str())
        .unwrap_or("object")
        .to_string();
    Some((name, type_name))
}

fn parse_runtime_property(entry: &Value) -> Option<PropertyMetadata> {
    if let Some(name) = entry.as_str() {
        return Some(PropertyMetadata {
            name: name.to_string(),
            prop_type: "object".to_string(),
            is_readable: true,
            is_writable: true,
        });
    }

    let obj = entry.as_object()?;
    let name = obj.get("name")?.as_str()?.to_string();
    let prop_type = obj
        .get("propType")
        .or_else(|| obj.get("prop_type"))
        .and_then(|value| value.as_str())
        .unwrap_or("object")
        .to_string();
    let is_readable = obj
        .get("readable")
        .or_else(|| obj.get("isReadable"))
        .or_else(|| obj.get("is_readable"))
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    let is_writable = obj
        .get("writable")
        .or_else(|| obj.get("isWritable"))
        .or_else(|| obj.get("is_writable"))
        .and_then(|value| value.as_bool())
        .unwrap_or(true);

    Some(PropertyMetadata {
        name,
        prop_type,
        is_readable,
        is_writable,
    })
}
