$ErrorActionPreference = 'Stop'
$installerDir = Join-Path $PSScriptRoot '..\installer'
$url = 'https://github.com/bajins/NSIS_SetupSkin/archive/refs/heads/master.zip'
$zip = Join-Path $env:TEMP 'nsis-setupskin.zip'
$extractDir = Join-Path $env:TEMP 'nsis-setupskin'

Write-Host "Downloading bajins/NSIS_SetupSkin..."
Invoke-WebRequest -Uri $url -OutFile $zip -UseBasicParsing

Write-Host "Extracting..."
if (Test-Path $extractDir) { Remove-Item $extractDir -Recurse -Force }
Expand-Archive -Path $zip -DestinationPath $extractDir -Force

$src = Join-Path $extractDir 'NSIS_SetupSkin-master'
Write-Host "Copying toolchain to $installerDir..."
Copy-Item (Join-Path $src 'NSIS') $installerDir -Recurse -Force
Copy-Item (Join-Path $src '7z.dll'), (Join-Path $src '7z.exe') $installerDir -Force
Copy-Item (Join-Path $src 'OriginPlugin') $installerDir -Recurse -Force

Remove-Item $zip -Force
Remove-Item $extractDir -Recurse -Force
Write-Host "bajins toolchain fetched."
