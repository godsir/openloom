@echo off
chcp 65001 >nul
cd /d %~dp0

@call ..\..\makeapp.bat
cd /d %~dp0
@call ..\..\makeskinzip.bat openloom
cd /d %~dp0

if not exist "..\..\..\dist" md "..\..\..\dist"
"..\..\NSIS\makensis.exe" ".\info.nsi"

if exist ".\skin.zip" del /f ".\skin.zip"
if exist "..\app.nsh" del /f "..\app.nsh"
if exist "..\app.7z" del /f "..\app.7z"
