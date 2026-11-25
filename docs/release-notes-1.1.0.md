## Prism VoiceOpen 1.1.0 — first release on GitHub

### ภาษาไทย

กดคีย์แล้วพูด เปิดโปรแกรม เว็บ หรือสั่งคอมได้เลย — ฟังเสียงในเครื่อง 100% ไม่ส่งเสียงขึ้นเน็ต

**ใช้งาน**
- กด **Ctrl+Space** ค้างไว้แล้วพูด (หรือแตะครั้งเดียวแล้วพูดจนจบ) ขอบจอเรืองแสงตามเสียง เข้าใจแล้วเป็นสีเขียว ไม่เข้าใจเป็นสีส้ม
- คำสั่งเริ่มต้นพร้อมใช้: เปิด YouTube / Google / Facebook ค้นหาด้วยเสียง ("หายูทูป แมวน่ารัก") ปรับเสียง ล็อกจอ แคปจอ เพลงถัดไป สั่ง Prism Rec ฯลฯ
- สร้างคำสั่งเอง: คำเดียวทำได้หลายอย่างตามลำดับ — เปิดโปรแกรม ลิงก์ โฟลเดอร์ ไฟล์ ค้นหา ควบคุมเครื่อง พิมพ์ข้อความ กดคีย์ รอ
- เข้าใจภาษาไทยแม้พูดไม่เป๊ะหรือพูดเป็นประโยคยาว · สอนด้วยเสียงตัวเอง 2–3 ครั้ง · ประวัติว่าได้ยินอะไร + "เพิ่มเป็นคำของคำสั่ง"
- คำปลุก (เช่น "เฮ้ พริซึม") ไม่ต้องกดคีย์ · อยู่ในถาดข้างนาฬิกา เปิดพร้อม Windows ได้

**ฟังเสียงในเครื่อง**
- whisper.cpp บนการ์ดจอ (Vulkan) + โมเดลภาษาไทย Typhoon Whisper Turbo ตอบประมาณ 0.5–1 วินาที ถ้าการ์ดจอใช้ไม่ได้จะสลับไปใช้ CPU ให้เอง

**ใหม่ในเวอร์ชันนี้**
- **เสียงรบกวนไม่กลายเป็นคำอีกแล้ว** — มีตัวกรองเสียงพูด (Silero VAD) ก่อนส่งให้ AI ฟัง เสียงเพลง เสียงพัดลม เสียงคีย์บอร์ด ไม่ถูกฟังเป็น "สวัสดีครับ" หรือ "โปรดติดตามต่อไป" อีก และคำสั่งที่มีช่วงเงียบตอบเร็วขึ้น
- **คำปลุกไม่ตื่นเองแล้ว** — เดิมเสียงเพลงหรือคนคุยกันทำให้โปรแกรมตื่นเองบ่อย ตอนนี้ตรวจสองชั้น: เสียงต้องคล้ายเสียงที่คุณสอนไว้ และ AI ต้องได้ยินคำที่คล้ายคำปลุกด้วย ถ้าไม่ใช่จะเงียบไปเฉยๆ ไม่มีแสงหรือเสียงเตือนขึ้นมา
- ถ้าไฟล์เสียงที่สอนไว้หายไป โปรแกรมจะบันทึกไว้ให้รู้ (เดิมเงียบไปเฉยๆ)

**วิธีติดตั้ง**
- `Prism-VoiceOpen-1.1.0-Setup.exe` (แนะนำ) หรือ `Prism-VoiceOpen-1.1.0-Portable.zip` (แตกไฟล์แล้วใช้ได้เลย) — ไฟล์ใหญ่ราว 800 MB เพราะมีโมเดลฟังเสียงมาในตัว
- Windows อาจขึ้น "Windows protected your PC" เพราะโปรแกรมยังไม่มีลายเซ็นดิจิทัล กด "More info" → "Run anyway"

---

### English

Hold a key and talk to open apps, sites or control your PC — speech recognition runs 100 % on your PC.

**Using it**
- Hold **Ctrl+Space** and talk (or tap once and talk until you stop). The screen edge glows with your voice: green = understood, amber = not.
- Ready-made commands: open YouTube / Google / Facebook, voice search ("search YouTube cute cats"), volume, lock, snip, next song, drive Prism Rec, and more.
- Build your own: one phrase can run several steps — open an app, link, folder, file, search, PC control, type text, press keys, wait.
- Forgiving Thai matching, even in long sentences · teach a phrase with your own voice (2–3 times) · history of what it heard + "add as a phrase".
- Wake word (e.g. "เฮ้ พริซึม") — no key needed · lives in the tray, can start with Windows.

**Offline speech**
- whisper.cpp on the graphics card (Vulkan) + the Thai-trained Typhoon Whisper Turbo model, about 0.5–1 s per command; falls back to the CPU by itself if the card can't be used.

**New in this version**
- **Noise no longer turns into words** — a speech detector (Silero VAD) runs before the AI listens, so music, a fan or typing are no longer heard as "สวัสดีครับ" or "โปรดติดตามต่อไป", and commands with pauses answer faster.
- **The wake word no longer wakes up by itself** — music or people talking used to wake it often. Now it checks twice: the sound must be close to the samples you taught, and the AI must hear words that look like the wake word. If not, it stays quiet — no glow, no sound.
- If your taught voice samples go missing, the app now logs it (it used to fail silently).

**Install**
- `Prism-VoiceOpen-1.1.0-Setup.exe` (recommended) or `Prism-VoiceOpen-1.1.0-Portable.zip` (unzip and run) — about 800 MB because the speech models are included.
- Windows may say "Windows protected your PC" because the app is not code-signed yet: "More info" → "Run anyway".
