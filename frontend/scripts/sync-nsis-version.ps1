# sync-nsis-version.ps1 - read package.json version and write it into info.nsi
$ErrorActionPreference = "Stop"
$pkg = Get-Content "frontend\package.json" -Raw | ConvertFrom-Json
$ver = $pkg.version

$nsisFile = "installer\SetupScripts\openloom\info.nsi"
$content = Get-Content $nsisFile -Raw -Encoding UTF8
$content = $content -replace 'PRODUCT_VERSION\s+"[^"]+"', "PRODUCT_VERSION       `"$ver.0`""
$content = $content -replace 'INSTALL_OUTPUT_NAME\s+"openLoom\.Setup\.[^"]+"', "INSTALL_OUTPUT_NAME    `"openLoom.Setup.$ver.exe`""
[System.IO.File]::WriteAllText($nsisFile, $content, [System.Text.UTF8Encoding]::new($false))
Write-Host "Synced NSIS version to $ver"
