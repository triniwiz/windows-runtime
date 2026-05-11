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
    [switch]$SkipDevtools
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# ── Paths ─────────────────────────────────────────────────────────────────────

$ScriptDir  = $PSScriptRoot
$RepoRoot   = (Resolve-Path (Join-Path $ScriptDir "..")).Path
$FrameworkLibs = Join-Path $ScriptDir "framework\libs"

Write-Host "Repo root  : $RepoRoot"
Write-Host "Output dir : $FrameworkLibs"

# ── Helper ────────────────────────────────────────────────────────────────────

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

# ── Targets ───────────────────────────────────────────────────────────────────

$Targets = @(
    @{ Arch = "x64";   RustTarget = "x86_64-pc-windows-msvc"  }
)
if (-not $SkipArm64) {
    $Targets += @{ Arch = "arm64"; RustTarget = "aarch64-pc-windows-msvc" }
}

# ── Build ─────────────────────────────────────────────────────────────────────

foreach ($t in $Targets) {
    $arch        = $t.Arch
    $rustTarget  = $t.RustTarget

    if (-not $SkipRelease) {
        Write-Host "`n=== Release ($arch) ===" -ForegroundColor Cyan
        Build-Crate -Profile "release" -Target $rustTarget
        $dll = Join-Path $RepoRoot "target\$rustTarget\release\nativescript.dll"
        Copy-Dll -Src $dll -Dest (Join-Path $FrameworkLibs "$arch\nativescript.dll")
    }

    if (-not $SkipDevtools) {
        Write-Host "`n=== Release-with-devtools ($arch) ===" -ForegroundColor Cyan
        Build-Crate -Profile "release-with-devtools" -Target $rustTarget -ExtraArgs @("--features", "devtools")
        $dll = Join-Path $RepoRoot "target\$rustTarget\release-with-devtools\nativescript.dll"
        Copy-Dll -Src $dll -Dest (Join-Path $FrameworkLibs "devtools\$arch\nativescript.dll")
    }
}

Write-Host ""
Write-Host "Done." -ForegroundColor Green
Write-Host ""
Write-Host "libs layout:"
Get-ChildItem -Recurse $FrameworkLibs -Filter "*.dll" |
    ForEach-Object { "  " + $_.FullName.Substring($FrameworkLibs.Length + 1) }
