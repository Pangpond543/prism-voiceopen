#!/usr/bin/env bash
# Copies the speech runtime next to the exe for a release (called by ../tools/release/tauri-release.sh with the target
# folder): whisper.cpp CPU build + runtime\gpu (Vulkan build) + models (small q8 = CPU, Typhoon Turbo q5 = GPU, Silero VAD).
set -euo pipefail
OUT="$1"
mkdir -p "$OUT"
cp -r app/bin/runtime/. "$OUT/"
rm -f "$OUT/whisper-cli.exe" "$OUT/gpu/whisper-cli.exe"   # the app only uses whisper-server
du -sh "$OUT"
