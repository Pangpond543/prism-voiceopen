# Prism VoiceOpen — changelog

- 2026-11-26 — **v1.1.0, first GitHub release** (`Pangpond543/prism-voiceopen`): Setup + Portable (~800 MB, speech runtime + models inside) built by `../tools/release/tauri-release.sh` + `tools/stage-runtime.sh`; `app/prism.config.json` repo set (updater on). Packaged exe checked: GPU server + Typhoon + "Open YouTube" understood.

- 2026-11-25 — **No more "heard" words from noise, and the wake word stops waking up by itself.** From the user's real log /
  history (copied out via schtasks): Whisper turned noise into "โปรดติดตามต่อไป" / "สวัสดีครับ", and on 11-19 the wake word
  woke the app 114 times from music and talk (voice-shape distance 0.82–0.99 under a 0.99 threshold).
  (1) Silero voice detection in the GPU server (`--vad`, `runtime\models\ggml-silero-v6.2.0.bin`, 885 KB, same file as
  Prism LLearn): pink / white noise, a tone and music → "" in ~50 ms; the user's 10 taught commands give the same words as
  before, clips with silence get faster. Not on the old CPU build (with VAD it made up a sentence for "เปิด Facebook").
  (2) Filler filter `stt::clean` on the sound skeleton (ติดตามต่อไป, ขอบคุณที่รับชม, กดไลค์ …; unit test `stt::fillers`).
  (3) Wake word step 2: after the voice match, Whisper must also hear something that looks like the wake word
  (`matcher::wake_score` vs the phrase + Whisper's own spelling of each wake sample, learned once in the background →
  `settings.wake_heard`). The mic records at once, but the glow + chime wait for the check (max 2.5 s); a false wake-up
  ends silently. Also rejects pieces much shorter / longer than the user's samples. Threshold 0.5 from real data: own
  samples ≥ 0.55, every false wake-up in the log ≤ 0.43 (unit test `matcher::wake_words`).
  (4) **Data found missing:** the user's real `voice\` folder had been empty since 2026-11-20 23:52 (220 taught samples —
  13 wake + command voices — gone, settings still listing them), so the wake word and teach-by-voice silently did
  nothing for 5 days. Restored all 220 from `%USERPROFILE%\PrismVoiceBuild
ealackupoice` (copy made 11-20 23:48);
  the wake loop now logs "N of M samples missing on disk". `tools/build.sh` now stops only our own whisper-server
  (it killed every `whisper-server.exe`, incl. Prism LLearn's).
  Verified in appin\prism-voiceopen.exe (23:47 build, sandbox data, PRISM_DRYRUN): learnwake, wakecheck (own sample yes;
  noise / music / "หยุดเพลง" / "Facebook" / "Open YouTube" no), wakesim (real wake → "หยุดเพลง" runs; noise or
  "เพลงถัดไป" as wake → ignored quietly). Not verified: a live wake-up from the user's own mic.

- 2026-11-24 — **Screen-edge glow is one smooth ring.** When the voice was loud, the glow grew past its 4 strip windows
  and was cut off at their inner edge, so it looked like 4 separate rectangles; the corners also took only the top/bottom
  edge's colour. Now each strip is 1.4× the glow size and the glow S-curves down to exactly 0 inside it, and the corners
  blend both edges' glow + colour (rounded corner, no seam). Checked with the new test hook `lv:1` (pretend full volume) +
  `dump`, which now saves the whole ring of the first monitor instead of the top strip only. — user: "ตอนแสงเข้ม
  เห็นเป็น 4 เหลี่ยม สีแยกกันแต่ละมุม ดูไม่กลมกลืน".

- 2026-11-23 — **Better speech model on the graphics card.** Added Typhoon Whisper Turbo q5_0 (SCB 10X's Thai fine-tune of
  Whisper large-v3-turbo, MIT; ggml file from huggingface.co/JoaoZaokk/typhoon-whisper-turbo-ggml, SHA-256 `2ffbb167…`,
  same file in 3 independent repos) + a Vulkan build of whisper.cpp v1.9.5 in `runtime\gpu\` (`tools/build-whisper-vulkan.sh`).
  Test (27 TTS clips, Thai + English): small/CPU 1.8 s, Thai mostly wrong (เพิ่มเสียง→"เพิ่มสิ่ง", หยุดเพลง→"หยุดกลิ่ม");
  Turbo/GPU 0.4 s, 11/14 Thai and 8/8 English exact; Turbo/CPU 8 s (so CPU keeps small). q5_0 ≈ q8_0 in accuracy → q5_0.
  Falls back to CPU + small by itself if the card's server fails; Windows error boxes from the child are suppressed.
  — user wanted better Thai+English recognition without a heavier PC.

- 2026-11-23 — **Appearance menu + Classic look** (suite-wide, design approved on Prism Size). The moon/sun button opens a small menu: Theme (Light / Dark) and Style (Glass / Classic). Classic = flat solid panels on a plain background (Discord-like greys), no moving background, no refraction / blur / glints, no decorative looping animation or hover bounce; status indicators still move. — user wanted a calm, use-focused mode.
- 2026-11-23 — Title-bar logo: outer square swings again (with the other two) and the glyph is ~25 % smaller. — user review.
- 2026-11-22 — **Logo v2** (user review): app icon = the layered tile at every size (no plain small version) with a bigger glyph; title-bar logo = squares + glyph with no tile, 26 → 44 px, outer square still. — user wanted the big icon everywhere and a much bigger logo in the bar.
- 2026-11-21 — **Own logo + "by 543" + update nudge** (suite-wide). Logo keeps the Prism family shape but now has this app's colour and a white microphone glyph (title bar, About, exe/taskbar icon; generated from `tools/brand/`). Faint "by 543" after the name in the title bar (the user's personal mark). Quiet update check 4 s after start (+ every 6 h): newer version → pink dot with ripple on the Info button and a glass chip "New version vX · Update" sliding out once per version; clicking opens Info already on "Update now". Test hook: `updfake`. — user: logos all looked the same, wanted 543 visible, and an eye-catching update hint.

## 2026-11-20 — bug hunt (v1.0.0, same version number)
Found by reading the code + replaying the user's real log / history (80 recognitions) through the matcher.
- 🔴 A panic inside one listening session used to leave "busy" on forever (talk key dead until restart) — now caught,
  glow hidden, a "Something went wrong" caption shown.
- 🔴 `settings.json` that did not parse as a whole made the app start with *no commands*. Now: copy kept as
  `settings.json.bad-<time>`, fields / commands recovered one by one; saves are serialised (one writer); all `Mutex`
  locks tolerate a poisoned lock (`settings::lock`).
- 🟠 False trigger from the user's log: "สปอติฟไฟฟังสิ่งเริ่มเป็นข้าไว้" (they said Spotify) started a screen recording via
  "เริ่มอัด" at 0.58. A short phrase found in a long sentence now loses score (matcher `cover`). Unit test added; a
  `replay` test (`--ignored`, `PVO_SETTINGS` / `PVO_HISTORY`) re-runs a saved history.
- 🟠 Mic unplugged / disabled while capturing: the capture loop now errors out (it used to deliver silence forever, the
  wake word stayed "live" but deaf); wake word shows "not live". Failure caption mentions Windows microphone privacy.
- 🟠 whisper-server that dies while loading: one restart on a new port (was: wait 40 s). A request with no answer after
  20 s (was 60 s) restarts the server instead of freezing every later command.
- 🟠 Re-assigning a hotkey then hiding the window / switching language left the global keys switched off — now re-enabled.
- 🟡 Undo after deleting a command now brings its taught voice samples back (files are removed at next start by
  `voice::prune`, skipped if settings had to be recovered).
- 🟡 Glow thickness / caption size use each monitor's own DPI. "Repeat last" ignored while listening and for disabled
  commands. Warning under the strictness slider below 65 % (the user had 55 %). NaN-safe sorting.
- Not done: see the list at the bottom of pitfalls.md.

## 2026-11-20 — first-run "start with Windows?" question
- User asked: on first open, ask whether VoiceOpen should start with Windows, keep it in Settings, and be able to ask again.
- New glass dialog on first open: Start with Windows / No thanks / Ask me later (Esc or click outside = later, asks next open).
  Settings → General: new "ถามอีกครั้ง" button reopens it. Not shown when already on, or during `PRISM_ACT` test runs
  (act `startup` opens it on purpose).
- Added on my own: registry code moved to `startup.rs` (shared with Prism Rec); turning on also clears a "disabled" mark
  from Task Manager → Startup apps; at launch the setting follows Windows' real state (off in Task Manager = off here) and
  the Run path is fixed if the folder moved.

## 2026-11-18 — v1.0.0 (new app)
- New Prism app requested by the user: background voice launcher, user-set key to open the mic, configurable words →
  open programs / links / anything, glowing screen edge while listening, keeps running in the tray when X is pressed
  (like Prism Rec). Name chosen by the user: "Prism VoiceOpen".
- Picked by the user: hold-or-tap key (both modes), search by voice, one word → several actions, PC / Prism-app
  commands, teach by voice, wake word (off by default).
- Speech: offline whisper.cpp (b5454 CPU build) + ggml-small-q8_0 (user chose small over the 514 MB Thai model for now).
- Added on my own: starter commands, Start-menu app picker with icons, drag & drop / paste links into the editor,
  history with one-click "add what it heard as a phrase", live level glow + caption, sound cues, mic test, Esc cancel,
  undo delete, duplicate, on/off per command, repeat-last hotkey, open-window hotkey, start with Windows, glow palettes,
  engine idle unload, dry-run + test hooks.
- Safety found while testing: removed Whisper command prompt (false triggers); voice samples can't override different words.
