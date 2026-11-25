// Prism VoiceOpen — press a key, say a word, and what you set up opens (programs, links, folders, searches, PC
// controls, other Prism apps). Lives in the tray; the screen edge glows while the mic is open.
// Tauri hosts the settings window; the hotkey, glow and caption are native (overlay.rs); speech runs offline
// through a bundled whisper.cpp server (stt.rs) plus "teach by voice" templates (voice.rs).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(non_snake_case)]

mod actions;
mod apps;
mod audio;
mod engine;
mod gfx;
mod matcher;
mod overlay;
mod settings;
mod sounds;
mod startup;
mod stt;
mod update;
mod voice;

use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::OnceLock;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

const PKG_NAME: &str = "prism-voiceopen"; // app/package.json "name" — the updater refuses anything else
static APP: OnceLock<AppHandle> = OnceLock::new();

fn app_dir() -> PathBuf {
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())).unwrap_or_default();
    if exe_dir.join("prism.config.json").exists() { return exe_dir; }
    if let Some(up) = exe_dir.parent() { if up.join("prism.config.json").exists() { return up.to_path_buf(); } }
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().map(|p| p.to_path_buf()).unwrap_or_default();
    if src.join("prism.config.json").exists() { src } else { exe_dir }
}
fn repo() -> String {
    let cfg: Value = std::fs::read_to_string(app_dir().join("prism.config.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null);
    let r = cfg["repo"].as_str().unwrap_or("").trim();
    let r = r.trim_start_matches("https://github.com/").trim_end_matches(".git").trim_matches('/');
    if update::valid_repo(r) { r.to_string() } else { String::new() }
}

/* ------------------------------------------------------------- main window */
fn main_win() -> Option<WebviewWindow> { APP.get().and_then(|a| a.get_webview_window("main")) }
fn page_visible(w: &WebviewWindow, on: bool) { let _ = w.with_webview(move |wv| unsafe { let _ = wv.controller().SetIsVisible(on); }); }
fn show_main() { if let Some(w) = main_win() { page_visible(&w, true); let _ = w.show(); let _ = w.unminimize(); let _ = w.set_focus(); } }
fn emit_js(ev: &str, v: Value) { if let Some(a) = APP.get() { let _ = a.emit(&format!("voice:{ev}"), v); } }

#[tauri::command]
fn app_log(a: String) { settings::log(&a.chars().take(400).collect::<String>()); }
#[tauri::command]
fn win_min(window: WebviewWindow) { let _ = window.minimize(); }
#[tauri::command]
fn win_max(window: WebviewWindow) { let _ = if window.is_maximized().unwrap_or(false) { window.unmaximize() } else { window.maximize() }; }
fn quit_app(app: AppHandle) { stt::stop(); app.exit(0); }
#[tauri::command(async)]
fn win_close(app: AppHandle) { quit_app(app); }
#[tauri::command]
fn win_hide(window: WebviewWindow) { let _ = window.hide(); page_visible(&window, false); overlay::send(overlay::Cmd::HotkeysPause(false)); }
#[tauri::command]
fn app_ready(window: WebviewWindow) {
    if !std::env::args().any(|a| a == "--tray") { let _ = window.show(); let _ = window.set_focus(); } else { page_visible(&window, false); }
}
#[tauri::command]
fn app_relaunch(app: AppHandle) { stt::stop(); app.restart(); }

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    let listen = MenuItem::with_id(app, "listen", format!("{}  ({})", settings::tr("พูดคำสั่ง", "Speak a command"), settings::get().hotkey), true, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", settings::tr("เปิด Prism VoiceOpen", "Open Prism VoiceOpen"), true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", settings::tr("ออกจากโปรแกรม", "Quit Prism VoiceOpen"), true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&listen, &open, &sep, &quit])?;
    let mut b = TrayIconBuilder::with_id("prism-voiceopen")
        .tooltip("Prism VoiceOpen")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, ev| match ev.id().as_ref() {
            "open" => show_main(),
            "listen" => { engine::begin(engine::Trigger::Tap); }
            "quit" => { let a = app.clone(); std::thread::spawn(move || quit_app(a)); }
            _ => {}
        })
        .on_tray_icon_event(|_t, ev| { if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev { show_main(); } });
    if let Some(icon) = app.default_window_icon() { b = b.icon(icon.clone()); }
    b.build(app)?;
    Ok(())
}

/* ------------------------------------------------------------ about/update */
#[tauri::command]
fn about_info() -> Value {
    json!({ "version": env!("CARGO_PKG_VERSION"), "repo": repo(), "tauri": tauri::VERSION, "model": stt::model_name(), "test": std::env::var("PRISM_ACT").unwrap_or_default() })
}
#[tauri::command]
fn about_openRepo() { let r = repo(); if !r.is_empty() { let _ = std::process::Command::new("explorer").arg(format!("https://github.com/{r}")).spawn(); } }
#[tauri::command(async)]
fn update_check() -> Value { update::check(&repo(), env!("CARGO_PKG_VERSION")) }
#[tauri::command(async)]
fn update_apply(app: AppHandle) -> Value {
    let c = update::check(&repo(), env!("CARGO_PKG_VERSION"));
    if c["ok"] != true { return c; }
    if c["hasUpdate"] != true { return json!({ "ok": false, "reason": "uptodate" }); }
    if engine::busy() { return json!({ "ok": false, "reason": "busy" }); }
    stt::stop();
    update::apply(c["zip"].as_str().unwrap_or(""), &app_dir(), PKG_NAME, &|step| { let _ = app.emit("update:step", json!({ "step": step, "f": 0 })); })
}

/* ---------------------------------------------------------------- settings */
#[tauri::command]
fn settings_get() -> Value {
    let s = settings::get();
    if s.autostart && !startup::active(STARTUP_NAME) { settings::update(|x| x.autostart = false); }
    serde_json::to_value(settings::get()).unwrap_or(Value::Null)
}
#[tauri::command]
fn settings_set(a: Value) -> Value {
    let before = settings::get();
    let s = settings::merge(&a);
    if before.stt_lang != s.stt_lang { stt::stop(); }
    if before.autostart != s.autostart { autostart(s.autostart); }
    overlay::send(overlay::Cmd::Reload);
    serde_json::to_value(s).unwrap_or(Value::Null)
}
const STARTUP_NAME: &str = "Prism VoiceOpen";
fn autostart(on: bool) { startup::set(STARTUP_NAME, "--tray", on); settings::log(&format!("autostart {on}")); }
/// Follow Windows' real state (Task Manager → Startup apps may have switched us off) and fix the exe path.
fn startup_sync() {
    let s = settings::get();
    let real = startup::sync(STARTUP_NAME, "--tray", s.autostart);
    if real != s.autostart || (real && !s.startup_asked) { settings::update(|x| { x.autostart = real; if real { x.startup_asked = true; } }); }
    settings::log(&format!("startup sync wanted={} real={real}", s.autostart));
}
#[tauri::command(async)]
fn sys_info() -> Value {
    json!({
        "mics": audio::mics().into_iter().map(|(id, name)| json!({ "id": id, "name": name })).collect::<Vec<_>>(),
        "engine": stt::available(), "model": stt::model_name(), "loaded": stt::running(), "device": stt::device(),
        "prism": (["size", "cam", "player", "studio", "rec"].iter().filter(|n| actions::prism_exe(n).is_some()).collect::<Vec<_>>()),
        "wakeLive": engine::WAKE_LIVE.load(std::sync::atomic::Ordering::Relaxed),
        "threads": std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4),
    })
}
#[tauri::command]
fn hotkeys_status() -> Value { serde_json::to_value(settings::lock(overlay::hotkey_status()).clone()).unwrap_or_default() }
#[tauri::command]
fn hotkeys_pause(a: bool) { overlay::send(overlay::Cmd::HotkeysPause(a)); }

/* ---------------------------------------------------------------- commands */
#[tauri::command]
fn cmd_save(a: Value) -> Value {
    let Ok(mut c) = serde_json::from_value::<settings::Command>(a) else { return json!({ "ok": false }) };
    if c.id.is_empty() { c.id = settings::new_id(); }
    c.phrases = c.phrases.into_iter().map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect();
    for a in c.actions.iter_mut() { if a.kind == "app" || a.kind == "file" || a.kind == "folder" { a.icon = apps::register(&a.target); } }
    let id = c.id.clone();
    settings::update(|s| match s.commands.iter_mut().find(|x| x.id == c.id) { Some(x) => { let keep = (x.uses, x.last_used); *x = c; x.uses = keep.0; x.last_used = keep.1; } None => s.commands.insert(0, c) });
    json!({ "ok": true, "id": id, "settings": settings::get() })
}
#[tauri::command]
fn cmd_delete(a: String) -> Value {
    // the voice-sample files stay until the next start (voice::prune), so "เลิกทำ" (Undo) brings them back too
    settings::update(|s| { if let Some(i) = s.commands.iter().position(|c| c.id == a) { s.commands.remove(i); } });
    settings_get()
}
/// Undo a delete: put the command back where it was.
#[tauri::command]
fn cmd_restore(a: Value) -> Value {
    let idx = a["index"].as_u64().unwrap_or(0) as usize;
    let Ok(c) = serde_json::from_value::<settings::Command>(a["cmd"].clone()) else { return settings_get() };
    settings::update(|s| { let i = idx.min(s.commands.len()); s.commands.insert(i, c); });
    settings_get()
}
#[tauri::command]
fn cmd_run(a: Value) -> Value {
    let id = a["id"].as_str().unwrap_or("");
    let Some(c) = settings::get().commands.into_iter().find(|c| c.id == id) else { return json!({ "ok": false }) };
    let q = a["q"].as_str().unwrap_or("").to_string();
    actions::run(&c, &q, |ok, total| emit_js("ran", json!({ "ok": ok, "total": total })));
    json!({ "ok": true })
}
/// Run one action from the editor ("ลอง").
#[tauri::command]
fn action_try(a: Value) -> Value {
    let Ok(act) = serde_json::from_value::<settings::Action>(a["action"].clone()) else { return json!({ "ok": false }) };
    let q = a["q"].as_str().unwrap_or("").to_string();
    std::thread::spawn(move || { let _ = unsafe { windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED) }; let ok = actions::run_one(&act, &q); emit_js("ran", json!({ "ok": ok as u32, "total": 1 })); });
    json!({ "ok": true })
}
#[tauri::command]
fn starter_reset() -> Value {
    // add the ready-made commands that are not there yet (by name)
    settings::update(|s| { for c in actions::starter_commands() { if !s.commands.iter().any(|x| x.name == c.name) { s.commands.push(c); } } });
    settings_get()
}

/* ------------------------------------------------------------------ voice */
#[tauri::command]
fn listen_start() -> bool { engine::begin(engine::Trigger::Tap) }
#[tauri::command]
fn listen_stop() { engine::finish(); }
#[tauri::command]
fn listen_cancel() { engine::cancel(); }
#[tauri::command]
fn train(a: String) -> bool { engine::begin(engine::Trigger::Train(a)) }
#[tauri::command]
fn sample_delete(a: Value) -> Value {
    let (target, id) = (a["target"].as_str().unwrap_or("").to_string(), a["id"].as_str().unwrap_or("").to_string());
    settings::update(|s| {
        if target == "__wake" { s.wake_samples.retain(|x| *x != id); }
        else if let Some(c) = s.commands.iter_mut().find(|c| c.id == target) { c.samples.retain(|x| *x != id); }
    });
    voice::remove(&id);
    settings_get()
}
#[tauri::command]
fn history_list() -> Value { engine::history_list() }
#[tauri::command]
fn history_clear() { engine::history_clear(); }
#[tauri::command]
fn add_phrase(a: Value) -> Value {
    let (id, ph) = (a["id"].as_str().unwrap_or("").to_string(), a["phrase"].as_str().unwrap_or("").trim().to_string());
    if ph.is_empty() { return json!({ "ok": false }); }
    settings::update(|s| if let Some(c) = s.commands.iter_mut().find(|c| c.id == id) { if !c.phrases.contains(&ph) { c.phrases.push(ph.clone()); } });
    json!({ "ok": true, "settings": settings::get() })
}
#[tauri::command]
fn engine_warm() { stt::warm(); }
/// Settings → "ดูของจริงบนจอ": the real glow for a few seconds (listening → understood).
#[tauri::command]
fn glow_demo() {
    if engine::busy() { return; }
    std::thread::spawn(|| {
        overlay::send(overlay::Cmd::Listen { train: None });
        std::thread::sleep(std::time::Duration::from_millis(2400));
        overlay::send(overlay::Cmd::Think);
        std::thread::sleep(std::time::Duration::from_millis(900));
        overlay::send(overlay::Cmd::Result { ok: true, title: settings::tr("ตัวอย่างแสงขอบจอ", "Edge glow preview").into(), sub: settings::tr("แบบนี้คือตอนที่โปรแกรมเข้าใจคำสั่ง", "This is how it looks when a command is understood").into() });
    });
}

/// Settings → mic test: live level for 60 s.
static MIC_TEST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[tauri::command]
fn mic_test(a: bool) {
    use std::sync::atomic::Ordering;
    if !a { MIC_TEST.store(false, Ordering::Relaxed); return; }
    if MIC_TEST.swap(true, Ordering::SeqCst) || engine::busy() { return; }
    std::thread::spawn(|| {
        let _ = unsafe { windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED) };
        let st = settings::get();
        let mut cap = audio::Cap::start(&st.mic_id, st.mic_gain);
        let t0 = std::time::Instant::now();
        while MIC_TEST.load(Ordering::Relaxed) && t0.elapsed().as_secs() < 60 && !engine::busy() {
            std::thread::sleep(std::time::Duration::from_millis(50));
            let s = cap.take();
            let peak = s.iter().fold(0f32, |m, v| m.max(v.abs()));
            emit_js("mictest", json!({ "rms": audio::level(), "peak": peak, "ok": !cap.failed.load(Ordering::Relaxed) }));
        }
        cap.end();
        MIC_TEST.store(false, Ordering::Relaxed);
        emit_js("mictest", json!({ "stopped": true }));
    });
}

/* ------------------------------------------------------------------ pickers */
#[tauri::command(async)]
fn apps_list(a: bool) -> Value { Value::Array(apps::list(a)) }
#[tauri::command(async)]
fn pick_path(a: String) -> Value {
    match unsafe { file_dialog(a == "folder", a == "app") } {
        Some(p) => { let key = apps::register(&p); json!({ "ok": true, "path": p, "name": apps::pretty_name(&p), "icon": format!("http://pvo.localhost/icon/{key}.png"), "key": key, "dir": std::path::Path::new(&p).is_dir() }) }
        None => json!({ "ok": false }),
    }
}
/// What a dropped path is (for drag & drop onto the editor).
#[tauri::command]
fn path_info(a: String) -> Value {
    let p = std::path::Path::new(&a);
    let key = apps::register(&a);
    let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let kind = if p.is_dir() { "folder" } else if ["exe", "lnk", "appref-ms", "bat", "cmd"].contains(&ext.as_str()) { "app" } else if ext == "url" { "url" } else { "file" };
    let mut target = a.clone();
    if ext == "url" {
        // internet shortcut → its address
        if let Ok(t) = std::fs::read_to_string(p) { if let Some(u) = t.lines().find_map(|l| l.strip_prefix("URL=")) { target = u.trim().to_string(); } }
    }
    json!({ "kind": kind, "path": target, "name": apps::pretty_name(&a), "icon": format!("http://pvo.localhost/icon/{key}.png"), "key": key })
}
unsafe fn file_dialog(folder: bool, app: bool) -> Option<String> {
    use windows::Win32::System::Com::*;
    use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
    use windows::Win32::UI::Shell::*;
    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    let d: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
    let o = d.GetOptions().ok()?;
    d.SetOptions(o | FOS_FORCEFILESYSTEM | if folder { FOS_PICKFOLDERS } else { FILEOPENDIALOGOPTIONS(0) } | FOS_NODEREFERENCELINKS).ok()?;
    let (n1, s1, n2, s2) = (gfx::wide(settings::tr("โปรแกรม", "Programs")), gfx::wide("*.exe;*.lnk;*.bat;*.cmd;*.appref-ms"), gfx::wide(settings::tr("ทุกไฟล์", "All files")), gfx::wide("*.*"));
    if app && !folder {
        let f = [COMDLG_FILTERSPEC { pszName: windows::core::PCWSTR(n1.as_ptr()), pszSpec: windows::core::PCWSTR(s1.as_ptr()) }, COMDLG_FILTERSPEC { pszName: windows::core::PCWSTR(n2.as_ptr()), pszSpec: windows::core::PCWSTR(s2.as_ptr()) }];
        let _ = d.SetFileTypes(&f);
    }
    let owner = main_win().and_then(|w| w.hwnd().ok()).map(|h| windows::Win32::Foundation::HWND(h.0 as _));
    d.Show(owner).ok()?;
    let it = d.GetResult().ok()?;
    let p = it.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
    let s = p.to_string().ok();
    CoTaskMemFree(Some(p.0 as _));
    s
}

/* ------------------------------------------------- http://pvo.localhost/… */
fn serve(req: &tauri::http::Request<Vec<u8>>) -> tauri::http::Response<Vec<u8>> {
    use tauri::http::{Response, StatusCode};
    let fail = |c: StatusCode| Response::builder().status(c).header("Access-Control-Allow-Origin", "*").body(Vec::new()).unwrap();
    let path = req.uri().path().to_string();
    let (body, mime) = if let Some(k) = path.strip_prefix("/icon/") {
        let _ = unsafe { windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED) };
        match apps::icon_png(k.trim_end_matches(".png")) { Some(b) => (b, "image/png"), None => return fail(StatusCode::NOT_FOUND) }
    } else if let Some(n) = path.strip_prefix("/voice/") {
        let n: String = n.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '.').collect();
        match std::fs::read(settings::voice_dir().join(n)) { Ok(b) => (b, "audio/wav"), Err(_) => return fail(StatusCode::NOT_FOUND) }
    } else { return fail(StatusCode::NOT_FOUND) };
    Response::builder().header("Content-Type", mime).header("Access-Control-Allow-Origin", "*").header("Cache-Control", "max-age=3600").body(body).unwrap()
}

/* ------------------------------------------------------------------ tests */
// PRISM_ACT native items: wav:<file> (say this instead of the mic) · train:<cmd id|__wake>:<file> · glow (show the
// listening look) · lv:<0..1> (pretend the voice is this loud) · think · ok · fail · dump · hide · wait:<ms> · set:<key>=<json> · press (simulate the talk key) · quit
fn native_acts() {
    let acts = std::env::var("PRISM_ACT").unwrap_or_default();
    if acts.is_empty() { return; }
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(1500));
        let dump = settings::data_dir().join("test");
        let wait_idle = || { let t0 = std::time::Instant::now(); std::thread::sleep(std::time::Duration::from_millis(200)); while engine::busy() && t0.elapsed().as_secs() < 60 { std::thread::sleep(std::time::Duration::from_millis(50)); } };
        for a in acts.split(',') {
            let (k, v) = a.split_once(':').unwrap_or((a, ""));
            match k {
                "wav" => {
                    if let Some(s) = std::fs::read(v).ok().and_then(|b| audio::read_wav(&b)) { engine::inject(s); engine::begin(engine::Trigger::Tap); wait_idle(); }
                    else { settings::log(&format!("test: cannot read {v}")); }
                }
                "inj" => { if let Some(s) = std::fs::read(v).ok().and_then(|b| audio::read_wav(&b)) { engine::inject(s); } }
                "idle" => wait_idle(),
                "train" => {
                    let (id, f) = v.split_once(':').unwrap_or((v, ""));
                    // id or command name
                    let id = settings::get().commands.iter().find(|c| c.id == id || c.name == id).map(|c| c.id.clone()).unwrap_or(id.to_string());
                    if let Some(s) = std::fs::read(f).ok().and_then(|b| audio::read_wav(&b)) { engine::inject(s); engine::begin(engine::Trigger::Train(id)); wait_idle(); }
                }
                "wakeprobe" => {
                    let st = settings::get();
                    let ss: Vec<voice::Sample> = st.wake_samples.iter().filter_map(|id| voice::load(id)).collect();
                    if let Some(s) = std::fs::read(v).ok().and_then(|b| audio::read_wav(&b)) {
                        let f = voice::features(&s);
                        let d = ss.iter().map(|x| voice::dtw(&f, &x.feats)).fold(f32::MAX, f32::min);
                        settings::log(&format!("test wakeprobe {v}: d={d:.3} thr={:.3}", voice::threshold(&ss, st.wake_sense) * 0.95));
                    }
                }
                // step 2 of the wake word on a sound file (logs "wake: heard … score … → yes/no")
                "wakecheck" => { if let Some(s) = std::fs::read(v).ok().and_then(|b| audio::read_wav(&b)) { let (ok, t) = engine::wake_check(&s); settings::log(&format!("test wakecheck {v}: {ok} {t:?}")); } }
                // whole wake flow without the mic: <wake wav>;<command wav> — step 2 checks the first, the session hears the second
                "wakesim" => {
                    let (w, c) = v.split_once(';').unwrap_or((v, ""));
                    let rd = |f: &str| std::fs::read(f).ok().and_then(|b| audio::read_wav(&b));
                    if let (Some(ws), Some(cs)) = (rd(w), rd(c)) { engine::sim_wake(ws, cs); wait_idle(); }
                }
                "learnwake" => { engine::learn_wake(); std::thread::sleep(std::time::Duration::from_millis(300)); let t0 = std::time::Instant::now(); while settings::get().wake_samples.iter().any(|id| !settings::get().wake_heard.contains_key(id)) && t0.elapsed().as_secs() < 90 { std::thread::sleep(std::time::Duration::from_millis(200)); } }
                "glow" => overlay::send(overlay::Cmd::Listen { train: None }),
                "think" => overlay::send(overlay::Cmd::Think),
                "lv" => overlay::FORCE_LV.store(v.parse::<f32>().unwrap_or(-1.0).to_bits(), std::sync::atomic::Ordering::Relaxed),
                "ok" => overlay::send(overlay::Cmd::Result { ok: true, title: "YouTube".into(), sub: "“เปิดยูทูป”".into() }),
                "fail" => overlay::send(overlay::Cmd::Result { ok: false, title: settings::tr("ไม่ตรงกับคำสั่งไหน", "No matching command").into(), sub: format!("{} “วันนี้อากาศดีมาก”", settings::tr("ได้ยินว่า", "Heard")) }),
                "hide" => overlay::send(overlay::Cmd::Hide),
                "dump" => { overlay::send(overlay::Cmd::Dump(dump.to_string_lossy().into())); std::thread::sleep(std::time::Duration::from_millis(300)); }
                "wait" => std::thread::sleep(std::time::Duration::from_millis(v.parse().unwrap_or(500))),
                "warm" => { stt::warm(); let t0 = std::time::Instant::now(); while !stt::running() && t0.elapsed().as_secs() < 40 { std::thread::sleep(std::time::Duration::from_millis(100)); } settings::log(&format!("test: engine ready in {} ms", t0.elapsed().as_millis())); }
                "set" => { let (k, val) = v.split_once('=').unwrap_or((v, "")); let jv: Value = serde_json::from_str(val).unwrap_or(Value::String(val.into())); settings::merge(&json!({ k: jv })); overlay::send(overlay::Cmd::Reload); }
                "press" => {
                    // hold the talk key for v ms (0 = tap) with real key events
                    let ms: u64 = v.parse().unwrap_or(0);
                    let keys = settings::get().hotkey;
                    let parts: Vec<&str> = keys.split('+').collect();
                    let down: Vec<String> = parts.iter().map(|s| s.to_string()).collect();
                    press_keys(&down, true); std::thread::sleep(std::time::Duration::from_millis(ms.max(40))); press_keys(&down, false);
                }
                "quit" => { if let Some(a) = APP.get() { quit_app(a.clone()); } }
                _ => {}
            }
        }
    });
}
fn press_keys(keys: &[String], down: bool) {
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    let vk = |k: &str| -> u16 { match k.to_ascii_lowercase().as_str() { "ctrl" => 0x11, "alt" => 0x12, "shift" => 0x10, "win" => 0x5B, _ => overlay::parse_hotkey(k).map(|x| x.1 as u16).unwrap_or(0) } };
    let order: Vec<&String> = if down { keys.iter().collect() } else { keys.iter().rev().collect() };
    let v: Vec<INPUT> = order.iter().map(|k| INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(vk(k)), wScan: 0, dwFlags: if down { KEYBD_EVENT_FLAGS(0) } else { KEYEVENTF_KEYUP }, time: 0, dwExtraInfo: 0 } } }).collect();
    unsafe { SendInput(&v, std::mem::size_of::<INPUT>() as i32); }
}

fn single_instance() -> bool {
    use windows::Win32::Foundation::*;
    use windows::Win32::System::Threading::CreateMutexW;
    use windows::Win32::UI::WindowsAndMessaging::*;
    unsafe {
        let name = gfx::wide("Local\\PrismVoiceOpen.single");
        let m = CreateMutexW(None, true, windows::core::PCWSTR(name.as_ptr()));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let cls = gfx::wide(overlay::HOST_CLASS);
            if let Ok(h) = FindWindowW(windows::core::PCWSTR(cls.as_ptr()), windows::core::PCWSTR::null()) { let _ = PostMessageW(Some(h), overlay::WM_SHOWMAIN, WPARAM(0), LPARAM(0)); }
            return false;
        }
        std::mem::forget(m);
    }
    true
}

fn main() {
    if !single_instance() { return; }
    std::panic::set_hook(Box::new(|i| settings::log(&format!("PANIC {i}"))));
    {
        let exe = std::env::current_exe().unwrap_or_default();
        let mt = std::fs::metadata(&exe).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
        settings::log(&format!("START v{} exe={} exe_mtime={} act={:?} engine={} model={}", env!("CARGO_PKG_VERSION"), exe.display(), mt, std::env::var("PRISM_ACT").ok(), stt::available(), stt::model_name()));
    }
    unsafe { let _ = windows::Win32::Media::timeBeginPeriod(1); }
    engine::set_emitter(|ev, v| emit_js(ev, v));
    overlay::set_hooks(overlay::Hooks { show_main, emit: |ev, v| emit_js(ev, v) });
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            app_log, win_min, win_max, win_close, win_hide, app_ready, app_relaunch, about_info, about_openRepo, update_check, update_apply,
            settings_get, settings_set, sys_info, hotkeys_status, hotkeys_pause,
            cmd_save, cmd_delete, cmd_restore, cmd_run, action_try, starter_reset,
            listen_start, listen_stop, listen_cancel, train, glow_demo, sample_delete, history_list, history_clear, add_phrase, engine_warm, mic_test,
            apps_list, pick_path, path_info
        ])
        .register_asynchronous_uri_scheme_protocol("pvo", |_ctx, req, responder| { std::thread::spawn(move || responder.respond(serve(&req))); })
        .setup(|app| {
            let _ = APP.set(app.handle().clone());
            let _ = settings::get(); // seeds the starter commands on first run
            startup_sync();
            overlay::start();
            if let Err(e) = setup_tray(app) { settings::log(&format!("tray failed: {e}")); }
            stt::watchdog();
            engine::wake_loop();
            // load the speech engine in the background so the first command is quick
            std::thread::spawn(|| { std::thread::sleep(std::time::Duration::from_secs(2)); stt::warm(); });
            std::thread::spawn(|| { let _ = apps::list(false); voice::prune(); });
            native_acts();
            Ok(())
        })
        .on_window_event(|w, e| {
            if let tauri::WindowEvent::Resized(_) = e {
                if let Some(ww) = w.app_handle().get_webview_window(w.label()) { let min = ww.is_minimized().unwrap_or(false); page_visible(&ww, !min && ww.is_visible().unwrap_or(true)); }
            }
            if let tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) = e {
                emit_js("drop", json!({ "paths": paths.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>() }));
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = e {
                // X only hides the window; Prism VoiceOpen keeps listening for its key in the tray
                api.prevent_close();
                if let Some(ww) = w.app_handle().get_webview_window(w.label()) { let _ = ww.hide(); page_visible(&ww, false); }
                // a key being re-assigned must not leave the global keys switched off while hidden
                overlay::send(overlay::Cmd::HotkeysPause(false));
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Prism VoiceOpen");
}
