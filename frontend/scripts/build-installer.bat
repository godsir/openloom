@echo off
chcp 65001 >nul
cd /d "%~dp0\.."

echo [0/5] fetch bajins toolchain...
powershell -ExecutionPolicy Bypass -File scripts\fetch-bajins.ps1
if errorlevel 1 exit /b 1

echo [1/5] copy EnVar plugin...
if not exist installer\NSIS\Plugins mkdir installer\NSIS\Plugins
copy /y resources\x86-unicode\EnVar.dll installer\NSIS\Plugins\EnVar.dll >nul

echo [2/5] electron-builder packing win-unpacked...
call node_modules\.bin\electron-builder.cmd --win --dir
if errorlevel 1 exit /b 1

echo [3/5] copy win-unpacked to FilesToInstall...
if exist "installer\FilesToInstall" rmdir /s /q "installer\FilesToInstall"
mkdir "installer\FilesToInstall"
xcopy /e /i /y "dist\win-unpacked\*" "installer\FilesToInstall\"

echo [4/5] nsNiuniuSkin packing installer...
cd installer\SetupScripts\openloom
call .\build.bat
if errorlevel 1 ( cd /d "%~dp0\.." & exit /b 1 )
cd /d "%~dp0\.."

echo [5/5] generate latest.yml...
node scripts\gen-latest-yml.js
