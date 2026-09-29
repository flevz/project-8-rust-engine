@echo off
setlocal
cd /d "%~dp0"
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
where cargo >nul 2>nul || (echo Rust is not installed. Get it from https://rustup.rs and restart your PC. & pause & exit /b 1)
echo Building the game (retail skater camera, spacewalk fix)...
cargo build --release -p p8-game --bins
if errorlevel 1 (echo. & echo Build failed. Send the red text above to your helper. & pause & exit /b 1)
echo.
echo Starting. Things to look for:
echo  1. Riding: camera about 2.5 m behind and a little above, following the direction you roll, not the board.
echo  2. Turning: the camera swings round smoothly and a little late.
echo  3. Ollie: the camera rises a bit while in the air and settles after landing.
echo  4. Vert air: the camera rises and looks down at the skater, then swings back behind after landing.
echo  5. Standing still: the camera stays behind where the skater faces.
echo  6. Wider view than before (81 degrees across).
echo Spacewalk (manual, then Left, Right, Square): the hips stay lined up with the board as it starts, no swing out and back.
echo Animation: smooth, without the small hitch about every half second.
echo Not done yet: grinds, lip tricks, bails, wallrides, right-stick look-around, camera going through walls.
"target\release\project8.exe"
if errorlevel 1 pause
