# push.ps1 - 自动统一版本号 + bump patch + push
# 用法: .\scripts\push.ps1 [git push 的额外参数]
[CmdletBinding()]
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$GitArgs
)

$ErrorActionPreference = "Stop"
$RootDir = Split-Path -Parent (Split-Path -Parent $PSCommandPath)
$CargoToml = Join-Path $RootDir "Cargo.toml"
$PkgJson = Join-Path $RootDir "frontend\package.json"

# --- 读取当前版本 ---
$cargoContent = Get-Content $CargoToml -Raw -Encoding UTF8
$pkgContent = Get-Content $PkgJson -Raw -Encoding UTF8

$cargoMatch = [regex]::Match($cargoContent, 'version = "([^"]+)"')
$pkgMatch   = [regex]::Match($pkgContent, '"version": "([^"]+)"')

if (-not $cargoMatch.Success) { throw "无法从 Cargo.toml 读取版本" }
if (-not $pkgMatch.Success)   { throw "无法从 package.json 读取版本" }

$cargoVer = $cargoMatch.Groups[1].Value
$pkgVer   = $pkgMatch.Groups[1].Value

Write-Host "Current version: Cargo.toml=$cargoVer  package.json=$pkgVer"

# --- 取较高的 patch 版本作为基准，再 +1 ---
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

# --- 写入 Cargo.toml ---
$cargoContent = $cargoContent -replace [regex]::Escape("version = `"$cargoVer`""), "version = `"$newVer`""
[System.IO.File]::WriteAllText($CargoToml, $cargoContent, [System.Text.UTF8Encoding]::new($false))
Write-Host "  Cargo.toml: $cargoVer -> $newVer"

# --- 写入 frontend/package.json ---
$pkgContent = $pkgContent -replace [regex]::Escape("`"version`": `"$pkgVer`""), "`"version`": `"$newVer`""
[System.IO.File]::WriteAllText($PkgJson, $pkgContent, [System.Text.UTF8Encoding]::new($false))
Write-Host "  package.json: $pkgVer -> $newVer"

# --- 提交版本变更 ---
Push-Location $RootDir
try {
    git add Cargo.toml frontend/package.json
    git commit --amend --no-edit --no-verify

    Write-Host ""
    Write-Host "Version bumped to $newVer and committed, pushing..."
    Write-Host ""

    # --- Push ---
    $pushArgs = @("push")
    if ($GitArgs) { $pushArgs += $GitArgs }
    git @pushArgs
} finally {
    Pop-Location
}
