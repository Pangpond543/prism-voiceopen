// Speech → text, fully offline: a bundled whisper.cpp server (runtime\whisper-server.exe + models\*.bin) runs as a
// hidden child process on 127.0.0.1 and keeps the model in memory, so each command takes ~1 s instead of reloading.
// It is started when the mic opens (it loads while the user is still speaking) and stopped after the idle time the
// user chose. A job object kills it if Prism VoiceOpen dies.
use crate::settings;
use serde_json::Value;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const NO_WINDOW: u32 = 0x0800_0000;

struct Srv { child: std::process::Child, port: u16, started: Instant, gpu: bool }
/// Set when the graphics-card server crashed: the rest of this run uses the CPU server.
static GPU_BAD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
fn srv() -> &'static Mutex<Option<Srv>> { static S: OnceLock<Mutex<Option<Srv>>> = OnceLock::new(); S.get_or_init(|| Mutex::new(None)) }
fn last_use() -> &'static Mutex<Instant> { static L: OnceLock<Mutex<Instant>> = OnceLock::new(); L.get_or_init(|| Mutex::new(Instant::now())) }

pub fn runtime_dir() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    let d = exe.parent().map(|p| p.join("runtime")).unwrap_or_default();
    if d.join("whisper-server.exe").exists() { return d; }
    // dev build in target\release → the app's bin\runtime
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().map(|p| p.join("bin").join("runtime")).unwrap_or_default();
    if src.join("whisper-server.exe").exists() { src } else { d }
}
/// The model file from runtime\models\ggml-*.bin: on the graphics card the **largest** one (Typhoon Turbo, ~0.4 s per
/// command on an RX 6600), on the CPU the **smallest** one (small: ~1.8 s on an i3; Turbo on the CPU took ~8 s).
pub fn model_for(gpu: bool) -> Option<PathBuf> {
    let dir = runtime_dir().join("models");
    let mut best: Option<(u64, PathBuf)> = None;
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        let n = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        if !n.starts_with("ggml-") || !n.ends_with(".bin") { continue; }
        let sz = e.metadata().map(|m| m.len()).unwrap_or(0);
        if sz < 20_000_000 { continue; } // half-downloaded / VAD models
        if best.as_ref().map_or(true, |b| if gpu { sz > b.0 } else { sz < b.0 }) { best = Some((sz, p)); }
    }
    best.map(|b| b.1)
}
pub fn model() -> Option<PathBuf> { model_for(device() == "gpu") }
pub fn available() -> bool { runtime_dir().join("whisper-server.exe").exists() && model_for(false).is_some() }
/// runtime\gpu\ = whisper.cpp built with Vulkan (any AMD / NVIDIA / Intel card). Used unless it crashed this run
/// or PRISM_CPU=1 (test switch). If the PC has no usable card it still works — ggml then runs on the CPU.
fn gpu_dir() -> PathBuf { runtime_dir().join("gpu") }
fn use_gpu() -> bool {
    !GPU_BAD.load(std::sync::atomic::Ordering::Relaxed) && std::env::var("PRISM_CPU").map_or(true, |v| v != "1") && gpu_dir().join("whisper-server.exe").exists()
}
/// "gpu" / "cpu" — what the running server uses, or what the next one will use.
pub fn device() -> &'static str {
    let g = crate::settings::lock(srv()).as_ref().map(|s| s.gpu);
    if g.unwrap_or_else(use_gpu) { "gpu" } else { "cpu" }
}
pub fn model_name() -> String { model().and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string())).unwrap_or_default() }

/// Silero voice detection (runtime\models\ggml-silero-*.bin, < 1 MB): the server drops everything that is not speech
/// before Whisper sees it, so music / fan / keyboard noise no longer comes back as "สวัสดีครับ" or "โปรดติดตามต่อไป"
/// (measured 2026-11-25: 4 kinds of noise → "" in ~50 ms; the user's 10 taught commands → same words). GPU build
/// (whisper.cpp 1.9.5) only: the old CPU build b5454 + VAD turned "เปิด Facebook" into a made-up sentence.
pub fn vad_model() -> Option<PathBuf> {
    std::fs::read_dir(runtime_dir().join("models")).ok()?.flatten().map(|e| e.path())
        .find(|p| p.file_name().map_or(false, |n| { let n = n.to_string_lossy().to_lowercase(); n.starts_with("ggml-silero") && n.ends_with(".bin") }))
}
fn vad_args(gpu: bool) -> Vec<std::ffi::OsString> {
    if !gpu || std::env::var("PRISM_NOVAD").map_or(false, |v| v == "1") { return Vec::new(); }
    match vad_model() { Some(m) => vec!["--vad".into(), "-vm".into(), m.into_os_string()], None => Vec::new() }
}

fn free_port() -> u16 { std::net::TcpListener::bind("127.0.0.1:0").ok().and_then(|l| l.local_addr().ok()).map(|a| a.port()).unwrap_or(18765) }
fn threads() -> usize { std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 8) }

/// Make sure the server is running (non-blocking: returns immediately, the model loads in the background).
pub fn warm() {
    *crate::settings::lock(last_use()) = Instant::now();
    let mut g = crate::settings::lock(srv());
    if let Some(s) = g.as_mut() {
        if s.child.try_wait().ok().flatten().is_none() { return; }
        if s.gpu && s.started.elapsed() < Duration::from_secs(60) {
            // the graphics-card build died soon after start (missing file, old driver …): CPU for the rest of this run
            GPU_BAD.store(true, std::sync::atomic::Ordering::Relaxed);
            settings::log("stt: gpu server exited early — using the CPU one");
        } else { settings::log("stt: server had exited, restarting"); }
        *g = None;
    }
    let gpu = use_gpu();
    let Some(model) = model_for(gpu) else { settings::log("stt: no model"); return };
    let dir = if gpu { gpu_dir() } else { runtime_dir() };
    let port = free_port();
    let st = settings::get();
    let lang = if st.stt_lang == "auto" { "auto".to_string() } else { st.stt_lang.clone() };
    let t = if gpu { threads().min(4) } else { threads() }; // the card does the heavy part
    // a missing / broken DLL must not pop a Windows error box on the user's screen (the child inherits this): it just exits
    unsafe { use windows::Win32::System::Diagnostics::Debug::*; SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOOPENFILEERRORBOX); }
    let mut cmd = std::process::Command::new(dir.join("whisper-server.exe"));
    cmd.current_dir(&dir)
        .args(["-m"]).arg(&model)
        .args(["--host", "127.0.0.1", "--port", &port.to_string(), "-t", &t.to_string(), "-l", &lang, "-nt", "-bs", "3", "-bo", "3", "-ac", "768"]) // 768 = 15 s window: ~40% faster, commands are short
        .args(vad_args(gpu))
        .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
        .creation_flags(NO_WINDOW);
    match cmd.spawn() {
        Ok(child) => {
            unsafe { kill_with_us(&child); }
            settings::log(&format!("stt: server pid={} port={port} device={} model={}", child.id(), if gpu { "gpu" } else { "cpu" }, model.display()));
            *g = Some(Srv { child, port, started: Instant::now(), gpu });
            if gpu {
                // the card's first job is ~1.5 s slower (it prepares its programs): do it on silence while the user speaks
                std::thread::spawn(move || {
                    let t0 = Instant::now();
                    while !ready_port(port) { if t0.elapsed() > Duration::from_secs(40) { return; } std::thread::sleep(Duration::from_millis(80)); }
                    let r = post(port, &crate::audio::wav_bytes(&vec![0.0f32; 8000]), &[("response_format", "json".into()), ("temperature", "0.0".into())]);
                    settings::log(&format!("stt: gpu warm-up {} ms ({})", t0.elapsed().as_millis(), if r.is_ok() { "ok" } else { "failed" }));
                });
            }
        }
        Err(e) if gpu => {
            settings::log(&format!("stt: gpu server spawn failed: {e} — using the CPU one"));
            GPU_BAD.store(true, std::sync::atomic::Ordering::Relaxed);
            drop(g);
            warm();
        }
        Err(e) => settings::log(&format!("stt: spawn failed: {e}")),
    }
}
unsafe fn kill_with_us(child: &std::process::Child) {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::JobObjects::*;
    static JOB: OnceLock<isize> = OnceLock::new();
    let job = *JOB.get_or_init(|| {
        let j = CreateJobObjectW(None, None).unwrap_or_default();
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let _ = SetInformationJobObject(j, JobObjectExtendedLimitInformation, &info as *const _ as _, std::mem::size_of_val(&info) as u32);
        j.0 as isize
    });
    let _ = AssignProcessToJobObject(HANDLE(job as _), HANDLE(child.as_raw_handle() as _));
}
pub fn stop() {
    if let Some(mut s) = crate::settings::lock(srv()).take() { let _ = s.child.kill(); let _ = s.child.wait(); settings::log("stt: server stopped"); }
}
pub fn running() -> bool { crate::settings::lock(srv()).as_ref().map_or(false, |s| ready_port(s.port)) }
fn ready_port(port: u16) -> bool { TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(150)).is_ok() }

/// Idle watchdog: unload the model when it has not been used for the chosen time.
pub fn watchdog() {
    std::thread::spawn(|| loop {
        std::thread::sleep(Duration::from_secs(20));
        let keep = settings::get().engine_keep;
        let mins: u64 = match keep.as_str() { "always" => continue, k => k.parse().unwrap_or(10) };
        let idle = crate::settings::lock(last_use()).elapsed();
        if idle > Duration::from_secs(mins * 60) && crate::settings::lock(srv()).is_some() { stop(); }
    });
}

/// Transcribe 16 kHz mono samples. `prompt` = the command words (helps Whisper spell them the way the user wrote them).
pub fn transcribe(samples: &[f32], prompt: &str) -> Result<String, String> {
    warm();
    let t0 = Instant::now();
    // wait for the server to finish loading the model
    let mut restarts = 0;
    let port = loop {
        let p = { let mut g = crate::settings::lock(srv()); g.as_mut().map(|s| (s.port, s.started, s.child.try_wait().ok().flatten(), s.gpu)) };
        let Some((port, started, exited, gpu)) = p else { return Err("noengine".into()) };
        if let (Some(code), true) = (exited, gpu) {
            // the graphics-card build died (old driver, card busy, no Vulkan …): fall back to the CPU build for this run
            settings::log(&format!("stt: gpu server exited while loading ({code}) — using the CPU one"));
            GPU_BAD.store(true, std::sync::atomic::Ordering::Relaxed);
            stop();
            warm();
            continue;
        }
        if let Some(code) = exited {
            // died while loading (port taken by another program, broken model …): one fresh start on a new port
            settings::log(&format!("stt: server exited while loading ({code})"));
            if restarts >= 1 { stop(); return Err("engineexited".into()); }
            restarts += 1;
            warm();
            continue;
        }
        if ready_port(port) { break port; }
        if started.elapsed() > Duration::from_secs(40) {
            stop();
            if gpu && restarts == 0 {
                settings::log("stt: gpu server never got ready — using the CPU one");
                GPU_BAD.store(true, std::sync::atomic::Ordering::Relaxed);
                restarts += 1;
                warm();
                continue;
            }
            return Err("enginetimeout".into());
        }
        std::thread::sleep(Duration::from_millis(60));
    };
    let wait_ms = t0.elapsed().as_millis();
    let wav = crate::audio::wav_bytes(samples);
    let st = settings::get();
    let lang = if st.stt_lang == "auto" { "auto" } else { st.stt_lang.as_str() };
    let mut fields: Vec<(&str, String)> = vec![("response_format", "json".into()), ("temperature", "0.0".into()), ("language", lang.into())];
    if !prompt.is_empty() { fields.push(("prompt", prompt.to_string())); }
    let r = post(port, &wav, &fields);
    // no answer in time = the server is stuck (or the PC was very busy): drop it so the next try starts a clean one
    if let Err(e) = &r { if e.starts_with("recv") || e.starts_with("send") || e.starts_with("connect") { settings::log(&format!("stt: restarting the server after: {e}")); stop(); } }
    *crate::settings::lock(last_use()) = Instant::now();
    let body = r?;
    let v: Value = serde_json::from_str(&body).map_err(|_| format!("badjson {}", body.chars().take(120).collect::<String>()))?;
    if let Some(e) = v["error"].as_str() { return Err(e.to_string()); }
    let text = v["text"].as_str().unwrap_or("").trim().to_string();
    settings::log(&format!("stt: {:.2}s audio → {:?} (wait {wait_ms} ms, total {} ms)", samples.len() as f32 / crate::audio::RATE as f32, text, t0.elapsed().as_millis()));
    Ok(clean(&text))
}
/// Whisper sometimes returns its hallucination fillers on silence / noise (YouTube-style endings it learned from
/// subtitles). Compared on the matcher's sound skeleton (no tone marks, spaces, punctuation; polite ending dropped).
pub fn clean(t: &str) -> String {
    let t = t.trim().trim_matches(|c: char| c == '"' || c == '“' || c == '”').trim();
    let mut s = t.replace("[BLANK_AUDIO]", "").trim().to_string();
    let wrapped = |a: char, b: char| s.starts_with(a) && s.ends_with(b);
    if wrapped('[', ']') || wrapped('(', ')') || wrapped('*', '*') || wrapped('♪', '♪') { s.clear(); }
    let sk = |x: &str| crate::matcher::norm(x).0.into_iter().collect::<String>();
    let full = sk(&s);
    let mut k = full.clone();
    for p in ["ครับผม", "ครับ", "ค่ะ", "นะคะ", "นะครับ", "จ้า"] { let p = sk(p); if k.len() > p.len() { if let Some(r) = k.strip_suffix(p.as_str()) { k = r.to_string(); } } }
    // the whole text is a filler
    const EXACT: [&str; 13] = ["ขอบคุณ", "ขอบคุณมาก", "ขอบคุณครับ", "ขอบคุณค่ะ", "สวัสดีครับ", "สวัสดีค่ะ", "เสียงดนตรี", "music", "thank you", "thanks", "you", "bye", "..."];
    // fillers that are never part of a command
    const PART: [&str; 13] = ["ติดตามต่อไป", "ติดตามตอนต่อไป", "ติดตามชม", "ขอบคุณที่รับชม", "ขอบคุณที่ติดตาม", "ขอบคุณที่ชม", "กดไลค์",
        "กดติดตาม", "กดซับ", "subscribe", "for watching", "subtitles by", "คำบรรยายโดย"];
    if k.is_empty() || EXACT.iter().any(|e| k == sk(e) || full == sk(e)) || PART.iter().any(|p| k.contains(&sk(p))) {
        if !s.is_empty() { settings::log(&format!("stt: dropped filler {s:?}")); }
        return String::new();
    }
    s
}

fn post(port: u16, wav: &[u8], fields: &[(&str, String)]) -> Result<String, String> {
    let boundary = format!("----prismvoice{}", settings::now_ms());
    let mut body: Vec<u8> = Vec::with_capacity(wav.len() + 1024);
    for (k, v) in fields {
        body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{k}\"\r\n\r\n{v}\r\n").as_bytes());
    }
    body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"say.wav\"\r\nContent-Type: audio/wav\r\n\r\n").as_bytes());
    body.extend_from_slice(wav);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let mut s = TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_secs(2)).map_err(|e| format!("connect {e}"))?;
    let _ = s.set_read_timeout(Some(Duration::from_secs(30))); // a command is ~1-3 s (first one ~15 s on a busy PC); 60 s looked like a frozen app
    let head = format!("POST /inference HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: multipart/form-data; boundary={boundary}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
    s.write_all(head.as_bytes()).and_then(|_| s.write_all(&body)).map_err(|e| format!("send {e}"))?;
    let mut resp = Vec::new();
    s.read_to_end(&mut resp).map_err(|e| format!("recv {e}"))?;
    let split = resp.windows(4).position(|w| w == b"\r\n\r\n").ok_or("noheader")?;
    let h = String::from_utf8_lossy(&resp[..split]).to_string();
    let b = &resp[split + 4..];
    if !h.starts_with("HTTP/1.1 200") && !h.starts_with("HTTP/1.0 200") { return Err(format!("http {}", h.lines().next().unwrap_or(""))); }
    if h.to_ascii_lowercase().contains("transfer-encoding: chunked") {
        // de-chunk on bytes (a chunk may end inside a Thai character)
        let mut out = Vec::new();
        let mut i = 0;
        while let Some(e) = b[i..].windows(2).position(|w| w == b"\r\n") {
            let n = usize::from_str_radix(String::from_utf8_lossy(&b[i..i + e]).trim(), 16).unwrap_or(0);
            let start = i + e + 2;
            if n == 0 || start + n > b.len() { break; }
            out.extend_from_slice(&b[start..start + n]);
            i = start + n + 2;
            if i >= b.len() { break; }
        }
        return Ok(String::from_utf8_lossy(&out).to_string());
    }
    Ok(String::from_utf8_lossy(b).to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn fillers() {
        for t in ["โปรดติดตามต่อไป", "โปรดติดตามตอนต่อไปนะครับ", "[เสียงดนตรี]", "(เสียงดนตรี)", "ขอบคุณที่รับชมครับ", "สวัสดีครับ", "Thank you.", "Thanks for watching!", "...", "กดไลค์ กดแชร์ กดติดตาม", "♪♪♪"] {
            assert_eq!(super::clean(t), "", "{t}");
        }
        for t in ["เปิดยูทูป", "ค้นหา ขอบคุณ ภาษาอังกฤษ", "หยุดเพลง", "Open YouTube", "เพลงถัดไป", "เปิด Facebook"] {
            assert_eq!(super::clean(t), t, "{t}");
        }
    }
}
