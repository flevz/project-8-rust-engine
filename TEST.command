#!/bin/bash
# macOS twin of TEST.bat. The checklist is in TEST_CHECKLIST.txt (shared).
cd "$(dirname "$0")" || exit 1
export PATH="$HOME/.cargo/bin:$PATH"
pause() { read -n 1 -s -r -p "Press any key to continue . . ."; echo; }
command -v cargo >/dev/null 2>&1 || { echo "Rust is not installed. Double-click INSTALL.command for help."; pause; exit 1; }
echo "Building the game for testing..."
if ! cargo build --release -p p8-game --bins; then
    echo; echo "Build failed. Send the red text above to your helper."; pause; exit 1
fi
echo
echo "Starting."
cat TEST_CHECKLIST.txt
./target/release/project8 || pause
