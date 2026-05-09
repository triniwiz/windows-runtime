//! Static Binding Generator (SBG)
//!
//! Pre-build phase tool that:
//! 1. Captures WinRT extension metadata
//! 2. Generates C# proxy classes
//! 3. Compiles them into an assembly
//! 4. Outputs a manifest for runtime linking
//!
//! Unlike the runtime binding generator, SBG outputs are compiled BEFORE
//! the app is finalized, ensuring all proxy classes are available at link time.

use anyhow::{anyhow, Result};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

mod generator;
mod manifest;
mod metadata_reader;

use generator::Generator;
use manifest::ProxyManifest;
use metadata_reader::MetadataReader;

/// Configuration for SBG execution
pub struct SbgConfig {
    /// Input metadata source (path to extension metadata JSON or WinRT files)
    pub metadata_source: PathBuf,
    
    /// Output directory for generated C# files
    pub output_dir: PathBuf,
    
    /// Path to dotnet executable
    pub dotnet_path: String,
    
    /// Target framework for generated C# (e.g., "net8.0-windows")
    pub target_framework: String,

    /// Optional list of directories with developer-authored C# sources that should be
    /// compiled into the generated proxy assembly.
    pub app_cs_sources_dirs: Vec<PathBuf>,
}

impl Default for SbgConfig {
    fn default() -> Self {
        Self {
            metadata_source: PathBuf::from("./sbg_output/sbg_metadata.json"),
            output_dir: PathBuf::from("./obj/_ns_/gen"),
            dotnet_path: "dotnet".to_string(),
            target_framework: "net8.0-windows10.0.19041.0".to_string(),
            app_cs_sources_dirs: Vec::new(),
        }
    }
}

/// Main SBG processor
pub struct StaticBindingGenerator {
    config: SbgConfig,
}

impl StaticBindingGenerator {
    /// Create a new SBG instance
    pub fn new(config: SbgConfig) -> Self {
        Self { config }
    }

    /// Run the full SBG pipeline
    pub fn generate(&self) -> Result<ProxyManifest> {
        println!("[SBG] Static Binding Generator - Pre-build Phase");
        println!("[SBG] Reading metadata from: {}", self.config.metadata_source.display());

        // Phase 1: Read metadata
        let metadata_reader = MetadataReader::new(&self.config.metadata_source);
        let extensions_metadata = metadata_reader.read()?;
        println!("[SBG] Phase 1: Metadata captured - {} extensions found", extensions_metadata.len());

        if extensions_metadata.is_empty() {
            println!("[SBG] No extensions to generate, skipping C# compilation");
            return Ok(ProxyManifest::default());
        }

        // Phase 2: Generate C# proxy code
        fs::create_dir_all(&self.config.output_dir)
            .map_err(|e| anyhow!("Failed to create output directory: {}", e))?;

        let generator = Generator::new(
            &self.config.output_dir,
            &self.config.target_framework,
            self.config.app_cs_sources_dirs.clone(),
        );
        let (project_path, app_sources_count) = generator.generate(&extensions_metadata)?;
        println!("[SBG] Phase 2: C# proxy code generated at: {}", project_path.display());
        if app_sources_count > 0 {
            println!("[SBG] Included {app_sources_count} app C# source file(s)");
        }

        // Phase 3: Compile C# project
        let assembly_path = self.compile_csharp(&project_path)?;
        println!("[SBG] Phase 3: C# compilation complete - Assembly: {}", assembly_path.display());

        // Phase 4: Create manifest for runtime linking
        let manifest = ProxyManifest::from_extensions(extensions_metadata, &assembly_path)?;
        println!("[SBG] Phase 4: Manifest created - {} proxy classes", manifest.proxy_classes.len());

        // Write manifest as JSON for runtime consumption
        let manifest_path = self.config.output_dir.join("sbg-manifest.json");
        let manifest_json = serde_json::to_string_pretty(&manifest)?;
        fs::write(&manifest_path, manifest_json)?;
        println!("[SBG] Manifest written to: {}", manifest_path.display());

        println!("[SBG] ✓ Pre-build phase complete");
        Ok(manifest)
    }

    /// Compile C# project using dotnet
    fn compile_csharp(&self, project_path: &Path) -> Result<PathBuf> {
        println!("[SBG] Compiling C# project...");

        let output = Command::new(&self.config.dotnet_path)
            .arg("build")
            .arg(project_path)
            .arg("-c")
            .arg("Release")
            .output()
            .map_err(|e| anyhow!("Failed to run dotnet: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("C# compilation failed:\n{}", stderr));
        }

        // Determine assembly output path (convention: bin/Release/<target framework>/ProjectName.dll)
        let assembly_path = project_path
            .parent()
            .ok_or_else(|| anyhow!("Invalid project path"))?
            .join("bin")
            .join("Release")
            .join(&self.config.target_framework)
            .join("NSWinRTProxies.dll");

        if !assembly_path.exists() {
            return Err(anyhow!(
                "Assembly not found at expected location: {}",
                assembly_path.display()
            ));
        }

        Ok(assembly_path)
    }
}

fn parse_compile_include_value(line: &str, attribute: &str) -> Option<String> {
    let marker = format!("{}=\"", attribute);
    let start = line.find(marker.as_str())? + marker.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn include_to_source_dir(csproj_dir: &Path, include: &str) -> Option<PathBuf> {
    let normalized = include.replace('\\', "/");
    let wildcard_index = normalized.find('*');
    let candidate_dir = if let Some(index) = wildcard_index {
        let prefix = normalized[..index].trim_end_matches('/').to_string();
        if prefix.is_empty() {
            csproj_dir.to_path_buf()
        } else {
            csproj_dir.join(prefix)
        }
    } else {
        let include_path = PathBuf::from(normalized);
        if include_path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("cs")) {
            csproj_dir.join(include_path.parent().unwrap_or_else(|| Path::new("")))
        } else {
            csproj_dir.join(include_path)
        }
    };

    let canonical = candidate_dir.canonicalize().unwrap_or(candidate_dir);
    if canonical.exists() {
        Some(canonical)
    } else {
        None
    }
}

fn discover_csproj_files(root: &Path, output: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            if matches!(name, ".git" | "target" | "sbg_output" | "bin" | "obj") {
                continue;
            }
            discover_csproj_files(&path, output);
            continue;
        }

        if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("csproj")) {
            output.push(path);
        }
    }
}

fn discover_app_source_dirs(workspace_root: &Path) -> Vec<PathBuf> {
    let mut discovered = BTreeSet::new();

    let conventional = workspace_root.join("App").join("CSharp");
    if conventional.exists() {
        let path = conventional.canonicalize().unwrap_or(conventional);
        discovered.insert(path);
    }

    let mut csproj_files = Vec::new();
    discover_csproj_files(workspace_root, &mut csproj_files);
    for csproj in csproj_files {
        let csproj_dir = csproj.parent().unwrap_or(workspace_root);
        let content = fs::read_to_string(&csproj).unwrap_or_default();
        let mut includes_found = false;

        for line in content.lines() {
            if !line.contains("<Compile") {
                continue;
            }

            let include = parse_compile_include_value(line, "Include")
                .or_else(|| parse_compile_include_value(line, "Update"));
            if let Some(include) = include {
                if let Some(path) = include_to_source_dir(csproj_dir, include.as_str()) {
                    includes_found = true;
                    discovered.insert(path);
                }
            }
        }

        if !includes_found {
            let fallback = csproj_dir.canonicalize().unwrap_or_else(|_| csproj_dir.to_path_buf());
            discovered.insert(fallback);
        }
    }

    discovered.into_iter().collect()
}

fn parse_source_dirs_from_env(raw: &str) -> Vec<PathBuf> {
    raw.split(';')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .collect()
}

fn main() -> Result<()> {
    // Default configuration - can be overridden by environment variables or CLI args
    let mut config = SbgConfig::default();

    // Check for environment variable overrides
    if let Ok(src) = std::env::var("SBG_METADATA_SOURCE") {
        config.metadata_source = PathBuf::from(src);
    }
    if let Ok(out) = std::env::var("SBG_OUTPUT_DIR") {
        config.output_dir = PathBuf::from(out);
    }
    if let Ok(framework) = std::env::var("SBG_TARGET_FRAMEWORK") {
        config.target_framework = framework;
    }
    if let Ok(app_sources) = std::env::var("SBG_APP_CS_SOURCES_DIR") {
        let parsed = parse_source_dirs_from_env(app_sources.as_str());
        if !parsed.is_empty() {
            config.app_cs_sources_dirs = parsed;
        }
    }

    if config.app_cs_sources_dirs.is_empty() {
        let workspace_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        config.app_cs_sources_dirs = discover_app_source_dirs(&workspace_root);
    }

    let sbg = StaticBindingGenerator::new(config);
    let _manifest = sbg.generate()?;

    Ok(())
}
