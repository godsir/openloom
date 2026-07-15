# push.ps1 - unify version + bump patch + push
# Usage: .\scripts\push.ps1 [extra git push args]
[CmdletBinding()]
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$GitArgs
)

$ErrorActionPreference = "Stop"
$RootDir = Split-Path -Parent (Split-Path -Parent $PSCommandPath)
$CargoToml = Join-Path $RootDir "Cargo.toml"
$PkgJson = Join-Path $RootDir "frontend\package.json"

# --- read current version ---
$cargoContent = Get-Content $CargoToml -Raw -Encoding UTF8
$pkgContent = Get-Content $PkgJson -Raw -Encoding UTF8

$cargoMatch = [regex]::Match($cargoContent, 'version = "([^"]+)"')
$pkgMatch   = [regex]::Match($pkgContent, '"version": "([^"]+)"')

if (-not $cargoMatch.Success) { throw "Cannot read version from Cargo.toml" }
if (-not $pkgMatch.Success)   { throw "Cannot read version from package.json" }

$cargoVer = $cargoMatch.Groups[1].Value
$pkgVer   = $pkgMatch.Groups[1].Value

Write-Host "Current version: Cargo.toml=$cargoVer  package.json=$pkgVer"

# --- use the higher patch version as base, then +1 ---
$cargoPatch = [int]($cargoVer -split '\.')[-1]
$pkgPatch   = [int]($pkgVer -split '\.')[-1]

if ($cargoPatch -ge $pkgPatch) {
    $base = $cargoVer
} else {
    $base = $pkgVer
}

$parts = $base -split '\.'
$newVer = "$($parts[0]).$($parts[1]).$([int]$parts[2] + 1)"

Write-Host "New version: $newVer"

# --- write Cargo.toml ---
$cargoContent = $cargoContent -replace [regex]::Escape("version = `"$cargoVer`""), "version = `"$newVer`""
[System.IO.File]::WriteAllText($CargoToml, $cargoContent, [System.Text.UTF8Encoding]::new($false))
Write-Host "  Cargo.toml: $cargoVer -> $newVer"

# --- write frontend/package.json ---
$pkgContent = $pkgContent -replace [regex]::Escape("`"version`": `"$pkgVer`""), "`"version`": `"$newVer`""
[System.IO.File]::WriteAllText($PkgJson, $pkgContent, [System.Text.UTF8Encoding]::new($false))
Write-Host "  package.json: $pkgVer -> $newVer"

# --- commit version change ---
Push-Location $RootDir
try {
    git add Cargo.toml frontend/package.json
    git commit --amend --no-edit --no-verify

    Write-Host ""
    Write-Host "Version bumped to $newVer and committed, pushing..."
    Write-Host ""

    # --- push ---
    $pushArgs = @("push")
    if ($GitArgs) { $pushArgs += $GitArgs }
    git @pushArgs
} finally {
    Pop-Location
}
