#!/bin/bash
# macOS twin of INSPECT.bat. Run from the repo folder: the report is
# saved here.
cd "$(dirname "$0")" || exit 1
pause() { read -n 1 -s -r -p "Press any key to continue . . ."; echo; }
[ -x target/release/p8-inspect ] || { echo "Run BUILD.command first."; pause; exit 1; }
GAME="$1"
[ -n "$GAME" ] || read -r -p "Drag your installed Project 8 folder into this window and press Enter: " GAME
# A folder dragged into Terminal arrives as /path/with\ escaped\ spaces or
# 'quoted', usually with a trailing space: undo that. ~ means your home folder.
GAME="${GAME%"${GAME##*[![:space:]]}"}"
GAME="${GAME#"${GAME%%[![:space:]]*}"}"
case "$GAME" in \'*\') GAME="${GAME:1:${#GAME}-2}" ;; \"*\") GAME="${GAME:1:${#GAME}-2}" ;; *) GAME="$(printf '%s' "$GAME" | sed 's/\\\(.\)/\1/g')" ;; esac
# shellcheck disable=SC2088 # a literal ~ typed by the user, expanded here
case "$GAME" in "~") GAME="$HOME" ;; "~/"*) GAME="$HOME/${GAME#"~/"}" ;; esac
./target/release/p8-inspect "$GAME"
echo
echo "The report was saved as p8-inspect-report.txt in this folder. Send it to your helper."
pause
