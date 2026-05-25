use clap::Parser;
use glob::glob;
use regex::Regex;
use serde::Serialize;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::fs;

#[derive(Parser)]
#[command(
    author,
    version,
    about = "dotnet-tool: scan JS/TS for .NET usage, detect Class.extend() patterns and publish DotNetBridge"
)]
struct Args {
    /// App root directory (default: '.')
    #[arg(long, default_value = ".")]
    app_root: String,

    /// Sub-directory to scan (default: 'app')
    #[arg(long, default_value = "app")]
    dir: String,

    /// Glob pattern for files to scan
    #[arg(long, default_value = "**/*.{js,mjs,cjs,ts,jsx,tsx}")]
    pattern: String,

    /// Force publishing/copying even if no matches are found
    #[arg(long, default_value_t = false)]
    force: bool,

    /// Skip generating C# proxy stubs (useful when SBG handles generation)
    #[arg(long, default_value_t = false)]
    no_codegen: bool,
}

// ── Serialisable output types ─────────────────────────────────────────────────

#[derive(Serialize)]
struct DotNetUsage {
    file: String,
    line: usize,
    call: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    assembly: Option<String>,
}

#[derive(Serialize, Clone)]
struct ExtensionMethod {
    name: String,
    /// true = property getter (prefixed with "get_" in C#)
    is_property: bool,
}

#[derive(Serialize, Clone)]
struct ClassExtension {
    file: String,
    line: usize,
    /// The fully-qualified .NET base type (e.g. "Windows.UI.Xaml.Controls.Grid")
    base_type: String,
    /// Detected method/property names from the extend body
    methods: Vec<ExtensionMethod>,
}

#[derive(Serialize)]
struct ScanResult {
    found: bool,
    dotnet_usages: Vec<DotNetUsage>,
    class_extensions: Vec<ClassExtension>,
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let args = Args::parse();

    let scan_dir_candidate = PathBuf::from(&args.app_root).join(&args.dir);
    let scan_dir = if scan_dir_candidate.exists() {
        scan_dir_candidate
    } else {
        PathBuf::from(&args.app_root)
    };

    let scan_dir_str = scan_dir.to_string_lossy().replace('\\', "/");
    let glob_pattern = format!("{}/{}", scan_dir_str, &args.pattern);

    let mut dotnet_usages: Vec<DotNetUsage> = Vec::new();
    let mut class_extensions: Vec<ClassExtension> = Vec::new();

    // DotNet.invokeMethod("AssemblyName", ...) — both quote styles
    let re_invoke_dq = Regex::new(
        r#"(?m)\b(?P<fn>DotNet\.invokeMethodAsync|DotNet\.invokeMethod)\s*\(\s*"(?P<asm>[^"]+)""#,
    )
    .unwrap();
    let re_invoke_sq = Regex::new(
        r#"(?m)\b(?P<fn>DotNet\.invokeMethodAsync|DotNet\.invokeMethod)\s*\(\s*'(?P<asm>[^']+)'"#,
    )
    .unwrap();

    // Class.extend({ ... }) — capture base type name
    // Matches patterns like:
    //   const MyView = SomeNamespace.SomeClass.extend({ ... })
    //   SomeClass.extend("MyView", { ... })
    let re_extend = Regex::new(
        r#"(?P<base>[\w.]+)\.extend\s*\(\s*(?:"[^"]*"\s*,\s*)?\{"#,
    )
    .unwrap();

    // Method definition inside an extend body: "methodName()" or "methodName: function"
    let re_method = Regex::new(
        r#"(?m)^\s+(?P<name>[a-zA-Z_$][\w$]*)\s*(?:\([^)]*\)\s*\{|:\s*function\s*\()"#,
    )
    .unwrap();

    for entry in glob(&glob_pattern).context("glob pattern failed")? {
        let path = entry?;
        let path_str = path.to_string_lossy().to_string();
        let content = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        // --- DotNet.invokeMethod usage ---
        for cap in re_invoke_dq.captures_iter(&content) {
            let start = cap.get(0).map(|m| m.start()).unwrap_or(0);
            dotnet_usages.push(DotNetUsage {
                file: path_str.clone(),
                line: content[..start].lines().count() + 1,
                call: cap.name("fn").map(|m| m.as_str().to_string()).unwrap_or_default(),
                assembly: cap.name("asm").map(|m| m.as_str().to_string()),
            });
        }
        for cap in re_invoke_sq.captures_iter(&content) {
            let start = cap.get(0).map(|m| m.start()).unwrap_or(0);
            dotnet_usages.push(DotNetUsage {
                file: path_str.clone(),
                line: content[..start].lines().count() + 1,
                call: cap.name("fn").map(|m| m.as_str().to_string()).unwrap_or_default(),
                assembly: cap.name("asm").map(|m| m.as_str().to_string()),
            });
        }

        // --- Class.extend() detection ---
        for cap in re_extend.captures_iter(&content) {
            let base = cap.name("base").map(|m| m.as_str()).unwrap_or("");
            // Only capture types that look like WinRT or .NET namespaced types
            // (contain a dot AND start with an uppercase letter segment).
            if !base.contains('.') {
                continue;
            }
            if !base.split('.').any(|seg| seg.starts_with(|c: char| c.is_uppercase())) {
                continue;
            }

            let start = cap.get(0).map(|m| m.start()).unwrap_or(0);
            let line = content[..start].lines().count() + 1;

            // Extract method/property names from the extend body (heuristic)
            let body_start = cap.get(0).map(|m| m.end()).unwrap_or(0);
            let body_snippet = &content[body_start..content.len().min(body_start + 4096)];
            let methods: Vec<ExtensionMethod> = re_method
                .captures_iter(body_snippet)
                .map(|mc| {
                    let name = mc.name("name").map(|m| m.as_str()).unwrap_or("").to_string();
                    let is_property = name.starts_with("get") && name.len() > 3;
                    ExtensionMethod { name, is_property }
                })
                .collect();

            class_extensions.push(ClassExtension {
                file: path_str.clone(),
                line,
                base_type: base.to_string(),
                methods,
            });
        }
    }

    let found = !dotnet_usages.is_empty() || !class_extensions.is_empty();
    let result = ScanResult { found, dotnet_usages, class_extensions: class_extensions.clone() };
    println!("{}", serde_json::to_string_pretty(&result)?);

    if (found || args.force) {
        // Generate C# proxy stubs unless disabled
        if !args.no_codegen && !class_extensions.is_empty() {
            let app_root = PathBuf::from(&args.app_root);
            if let Err(e) = generate_proxy_stubs(&app_root, &class_extensions) {
                eprintln!("Warning: C# stub generation failed: {}", e);
            }
        }

        if let Err(e) = publish_and_copy_bridge(PathBuf::from(&args.app_root)) {
            eprintln!("Warning: failed to publish/copy dotnet-bridge: {}", e);
        } else {
            eprintln!("dotnet-bridge published and copied into app root");
        }
    }

    Ok(())
}

// ── C# Proxy Stub Generation ──────────────────────────────────────────────────

/// Generate a C# file containing one proxy subclass per detected JS extension.
/// These classes forward virtual method calls back to JS via ProxyRuntime.
fn generate_proxy_stubs(app_root: &PathBuf, extensions: &[ClassExtension]) -> Result<()> {
    // Group by base type to avoid duplicate class definitions
    let mut by_base: HashMap<String, Vec<&ClassExtension>> = HashMap::new();
    for ext in extensions {
        by_base.entry(ext.base_type.clone()).or_default().push(ext);
    }

    let mut cs = String::from(
        "// <auto-generated by dotnet-tool> do not edit manually\n\
         // This file registers JS-extended .NET types so the runtime can\n\
         // forward virtual method calls back to JavaScript.\n\n\
         using System;\n\
         using NativeScriptBridge;\n\n\
         namespace NativeScriptGeneratedProxies;\n\n\
         /// <summary>Wires up ProxyRuntime callbacks for SBG-generated proxies.</summary>\n\
         public static class ProxyDispatcher\n\
         {\n\
             public static Func<object, string, object[], object?>? JsInvokeMethod { get; set; }\n\
             public static Action<object, string, object[]>? JsInvokeVoid { get; set; }\n\
             public static Func<object, string, object?>? JsGetProperty { get; set; }\n\
             public static Action<object, string, object?>? JsSetProperty { get; set; }\n\
             public static Action<object, string>? JsInitializeInstance { get; set; }\n\
         }\n\n",
    );

    for (base_type, exts) in &by_base {
        let safe_name = base_type.replace('.', "_");
        let short_name = base_type.split('.').last().unwrap_or(&safe_name);
        let proxy_name = format!("{}_JsProxy", safe_name);
        let merged_methods: Vec<ExtensionMethod> = exts
            .iter()
            .flat_map(|e| e.methods.iter().cloned())
            // deduplicate by name
            .fold(Vec::new(), |mut acc, m| {
                if !acc.iter().any(|e: &ExtensionMethod| e.name == m.name) {
                    acc.push(m);
                }
                acc
            });

        cs.push_str(&format!(
            "/// <summary>JS proxy for {base_type} extended classes.</summary>\n\
             public class {proxy_name} : {base_type}\n\
             {{\n\
                 public {proxy_name}() : base()\n\
                 {{\n\
                     ProxyDispatcher.JsInitializeInstance?.Invoke(this, \"{base_type}\");\n\
                 }}\n\n",
        ));

        for method in &merged_methods {
            if method.name == "constructor" || method.name.is_empty() {
                continue;
            }
            let cs_name = pascal_case(&method.name);
            if method.is_property {
                cs.push_str(&format!(
                    "    public virtual object? {cs_name}\n\
                     {{\n\
                         get => ProxyDispatcher.JsGetProperty?.Invoke(this, \"{cs_name}\");\n\
                         set => ProxyDispatcher.JsSetProperty?.Invoke(this, \"{cs_name}\", value);\n\
                     }}\n\n"
                ));
            } else {
                cs.push_str(&format!(
                    "    public virtual void {cs_name}(object?[] args)\n\
                     {{\n\
                         ProxyDispatcher.JsInvokeVoid?.Invoke(this, \"{cs_name}\", args);\n\
                     }}\n\n"
                ));
            }
        }

        cs.push_str("}\n\n");
    }

    // Write to the bridge's generated directory
    let gen_dir = find_bridge_dir(app_root)
        .map(|d| d.join("Generated"))
        .unwrap_or_else(|| app_root.join("dotnet-bridge").join("Generated"));

    fs::create_dir_all(&gen_dir)?;
    let out_path = gen_dir.join("JsProxies.g.cs");
    fs::write(&out_path, &cs)
        .with_context(|| format!("Failed to write generated proxies to {}", out_path.display()))?;
    eprintln!("Generated {} proxy classes -> {}", by_base.len(), out_path.display());
    Ok(())
}

fn pascal_case(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

fn find_bridge_dir(start: &PathBuf) -> Option<PathBuf> {
    let mut cur = start.clone();
    for _ in 0..6 {
        let candidate = cur.join("dotnet-bridge");
        if candidate.exists() {
            return Some(candidate);
        }
        if !cur.pop() {
            break;
        }
    }
    None
}

// ── Bridge Publishing ─────────────────────────────────────────────────────────

fn publish_and_copy_bridge(app_root: PathBuf) -> Result<()> {
    let bridge_dir = find_bridge_dir(&app_root)
        .ok_or_else(|| anyhow::anyhow!("dotnet-bridge project not found (searched up to 6 levels from app_root)"))?;

    let status = std::process::Command::new("dotnet")
        .args(["publish", "-c", "Release", "-o", "publish", "-p:CopyLocalLockFileAssemblies=true"])
        .current_dir(&bridge_dir)
        .status()
        .context("failed to spawn dotnet publish; ensure .NET SDK is installed and on PATH")?;

    if !status.success() {
        anyhow::bail!("dotnet publish failed with exit code: {:?}", status.code());
    }

    let src = bridge_dir.join("publish");
    if !src.exists() {
        anyhow::bail!("dotnet publish did not produce a publish folder at {}", src.display());
    }

    let dest = app_root.join("dotnet-bridge").join("publish");
    fs::create_dir_all(&dest)?;
    copy_dir_recursive(&src, &dest)?;

    if let Err(e) = publish_app_projects(&app_root, &dest) {
        eprintln!("Warning: publishing app projects failed: {}", e);
    }

    let marker = dest.join(".dotnet_tool_done");
    fs::write(&marker, "dotnet-tool-published")
        .with_context(|| format!("failed to write marker at {}", marker.display()))?;
    Ok(())
}

fn copy_dir_recursive(src: &PathBuf, dst: &PathBuf) -> Result<()> {
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let dest_path = dst.join(path.strip_prefix(src).unwrap());
        if path.is_dir() {
            fs::create_dir_all(&dest_path)?;
            copy_dir_recursive(&path, &dest_path)?;
        } else {
            if let Some(p) = dest_path.parent() {
                fs::create_dir_all(p)?;
            }
            fs::copy(&path, &dest_path)?;
        }
    }
    Ok(())
}

fn publish_app_projects(app_root: &PathBuf, dest: &PathBuf) -> Result<()> {
    let pattern = format!("{}/**/*.csproj", app_root.to_string_lossy().replace('\\', "/"));
    for entry in glob(&pattern).context("csproj glob failed")? {
        let csproj = entry?;
        let csproj_str = csproj.to_string_lossy();
        if csproj_str.contains("dotnet-bridge") {
            continue;
        }
        let proj_dir = csproj.parent().unwrap_or(app_root.as_path());
        eprintln!("Publishing app project: {}", csproj.display());
        let status = std::process::Command::new("dotnet")
            .args(["publish", "-c", "Release", "-o", "publish", "-p:CopyLocalLockFileAssemblies=true"])
            .current_dir(proj_dir)
            .status()
            .with_context(|| format!("failed to publish {}", csproj.display()))?;
        if !status.success() {
            eprintln!("dotnet publish failed for {}", csproj.display());
            continue;
        }
        let src = proj_dir.join("publish");
        if src.exists() {
            copy_dir_recursive(&src, dest)?;
        }
    }
    Ok(())
}
