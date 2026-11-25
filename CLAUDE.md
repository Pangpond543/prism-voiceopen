# Prism VoiceOpen — notes for Claude

Background voice launcher: press (hold or tap) a global key, the screen edge glows, say a word → the programs, links,
folders, searches, PC controls or Prism-app actions the user set up run. Lives in the tray (X = hide, like Prism Rec).
Built 2026-11-18 at the user's request. Fully offline (bundled whisper.cpp + "teach by voice" templates).

Read first: `../CLAUDE.md` (suite rules) and `../docs/LESSONS.md`.
More detail: `docs/features.md`, `docs/pitfalls.md`, `docs/changelog.md`, `docs/ideas.md`.

## What the user opens
Desktop `Prism VoiceOpen.lnk` → `prism-voiceopen\app\bin\prism-voiceopen.exe`, with `app\bin\runtime\` beside it:
`whisper-server.exe` + `whisper.dll` + `ggml*.dll` (whisper.cpp release b5454, `whisper-bin-x64.zip`, CPU) and
`runtime\models\ggml-small-q8_0.bin` (264 MB, CPU) + `ggml-typhoon-turbo-q5_0.bin` (574 MB, Thai-trained, GPU), and
`runtime\gpu\` = whisper.cpp v1.9.5 built with Vulkan (rebuild: `bash tools/build-whisper-vulkan.sh`). GPU server + largest
model by default; CPU server + smallest model if the GPU one fails (`stt::model_for`). The page (`app/renderer`) is embedded in the exe.

## Build
`bash tools/build.sh` (Git Bash, from `prism-voiceopen/`) — WinLibs + GNU Rust, target `%USERPROFILE%\PrismVoiceBuild\target`,
kills the running app + whisper-server, copies the exe into `app\bin`. Check `ls -l app/bin/prism-voiceopen.exe` afterwards.
Unit test for the word matcher: `cargo test --release --manifest-path app/src-tauri/Cargo.toml matcher` (same env as build.sh).

## Layout (`app/src-tauri/src`)
- `main.rs` — Tauri shell (Prism IPC names: `win:min` → `win_min`), commands for settings / commands / training /
  history / pickers, tray, autostart (`startup.rs`: HKCU Run `--tray` + Task Manager sync, first-run question), `http://pvo.localhost/icon|voice/…`, single instance, test hooks.
- `overlay.rs` — native thread: hidden host window (`PrismVoiceHost`) owns the global hotkeys; hold detection polls
  `GetAsyncKeyState`; glow = 4 click-through layered strips per monitor (tiny-skia pixels, ~40 fps only while visible);
  glass caption bottom-centre. All excluded from capture (`WDA_EXCLUDEFROMCAPTURE`).
- `engine.rs` — one session at a time: mic → VAD → voice templates (instant) → Whisper → matcher → actions. History,
  "repeat last", wake-word loop.
- `stt.rs` — whisper-server child process (job object kills it with us), warm on mic open, idle unload (`engine_keep`).
- `voice.rs` — MFCC + DTW "teach by voice" samples (`%APPDATA%\com.prism.voiceopen\voice\*.json|wav`).
- `matcher.rs` — Thai-aware fuzzy match (tone marks dropped, sound-alike letters merged, approximate substring).
- `actions.rs` — app / url / search / folder / file / system / prism / type / keys / wait + starter commands.
- `apps.rs` — Start-menu apps from the shell AppsFolder (classic + Store) with real icons (PNG cache).
- `audio.rs` — WASAPI mic → 16 kHz mono. `sounds.rs` — generated cue tones. `gfx.rs`/`update.rs` — copied from Prism Rec.
- Settings: `%APPDATA%\com.prism.voiceopen\settings.json`; log `%LOCALAPPDATA%\com.prism.voiceopen\prism.log`.

## Test hooks
`PRISM_ACT` (comma list). Page: `en` `th` `dark` `light` `info` `startup` `view:history|settings|keys` `new` `edit:<n>` `picker`
`mictest` `scroll:<px>`. Native: `warm` (load engine), `wav:<file>` (say this file instead of the mic, full pipeline),
`inj:<file>` + `press:<ms>` (real key events: hold / tap test), `train:<cmd id|name|__wake>:<file>`, `wakeprobe:<file>`,
`glow` `think` `ok` `fail` `hide` `lv:<0..1>` (pretend voice loudness) `dump` (whole glow ring + caption → `%APPDATA%\com.prism.voiceopen\test\*.png`), `idle`, `wait:<ms>`,
`set:<key>=<json>`, `quit`. **`PRISM_DRYRUN=1` = actions are logged, not run** — always use it when testing so nothing
opens or changes on the user's PC. `PRISM_NOEXCLUDE=1` = overlay visible to screen capture.
Thai test speech: `%USERPROFILE%\PrismVoiceBuild\test\tts.ps1` / `tts2.ps1` (Windows "Pattara" voice; reads list.txt / list2.txt).
Window screenshots: `shot.ps1` (PrintWindow of our window only — never capture the whole desktop, LESSONS #4).

## Rules specific to this app
- Thai strings: page `` T`…` `` → English in `app/renderer/js/lang-en.js`; native strings use `settings::tr(th, en)`.
  `extract.py` in the test folder lists page strings that have no English yet.
- Never put the command list into Whisper's prompt (it then "hears" commands nobody said — pitfalls).
- A voice-sample match only acts alone when it is very close; otherwise the text decides (pitfalls).
- `PKG_NAME` in main.rs must equal `app/package.json` "name" (`prism-voiceopen`). Repo in `prism.config.json` is empty.
