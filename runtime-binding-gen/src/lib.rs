//! Runtime Binding Generator Library
//!
//! Provides runtime-phase metadata capture and dynamic JS dispatch registration
//! This runs at app startup and captures any extensions defined dynamically,
//! falling back to SBG-generated proxies when available

use ahash::AHashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeParameterMetadata {
    /// Parameter name
    pub name: String,

    /// Parameter type (string representation)
    #[serde(alias = "type")]
    pub type_name: String,
}

/// Extension metadata captured at runtime
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeExtensionMetadata {
    /// Full JS-visible type name
    #[serde(default)]
    pub type_name: Option<String>,

    /// Class name
    #[serde(default)]
    pub class_name: String,

    /// Namespace containing the class
    #[serde(default)]
    pub namespace: Option<String>,
    
    /// Base class name
    #[serde(default)]
    pub base_class: Option<String>,
    
    /// Methods defined on the extension
    #[serde(default)]
    pub methods: Vec<RuntimeMethodMetadata>,
    
    /// Properties defined on the extension
    #[serde(default)]
    pub properties: Vec<RuntimePropertyMetadata>,
    
    /// Interfaces implemented
    #[serde(default)]
    pub interfaces: Vec<String>,

    /// Whether the JS-visible type name was synthesized automatically
    #[serde(default)]
    pub is_auto_generated_name: bool,
    
    /// When this extension was registered
    #[serde(default)]
    pub registered_at: Option<String>,
}

/// Method metadata captured at runtime
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeMethodMetadata {
    /// Method name
    pub name: String,
    
    /// Return type (string representation)
    pub return_type: String,
    
    /// Parameter names and types
    #[serde(default)]
    pub parameters: Vec<RuntimeParameterMetadata>,
}

/// Property metadata captured at runtime
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimePropertyMetadata {
    /// Property name
    pub name: String,
    
    /// Property type (string representation)
    pub prop_type: String,
    
    /// Whether property is readable
    pub readable: bool,
    
    /// Whether property is writable
    pub writable: bool,
}

/// Registry for runtime-captured extensions
pub struct RuntimeExtensionRegistry {
    /// Map of class name -> metadata
    extensions: AHashMap<String, RuntimeExtensionMetadata>,
}

impl RuntimeExtensionRegistry {
    /// Create a new registry
    pub fn new() -> Self {
        Self {
            extensions: AHashMap::new(),
        }
    }

    /// Register an extension at runtime
    pub fn register(&mut self, metadata: RuntimeExtensionMetadata) {
        self.extensions.insert(metadata.class_name.clone(), metadata);
    }

    /// Get metadata for an extension
    pub fn get(&self, class_name: &str) -> Option<&RuntimeExtensionMetadata> {
        self.extensions.get(class_name)
    }

    /// Get all registered extensions
    pub fn all(&self) -> impl Iterator<Item = &RuntimeExtensionMetadata> {
        self.extensions.values()
    }

    /// Get count of registered extensions
    pub fn count(&self) -> usize {
        self.extensions.len()
    }

    /// Export registry as JSON (for debugging/logging)
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(&self.extensions)
    }
}

impl Default for RuntimeExtensionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// JS dispatch information for a method
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodDispatchInfo {
    /// Method name
    pub name: String,
    
    /// Unique dispatch ID
    pub dispatch_id: u32,
    
    /// Return type info
    pub return_type: String,
    
    /// Parameter count
    pub param_count: usize,
}

/// Builder for JS dispatch metadata
pub struct DispatchMetadataBuilder {
    next_id: u32,
    methods: Vec<MethodDispatchInfo>,
}

impl DispatchMetadataBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            next_id: 1,
            methods: Vec::new(),
        }
    }

    /// Add a method to dispatch table; returns the assigned dispatch ID.
    pub fn add_method(
        &mut self,
        name: String,
        return_type: String,
        param_count: usize,
    ) -> u32 {
        let id = self.next_id;
        self.methods.push(MethodDispatchInfo { name, dispatch_id: id, return_type, param_count });
        self.next_id += 1;
        id
    }

    /// Get all dispatch info
    pub fn build(self) -> Vec<MethodDispatchInfo> {
        self.methods
    }
}

impl Default for DispatchMetadataBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_operations() {
        let mut registry = RuntimeExtensionRegistry::new();
        assert_eq!(registry.count(), 0);

        let metadata = RuntimeExtensionMetadata {
            type_name: Some("Test.Namespace.TestClass".to_string()),
            class_name: "TestClass".to_string(),
            namespace: Some("Test.Namespace".to_string()),
            base_class: None,
            methods: vec![],
            properties: vec![],
            interfaces: vec![],
            is_auto_generated_name: false,
            registered_at: Some("2026-05-07T00:00:00Z".to_string()),
        };

        registry.register(metadata);
        assert_eq!(registry.count(), 1);
        assert!(registry.get("TestClass").is_some());
    }

    #[test]
    fn test_dispatch_builder() {
        let mut builder = DispatchMetadataBuilder::new();
        let id1 = builder.add_method("method1".to_string(), "void".to_string(), 0);
        let id2 = builder.add_method("method2".to_string(), "int".to_string(), 2);

        assert_eq!(id1, 1);
        assert_eq!(id2, 2);

        let methods = builder.build();
        assert_eq!(methods.len(), 2);
        assert_eq!(methods[0].dispatch_id, 1);
        assert_eq!(methods[1].dispatch_id, 2);
    }
}
