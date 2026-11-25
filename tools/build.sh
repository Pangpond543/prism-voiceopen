#!/usr/bin/env bash
# Build Prism VoiceOpen and put the exe where the desktop shortcut points (app/bin). Dev helper — not for the user.
set -e
cd "$(dirname "$0")/.."
W=$(cygpath -u "$(ls -d "$LOCALAPPDATA/Microsoft/WinGet/Packages/"*WinLibs*/mingw64/bin | head -1)")
export PATH="$W:$HOME/.cargo/bin:$HOME/.rustup/toolchains/stable-x86_64-pc-windows-gnu/bin:$PATH"
export CARGO_TARGET_DIR="$USERPROFILE/PrismVoiceBuild/target"      # NEVER under AppData
cargo build --release --manifest-path app/src-tauri/Cargo.toml
taskkill //F //IM prism-voiceopen.exe 2>/dev/null || true
# only OUR whisper-server (Prism LLearn runs its own copy — never kill by name, LESSONS #10)
powershell -NoProfile -Command "Get-Process whisper-server -ErrorAction SilentlyContinue | Where-Object { \$_.Path -like '*prism-voiceopen*' } | Stop-Process -Force" 2>/dev/null || true
sleep 1
cp "$CARGO_TARGET_DIR/release/prism-voiceopen.exe" "$CARGO_TARGET_DIR/release/WebView2Loader.dll" app/bin/
ls -l app/bin/prism-voiceopen.exe
