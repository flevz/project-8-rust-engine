@echo off
setlocal
cd /d "%~dp0"
if not exist "target\release\p8-setup.exe" (echo Run BUILD.bat first. & pause & exit /b 1)
set "GAME=%~1"
if "%GAME%"=="" set /p "GAME=Drag your installed Project 8 game folder into this window and press Enter: "
set "GAME=%GAME:"=%"
"target\release\p8-setup.exe" "%GAME%"
echo.
echo If it says "written to tuning.json", PLAY.bat now runs the translated Project 8 physics with your own game scripts.
pause
