#!/bin/bash
# macOS twin of PLAY.bat.
cd "$(dirname "$0")" || exit 1
pause() { read -n 1 -s -r -p "Press any key to continue . . ."; echo; }
[ -x target/release/project8 ] || { echo "The game is not built yet. Run BUILD.command first."; pause; exit 1; }
./target/release/project8 || pause
