use clap::Parser;
use glob::glob;
use regex::Regex;
use serde::Serialize;
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::fs;

#[derive(Parser)]
#[command(author, version, about = "dotnet-tool: scan JS/TS for .NET usage and assist publishing DotNetBridge", long_about = None)]
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
}

#[derive(Serialize)]
struct MatchEntry {
    file: String,
    line: usize,
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    assembly: Option<String>,
}

#[derive(Serialize)]
struct ScanResult {
    found: bool,
    matches: Vec<MatchEntry>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let scan_dir_candidate = PathBuf::from(&args.app_root).join(&args.dir);
    let scan_dir = if scan_dir_candidate.exists() { scan_dir_candidate } else { PathBuf::from(&args.app_root) };

    let pattern = &args.pattern;
    let scan_dir_str = scan_dir.to_string_lossy().replace("\\", "/");
    let glob_pattern = format!("{}/{}", scan_dir_str, pattern);

    let mut matches_vec: Vec<MatchEntry> = Vec::new();

    let re_dotnet_dq = Regex::new(r#"(?m)\b(?P<fn>DotNet\.invokeMethodAsync|DotNet\.invokeMethod)\s*\(\s*\"(?P<assembly>[^"]+)\""#).unwrap();
    let re_dotnet_sq = Regex::new(r#"(?m)\b(?P<fn>DotNet\.invokeMethodAsync|DotNet\.invokeMethod)\s*\(\s*\'(?P<assembly>[^']+)\'"#).unwrap();
    let re_tokens = Regex::new(r"\b(System|NSWinRT|Windows|NativeScript|DotNet)\b").unwrap();

    for entry in glob(&glob_pattern).context("glob pattern failed")? {
        let path = entry?;
        let path_str = path.to_string_lossy().to_string();
        let content = fs::read_to_string(&path).unwrap_or_default();

        // DotNet.invokeMethod(...) captures assembly when present as string literal
        for cap in re_dotnet_dq.captures_iter(&content) {
            let assembly = cap.name("assembly").map(|m| m.as_str().to_string());
            let start = cap.get(0).map(|m| m.start()).unwrap_or(0);
            let line = content[..start].lines().count() + 1;
            matches_vec.push(MatchEntry {
                file: path_str.clone(),
                line,
                path: cap.name("fn").map(|m| m.as_str().to_string()).unwrap_or_else(|| "DotNet.invokeMethod".to_string()),
                assembly,
            });
        }
        for cap in re_dotnet_sq.captures_iter(&content) {
            let assembly = cap.name("assembly").map(|m| m.as_str().to_string());
            let start = cap.get(0).map(|m| m.start()).unwrap_or(0);
            let line = content[..start].lines().count() + 1;
            matches_vec.push(MatchEntry {
                file: path_str.clone(),
                line,
                path: cap.name("fn").map(|m| m.as_str().to_string()).unwrap_or_else(|| "DotNet.invokeMethod".to_string()),
                assembly,
            });
        }

        // token fallback: first line containing a token
        if re_tokens.is_match(&content) {
            for (idx, line_content) in content.lines().enumerate() {
                if re_tokens.is_match(line_content) {
                    matches_vec.push(MatchEntry { file: path_str.clone(), line: idx + 1, path: line_content.trim().to_string(), assembly: None });
                    break;
                }
            }
        }
    }

    let out = ScanResult { found: !matches_vec.is_empty(), matches: matches_vec };
    println!("{}", serde_json::to_string_pretty(&out)?);

    // If matches were found (or `--force` requested), attempt to publish and
    // copy dotnet-bridge into the app root (Debug/Release defaults handled by calling script)
    if out.found || args.force {
        if let Err(e) = publish_and_copy_bridge(PathBuf::from(&args.app_root)) {
            eprintln!("Warning: failed to publish/copy dotnet-bridge: {}", e);
            // still exit success so caller can decide; emit a small JSON note to stderr
        } else {
            eprintln!("dotnet-bridge published and copied into app root");
        }
    }
    Ok(())
}

fn publish_and_copy_bridge(app_root: PathBuf) -> Result<()> {
    // Locate the dotnet-bridge project directory relative to repo root (search upwards until found)
    let mut bridge_dir = PathBuf::from("dotnet-bridge");
    if !bridge_dir.exists() {
        // try parent paths from current dir
        let mut cur = std::env::current_dir()?;
        let mut found = false;
        for _ in 0..6 {
            let candidate = cur.join("dotnet-bridge");
            if candidate.exists() {
                bridge_dir = candidate;
                found = true;
                break;
            }
            if !cur.pop() { break; }
        }
        if !found && !PathBuf::from("dotnet-bridge").exists() {
            anyhow::bail!("dotnet-bridge project not found in repository (expected ./dotnet-bridge)");
        }
    }

    // Run `dotnet publish -c Release -o publish` in the bridge dir
    // Request that local package assemblies be copied into the publish output
    // so runtime consumers get package DLLs (e.g. System.Drawing.Common).
    let status = std::process::Command::new("dotnet")
        .arg("publish")
        .arg("-c")
        .arg("Release")
        .arg("-o")
        .arg("publish")
        .arg("-p:CopyLocalLockFileAssemblies=true")
        .current_dir(&bridge_dir)
        .status()
        .context("failed to spawn dotnet publish command; ensure dotnet SDK is installed and on PATH")?;

    if !status.success() {
        anyhow::bail!("dotnet publish failed with exit code: {:?}", status.code());
    }

    // Copy contents of bridge_dir/publish -> app_root/dotnet-bridge/publish
    let src = bridge_dir.join("publish");
    if !src.exists() {
        anyhow::bail!("dotnet publish did not produce a publish folder at {}", src.display());
    }

    let dest = app_root.join("dotnet-bridge").join("publish");
    if !dest.exists() { fs::create_dir_all(&dest)?; }
    copy_dir_recursive(&src, &dest)?;

    // Additionally, detect any `.csproj` files in the app and publish them as well
    // This ensures NuGet package assemblies referenced by the app are included
    // in the final app output we copy for runtime consumption.
    if let Err(e) = publish_app_projects(&app_root, &bridge_dir, &dest) {
        eprintln!("Warning: publishing app projects failed: {}", e);
    }
    // Write a marker file so repeated runs can be skipped by callers
    let marker = dest.join(".dotnet_tool_done");
    fs::write(&marker, "dotnet-tool-published").with_context(|| format!("failed to write marker file at {}", marker.display()))?;
    Ok(())
}

fn copy_dir_recursive(src: &PathBuf, dst: &PathBuf) -> Result<()> {
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let rel = path.strip_prefix(src).unwrap();
        let dest_path = dst.join(rel);
        if path.is_dir() {
            fs::create_dir_all(&dest_path)?;
            copy_dir_recursive(&path, &dest_path)?;
        } else {
            if let Some(parent) = dest_path.parent() { fs::create_dir_all(parent)?; }
            eprintln!("copy: {} -> {}", path.display(), dest_path.display());
            fs::copy(&path, &dest_path)?;
        }
    }
    Ok(())
}

fn publish_app_projects(app_root: &PathBuf, bridge_dir: &PathBuf, dest: &PathBuf) -> Result<()> {
    // Find all .csproj files under app_root (recursively)
    let app_root_str = app_root.to_string_lossy().replace("\\", "/");
    let pattern = format!("{}/**/*.csproj", app_root_str);

    for entry in glob(&pattern).context("glob pattern failed for csproj search")? {
        let csproj = entry?;
        let csproj_str = csproj.to_string_lossy();

        // Skip projects inside the dotnet-bridge project directory itself
        if csproj_str.contains("dotnet-bridge") {
            continue;
        }

        let proj_dir = csproj.parent().unwrap_or(app_root);
        eprintln!("Publishing app project: {}", csproj.display());

        let status = std::process::Command::new("dotnet")
            .arg("publish")
            .arg("-c")
            .arg("Release")
            .arg("-o")
            .arg("publish")
            .arg("-p:CopyLocalLockFileAssemblies=true")
            .current_dir(proj_dir)
            .status()
            .with_context(|| format!("failed to spawn dotnet publish for {}", csproj.display()))?;

        if !status.success() {
            eprintln!("dotnet publish failed for {}: exit {:?}", csproj.display(), status.code());
            continue;
        }

        let src = proj_dir.join("publish");
        if !src.exists() {
            eprintln!("publish folder not found for {} at {}", csproj.display(), src.display());
            continue;
        }

        copy_dir_recursive(&src, dest)?;
    }

    Ok(())
}
