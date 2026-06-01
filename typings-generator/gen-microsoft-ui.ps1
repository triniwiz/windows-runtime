# Regenerates the Microsoft.UI.* (WinUI 3 / Windows App SDK) TypeScript declarations and writes them
# next to the system Windows.* typings in @nativescript/core's types package.
#
# Why the projected .dll and not the .winmd:
#   The raw Microsoft.UI.Xaml.winmd has heavy cross-assembly TypeRefs that aren't in the WinRT system
#   catalog; rendering it through the COM metadata path is unreliable (faults). The CsWinRT-projected
#   .dll set (Microsoft.WinUI.dll + Microsoft.InteractiveExperiences.Projection.dll) is read via the
#   safe System.Reflection.Metadata PE reader (the generator's "dotnet mode").
#
# Usage:  pwsh typings-generator/gen-microsoft-ui.ps1 [-AppSdkVersion 1.6.250108002] [-OutFile <path>]

param(
    [string]$AppSdkVersion = "1.6.250108002",
    [string]$OutFile = "$PSScriptRoot/../../NativeScript/packages/types-minimal/src/lib/windows/microsoft.ui.d.ts"
)

$ErrorActionPreference = "Stop"

# 1. Ensure the Windows App SDK package is restored so its projected assemblies are in the nuget cache.
$pkgRoot = Join-Path $env:USERPROFILE ".nuget/packages/microsoft.windowsappsdk/$AppSdkVersion"
if (-not (Test-Path $pkgRoot)) {
    Write-Host "Restoring Microsoft.WindowsAppSDK $AppSdkVersion ..."
    $tmp = Join-Path ([System.IO.Path]::GetTempPath()) "ns-appsdk-fetch"
    New-Item -ItemType Directory -Force -Path $tmp | Out-Null
    @"
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup><TargetFramework>net8.0-windows10.0.19041.0</TargetFramework></PropertyGroup>
  <ItemGroup><PackageReference Include="Microsoft.WindowsAppSDK" Version="$AppSdkVersion" /></ItemGroup>
</Project>
"@ | Set-Content -Encoding utf8 (Join-Path $tmp "fetch.csproj")
    dotnet restore (Join-Path $tmp "fetch.csproj") | Out-Null
}

# 2. Locate the projected assemblies (pick the highest net*-windows lib variant).
$libBase = Join-Path $pkgRoot "lib"
$tfm = Get-ChildItem $libBase -Directory | Where-Object { $_.Name -like "net*-windows*" } |
       Sort-Object Name -Descending | Select-Object -First 1
$winui = Join-Path $tfm.FullName "Microsoft.WinUI.dll"
$ie    = Join-Path $tfm.FullName "Microsoft.InteractiveExperiences.Projection.dll"
foreach ($dll in @($winui, $ie)) {
    if (-not (Test-Path $dll)) { throw "Projected assembly not found: $dll" }
}

# 3. Generate. --root Microsoft.UI captures Xaml, Composition, Dispatching, Input, Text, etc.
Write-Host "Generating $OutFile ..."
& "$PSScriptRoot/../target/release/typings-generator.exe" `
    --input $winui --input $ie --root Microsoft.UI --out $OutFile

Write-Host "Done. Ensure it is referenced from packages/core/references.d.ts."
