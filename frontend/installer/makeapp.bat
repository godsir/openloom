cd /d %~dp0

if exist ".\SetupScripts\app.7z" del /f ".\SetupScripts\app.7z"

rem ����app.7z��ʼ
.\7z.exe a ".\SetupScripts\app.7z" ".\FilesToInstall\*.*"

@set DestPath=%cd%\FilesToInstall\
@echo off& setlocal EnableDelayedExpansion

for /f "delims=" %%a in ('dir /ad/b %DestPath%') do (
.\7z.exe a ".\SetupScripts\app.7z" ".\FilesToInstall\%%a"
@echo "compressing .\FilesToInstall\%%a"
)

rem ����app.7z����