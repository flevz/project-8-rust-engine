@echo off
setlocal
cd /d "%~dp0"
if not exist "target\release\p8-inspect.exe" (echo Run BUILD.bat first. & pause & exit /b 1)
set "GAME=%~1"
if "%GAME%"=="" set /p "GAME=Drag your installed Project 8 folder into this window and press Enter: "
set "GAME=%GAME:"=%"
"target\release\p8-inspect.exe" "%GAME%"
echo.
echo The report was saved as p8-inspect-report.txt in this folder. Send it to your helper.
pause
