#!/bin/bash
# macOS twin of BUILD.bat.
cd "$(dirname "$0")" || exit 1
export PATH="$HOME/.cargo/bin:$PATH"
pause() { read -n 1 -s -r -p "Press any key to continue . . ."; echo; }
command -v cargo >/dev/null 2>&1 || { echo "Rust is not installed. Double-click INSTALL.command for help."; pause; exit 1; }
echo "Building Project 8 Rust Engine. The first build takes several minutes..."
if ! cargo build --release -p p8-game -p p8-formats --bins; then
    echo; echo "Build failed. Send the red text above to your helper."; pause; exit 1
fi
echo
echo "Done. Run SETUP.command once with your installed game folder, then PLAY.command."
pause
