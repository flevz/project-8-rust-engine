@echo off
cd /d "%~dp0"
if not exist "target\release\project8.exe" (echo The game is not built yet. Run BUILD.bat first. & pause & exit /b 1)
"target\release\project8.exe"
if errorlevel 1 pause
