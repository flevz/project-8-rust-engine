@echo off
setlocal
cd /d "%~dp0"
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
where cargo >nul 2>nul || (echo Rust is not installed. Run INSTALL.bat for help. & pause & exit /b 1)
echo Building the game for testing...
cargo build --release -p p8-game --bins
if errorlevel 1 (echo. & echo Build failed. Send the red text above to your helper. & pause & exit /b 1)
echo.
echo Starting.
type TEST_CHECKLIST.txt
"target\release\project8.exe"
if errorlevel 1 pause
