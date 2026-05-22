<#
.SYNOPSIS
  Build, publish, install and run TestApp via CLI for local testing.

.DESCRIPTION
  - Default mode `exe` publishes and launches the produced `TestApp.exe` from the publish folder.
  - Mode `msix` tries to register the publish layout (no signing). If that fails it will pack, sign
    (self-signed cert) and install the resulting .appx, then launch the installed UWP.

USAGE
  PowerShell -ExecutionPolicy Bypass -File .\scripts\run-testapp-cli.ps1 -Mode exe
  PowerShell -ExecutionPolicy Bypass -File .\scripts\run-testapp-cli.ps1 -Mode msix
#>

param(
    [ValidateSet('exe','msix')]
    [string]$Mode = 'exe',
    [string]$Configuration = 'Release',
    [string]$Platform = 'x64',
    [string]$PublishProfile = 'win-x64.pubxml',
    [int]$TimeoutSeconds = 60
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Definition
$repoRoot = Resolve-Path (Join-Path $scriptRoot '..')
$projDir = Join-Path $repoRoot 'TestApp'
$projFile = Join-Path $projDir 'TestApp.csproj'

Write-Host "Repo: $repoRoot"
Write-Host "Project: $projFile"
Write-Host "Mode: $Mode, Configuration: $Configuration, Platform: $Platform"

Write-Host "Publishing TestApp..."
dotnet msbuild $projFile -t:Publish -p:Configuration=$Configuration -p:Platform=$Platform -p:PublishProfile=$PublishProfile

# Find publish dir (try to find folder that contains TestApp.exe under bin)
$publishDir = $null
$testAppFile = Get-ChildItem -Path (Join-Path $projDir 'bin') -Filter 'TestApp.exe' -Recurse -File -ErrorAction SilentlyContinue | Select-Object -First 1
if ($testAppFile) {
    $publishDir = Split-Path -Parent $testAppFile.FullName
    if ((Split-Path $publishDir -Leaf) -ne 'publish') {
        $possible = Join-Path $publishDir 'publish'
        if (Test-Path $possible) { $publishDir = $possible }
    }
}

if (-not $publishDir) {
    # best-effort fallback (matches current project TF and layout)
    $fw = 'net10.0-windows10.0.26100.0'
    $candidate = Join-Path $projDir ("bin\$Configuration\$fw\$Platform\publish")
    if (Test-Path $candidate) { $publishDir = $candidate } else { Write-Error "Publish directory not found"; exit 1 }
}

Write-Host "Publish dir: $publishDir"

if ($Mode -eq 'exe') {
    $exe = Join-Path $publishDir 'TestApp.exe'
    if (-not (Test-Path $exe)) { Write-Error "Executable not found at $exe"; exit 1 }
    Write-Host "Launching EXE: $exe"
    Start-Process -FilePath $exe -WorkingDirectory $publishDir

    # Tail runtime trace log in system temp
    $log = Join-Path $env:TEMP 'ns_trace.log'
    Write-Host "Tailing runtime trace log at: $log"
    $wait = $TimeoutSeconds
    while (-not (Test-Path $log) -and $wait -gt 0) { Start-Sleep -Seconds 1; $wait-- }
    if (Test-Path $log) {
        Get-Content -Path $log -Wait -Tail 0
    } else {
        Write-Warning "Trace log did not appear within timeout ($TimeoutSeconds s)."
    }
    exit 0
}

# Mode = msix: prepare manifest and assets
$manifestSrc = Join-Path $projDir 'Package.appxmanifest'
$manifestDst = Join-Path $publishDir 'AppxManifest.xml'
Copy-Item -Path $manifestSrc -Destination $manifestDst -Force
(Get-Content $manifestDst) -replace '\$targetnametoken\$','TestApp' | Set-Content $manifestDst

$assetsSrc = Join-Path $projDir 'Assets'
$assetsDst = Join-Path $publishDir 'Assets'
if (Test-Path $assetsSrc) {
    Copy-Item -Path (Join-Path $assetsSrc '*') -Destination $assetsDst -Recurse -Force -ErrorAction SilentlyContinue
} else {
    New-Item -ItemType Directory -Path $assetsDst -Force | Out-Null
    foreach ($f in 'StoreLogo.png','Square150x150Logo.png','Square44x44Logo.png','Wide310x150Logo.png','SplashScreen.png') {
        $path = Join-Path $assetsDst $f
        if (-not (Test-Path $path)) { New-Item -Path $path -ItemType File -Force | Out-Null }
    }
}

# Try registering layout (no signing required)
Write-Host "Attempting Add-AppxPackage -Register from layout..."
try {
    Add-AppxPackage -ForceApplicationShutdown -Register -Path $manifestDst -ErrorAction Stop
    Write-Host "Registered from layout successfully."
} catch {
    Write-Warning "Register failed: $($_.Exception.Message)"
    Write-Host "Falling back to pack -> sign -> install flow..."

    $package = Join-Path (Split-Path $publishDir -Parent) 'TestApp.appx'

    # find makeappx.exe
    $makeappx = (Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin' -Recurse -Filter makeappx.exe -ErrorAction SilentlyContinue | Select-Object -Last 1).FullName
    if (-not $makeappx) { Write-Error "makeappx.exe not found in Windows Kits"; exit 1 }
    & $makeappx pack /d $publishDir /p $package

    # create self-signed cert (try legacy provider for SignTool compatibility)
    $cert = New-SelfSignedCertificate -Type Custom -Subject 'CN=TestAppCert' -KeySpec Signature -KeyExportPolicy Exportable -HashAlgorithm SHA256 -KeyLength 2048 -CertStoreLocation 'Cert:\CurrentUser\My' -Provider 'Microsoft Enhanced RSA and AES Cryptographic Provider'
    $pwd = ConvertTo-SecureString -String 'testpassword' -AsPlainText -Force
    $pfx = Join-Path $repoRoot 'TestAppCert.pfx'
    Export-PfxCertificate -Cert "Cert:\CurrentUser\My\$($cert.Thumbprint)" -FilePath $pfx -Password $pwd | Out-Null

    $signtool = (Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin' -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue | Select-Object -Last 1).FullName
    if (-not $signtool) { Write-Error "signtool.exe not found in Windows Kits"; exit 1 }

    & $signtool sign /fd SHA256 /f $pfx /p testpassword $package
    Add-AppxPackage -Path $package -ForceApplicationShutdown
}

# Determine PackageFamilyName from manifest
[xml]$xml = Get-Content $manifestDst
$appName = $xml.Package.Identity.Name
Write-Host "Package Identity Name: $appName"

$pfn = (Get-AppxPackage | Where-Object { $_.Name -eq $appName -or $_.PackageFullName -like "*$appName*" } | Select-Object -First 1).PackageFamilyName
if (-not $pfn) { $pfn = $appName }
Write-Host "Launching UWP: $pfn!App"
Start-Process -FilePath 'explorer.exe' -ArgumentList "shell:AppsFolder\$pfn!App"

# Tail container trace log (UWP)
$log = Join-Path $env:LOCALAPPDATA "Packages\$pfn\AC\Temp\ns_trace.log"
Write-Host "Tailing runtime trace log at: $log"
$wait = 30
while (-not (Test-Path $log) -and $wait -gt 0) { Start-Sleep -Seconds 1; $wait-- }
if (Test-Path $log) { Get-Content -Path $log -Wait -Tail 0 } else { Write-Warning "Trace log not found." }
