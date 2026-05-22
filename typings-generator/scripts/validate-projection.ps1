param(
    [string]$WorkspaceRoot = (Resolve-Path "$PSScriptRoot\..\..").Path
)

Set-Location $WorkspaceRoot

$ErrorActionPreference = 'Stop'

cargo check -p typings-generator

$outs = @(
    @{ Root = 'Windows.Foundation.Collections'; Out = 'typings-generator\_validate_collections.d.ts' },
    @{ Root = 'Windows.Foundation'; Out = 'typings-generator\_validate_foundation.d.ts' },
    @{ Root = 'Windows.UI'; Out = 'typings-generator\_validate_ui.d.ts' }
)

foreach ($item in $outs) {
    cargo run -p typings-generator --bin typings-generator -- --root $item.Root --out $item.Out | Out-Null
    if (-not (Test-Path $item.Out)) {
        throw "Missing output: $($item.Out)"
    }

    $content = Get-Content $item.Out -Raw
    if ($content -match '// No declarations were generated') {
        throw "No declarations generated for root $($item.Root)"
    }
}

Write-Host 'Projection validation passed.'
