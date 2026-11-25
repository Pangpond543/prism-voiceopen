// Settings + voice commands. JSON in %APPDATA%\com.prism.voiceopen\settings.json (voice samples in \voice\).
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// One thing a command does. `kind`: app | url | file | folder | search | system | prism | type | keys | wait
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Action {
    pub kind: String,
    pub target: String, // path / AUMID / url / url template with {q} / system id / text / key combo / ms
    pub args: String,
    pub label: String,  // shown name (app name, page title …)
    pub icon: String,   // app icon key (apps.rs) or ""
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Command {
    pub id: String,
    pub name: String,
    pub phrases: Vec<String>, // what the user says (several ways); for search commands = the words before the query
    pub actions: Vec<Action>,
    pub enabled: bool,
    pub samples: Vec<String>, // voice sample ids (teach by voice)
    pub uses: u32,
    pub last_used: u64,
}
impl Command {
    pub fn is_search(&self) -> bool { self.actions.iter().any(|a| a.kind == "search") }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    pub lang: String,
    pub dark: bool,
    pub hotkey: String,       // push-to-talk key, e.g. "Ctrl+Space"
    pub mode: String,         // both | hold | tap
    pub max_secs: u32,        // longest listen
    pub mic_id: String,
    pub mic_gain: f32,
    pub stt_lang: String,     // th | en | auto
    pub strict: f32,          // 0.5 … 0.95 — how close the words must be
    pub engine_keep: String,  // always | 10 | 2  (minutes to keep the speech engine loaded after use)
    pub glow: String,         // prism | aurora | sunset | ocean | mono
    pub glow_size: f32,       // 0.5 … 1.6
    pub glow_all: bool,       // every monitor (else only the one with the mouse)
    pub sounds: bool,
    pub caption: bool,
    pub autostart: bool,
    pub startup_asked: bool, // the first-run "open with Windows?" question was answered
    pub confirm_run: bool,    // show "เปิด …" note (always) — kept for future
    pub wake_on: bool,
    pub wake_phrase: String,
    pub wake_samples: Vec<String>,
    pub wake_sense: f32,      // 0.5 … 1.5
    /// What Whisper hears for each wake sample (sample id → text). The wake word is accepted only when the words heard
    /// also look like one of these (2-step check: the 11-19 log had 114 wake-ups from music / talk, d≈0.82–0.99).
    pub wake_heard: std::collections::BTreeMap<String, String>,
    pub voice_sense: f32,     // 0.5 … 1.5 — teach-by-voice tolerance
    pub commands: Vec<Command>,
    pub seeded: bool,
    pub hotkeys: std::collections::BTreeMap<String, String>, // extra global keys: "open" (main window), "repeat"
}
impl Default for Settings {
    fn default() -> Self {
        Settings {
            lang: "th".into(), dark: false, hotkey: "Ctrl+Space".into(), mode: "both".into(), max_secs: 8,
            mic_id: String::new(), mic_gain: 1.0, stt_lang: "th".into(), strict: 0.72, engine_keep: "10".into(),
            glow: "prism".into(), glow_size: 1.0, glow_all: true, sounds: true, caption: true, autostart: false, startup_asked: false, confirm_run: true,
            wake_on: false, wake_phrase: "เฮ้ พริซึม".into(), wake_samples: Vec::new(), wake_sense: 1.0, wake_heard: Default::default(), voice_sense: 1.0,
            commands: Vec::new(), seeded: false, hotkeys: default_hotkeys(),
        }
    }
}
pub const HOTKEY_NAMES: [(&str, i32); 3] = [("talk", 1), ("open", 2), ("repeat", 3)];
pub fn default_hotkeys() -> std::collections::BTreeMap<String, String> {
    [("open", "Ctrl+Alt+O"), ("repeat", "")].into_iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
}
pub fn hotkey(name: &str) -> String {
    let s = get();
    if name == "talk" { return s.hotkey; }
    s.hotkeys.get(name).cloned().unwrap_or_else(|| default_hotkeys().get(name).cloned().unwrap_or_default())
}

pub fn data_dir() -> PathBuf { PathBuf::from(std::env::var("APPDATA").unwrap_or_default()).join("com.prism.voiceopen") }
pub fn voice_dir() -> PathBuf { data_dir().join("voice") }
fn path() -> PathBuf { data_dir().join("settings.json") }

/// Lock that survives a panic on another thread (a poisoned lock would otherwise break every later call).
pub fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> { m.lock().unwrap_or_else(|e| e.into_inner()) }

/// Read settings.json without ever throwing the user's commands away: a file that does not parse as a whole is
/// read field by field (and command by command), and the original is kept as settings.json.bad-<time>.
/// settings.json had to be recovered this run (then nothing that depends on the full command list is cleaned up).
pub static RECOVERED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
fn load() -> Settings {
    let Ok(text) = std::fs::read_to_string(path()) else {
        // no file yet (first run) — or the save was cut between write and rename
        let tmp = data_dir().join("settings.json.tmp");
        return std::fs::read_to_string(&tmp).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    };
    if let Ok(s) = serde_json::from_str::<Settings>(&text) { return s; }
    RECOVERED.store(true, std::sync::atomic::Ordering::Relaxed);
    let bad = data_dir().join(format!("settings.json.bad-{}", now_ms()));
    let _ = std::fs::copy(path(), &bad);
    log(&format!("settings: settings.json could not be read as a whole — kept a copy as {}, recovering what is readable", bad.display()));
    let Ok(Value::Object(o)) = serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) else {
        // not JSON at all: start fresh but never re-seed over a file that had commands (keep seeded = true)
        return Settings { seeded: text.contains("\"commands\""), ..Default::default() };
    };
    let mut cur = serde_json::to_value(Settings::default()).unwrap_or(Value::Null);
    if let Some(c) = cur.as_object_mut() {
        for (k, v) in o.iter() {
            if k == "commands" { continue; }
            let mut probe = c.clone();
            probe.insert(k.clone(), v.clone());
            if serde_json::from_value::<Settings>(Value::Object(probe)).is_ok() { c.insert(k.clone(), v.clone()); }
        }
        let cmds: Vec<Value> = o.get("commands").and_then(|v| v.as_array()).map(|a| a.iter().filter(|x| serde_json::from_value::<Command>((*x).clone()).is_ok()).cloned().collect()).unwrap_or_default();
        if o.contains_key("commands") { c.insert("seeded".into(), Value::Bool(true)); }
        c.insert("commands".into(), Value::Array(cmds));
    }
    serde_json::from_value(cur).unwrap_or_default()
}
fn cell() -> &'static Mutex<Settings> {
    static S: OnceLock<Mutex<Settings>> = OnceLock::new();
    S.get_or_init(|| {
        let mut s = load();
        if !s.seeded { s.commands = crate::actions::starter_commands(); s.seeded = true; }
        Mutex::new(s)
    })
}
pub fn get() -> Settings { lock(cell()).clone() }
pub fn save() {
    // one writer at a time: two threads writing the same .tmp at once could leave a broken file
    static W: Mutex<()> = Mutex::new(());
    let _w = lock(&W);
    let s = get();
    let dir = data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let tmp = dir.join("settings.json.tmp");
    match serde_json::to_string_pretty(&s) {
        Ok(t) => { if std::fs::write(&tmp, t).is_ok() { if let Err(e) = std::fs::rename(&tmp, path()) { log(&format!("settings: save failed: {e}")); } } }
        Err(e) => log(&format!("settings: cannot serialise: {e}")),
    }
}
pub fn update(f: impl FnOnce(&mut Settings)) { { let mut s = lock(cell()); f(&mut s); } save(); }
/// Merge a JSON object of changed fields (from the main window).
pub fn merge(patch: &Value) -> Settings {
    {
        let mut s = lock(cell());
        let mut cur = serde_json::to_value(&*s).unwrap_or(Value::Null);
        if let (Some(o), Some(p)) = (cur.as_object_mut(), patch.as_object()) { for (k, v) in p { o.insert(k.clone(), v.clone()); } }
        if let Ok(n) = serde_json::from_value::<Settings>(cur) { *s = n; }
    }
    save();
    get()
}

pub fn en() -> bool { get().lang == "en" }
/// Native-UI string: Thai source, English when the app language is EN.
pub fn tr<'a>(th: &'a str, en_s: &'a str) -> &'a str { if en() { en_s } else { th } }

pub fn now_ms() -> u64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0) }
pub fn new_id() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    format!("{:x}{:02x}", now_ms(), N.fetch_add(1, Ordering::Relaxed) & 0xff)
}

/* ------------------------------------------------------------------ log */
pub fn log_path() -> PathBuf { PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_default()).join("com.prism.voiceopen").join("prism.log") }
pub fn log(s: &str) {
    use std::io::Write;
    let p = log_path();
    if let Some(d) = p.parent() { let _ = std::fs::create_dir_all(d); }
    if std::fs::metadata(&p).map(|m| m.len() > 400_000).unwrap_or(false) { let _ = std::fs::remove_file(&p); }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&p) { let _ = writeln!(f, "[{}] {s}", now_ms()); }
}
