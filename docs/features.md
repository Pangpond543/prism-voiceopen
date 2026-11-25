# Prism VoiceOpen — features

## Talking to it
- Global talk key (default **Ctrl+Space**, change it on the home card / Settings / Shortcuts).
  - **Both** (default): hold = listen while held, release = run; a quick tap (< 0.35 s) = listen until you stop talking.
  - **Hold** only / **Tap** only (tap again = done talking).
- Mic button on the home card and "พูดเลย" in the title bar; tray menu "พูดคำสั่ง".
- Esc cancels while listening (registered only during the session). Max listen 5 / 8 / 12 / 20 s.
- Soft generated sound cues (open / understood / not understood / cancel) — can be turned off.
- Wake word (off by default): teach it 3× by voice, then switch on; listens with energy VAD + DTW (no speech-to-text
  until it hears the wake word). Windows shows the mic as in use while it is on. Sensitivity slider.
  Step 2 (2026-11-25): the piece that matched is also run through Whisper and must look like the wake word
  (`matcher::wake_score` ≥ 0.5 vs the phrase + Whisper's spelling of each wake sample, `settings.wake_heard`, learned in
  the background after teaching / at start). Glow + chime appear only after that check (≤ 2.5 s); a false wake-up is
  ignored silently. Pieces much shorter / longer than the samples are skipped.
- Noise never becomes words: Silero VAD in the GPU whisper-server (`stt::vad_args`, `PRISM_NOVAD=1` turns it off for
  tests) + filler filter `stt::clean` (ติดตามต่อไป, ขอบคุณที่รับชม, [เสียงดนตรี] …).

## Screen-edge glow + caption
- 4 click-through strips per monitor, flowing gradient (Prism / aurora / sunset / ocean / soft white), thickness follows
  the voice level; "thinking" flows faster; green sweep when understood; amber when not. All monitors or only the one with
  the mouse; size slider; preview in Settings ("ดูของจริงบนจอ"). Hidden from screen capture / recordings.
  The 4 strips read as one ring: the glow fades to exactly 0 before each strip's inner edge (strips are 1.4× the glow
  size), and the corners mix both edges' glow and colour into a rounded curve — no square blocks even at full volume.
- Glass caption bottom-centre: listening (+ live level bars + hint), thinking (bouncing dots), result (command name +
  what it heard / the search words), or why it failed (heard text + "close to …"). Can be turned off.

## Recognition (offline)
- whisper.cpp server, Thai by default; English / auto selectable — started when the mic opens, kept loaded for 10 min
  (or always / 2 min).
  - **Graphics card (default):** `runtime\gpu\` (Vulkan build, any AMD / NVIDIA / Intel card) + the largest model =
    **Typhoon Whisper Turbo q5_0** (Thai-trained large-v3-turbo, 574 MB). ~0.4–0.9 s per command on the user's RX 6600,
    ~400–450 MB RAM (the model sits in the card's memory). A silent warm-up job runs right after loading so the first
    real command isn't 1.5 s slower.
  - **CPU fallback:** if the GPU server crashes / never gets ready / its files are missing → CPU server + the smallest
    model (`ggml-small-q8_0`, ~1.8 s) for the rest of that run. No error box is shown (`SetErrorMode`). Turbo on the
    CPU was ~8 s, so it is never used there. `PRISM_CPU=1` forces the CPU path (test).
  - Settings → speech engine shows the model + "runs on the graphics card / CPU"; Home shows "· GPU / · CPU".
- Matcher: tone marks / silent marks dropped, sound-alike consonants and long/short vowels merged, approximate substring
  search — "ช่วยเปิดยูทูบหน่อยครับ" finds "เปิดยูทูป"; strictness slider (default 72 %).
- Teach by voice: record 2–3 samples per command → MFCC + DTW; a very close match runs instantly without Whisper.
  Voice alone never overrides words that point to a different command.
- Search commands: lead-in words + whatever follows is the query ("หายูทูป แมวน่ารัก" → YouTube search "แมวน่ารัก").

## Commands (home page)
- Cards: real app icon, name, what it does, the phrases, voice-sample badge, use count, on/off switch, run now, edit,
  duplicate, delete (with Undo toast), right-click menu, double-click = edit, search (Ctrl+F), add (Ctrl+N / dock / card).
- Starter set on first run: YouTube, search YouTube/Google, Facebook, Downloads folder, Calculator, volume up/down/mute,
  play/pause, next track, lock, screenshot, Prism Rec record/replay (if Prism Rec is installed).
- Editor: name (auto-filled), phrase chips (Enter / comma / Backspace), actions in order (move up, try one, remove),
  kinds: program (Start-menu picker with icons + search, or pick an .exe), web link, search (12 presets or custom `{q}`),
  folder, file, PC control (volume, media, lock, snip, desktop, task view, close window, Explorer, Task Manager, Settings),
  Prism app (Rec record/pause/replay/draw, open Size/Cam/Player/Studio/Rec), type text, press keys, wait.
  Drag & drop programs/files/folders/.url onto the editor; Ctrl+V a link; Ctrl+S / Ctrl+Enter save; Esc asks before
  throwing away changes; teach-by-voice samples (play back / delete).

## History
- Last 80 recognitions: heard text, result, how (text / voice), time, latency. "เพิ่มเป็นคำของคำสั่ง…" adds a misheard text
  as a phrase of any command (fixes recurring mishearings in one click). Run again. Clear.
- The home card shows the last result for an hour (with the same "add as words" button when it failed).

## Settings
Key + mode + max listen + strictness · microphone + volume + live mic test · language + keep-loaded + engine status /
load now + teach-by-voice tolerance · glow colour/size/all monitors/caption/preview · wake word · sounds · start with
Windows (`--tray`) · bring back starter commands.

## Shell
Prism suite title bar (TH/EN, appearance menu on the moon/sun button (Light/Dark + Glass/Classic; Classic = flat, still, no refraction), About/updater — repo empty), dock, tray (left click = window; right click =
speak / open / quit), X hides the window, single instance (2nd launch opens the window), extra global keys:
open window (Ctrl+Alt+O), repeat last command (none by default).

## Start with Windows
- First open: dialog asks "ให้ Prism VoiceOpen เปิดพร้อม Windows ไหม?" — Start with Windows / No thanks / Ask me later.
- Settings → General: checkbox + "ถามอีกครั้ง"; home quick chip too. Starts hidden in the tray (`--tray`).
- Follows Task Manager → Startup apps: switched off there = shown off here; turning on here re-enables it.

## Safety nets (2026-11-20)
- Broken `settings.json` → recovered field by field, bad copy kept next to it. A crash inside a listening session shows a
  caption and the talk key keeps working. Strictness below 65 % shows a warning. Speech engine stuck > 30 s → restarted.
