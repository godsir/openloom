# sync-nsis-version.ps1 - read package.json version and write it into info.nsi
# Called from build-installer.bat which cd's to the frontend dir first.
$ErrorActionPreference = "Stop"
$pkg = Get-Content "package.json" -Raw | ConvertFrom-Json
$ver = $pkg.version

$nsisFile = "installer\SetupScripts\openloom\info.nsi"
$content = Get-Content $nsisFile -Raw -Encoding UTF8

# NSIS VIProductVersion must be purely numeric X.X.X.X.
# Convert semver pre-release tags (e.g. "0.4.30-beta.100" -> "0.4.30.100",
# "0.4.30" -> "0.4.30.0").
$nsisVer = $ver -replace '-beta\.', '.'
$parts = $nsisVer.Split('.')
if ($parts.Count -eq 3) {
    $nsisVer = "$nsisVer.0"
}

$content = $content -replace 'PRODUCT_VERSION\s+"[^"]+"', "PRODUCT_VERSION       `"$nsisVer`""
$content = $content -replace 'INSTALL_OUTPUT_NAME\s+"openLoom\.Setup\.[^"]+"', "INSTALL_OUTPUT_NAME    `"openLoom.Setup.$ver.exe`""
[System.IO.File]::WriteAllText($nsisFile, $content, [System.Text.UTF8Encoding]::new($false))
Write-Host "Synced NSIS version to $ver (NSIS VIProductVersion: $nsisVer)"
