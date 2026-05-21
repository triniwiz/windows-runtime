#!/usr/bin/env pwsh
# SBG + Runtime Build Script
# Implements the separated architecture: SBG -> Runtime Binding Gen -> Runtime

param(
    [ValidateSet("debug", "release", "release-with-devtools")]
    [string]$Profile = "debug",
    [switch]$RunSBG = $false,
    [switch]$Clean = $false,
    [switch]$BuildNativeLibs = $true,
    [ValidateSet("x64", "arm64", "all")]
    [string]$NativeArch = "all"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$CargoProfile = if ($Profile -eq "debug") { "dev" } else { $Profile }

Write-Host "Windows Runtime - Modular Build Script" -ForegroundColor Cyan
Write-Host "======================================" -ForegroundColor Cyan

# Colors for output
function Write-Step { param([string]$msg); Write-Host "[$($Profile.ToUpper())] $msg" -ForegroundColor Yellow }
function Write-Success { param([string]$msg); Write-Host "[+] $msg" -ForegroundColor Green }
function Write-Error { param([string]$msg); Write-Host "[-] $msg" -ForegroundColor Red }

function Invoke-CargoStep {
    param(
        [string]$Label,
        [scriptblock]$Action,
        [string]$FailureMessage
    )

    Write-Step $Label
    try {
        & $Action
        if ($LASTEXITCODE -ne 0) {
            throw "Command exited with code $LASTEXITCODE"
        }
        Write-Success $Label
    }
    catch {
        Write-Error "$FailureMessage $_"
        exit 1
    }
}

function Build-NativeLibraries {
    param(
        [string]$NativeProfile,
        [string]$Arch
    )

    $allTargets = @(
        @{ RustTarget = "x86_64-pc-windows-msvc"; ArchFolder = "x64" },
        @{ RustTarget = "aarch64-pc-windows-msvc"; ArchFolder = "arm64" }
    )

    $targets = if ($Arch -eq "all") {
        $allTargets
    }
    else {
        $allTargets | Where-Object { $_.ArchFolder -eq $Arch }
    }

    if ($targets.Count -eq 0) {
        throw "No native build targets selected. NativeArch was '$Arch'."
    }

    if (-not (Get-Command "rustup" -ErrorAction SilentlyContinue)) {
        throw "rustup not found in PATH"
    }

    if (-not (Get-Command "cl.exe" -ErrorAction SilentlyContinue)) {
        Write-Warning "cl.exe is not available in this shell."
        Write-Warning "Open a Visual Studio Developer PowerShell and run this script again if native compilation fails."
    }

    $installedTargets = @(rustup target list --installed)
    foreach ($target in $targets) {
        if (-not ($installedTargets -contains $target.RustTarget)) {
            throw "Missing Rust target '$($target.RustTarget)'. Install with: rustup target add $($target.RustTarget)"
        }
    }

    $outputProfileDir = if ($NativeProfile -eq "release") { "release" } else { "debug" }

    foreach ($target in $targets) {
        $rustTarget = $target.RustTarget
        $archFolder = $target.ArchFolder

        Invoke-CargoStep -Label "Building nativescript for $rustTarget ($NativeProfile)..." -FailureMessage "Native build failed:" -Action {
            if ($NativeProfile -eq "release") {
                cargo build -p nativescript --target $rustTarget --release
            }
            else {
                cargo build -p nativescript --target $rustTarget
            }
        }

        $sourceDll = Join-Path $PSScriptRoot "target\$rustTarget\$outputProfileDir\nativescript.dll"
        if (-not (Test-Path $sourceDll)) {
            throw "Expected output not found: $sourceDll"
        }

        $sharedArchDir = Join-Path (Join-Path $PSScriptRoot "libs") $archFolder
        $sharedDll = Join-Path $sharedArchDir "nativescript.dll"

        Write-Step "Copying $archFolder DLL to shared libs..."
        New-Item -ItemType Directory -Force $sharedArchDir | Out-Null
        Copy-Item -Force $sourceDll $sharedDll
        Write-Success "Copied $archFolder native DLL"
    }
}

if (-not (Get-Command "cargo" -ErrorAction SilentlyContinue)) {
    Write-Error "cargo not found in PATH"
    exit 1
}

# Clean workspace if requested
if ($Clean) {
    Write-Step "Cleaning workspace..."
    cargo clean
    if ($LASTEXITCODE -ne 0) {
        Write-Error "cargo clean failed"
        exit 1
    }
    Write-Success "Workspace cleaned"
}

if ($BuildNativeLibs) {
    $nativeProfile = if ($Profile -eq "debug") { "debug" } else { "release" }

    Write-Step "Building native libs ($NativeArch, $nativeProfile)..."
    try {
        Build-NativeLibraries -NativeProfile $nativeProfile -Arch $NativeArch
        Write-Success "Native libraries built"
    }
    catch {
        Write-Error "Native library build failed: $_"
        exit 1
    }
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
        cargo run -p playground --profile $CargoProfile
        if ($LASTEXITCODE -ne 0) {
            throw "playground metadata capture failed"
        }

        # Build SBG binary
        Write-Host "  - Building SBG binary..."
        cargo build -p sbg --release
        if ($LASTEXITCODE -ne 0) {
            throw "sbg build failed"
        }
        
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
Invoke-CargoStep -Label "Building Runtime Binding Generator..." -FailureMessage "Runtime Binding Generator build failed:" -Action {
    cargo build -p runtime-binding-gen --profile $CargoProfile
}

# Phase 3: Build Runtime (core library)
Invoke-CargoStep -Label "Building Runtime library..." -FailureMessage "Runtime build failed:" -Action {
    cargo build -p runtime --profile $CargoProfile
}

# Phase 4: Build main binaries
Invoke-CargoStep -Label "Building playground application..." -FailureMessage "Playground build failed:" -Action {
    cargo build -p playground --profile $CargoProfile
}

# Summary
Write-Host ""
Write-Host "Build Summary" -ForegroundColor Cyan
Write-Host "=============" -ForegroundColor Cyan
Write-Host "Profile: $Profile" -ForegroundColor White
Write-Host "Native libs built: $BuildNativeLibs" -ForegroundColor White
if ($BuildNativeLibs) {
    Write-Host "Native arch: $NativeArch" -ForegroundColor White
}
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
