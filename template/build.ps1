#Requires -Version 5.1
<#
.SYNOPSIS
    Builds both nativescript.dll configurations and copies them into
    template/framework/libs/ ready for npm publish.

.DESCRIPTION
    Produces four DLLs (x64 + arm64, release + devtools) from the workspace
    Rust crate and places them at:

        framework/libs/x64/nativescript.dll            # release, no devtools
        framework/libs/arm64/nativescript.dll
        framework/libs/devtools/x64/nativescript.dll   # release-with-devtools
        framework/libs/devtools/arm64/nativescript.dll

    Run from anywhere; the script locates the repo root via its own path.

.PARAMETER SkipArm64
    Skip the arm64 cross-compilation steps (faster, x64-only output).

.PARAMETER SkipRelease
    Skip the stripped release builds (devtools builds only).

.PARAMETER SkipDevtools
    Skip the devtools builds (release builds only).
#>
param(
    [switch]$SkipArm64,
    [switch]$SkipRelease,
    [switch]$SkipDevtools,
    [switch]$SkipDotnet
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# Paths
$ScriptDir  = $PSScriptRoot
$RepoRoot   = (Resolve-Path (Join-Path $ScriptDir "..")).Path
$FrameworkLibs = Join-Path $ScriptDir "framework\libs"

Write-Host "Repo root  : $RepoRoot"
Write-Host "Output dir : $FrameworkLibs"

# Helper
function Copy-Dll {
    param([string]$Src, [string]$Dest)
    $null = New-Item -ItemType Directory -Force -Path (Split-Path $Dest)
    Copy-Item -Force $Src $Dest
    Write-Host "  copied  $(Split-Path $Src -Leaf)  →  $(Resolve-Path $Dest -Relative 2>$null)"
}

function Build-Crate {
    param(
        [string]$Profile,       # e.g. "release" or "release-with-devtools"
        [string]$Target,        # e.g. "x86_64-pc-windows-msvc"
        [string[]]$ExtraArgs    # e.g. @("--features","devtools")
    )
    $args = @("build", "--profile", $Profile, "-p", "nativescript", "--target", $Target) + $ExtraArgs
    Write-Host ""
    Write-Host "cargo $($args -join ' ')"
    Push-Location $RepoRoot
    try {
        & cargo @args
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed (exit $LASTEXITCODE)" }
    } finally {
        Pop-Location
    }
}

# DotNet Bridge
# Copies the dotnet-bridge source (csproj + .cs files) into
# template/framework/dotnet-bridge/ so it ships inside the npm package.
# The actual `dotnet publish` is deferred to the app's MSBuild process —
# see the PublishDotNetBridge target in __PROJECT_NAME__.csproj.

if (-not $SkipDotnet) {
    Write-Host "`n=== DotNet Bridge (source copy) ===" -ForegroundColor Cyan
    $BridgeSrc  = Join-Path $RepoRoot "dotnet-bridge"
    $BridgeDest = Join-Path $ScriptDir "framework\dotnet-bridge"

    # Wipe the destination so stale files don't linger between runs.
    if (Test-Path $BridgeDest) { Remove-Item -Recurse -Force $BridgeDest }
    $null = New-Item -ItemType Directory -Force -Path $BridgeDest

    # Copy only source files; exclude build/publish artefacts.
    Get-ChildItem -Path $BridgeSrc -Recurse |
        Where-Object {
            $rel = $_.FullName.Substring($BridgeSrc.Length + 1)
            $rel -notmatch '^(bin|obj|publish)(\\|$)'
        } |
        ForEach-Object {
            $dest = Join-Path $BridgeDest $_.FullName.Substring($BridgeSrc.Length + 1)
            if ($_.PSIsContainer) {
                $null = New-Item -ItemType Directory -Force -Path $dest
            } else {
                Copy-Item -Force $_.FullName $dest
                Write-Host "  copied  $($_.FullName.Substring($BridgeSrc.Length + 1))"
            }
        }
}

# Targets
$Targets = @(
    @{ Arch = "x64";   RustTarget = "x86_64-pc-windows-msvc"  }
)
if (-not $SkipArm64) {
    $Targets += @{ Arch = "arm64"; RustTarget = "aarch64-pc-windows-msvc" }
}

# Build
foreach ($t in $Targets) {
    $arch        = $t.Arch
    $rustTarget  = $t.RustTarget

    if (-not $SkipRelease) {
        Write-Host "`n=== Release ($arch) ===" -ForegroundColor Cyan
        $releaseArgs = @()
        Build-Crate -Profile "release" -Target $rustTarget -ExtraArgs $releaseArgs
        $dll = Join-Path $RepoRoot "target\$rustTarget\release\nativescript.dll"
        Copy-Dll -Src $dll -Dest (Join-Path $FrameworkLibs "$arch\nativescript.dll")
    }

    if (-not $SkipDevtools) {
        Write-Host "`n=== Release-with-devtools ($arch) ===" -ForegroundColor Cyan
        $devtoolsArgs = @("--features", "devtools")
        Build-Crate -Profile "release-with-devtools" -Target $rustTarget -ExtraArgs $devtoolsArgs
        $dll = Join-Path $RepoRoot "target\$rustTarget\release-with-devtools\nativescript.dll"
        Copy-Dll -Src $dll -Dest (Join-Path $FrameworkLibs "devtools\$arch\nativescript.dll")
    }
}

# dotnet-tool prebuilt binaries
Write-Host "`n=== Build dotnet-tool prebuilt binaries ===" -ForegroundColor Cyan
# Place prebuilt tools into the framework folder so they are included in the
# packaged framework (npm publish copies framework/* into the package root).
$ToolsDir = Join-Path $ScriptDir "framework\tools"
if (-not (Test-Path $ToolsDir)) { New-Item -ItemType Directory -Force -Path $ToolsDir | Out-Null }


foreach ($t in $Targets) {
    $arch = $t.Arch
    $rustTarget = $t.RustTarget
    Write-Host "Building dotnet-tool for $arch ($rustTarget)..."
    Push-Location $RepoRoot
    try {
        & cargo build -p dotnet-tool --release --target $rustTarget
        $buildExit = $LASTEXITCODE
    } finally {
        Pop-Location
    }
    if ($buildExit -ne 0) {
        Write-Host "cargo build for dotnet-tool failed for target $rustTarget (exit $buildExit). Skipping copy for $arch." -ForegroundColor Yellow
        continue
    }
    $candidate = Join-Path $RepoRoot "target\$rustTarget\release\dotnet-tool.exe"
    if (Test-Path $candidate) {
        $dest = Join-Path $ToolsDir "dotnet-tool-$arch.exe"
        Copy-Item -Force $candidate $dest
        Write-Host "  copied dotnet-tool -> $(Resolve-Path $dest -Relative)"
    } else {
        Write-Host "Expected build output not found: $candidate" -ForegroundColor Yellow
    }
}

# Provide a generic `dotnet-tool.exe` fallback (copy x64 if available)
$x64Path = Join-Path $ToolsDir "dotnet-tool-x64.exe"
if (Test-Path $x64Path) {
    Copy-Item -Force $x64Path (Join-Path $ToolsDir "dotnet-tool.exe")
    Write-Host "  copied dotnet-tool-x64.exe -> dotnet-tool.exe"
}

# sbg prebuilt binaries
Write-Host "`n=== Build sbg prebuilt binaries ===" -ForegroundColor Cyan

foreach ($t in $Targets) {
    $arch = $t.Arch
    $rustTarget = $t.RustTarget
    Write-Host "Building sbg for $arch ($rustTarget)..."
    Push-Location $RepoRoot
    try {
        & cargo build -p sbg --release --target $rustTarget
        $buildExit = $LASTEXITCODE
    } finally {
        Pop-Location
    }
    if ($buildExit -ne 0) {
        Write-Host "cargo build for sbg failed for target $rustTarget (exit $buildExit). Skipping copy for $arch." -ForegroundColor Yellow
        continue
    }
    $candidate = Join-Path $RepoRoot "target\$rustTarget\release\sbg.exe"
    if (Test-Path $candidate) {
        $dest = Join-Path $ToolsDir "sbg-$arch.exe"
        Copy-Item -Force $candidate $dest
        Write-Host "  copied sbg -> $(Resolve-Path $dest -Relative)"
    } else {
        Write-Host "Expected build output not found: $candidate" -ForegroundColor Yellow
    }
}

$x64Path = Join-Path $ToolsDir "sbg-x64.exe"
if (Test-Path $x64Path) {
    Copy-Item -Force $x64Path (Join-Path $ToolsDir "sbg.exe")
    Write-Host "  copied sbg-x64.exe -> sbg.exe"
}

# typings-generator: shipped so `ns typings windows` can generate WinRT/.NET typings.
Write-Host "`n=== Build typings-generator prebuilt binaries ===" -ForegroundColor Cyan

foreach ($t in $Targets) {
    $arch = $t.Arch
    $rustTarget = $t.RustTarget
    Write-Host "Building typings-generator for $arch ($rustTarget)..."
    Push-Location $RepoRoot
    try {
        & cargo build -p typings-generator --release --target $rustTarget
        $buildExit = $LASTEXITCODE
    } finally {
        Pop-Location
    }
    if ($buildExit -ne 0) {
        Write-Host "cargo build for typings-generator failed for target $rustTarget (exit $buildExit). Skipping copy for $arch." -ForegroundColor Yellow
        continue
    }
    $candidate = Join-Path $RepoRoot "target\$rustTarget\release\typings-generator.exe"
    if (Test-Path $candidate) {
        $dest = Join-Path $ToolsDir "typings-generator-$arch.exe"
        Copy-Item -Force $candidate $dest
        Write-Host "  copied typings-generator -> $(Resolve-Path $dest -Relative)"
    } else {
        Write-Host "Expected build output not found: $candidate" -ForegroundColor Yellow
    }
}

$x64Path = Join-Path $ToolsDir "typings-generator-x64.exe"
if (Test-Path $x64Path) {
    Copy-Item -Force $x64Path (Join-Path $ToolsDir "typings-generator.exe")
    Write-Host "  copied typings-generator-x64.exe -> typings-generator.exe"
}

# dotnet-typings-gen: .NET sub-tool typings-generator shells to for managed/non-Windows assemblies.
$DotnetTypingsProj = Join-Path $RepoRoot "typings-generator\dotnet-src\dotnet-typings-gen.csproj"
if (Test-Path $DotnetTypingsProj) {
    Write-Host "`n=== Publish dotnet-typings-gen (.NET typings sub-tool) ===" -ForegroundColor Cyan
    $DotnetTypingsOut = Join-Path $ToolsDir "dotnet-typings-gen"
    & dotnet publish $DotnetTypingsProj -c Release -o $DotnetTypingsOut --nologo
    if ($LASTEXITCODE -ne 0) {
        Write-Host "dotnet publish for dotnet-typings-gen failed (exit $LASTEXITCODE)." -ForegroundColor Yellow
    } else {
        Write-Host "  published dotnet-typings-gen -> $(Resolve-Path $DotnetTypingsOut -Relative)"
    }
}

Write-Host ""
Write-Host "Done." -ForegroundColor Green
Write-Host ""
Write-Host "libs layout:"
Get-ChildItem -Recurse $FrameworkLibs -Filter "*.dll" |
    ForEach-Object { "  " + $_.FullName.Substring($FrameworkLibs.Length + 1) }
