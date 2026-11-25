// One listening session at a time: open the mic → collect speech (until the key is released, or the user stops
// talking) → recognise (voice samples first — instant — then Whisper) → run the command. Also the wake-word
// listener and the history of what was heard.
use crate::audio::{self, Cap, RATE};
use crate::settings::{self, tr};
use crate::{actions, matcher, overlay, stt, voice};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
pub enum Trigger {
    Hold,               // key held: listen until released
    Tap,                // key tapped / mic button: listen until the user stops talking
    Wake,               // wake word heard
    Train(String),      // record a voice sample for a command id ("__wake" = wake word)
}
impl Trigger { fn auto_stop(&self) -> bool { !matches!(self, Trigger::Hold) } }

static BUSY: AtomicBool = AtomicBool::new(false);
static FINISH: AtomicBool = AtomicBool::new(false);
static CANCEL: AtomicBool = AtomicBool::new(false);
static KIND: AtomicU8 = AtomicU8::new(0); // 1 = hold (for the overlay hint)
pub fn busy() -> bool { BUSY.load(Ordering::Relaxed) }
pub fn finish() { FINISH.store(true, Ordering::Relaxed); }
pub fn cancel() { CANCEL.store(true, Ordering::Relaxed); }
pub fn hold_mode() -> bool { KIND.load(Ordering::Relaxed) == 1 }
/// Switch a running hold-session to "stop when I stop talking" (the key was only tapped).
pub fn to_tap() { KIND.store(2, Ordering::Relaxed); }

type Emit = fn(&str, Value);
static EMIT: OnceLock<Emit> = OnceLock::new();
pub fn set_emitter(f: Emit) { let _ = EMIT.set(f); }
pub fn emit(ev: &str, v: Value) { if let Some(f) = EMIT.get() { f(ev, v); } }

/// Test hook: the next session reads this sound instead of the microphone.
fn injected() -> &'static Mutex<Option<Vec<f32>>> { static I: OnceLock<Mutex<Option<Vec<f32>>>> = OnceLock::new(); I.get_or_init(|| Mutex::new(None)) }
pub fn inject(s: Vec<f32>) { *crate::settings::lock(injected()) = Some(s); }

/// Start listening (ignored when a session is already running).
pub fn begin(t: Trigger) -> bool {
    if BUSY.swap(true, Ordering::SeqCst) { return false; }
    FINISH.store(false, Ordering::Relaxed);
    CANCEL.store(false, Ordering::Relaxed);
    KIND.store(if t == Trigger::Hold { 1 } else { 2 }, Ordering::Relaxed);
    std::thread::spawn(move || {
        let _ = unsafe { windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED) };
        // a panic inside must never leave BUSY on (the talk key would be dead until restart) or the glow on screen
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| session(t)));
        audio::LEVEL.store(0, Ordering::Relaxed);
        if r.is_err() {
            settings::log("engine: session crashed (see PANIC above) — recovered");
            overlay::send(overlay::Cmd::Result { ok: false, title: tr("เกิดข้อผิดพลาด", "Something went wrong").into(), sub: tr("ลองพูดใหม่อีกครั้ง", "Please try again").into() });
        }
        BUSY.store(false, Ordering::SeqCst);
    });
    true
}

fn session(t: Trigger) {
    let st = settings::get();
    let train = if let Trigger::Train(id) = &t { Some(id.clone()) } else { None };
    if train.is_none() && !stt::available() && !st.commands.iter().any(|c| !c.samples.is_empty()) {
        overlay::send(overlay::Cmd::Result { ok: false, title: tr("ยังไม่มีตัวฟังเสียง", "Speech engine missing").into(), sub: tr("ไม่พบไฟล์ในโฟลเดอร์ runtime — ติดตั้งโปรแกรมใหม่", "Files in the runtime folder are missing — reinstall the app").into() });
        return;
    }
    if train.is_none() { stt::warm(); }
    let inj = crate::settings::lock(injected()).take();
    let t0 = Instant::now();
    let mut buf: Vec<f32> = Vec::new();
    // wake word: the mic records at once, but the glow + chime wait until the words heard confirm it was the wake word
    // (or 2.5 s pass) — a false wake-up from music / talk then ends silently instead of flashing the screen
    let wake = t == Trigger::Wake;
    let mut shown = false;
    let mut chime_at: Option<usize> = None;
    let mut show = |shown: &mut bool, buf_len: usize| {
        *shown = true;
        overlay::send(overlay::Cmd::Listen { train: train.as_ref().map(|id| train_label(id)) });
        crate::sounds::play(crate::sounds::Cue::Open);
        chime_at = Some(buf_len);
    };
    if !wake { show(&mut shown, 0); }
    let mut cap = if inj.is_none() { Some(Cap::start(&st.mic_id, st.mic_gain)) } else { None };
    let (mut spoke, mut quiet_since, mut floor) = (false, Instant::now(), 0.004f32);
    let max = Duration::from_secs(st.max_secs.clamp(3, 30) as u64);
    let mut inj_pos = 0usize;
    loop {
        std::thread::sleep(Duration::from_millis(30));
        if wake && !shown {
            match WAKE_VERDICT.load(Ordering::Relaxed) {
                WAKE_BAD => { drop(cap); settings::log("wake: not the wake word — ignored quietly"); return; }
                WAKE_OK_V => show(&mut shown, buf.len()),
                _ if t0.elapsed() > Duration::from_millis(2500) => show(&mut shown, buf.len()),
                _ => {}
            }
        }
        let chunk: Vec<f32> = match (&cap, &inj) {
            (Some(c), _) => c.take(),
            (None, Some(s)) => { let n = (RATE as usize * 30 / 1000).min(s.len() - inj_pos); let v = s[inj_pos..inj_pos + n].to_vec(); inj_pos += n; v }
            _ => Vec::new(),
        };
        if let Some(s) = &inj { if inj_pos < s.len() { audio::LEVEL.store(rms(&chunk).to_bits(), Ordering::Relaxed); } else { audio::LEVEL.store(0, Ordering::Relaxed); } }
        buf.extend_from_slice(&chunk);
        if CANCEL.load(Ordering::Relaxed) {
            drop(cap);
            overlay::send(overlay::Cmd::Hide);
            crate::sounds::play(crate::sounds::Cue::Cancel);
            return;
        }
        if cap.as_ref().map_or(false, |c| c.failed.load(Ordering::Relaxed)) {
            overlay::send(overlay::Cmd::Result { ok: false, title: tr("เปิดไมโครโฟนไม่ได้", "Cannot open the microphone").into(), sub: tr("ตรวจว่าเสียบไมค์อยู่ และ Windows อนุญาตให้ใช้ไมค์ (ตั้งค่า → ความเป็นส่วนตัว → ไมโครโฟน)", "Check the mic is plugged in and allowed (Windows Settings → Privacy → Microphone)").into() });
            crate::sounds::play(crate::sounds::Cue::Fail);
            return;
        }
        // voice activity: adaptive to the room
        let r = rms(&chunk);
        if !chunk.is_empty() {
            if t0.elapsed() < Duration::from_millis(400) { floor = floor.min(r.max(0.0008)).max(0.0008); if t0.elapsed() < Duration::from_millis(60) { floor = r.max(0.0008); } }
            let loud = r > (floor * 3.5).max(0.012);
            if loud { if !spoke && t0.elapsed() > Duration::from_millis(150) { spoke = true; } quiet_since = Instant::now(); } else if !spoke { floor = floor * 0.98 + r.max(0.0008) * 0.02; }
        }
        let auto = t.auto_stop() || KIND.load(Ordering::Relaxed) == 2;
        if FINISH.load(Ordering::Relaxed) { break; }
        if inj.as_ref().map_or(false, |s| inj_pos >= s.len()) && (spoke || t0.elapsed() > Duration::from_secs(2)) { break; }
        if auto && spoke && quiet_since.elapsed() > Duration::from_millis(if train.is_some() { 600 } else { 800 }) { break; }
        if auto && !spoke && t0.elapsed() > Duration::from_secs(6) { break; }
        if t0.elapsed() > max { break; }
    }
    drop(cap);
    audio::LEVEL.store(0, Ordering::Relaxed);
    // the opening chime may leak into the ~120 ms after it played (at the start, or later for the wake word)
    let n = RATE as usize * 12 / 100;
    let skip = if chime_at == Some(0) { n.min(buf.len()) } else { 0 };
    if let Some(at) = chime_at.filter(|a| *a > 0) { let end = (at + n + 800).min(buf.len()); for v in &mut buf[at.min(end)..end] { *v = 0.0; } }
    let speech = &buf[skip..];
    match train {
        Some(id) => finish_train(&id, speech),
        None => recognise(speech, t),
    }
}

fn rms(s: &[f32]) -> f32 { if s.is_empty() { 0.0 } else { (s.iter().map(|v| v * v).sum::<f32>() / s.len() as f32).sqrt() } }
fn train_label(id: &str) -> String {
    if id == "__wake" { return settings::get().wake_phrase; }
    settings::get().commands.iter().find(|c| c.id == id).map(|c| c.phrases.first().cloned().unwrap_or(c.name.clone())).unwrap_or_default()
}

/* ------------------------------------------------------------ recognise */
fn recognise(speech: &[f32], t: Trigger) {
    if t == Trigger::Wake {
        let t0 = Instant::now();
        while WAKE_VERDICT.load(Ordering::Relaxed) == WAKE_PENDING && t0.elapsed() < Duration::from_secs(8) { std::thread::sleep(Duration::from_millis(30)); }
        if WAKE_VERDICT.load(Ordering::Relaxed) == WAKE_BAD {
            overlay::send(overlay::Cmd::Hide);
            settings::log("wake: not the wake word — ignored quietly");
            return;
        }
    }
    let st = settings::get();
    let Some(trimmed) = voice::trim(speech) else {
        overlay::send(overlay::Cmd::Result { ok: false, title: tr("ไม่ได้ยินเสียง", "Didn't hear anything").into(), sub: if t == Trigger::Hold { tr("กดคีย์ค้างไว้ระหว่างพูด แล้วปล่อยเมื่อพูดจบ", "Hold the key while you speak, release when done").into() } else { tr("ลองพูดดังขึ้นอีกนิด หรือเช็กไมค์ในหน้าตั้งค่า", "Try speaking a little louder, or check the mic in Settings").into() } });
        crate::sounds::play(crate::sounds::Cue::Fail);
        return;
    };
    overlay::send(overlay::Cmd::Think);
    let t0 = Instant::now();
    let feats = voice::features(speech);
    let vm = voice::best(&feats, &st.commands, st.voice_sense);
    let mut heard = String::new();
    let mut chosen: Option<(String, String, &'static str)> = None; // (command id, query, how)
    if let Some((id, d, thr, second)) = &vm {
        settings::log(&format!("voice: best {id} d={d:.3} thr={thr:.3} second={second:.3}"));
        // only a very close voice match skips the speech-to-text check ("ลดเสียง" sounds a lot like "เพิ่มเสียง")
        if *d < thr * 0.6 && *d < second * 0.7 { chosen = Some((id.clone(), String::new(), "voice")); }
    }
    let mut near: Option<String> = None;
    let mut near_id: Option<String> = None;
    if chosen.is_none() || st.commands.iter().any(|c| Some(&c.id) == chosen.as_ref().map(|c| &c.0) && c.is_search()) {
        // no command-list prompt: with it Whisper "hears" commands that were never said (false triggers)
        match stt::transcribe(trimmed, "") {
            Ok(text) => {
                heard = text.clone();
                let hits = matcher::rank(&text, &st.commands);
                if let Some(h) = hits.first() {
                    settings::log(&format!("match: {:?} → {} ({:.2}) q={:?}", text, h.id, h.score, h.query));
                    if h.score >= st.strict { chosen = Some((h.id.clone(), h.query.clone(), "text")); }
                    else if h.score >= st.strict - 0.2 { near = st.commands.iter().find(|c| c.id == h.id).map(|c| c.name.clone()); near_id = Some(h.id.clone()); }
                }
            }
            Err(e) => {
                settings::log(&format!("stt error: {e}"));
                if chosen.is_none() {
                    overlay::send(overlay::Cmd::Result { ok: false, title: tr("ตัวฟังเสียงมีปัญหา", "Speech engine problem").into(), sub: tr("ลองอีกครั้ง — ครั้งแรกอาจใช้เวลาโหลดสักครู่", "Try again — the first time can take a moment to load").into() });
                    crate::sounds::play(crate::sounds::Cue::Fail);
                    return;
                }
            }
        }
    }
    // voice sample within tolerance, text didn't decide
    // ...unless the words point at a different command (then it's safer to do nothing)
    if chosen.is_none() { if let Some((id, d, thr, second)) = &vm { if *d < thr * 0.85 && *d < second * 0.85 && near_id.as_ref().map_or(true, |n| n == id) { chosen = Some((id.clone(), String::new(), "voice")); } } }
    let ms = t0.elapsed().as_millis() as u64;
    let Some((id, q, how)) = chosen else {
        let sub = if heard.is_empty() { tr("ฟังไม่ออก — ลองพูดช้าลงอีกนิด", "Couldn't make it out — try a bit slower").to_string() }
            else if let Some(n) = &near { format!("{} “{}” · {} {}", tr("ได้ยินว่า", "Heard"), heard, tr("ใกล้กับ", "close to"), n) }
            else { format!("{} “{}”", tr("ได้ยินว่า", "Heard"), heard) };
        overlay::send(overlay::Cmd::Result { ok: false, title: tr("ไม่ตรงกับคำสั่งไหน", "No matching command").into(), sub });
        crate::sounds::play(crate::sounds::Cue::Fail);
        history(json!({ "t": settings::now_ms(), "heard": heard, "ok": false, "near": near, "ms": ms }));
        return;
    };
    let Some(cmd) = settings::get().commands.into_iter().find(|c| c.id == id) else { return };
    let title = cmd.name.clone();
    let sub = if !q.is_empty() { format!("{} “{}”", tr("ค้นหา", "Search"), q) } else if how == "voice" && heard.is_empty() { tr("จำได้จากเสียงของคุณ", "Recognised by your voice").to_string() } else { format!("“{}”", heard) };
    overlay::send(overlay::Cmd::Result { ok: true, title, sub });
    crate::sounds::play(crate::sounds::Cue::Ok);
    settings::update(|s| if let Some(c) = s.commands.iter_mut().find(|c| c.id == id) { c.uses += 1; c.last_used = settings::now_ms(); });
    *crate::settings::lock(last_cmd()) = Some((id.clone(), q.clone()));
    history(json!({ "t": settings::now_ms(), "heard": heard, "ok": true, "id": id, "name": cmd.name, "query": q, "how": how, "ms": ms }));
    let name = cmd.name.clone();
    actions::run(&cmd, &q, move |ok, total| {
        if ok < total { overlay::send(overlay::Cmd::Result { ok: false, title: format!("{} — {}", name, tr("ทำไม่สำเร็จ", "failed")), sub: tr("ตรวจว่าโปรแกรม / ไฟล์ / ลิงก์ ยังอยู่ (แก้ในหน้าคำสั่ง)", "Check the program / file / link still exists (edit the command)").into() }); }
        emit("settings", serde_json::to_value(settings::get()).unwrap_or_default());
    });
}
fn last_cmd() -> &'static Mutex<Option<(String, String)>> { static L: OnceLock<Mutex<Option<(String, String)>>> = OnceLock::new(); L.get_or_init(|| Mutex::new(None)) }
/// "Repeat last command" hotkey.
pub fn repeat_last() {
    if busy() { return; } // don't cover the listening glow / run something while a command is being spoken
    let Some((id, q)) = crate::settings::lock(last_cmd()).clone() else { return };
    if let Some(c) = settings::get().commands.into_iter().find(|c| c.id == id && c.enabled) {
        overlay::send(overlay::Cmd::Result { ok: true, title: c.name.clone(), sub: if q.is_empty() { tr("ทำคำสั่งล่าสุดอีกครั้ง", "Repeating the last command").into() } else { format!("“{q}”") } });
        actions::run(&c, &q, |_, _| {});
    }
}

fn finish_train(id: &str, speech: &[f32]) {
    match voice::save(speech) {
        Some(s) => {
            let mut n = 0;
            if id == "__wake" { settings::update(|st| { st.wake_samples.push(s.id.clone()); n = st.wake_samples.len(); }); learn_wake(); }
            else { settings::update(|st| if let Some(c) = st.commands.iter_mut().find(|c| c.id == id) { c.samples.push(s.id.clone()); n = c.samples.len(); }); }
            overlay::send(overlay::Cmd::Result { ok: true, title: format!("{} ({n})", tr("จำเสียงแล้ว", "Voice saved")), sub: if n < 3 { tr("พูดอีกครั้งจะจำได้แม่นขึ้น", "Say it again for better accuracy").into() } else { tr("พร้อมใช้ — ลองพูดดูได้เลย", "Ready — try it out").into() } });
            crate::sounds::play(crate::sounds::Cue::Ok);
            emit("sample", json!({ "target": id, "ok": true, "id": s.id, "secs": s.secs, "n": n }));
        }
        None => {
            overlay::send(overlay::Cmd::Result { ok: false, title: tr("ไม่ได้ยินเสียง", "Didn't hear anything").into(), sub: tr("ลองพูดใหม่ ใกล้ไมค์ขึ้นอีกนิด", "Try again, a little closer to the mic").into() });
            crate::sounds::play(crate::sounds::Cue::Fail);
            emit("sample", json!({ "target": id, "ok": false }));
        }
    }
    emit("settings", serde_json::to_value(settings::get()).unwrap_or_default());
}

/* -------------------------------------------------------------- history */
fn hist() -> &'static Mutex<Vec<Value>> {
    static H: OnceLock<Mutex<Vec<Value>>> = OnceLock::new();
    H.get_or_init(|| Mutex::new(std::fs::read_to_string(settings::data_dir().join("history.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()))
}
fn history(v: Value) {
    let list = { let mut h = crate::settings::lock(hist()); h.insert(0, v.clone()); h.truncate(80); h.clone() };
    let _ = std::fs::write(settings::data_dir().join("history.json"), serde_json::to_string(&list).unwrap_or_default());
    emit("history", v);
}
pub fn history_list() -> Value { Value::Array(crate::settings::lock(hist()).clone()) }
pub fn history_clear() { crate::settings::lock(hist()).clear(); let _ = std::fs::remove_file(settings::data_dir().join("history.json")); }

/* ------------------------------------------------------------ wake word */
pub static WAKE_LIVE: AtomicBool = AtomicBool::new(false);
// Step 2 of the wake word: the voice-shape match (DTW) alone woke the app 114 times in one afternoon (11-19 log: music,
// talk, commands said without the wake word). The words Whisper hears in that same piece of sound must also look like
// the wake word (`matcher::wake_score`) — compared with what Whisper heard for the user's own samples (`wake_heard`).
const WAKE_PENDING: u8 = 1;
const WAKE_OK_V: u8 = 2;
const WAKE_BAD: u8 = 3;
static WAKE_VERDICT: AtomicU8 = AtomicU8::new(0);
static LEARNING: AtomicBool = AtomicBool::new(false);

/// What the words must look like: the wake phrase, "hey prism", and Whisper's spelling of each wake sample.
fn wake_refs(st: &settings::Settings) -> Vec<String> {
    let mut r = vec![st.wake_phrase.clone(), "hey prism".to_string()];
    r.extend(st.wake_samples.iter().filter_map(|id| st.wake_heard.get(id)).filter(|t| !t.is_empty()).cloned());
    r
}
/// Until Whisper's spelling of at least 2 samples is known, step 2 can't judge fairly → it lets the voice match decide.
fn wake_learned(st: &settings::Settings) -> bool {
    st.wake_samples.iter().filter(|id| st.wake_heard.get(*id).map_or(false, |t| !t.is_empty())).count() >= 2
}
/// Check one piece of sound. Returns (accepted, heard text).
pub fn wake_check(seg: &[f32]) -> (bool, String) {
    let st = settings::get();
    if !stt::available() || !wake_learned(&st) { return (true, String::new()); }
    let t0 = Instant::now();
    match stt::transcribe(voice::trim(seg).unwrap_or(seg), "") {
        Ok(text) => {
            let s = matcher::wake_score(&text, &wake_refs(&st));
            let ok = s >= matcher::WAKE_OK;
            settings::log(&format!("wake: heard {text:?} score={s:.2} → {} ({} ms)", if ok { "yes" } else { "no" }, t0.elapsed().as_millis()));
            (ok, text)
        }
        // the speech engine has a problem: don't make the wake word useless because of it
        Err(e) => { settings::log(&format!("wake: check skipped ({e})")); (true, String::new()) }
    }
}
/// Test hook: what the wake loop does after a voice match, with `wake` as the heard piece and `cmd` as the mic.
pub fn sim_wake(wake: Vec<f32>, cmd: Vec<f32>) {
    WAKE_VERDICT.store(WAKE_PENDING, Ordering::Relaxed);
    std::thread::spawn(move || { let (ok, _) = wake_check(&wake); WAKE_VERDICT.store(if ok { WAKE_OK_V } else { WAKE_BAD }, Ordering::Relaxed); });
    inject(cmd);
    begin(Trigger::Wake);
}
/// Learn (in the background) how Whisper spells each wake sample that has no entry yet; forget removed samples.
pub fn learn_wake() {
    if !stt::available() || LEARNING.swap(true, Ordering::SeqCst) { return; }
    std::thread::spawn(|| {
        let _ = std::panic::catch_unwind(|| {
            let st = settings::get();
            for id in st.wake_samples.iter().filter(|id| !st.wake_heard.contains_key(*id)) {
                let Some(s) = std::fs::read(settings::voice_dir().join(format!("{id}.wav"))).ok().and_then(|b| audio::read_wav(&b)) else { continue };
                match stt::transcribe(&s, "") {
                    Ok(text) => {
                        settings::log(&format!("wake: sample {id} is heard as {text:?}"));
                        let id = id.clone();
                        settings::update(|x| { x.wake_heard.insert(id, text); });
                    }
                    Err(e) => { settings::log(&format!("wake: learning stopped ({e})")); break; }
                }
            }
            settings::update(|x| { let keep = x.wake_samples.clone(); x.wake_heard.retain(|k, _| keep.contains(k)); });
        });
        LEARNING.store(false, Ordering::SeqCst);
    });
}
/// Listens all the time (only when the user switched it on and taught the wake word). Short sounds are compared with
/// the wake samples by DTW — no speech-to-text runs until the wake word is heard.
pub fn wake_loop() {
    std::thread::spawn(|| {
        let _ = unsafe { windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED) };
        let mut cap: Option<Cap> = None;
        let mut samples: Vec<voice::Sample> = Vec::new();
        let mut key = String::new();
        let mut seg: Vec<f32> = Vec::new();
        let (mut in_speech, mut quiet, mut floor) = (false, 0usize, 0.004f32);
        let mut cool = Instant::now();
        let mut ref_secs = 0f32; // typical length of the user's wake samples
        loop {
            std::thread::sleep(Duration::from_millis(40));
            let st = settings::get();
            let want = st.wake_on && !st.wake_samples.is_empty() && !busy();
            if !want {
                if cap.is_some() { cap = None; WAKE_LIVE.store(false, Ordering::Relaxed); emit("wake", json!({ "live": false })); }
                std::thread::sleep(Duration::from_millis(250));
                continue;
            }
            let k = st.wake_samples.join(",");
            if k != key {
                samples = st.wake_samples.iter().filter_map(|id| voice::load(id)).collect();
                if samples.len() < st.wake_samples.len() { settings::log(&format!("wake: {} of {} samples missing on disk", st.wake_samples.len() - samples.len(), st.wake_samples.len())); }
                let mut l: Vec<f32> = samples.iter().map(|s| s.secs).filter(|s| *s > 0.1).collect();
                l.sort_by(|a, b| a.total_cmp(b));
                ref_secs = l.get(l.len() / 2).copied().unwrap_or(0.0);
                key = k;
                if st.wake_samples.iter().any(|id| !st.wake_heard.contains_key(id)) { learn_wake(); }
            }
            if cap.is_none() { cap = Some(Cap::start(&st.mic_id, st.mic_gain)); seg.clear(); in_speech = false; WAKE_LIVE.store(true, Ordering::Relaxed); emit("wake", json!({ "live": true })); }
            let Some(c) = &cap else { continue };
            if c.failed.load(Ordering::Relaxed) { cap = None; WAKE_LIVE.store(false, Ordering::Relaxed); emit("wake", json!({ "live": false })); std::thread::sleep(Duration::from_secs(5)); continue; }
            let chunk = c.take();
            for part in chunk.chunks(320) {
                let r = rms(part);
                let loud = r > (floor * 3.5).max(0.012);
                if !in_speech {
                    floor = floor * 0.995 + r.max(0.0008) * 0.005;
                    // keep 200 ms of lead-in
                    seg.extend_from_slice(part);
                    if seg.len() > 3200 { seg.drain(..seg.len() - 3200); }
                    if loud { in_speech = true; quiet = 0; }
                } else {
                    seg.extend_from_slice(part);
                    quiet = if loud { 0 } else { quiet + 1 };
                    let secs = seg.len() as f32 / RATE as f32;
                    if quiet >= 18 || secs > 3.2 {
                        // a whole utterance (ended by 360 ms of quiet)
                        if secs >= 0.35 && secs <= 3.0 && cool.elapsed() > Duration::from_secs(2) {
                            let f = voice::features(&seg);
                            let thr = voice::threshold(&samples, st.wake_sense) * 0.95;
                            let d = samples.iter().map(|s| voice::dtw(&f, &s.feats)).fold(f32::MAX, f32::min);
                            // much shorter / longer than the user's own wake samples = something else
                            let len_ok = ref_secs <= 0.0 || (secs >= ref_secs * 0.5 && secs <= ref_secs * 2.0 + 0.4);
                            if d < 3.0 { settings::log(&format!("wake: d={d:.3} thr={thr:.3} secs={secs:.2}{}", if len_ok { "" } else { " (length differs)" })); }
                            if d < thr && len_ok {
                                cap = None; // free the mic for the session
                                WAKE_LIVE.store(false, Ordering::Relaxed);
                                cool = Instant::now();
                                WAKE_VERDICT.store(WAKE_PENDING, Ordering::Relaxed);
                                let piece = seg.clone();
                                std::thread::spawn(move || {
                                    let (ok, _) = wake_check(&piece);
                                    WAKE_VERDICT.store(if ok { WAKE_OK_V } else { WAKE_BAD }, Ordering::Relaxed);
                                });
                                begin(Trigger::Wake);
                                seg.clear(); in_speech = false;
                                break;
                            }
                        }
                        seg.clear(); in_speech = false;
                    }
                }
            }
        }
    });
}
