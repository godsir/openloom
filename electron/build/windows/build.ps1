$ErrorActionPreference = "Stop"
Write-Host "Building openLoom for Windows..."
Set-Location "$PSScriptRoot\..\..\.."
cargo build --release
Set-Location web; npm run build; Set-Location ..
Set-Location electron; npx electron-builder --win
Write-Host "Done: dist/openLoom-*.msi"
