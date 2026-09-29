@echo off
setlocal
cd /d "%~dp0"
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
echo Checking what the Project 8 Rust Engine needs to build...
echo.
set "MISSING="
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
set "VCTOOLS="
if exist "%VSWHERE%" for /f "usebackq delims=" %%i in (`"%VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set "VCTOOLS=%%i"
if defined VCTOOLS (
    echo [OK] C++ build tools: found.
) else (
    set "MISSING=1"
    echo [MISSING] C++ build tools. Rust needs these to make the game program.
    echo    1. Open https://visualstudio.microsoft.com/visual-cpp-build-tools/
    echo    2. Download and run "Build Tools for Visual Studio".
    echo    3. Tick "Desktop development with C++" and click Install.
    echo    If you install Rust first, its installer offers to do this for you.
)
echo.
where cargo >nul 2>nul
if errorlevel 1 (
    set "MISSING=1"
    echo [MISSING] Rust. This is what builds the game.
    echo    1. Open https://rustup.rs and download rustup-init.exe.
    echo    2. Run it and press Enter to accept the defaults.
    echo    3. Restart your PC when it finishes.
) else (
    echo [OK] Rust: found.
    cargo --version
)
echo.
if defined MISSING (
    echo Install what is missing, then double-click INSTALL.bat again to check.
) else (
    echo Everything is ready. Next: double-click BUILD.bat.
)
pause
