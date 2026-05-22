$errors = $null
$tokens = $null
[void][System.Management.Automation.Language.Parser]::ParseFile('.\run-testapp-cli.ps1',[ref]$tokens,[ref]$errors)
if ($errors -and $errors.Count -gt 0) {
    $errors | Format-List
    exit 1
} else {
    Write-Host 'Parse OK'
}
