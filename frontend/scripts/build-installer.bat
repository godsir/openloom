@echo off
chcp 65001 >nul
cd /d "%~dp0\.."

echo [0/6] sync version to NSIS info.nsi...
powershell -ExecutionPolicy Bypass -File scripts\sync-nsis-version.ps1
if errorlevel 1 exit /b 1

echo [1/6] fetch bajins toolchain...
powershell -ExecutionPolicy Bypass -File scripts\fetch-bajins.ps1
if errorlevel 1 exit /b 1

echo [2/6] copy EnVar plugin...
if not exist installer\NSIS\Plugins mkdir installer\NSIS\Plugins
copy /y resources\x86-unicode\EnVar.dll installer\NSIS\Plugins\EnVar.dll >nul

echo [3/6] electron-builder packing win-unpacked...
call node_modules\.bin\electron-builder.cmd --win --dir
if errorlevel 1 exit /b 1

echo [4/6] copy win-unpacked to FilesToInstall...
if exist "installer\FilesToInstall" rmdir /s /q "installer\FilesToInstall"
mkdir "installer\FilesToInstall"
xcopy /e /i /y "dist\win-unpacked\*" "installer\FilesToInstall\"

echo [5/6] nsNiuniuSkin packing installer...
cd installer\SetupScripts\openloom
call .\build.bat
if errorlevel 1 ( cd /d "%~dp0\.." & exit /b 1 )
cd /d "%~dp0\.."

echo [6/6] generate latest.yml...
node scripts\gen-latest-yml.js
