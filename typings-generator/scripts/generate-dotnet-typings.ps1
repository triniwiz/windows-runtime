<#
.SYNOPSIS
Generate TypeScript typings for .NET 9 and .NET 10 using the project's typings-generator.

.DESCRIPTION
This script locates local .NET shared framework installations (if not provided),
then invokes `cargo run -p typings-generator` twice to emit typings to:
  - `typings/dotnet9/`
  - `typings/dotnet10/`

.PARAMETER Dotnet9Dir
Optional path to the .NET 9 shared framework directory containing DLLs.

.PARAMETER Dotnet10Dir
Optional path to the .NET 10 shared framework directory containing DLLs.

.EXAMPLE
# Run using auto-detection:
powershell -ExecutionPolicy Bypass -File .\typings-generator\scripts\generate-dotnet-typings.ps1

.EXAMPLE
# Specify explicit shared framework paths:
powershell -ExecutionPolicy Bypass -File .\typings-generator\scripts\generate-dotnet-typings.ps1 `
  -Dotnet9Dir "C:\Program Files\dotnet\shared\Microsoft.NETCore.App\9.0.0" `
  -Dotnet10Dir "C:\Program Files\dotnet\shared\Microsoft.NETCore.App\10.0.0"
#>
Param(
    [string] $Dotnet9Dir = "",
    [string] $Dotnet10Dir = ""
)

Set-StrictMode -Version Latest

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Definition
$repoRoot = Resolve-Path (Join-Path $scriptRoot "..\..")
$repoRoot = $repoRoot.Path

function Find-SharedFrameworks {
    param([string] $Major)

    $runtimeNames = @("Microsoft.NETCore.App", "Microsoft.WindowsDesktop.App")
    $found = @()

    if (Get-Command dotnet -ErrorAction SilentlyContinue) {
        try { $lines = & dotnet --list-runtimes 2>$null } catch { $lines = @() }
        foreach ($line in $lines) {
            # Example: Microsoft.NETCore.App 6.0.9 [C:\Program Files\dotnet\shared\Microsoft.NETCore.App]
            if ($line -match '^([^\s]+)\s+([^\s]+)\s+\[(.+)\]$') {
                $name = $matches[1]
                $versionStr = $matches[2]
                $basePath = $matches[3]
                $maj = ($versionStr -split '\.')[0]
                if ($maj -eq $Major -and $runtimeNames -contains $name) {
                    $full = Join-Path $basePath $versionStr
                    if (Test-Path $full) {
                        $found += $full
                    }
                }
            }
        }
    }

    # Fallback locations to catch side-by-side or repo-local frameworks
    $fallbackBases = @(
        (Join-Path ${env:ProgramFiles} 'dotnet\shared'),
        (Join-Path ${env:ProgramFiles(x86)} 'dotnet\shared'),
        (Join-Path ${env:USERPROFILE} '.dotnet\shared'),
        (Join-Path $repoRoot 'dotnet\shared'),
        (Join-Path $repoRoot '.dotnet\shared')
    )

    foreach ($baseRoot in $fallbackBases) {
        if (-not $baseRoot) { continue }
        if (-not (Test-Path $baseRoot)) { continue }
        foreach ($runtimeName in $runtimeNames) {
            $candidateRoot = Join-Path $baseRoot $runtimeName
            if (-not (Test-Path $candidateRoot)) { continue }
            $dirs = @(Get-ChildItem -Path $candidateRoot -Directory -ErrorAction SilentlyContinue | Where-Object { $_.Name -like "$Major.*" } | Sort-Object -Property Name -Descending)
            foreach ($d in $dirs) {
                $found += $d.FullName
            }
        }
    }

    # Deduplicate and return
    $found = $found | Sort-Object -Unique
    return $found
}

if (-not $Dotnet9Dir) {
    $paths = Find-SharedFrameworks "9"
    if ($paths -and $paths.Count -gt 0) { $Dotnet9Dir = ($paths -join ',') }
}
if (-not $Dotnet10Dir) {
    $paths = Find-SharedFrameworks "10"
    if ($paths -and $paths.Count -gt 0) { $Dotnet10Dir = ($paths -join ',') }
}

if (-not $Dotnet9Dir) {
    Write-Host "Could not auto-detect .NET 9 shared framework; pass -Dotnet9Dir to the script." -ForegroundColor Yellow
}
if (-not $Dotnet10Dir) {
    Write-Host "Could not auto-detect .NET 10 shared framework; pass -Dotnet10Dir to the script." -ForegroundColor Yellow
}

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Error "cargo (Rust toolchain) not found in PATH. Install Rust/cargo to run the generator."
    exit 2
}

Push-Location $repoRoot

$outs = @(
    @{ Version = "dotnet9"; Src = $Dotnet9Dir; Out = Join-Path $repoRoot "typings\dotnet9" },
    @{ Version = "dotnet10"; Src = $Dotnet10Dir; Out = Join-Path $repoRoot "typings\dotnet10" }
)

foreach ($entry in $outs) {
    $ver = $entry.Version
    $src = $entry.Src
    # Support comma-separated source paths (core + desktop). Normalize to an array.
    $srcPaths = @()
    if (-not [string]::IsNullOrEmpty($src)) {
        $srcPaths = $src -split ',' | ForEach-Object { $_.Trim() } | Where-Object { -not [string]::IsNullOrEmpty($_) }
    }
    $out = $entry.Out

    if ($srcPaths.Count -eq 0) {
        Write-Host "Skipping ${ver}: no source path available." -ForegroundColor Yellow
        continue
    }

    $existing = $srcPaths | Where-Object { Test-Path $_ }
    if ($existing.Count -eq 0) {
        Write-Host "Skipping ${ver}: none of the source paths exist:" -ForegroundColor Yellow
        foreach ($p in $srcPaths) { Write-Host "  $p" }
        continue
    }

    Write-Host ""
    Write-Host "Generating typings for $ver"
    Write-Host "  Source(s): $($existing -join ', ')"
    Write-Host "  Output: $out"

    $null = New-Item -ItemType Directory -Path $out -Force -ErrorAction SilentlyContinue

    $libsArg = $existing -join ','
    $args = @("run", "-p", "typings-generator", "--", "--libs", $libsArg, "--out-dir", $out)
    Write-Host "Running: cargo $($args -join ' ')"
    & cargo @args
    if ($LASTEXITCODE -ne 0) {
        Write-Error "typings-generator failed for $ver (exit $LASTEXITCODE). Aborting."
        Pop-Location
        exit $LASTEXITCODE
    }

    Write-Host "Completed: generated typings in: $out" -ForegroundColor Green
}

Pop-Location

Write-Host ""
Write-Host "Done."
