#!/bin/bash
# Makes the two player downloads from the committed files (run from anywhere):
#   bash scripts/package.sh [output folder, default dist] [commit, default HEAD]
#   Project8-Windows.zip  everything except the macOS .command launchers
#   Project8-macOS.zip    everything except the Windows .bat launchers
# The shared parts (source, TEST_CHECKLIST.txt, README) are in both.
# git archive keeps the .command files' "can run" mark and line endings.
set -euo pipefail
cd "$(dirname "$0")/.."
OUT="${1:-dist}"
REV="${2:-HEAD}"
mkdir -p "$OUT"
git archive --format=zip --prefix=Project8-Windows/ -o "$OUT/Project8-Windows.zip" "$REV" \
    -- . ':(exclude)*.command' ':(exclude).github'
git archive --format=zip --prefix=Project8-macOS/ -o "$OUT/Project8-macOS.zip" "$REV" \
    -- . ':(exclude)*.bat' ':(exclude).github'
ls -l "$OUT"/Project8-*.zip
