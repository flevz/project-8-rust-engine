@echo off
setlocal
cd /d "%~dp0"
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
where cargo >nul 2>nul || (echo Rust is not installed. Get it from https://rustup.rs and restart your PC. & pause & exit /b 1)
echo Building Project 8 Rust Engine. The first build takes several minutes...
cargo build --release -p p8-game -p p8-formats --bins
if errorlevel 1 (echo. & echo Build failed. Send the red text above to your helper. & pause & exit /b 1)
echo.
echo Done. Run SETUP.bat once with your installed game folder, then PLAY.bat.
pause
