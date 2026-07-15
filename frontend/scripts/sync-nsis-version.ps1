# sync-nsis-version.ps1 - read package.json version and write it into info.nsi
# Called from build-installer.bat which cd's to the frontend dir first.
#
# 重要：本脚本必须保存为 UTF-8 with BOM。PowerShell 5.1 对无 BOM 的文件用系统
# 码页（GBK）解析，会导致下方中文注释的 UTF-8 字节被误解析，在注释行末产生
# 类似反引号行续接的效果，把后续代码行吞进注释，使 $content 读到空值，进而
# 把空内容写回 info.nsi 清空文件。
$ErrorActionPreference = "Stop"
$pkg = Get-Content "package.json" -Raw | ConvertFrom-Json
$ver = $pkg.version

$nsisFile = [System.IO.Path]::GetFullPath([System.IO.Path]::Combine($PSScriptRoot, "..\installer\SetupScripts\openloom\info.nsi"))

# info.nsi 是 UTF-16 LE BOM（与 ui.nsh/commonfunc.nsh 一致），读写都必须用
# Unicode（UTF-16 LE）编码，否则会把文件转成 UTF-8 破坏 makensis 的 BOM 识别。
$content = [System.IO.File]::ReadAllText($nsisFile, [System.Text.Encoding]::Unicode)

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
[System.IO.File]::WriteAllText($nsisFile, $content, [System.Text.Encoding]::Unicode)
Write-Host "Synced NSIS version to $ver (NSIS VIProductVersion: $nsisVer)"
