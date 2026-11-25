# Prism VoiceOpen

**TH** — กดคีย์แล้วพูด เปิดโปรแกรม เว็บ โฟลเดอร์ หรือสั่งคอมได้เลย: กด **Ctrl+Space** แล้วพูด "เปิดยูทูป" ขอบจอจะเรืองแสงตามเสียง
แล้วสิ่งที่ตั้งไว้ก็ทำงาน ฟังเสียงแบบ **ออฟไลน์ 100%** (whisper.cpp บนการ์ดจอ + โมเดลภาษาไทย Typhoon Whisper) เข้าใจภาษาไทยแม้พูดไม่เป๊ะ
สอนคำด้วยเสียงตัวเองได้ มีคำปลุก ("เฮ้ พริซึม") และสร้างคำสั่งเองได้ไม่จำกัด (หลายอย่างในคำเดียว)

**EN** — Hold a key and talk to open apps, sites, folders or control your PC: press **Ctrl+Space**, say "open YouTube", the
screen edge glows with your voice and the command runs. Speech is recognised **100 % offline** (whisper.cpp on the graphics
card + the Thai-trained Typhoon Whisper model), forgiving with Thai, can learn your own voice, has a wake word, and you can
build unlimited commands (several actions per phrase).

คู่มือแบบชี้แล้วรู้ / Point-and-learn guide: https://pangpond543.github.io/prism/voiceopen.html

## ดาวน์โหลด / Download

ไปที่หน้า **[Releases](../../releases/latest)** แล้วเลือกไฟล์ใดไฟล์หนึ่ง (Windows 10/11, 64-bit, ~800 MB เพราะมีโมเดลฟังเสียงมาในตัว) /
Go to the **[Releases page](../../releases/latest)** and pick one (~800 MB — the speech models are included):

| ไฟล์ / File | ใช้เมื่อ / For |
| --- | --- |
| `Prism-VoiceOpen-x.y.z-Setup.exe` | **แนะนำ** — ตัวติดตั้ง มีไอคอนบนเดสก์ท็อปและเมนู Start · **Recommended** — installer with shortcuts |
| `Prism-VoiceOpen-x.y.z-Portable.zip` | ไม่ต้องติดตั้ง — แตกไฟล์แล้วดับเบิลคลิก `prism-voiceopen.exe` · No install — unzip and run |

> **Windows เตือน "ไม่ทราบผู้เผยแพร่" / SmartScreen warning** — กด **ข้อมูลเพิ่มเติม → เรียกใช้อยู่ดี** · Click **More info → Run anyway**.

การ์ดจอที่รองรับ Vulkan (AMD / NVIDIA / Intel) จะตอบเร็วที่สุด (~0.5 วินาที) ไม่มีก็ใช้ CPU ได้ ·
A Vulkan graphics card answers fastest (~0.5 s); without one it falls back to the CPU.
อัปเดต: ปุ่ม ⓘ → **ตรวจสอบอัปเดต** หรือผ่าน Prism Hub · Updates: ⓘ → **Check for updates**, or Prism Hub.

## ข้อมูลส่วนตัว / Privacy
เสียงของคุณไม่ออกจากเครื่อง ไม่มีบัญชี ไม่มีการเก็บสถิติ · Your voice never leaves your PC. No account, no telemetry.

## Build from source
Tauri 2 + Rust (GNU toolchain + WinLibs). `bash tools/build.sh` (exe → `app/bin`), `bash tools/build-whisper-vulkan.sh`
(GPU whisper.cpp). Release: `bash ../tools/release/tauri-release.sh prism-voiceopen "Prism VoiceOpen" prism-voiceopen PrismVoiceBuild "" --runtime prism-voiceopen/tools/stage-runtime.sh`.

## Licenses
MIT — © 2026 Thanakorn Damrikitjaroen. Made with Claude AI. Bundled: whisper.cpp (MIT), Whisper small (MIT, OpenAI),
Typhoon Whisper Turbo (MIT, SCB 10X), Silero VAD (MIT).
