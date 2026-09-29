#!/bin/bash
# macOS twin of INSTALL.bat: checks what the engine needs to build.
cd "$(dirname "$0")" || exit 1
export PATH="$HOME/.cargo/bin:$PATH"
pause() { read -n 1 -s -r -p "Press any key to continue . . ."; echo; }

echo "Checking what the Project 8 Rust Engine needs to build..."
echo
MISSING=""
if xcode-select -p >/dev/null 2>&1; then
    echo "[OK] Apple's Command Line Tools: found."
else
    MISSING=1
    echo "[MISSING] Apple's Command Line Tools. Rust needs these to make the game program."
    echo "   1. Type y and press Enter below. Apple's installer window opens."
    echo "   2. Click Install (not Get Xcode) and wait for it to finish."
    echo "   (Or type this in Terminal yourself: xcode-select --install)"
    read -r -p "Open Apple's installer now? (y/n) " ANSWER
    case "$ANSWER" in [yY]*) xcode-select --install ;; esac
fi
echo
if command -v cargo >/dev/null 2>&1; then
    echo "[OK] Rust: found."
    cargo --version
else
    MISSING=1
    echo "[MISSING] Rust. This is what builds the game."
    echo "   Type y and press Enter below to install it now, then accept the"
    echo "   default choice (press Enter) when it asks."
    echo "   (Or paste this into Terminal yourself:"
    echo "    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh )"
    read -r -p "Install Rust now? (y/n) " ANSWER
    case "$ANSWER" in
        [yY]*) curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh ;;
    esac
fi
echo
if [ -n "$MISSING" ]; then
    echo "Install what is missing, then double-click INSTALL.command again to check."
else
    echo "Everything is ready. Next: double-click BUILD.command."
fi
pause
