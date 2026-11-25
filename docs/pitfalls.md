# Prism VoiceOpen — pitfalls

- **Whisper prompt with the command list = false triggers.** With the phrases as `prompt`, Whisper-small turned unclear
  audio into a command ("เสียงดังขึ้น" for TTS "เปิดยูทูป") and didn't improve accuracy in a 11-phrase test. No prompt now.
- **Voice samples confuse similar commands.** "ลดเสียง" was 0.50 from samples of "เพิ่มเสียง" (threshold 0.62) — shared
  "เสียง". Rule: voice alone acts only when d < 0.6×threshold; otherwise text decides, and the voice fallback is refused
  when the text's near-match is a different command (doing nothing beats doing the wrong thing).
- **Whisper small is weak for Thai** (esp. Windows TTS voice). Mitigations: fuzzy matcher, English aliases in starters
  ("เปิด Facebook"), history "add as words", teach by voice. Better model option in ideas.md.
- **Cross-script**: Whisper often writes brand names in English ("เปิด Facebook") — give commands both spellings.
- Overlay windows are excluded from capture — screenshots can't see them: use `PRISM_ACT=…,dump` (or `PRISM_NOEXCLUDE=1`).
  **Never screenshot the whole desktop to check them** — it captured the user's own windows / camera (LESSONS #4).
- The hidden host window must be a real (hidden) popup, not message-only: single-instance uses `FindWindow` on it.
- Talk key "hold" = `RegisterHotKey` for the press + `GetAsyncKeyState` polling for the release (the hotkey message has
  no key-up). Typing / key actions wait until modifiers are released (`wait_keys_up`) or Ctrl+letters would be sent.
- Bash tool + heredoc mangles `\` in regexes / CSS (`[^`\]`): write such files with the Write tool.
- `"$O\$3.png"` in bash produced a file literally named `test$3.png` — use `cygpath -m` and `/`.
- whisper.cpp model download can stop early without an error: check size (small-q8_0 = 264,464,607 bytes) — a short file
  fails with "not all tensors loaded". `stt::model()` ignores files < 20 MB.
- `ShellExecute "shell:AppsFolder\<AUMID>"` launches Store apps; classic apps from the AppsFolder have a real path.
- Start with Windows (`startup.rs`, same file in Rec + VoiceOpen): HKCU `Run` value + Task Manager's switch in
  `Explorer\StartupApproved\Run` (first byte odd = disabled by the user). Turning on deletes the StartupApproved value, or
  Windows keeps skipping us. At launch `startup::sync` follows Windows' real state and rewrites the path if the folder moved.
  Tests write the real Run key — note what was there first and delete our test values afterwards.
- **Low strictness = wrong command.** At strict 0.55 a long unrelated sentence containing a near-copy of a short phrase
  ran that command. Matcher now penalises phrases that cover < 40 % of the heard text; the slider warns below 65 %.
- **Never `unwrap()` a Mutex** — use `settings::lock()` (a panic on one thread poisons it and would kill every later call).
- **Never leave BUSY set**: `engine::begin` wraps the session in `catch_unwind`.
- Test runs read `%APPDATA%` — my shell sees the MSIX *virtual* copy, plain Python sees the user's real one. Check which
  before trusting a log; copy real files to `%USERPROFILE%/PrismVoiceBuild/real/` and only read them.
- `voice::prune` deletes `s*.json|wav` not referenced by any command / wake sample at start — never run it after a failed
  settings read (guarded by `settings::RECOVERED`).
- Whisper "hears" words in noise (Typhoon Turbo: pink noise → "สวัสดีครับ"; `no_speech_prob` is always 0 and
  `avg_logprob` looks confident, so neither can filter it) → fixed 2026-11-25 with Silero VAD in the GPU server + `clean()`.
  Don't add VAD to the old CPU build b5454: it garbled real speech.
- DTW alone is not a wake-word detector: music and ordinary talk score 0.82–0.99 against a 0.99 threshold. Keep step 2
  (Whisper text vs `wake_heard`). Whisper spells the user's "เฮ้ พริซึม" as "ให้ไปเสริม", "เอพิซึม", "เอฟิเซ็นต์"… —
  compare with those, not only with the phrase.
- Settings can list voice samples whose files are gone (11-20: the whole `voice\` folder emptied, cause unknown) — the
  wake loop logs "samples missing on disk". Before testing anything that writes `voice\`, back it up.
- Settings has no schema version.

- **Changed icon not in the exe** (2026-11-21): tauri-build does not rerun when `icons/icon.ico` changes → `touch app/src-tauri/build.rs` before building. Icons come from `../tools/brand` (see `../docs/brand.md`).
- **Classic look (light) shows white-on-white boxes** → old light styles paint wells with translucent white over glass; on a solid white panel they vanish → add the class to the `html[data-look=classic]:not([data-theme=dark])` list at the end of this app's CSS (`var(--c-fill)` / `var(--c-hover)`). Test hooks must press `#theme-pop [data-look=…]`, never `[data-look=…]` (`<html>` carries the same attribute).

- **GPU speech build (2026-11-23):** a child exe with a missing DLL shows a Windows error box and *keeps running*
  (looks like a server that never gets ready) → `SetErrorMode(SEM_FAILCRITICALERRORS)` before spawning + a 40 s
  "never ready" fallback to the CPU server. Typhoon Turbo on the CPU = ~8 s per command: never pair a big model with
  the CPU server (`stt::model_for(gpu)` picks largest for GPU, smallest for CPU). Vulkan build: `GGML_BACKEND_DL` needs
  `BUILD_SHARED_LIBS=ON`; `-static-libgcc` breaks the link (`_Unwind_Resume` twice) → ship WinLibs' libgcc_s_seh-1,
  libstdc++-6, libwinpthread-1, libgomp-1 **and libdl** (libgomp needs it — only noticed with a clean PATH).
  Git Bash `tar` can't open MSYS2 `.pkg.tar.zst` (no zstd) → use `C:\Windows\System32\tar.exe`.
  The first GPU job after loading is ~1.5 s slower (shader prep) → silent warm-up job; first load ever ≈ 9 s (cold disk).
- Windows TTS "ล็อกจอ" is heard as "รถจบ" and Thai-voice "เปิด Spotify" as "เปิ้ลสภาพที่ไฟ" even by Turbo — test-voice
  artefacts; real voices were not tested. Users can fix via history "add as words" / teach by voice.
- **Glow split into 4 windows = hard edges unless every window fades to 0 inside itself.** The glow thickness follows the
  voice, so at full volume its tail reached past the strip window and was clipped → visible rectangles. Any change to the
  glow look must keep `look()`'s fade (0 at the window edge) and the corner colour mix; check with
  `PRISM_ACT=lv:1,glow,wait:1200,dump` and look at the corners of `test\glow-*.png`.
