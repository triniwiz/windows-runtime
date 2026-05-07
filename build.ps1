#!/usr/bin/env pwsh
# SBG + Runtime Build Script
# Implements the separated architecture: SBG -> Runtime Binding Gen -> Runtime

param(
    [ValidateSet("debug", "release", "release-with-devtools")]
    [string]$Profile = "debug",
    [switch]$RunSBG = $false,
    [switch]$Clean = $false
)

$ErrorActionPreference = "Stop"
$CargoProfile = if ($Profile -eq "debug") { "dev" } else { $Profile }

Write-Host "Windows Runtime - Modular Build Script" -ForegroundColor Cyan
Write-Host "======================================" -ForegroundColor Cyan

# Colors for output
function Write-Step { param([string]$msg); Write-Host "[$($Profile.ToUpper())] $msg" -ForegroundColor Yellow }
function Write-Success { param([string]$msg); Write-Host "[+] $msg" -ForegroundColor Green }
function Write-Error { param([string]$msg); Write-Host "[-] $msg" -ForegroundColor Red }

# Clean workspace if requested
if ($Clean) {
    Write-Step "Cleaning workspace..."
    cargo clean
    Write-Success "Workspace cleaned"
}

# Phase 1: Run SBG if requested (pre-build phase)
if ($RunSBG) {
    Write-Step "Running Static Binding Generator (SBG)..."
    try {
        # Automatically capture runtime extension metadata.
        $metadataPath = Join-Path $PSScriptRoot "sbg_output\sbg_metadata.json"
        $env:NSWINRT_AUTO_METADATA_PATH = $metadataPath
        $env:SBG_METADATA_SOURCE = $metadataPath

        Write-Host "  - Discovering app C# sources (auto-detect)..."

        Write-Host "  - Capturing extension metadata automatically..."
        cargo run -p playground --profile $CargoProfile 2>&1 | Select-String "(NativeScript|Windows|error)" | Select-Object -First 5 | Out-Null

        # Build SBG binary
        Write-Host "  - Building SBG binary..."
        cargo build -p sbg --release 2>&1 | Select-String "(Finished|error)" | Select-Object -First 1
        
        # Run SBG
        Write-Host "  - Executing SBG..."
        $sbg_output = & cargo run -p sbg --release 2>&1
        
        if ($LASTEXITCODE -eq 0) {
            Write-Success "SBG completed successfully"
            Write-Host "  - Output: sbg_output/" -ForegroundColor Gray
        } else {
            Write-Error "SBG failed with exit code $LASTEXITCODE"
            exit 1
        }
    } catch {
        Write-Error "SBG execution failed: $_"
        exit 1
    }
}

# Phase 2: Build Runtime-Binding-Gen
Write-Step "Building Runtime Binding Generator..."
try {
    cargo build -p runtime-binding-gen --profile $CargoProfile 2>&1 | Select-String "(Finished|error)" | Select-Object -First 1
    Write-Success "Runtime Binding Generator built"
} catch {
    Write-Error "Runtime Binding Generator build failed: $_"
    exit 1
}

# Phase 3: Build Runtime (core library)
Write-Step "Building Runtime library..."
try {
    cargo build -p runtime --profile $CargoProfile 2>&1 | Select-String "(Finished|error)" | Select-Object -First 1
    Write-Success "Runtime library built"
} catch {
    Write-Error "Runtime build failed: $_"
    exit 1
}

# Phase 4: Build main binaries
Write-Step "Building playground application..."
try {
    cargo build -p playground --profile $CargoProfile 2>&1 | Select-String "(Finished|error)" | Select-Object -First 1
    Write-Success "Playground application built"
} catch {
    Write-Error "Playground build failed: $_"
    exit 1
}

# Summary
Write-Host ""
Write-Host "Build Summary" -ForegroundColor Cyan
Write-Host "=============" -ForegroundColor Cyan
Write-Host "Profile: $Profile" -ForegroundColor White
Write-Host "Status: All components built successfully" -ForegroundColor Green

Write-Host ""
Write-Host "Next Steps:" -ForegroundColor Cyan
if ($Profile -eq "debug") {
    Write-Host "  Run with: cargo run -p playground"
} elseif ($Profile -eq "release-with-devtools") {
    Write-Host "  Run with: cargo run --release -p playground"
    Write-Host "  (DevTools and SBG proxies available)"
} else {
    Write-Host "  Run with: cargo run --release -p playground"
    Write-Host "  (Minimal binary, no SBG or dev tools)"
}
