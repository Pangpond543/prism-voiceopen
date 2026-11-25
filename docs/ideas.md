# Prism VoiceOpen — ideas

- 2026-11-18 — Thai-trained speech model **Thonburian Whisper medium** (`thonburian-medium-q5_0.bin`, 514 MB,
  huggingface.co/suebphatt/thonburian-whisper-ggml — community ggml conversion of biodatlab/whisper-th-medium-combined).
  Much better Thai than the bundled multilingual `ggml-small-q8_0.bin`, ~2× slower (≈1.5–2.5 s per command on the
  user's i3-12100F), ~700 MB RAM while loaded. — แทนที่แล้วด้วย Typhoon Whisper Turbo บนการ์ดจอ (2026-11-23).
  How to switch: drop the .bin into `app\bin\runtime\models\` — `stt::model()` picks the largest ggml-*.bin.
  Check `checksums.sha256` from the same repo first. Also: large-v3 Thai q5_0 (1031 MB) / distill-th-large-v3 q8 (834 MB) exist — too slow on CPU.
- 2026-11-18 — แต่ละคำใช้ได้เฉพาะบางแอป (คำเดียวกันทำต่างกันตามแอปที่เปิดอยู่) — ยังไม่ทำ (ผู้ใช้ไม่ได้เลือก)
- 2026-11-18 — ปุ่มข้างเมาส์ (XButton) เป็นปุ่มพูด — ยังไม่ทำ (ต้องใช้ low-level mouse hook)
- 2026-11-18 — Vulkan build of whisper.cpp to use the RX 6600 — ทำแล้ว 2026-11-23 (`tools/build-whisper-vulkan.sh`, MSYS2 Vulkan tools, no SDK install)
- 2026-11-23 — Two-step recognition: Turbo first, Typhoon Whisper Large v3 (~1.1 GB) only when the matcher is unsure — ยังไม่ทำ (ผู้ใช้เลือก Turbo ตัวเดียวก่อน; ทำถ้ายังพลาดบ่อย)
- 2026-11-23 — Typhoon ASR Real-time (115M FastConformer, Thai only, very light) — ไม่เอาตอนนี้ (Thai only, English words break; needs a different runtime)
