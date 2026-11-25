// What a command does: open a program / link / file / folder, search a site, control the PC (volume, media, lock …),
// drive another Prism app, type text, press keys, wait. Several actions run in order (= a macro).
use crate::settings::{self, Action, Command};
use std::path::PathBuf;
use windows::core::PCWSTR;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

fn w(s: &str) -> Vec<u16> { crate::gfx::wide(s) }
fn shell(file: &str, args: &str) -> bool {
    let (f, a, verb) = (w(file), w(args), w("open"));
    let r = unsafe { ShellExecuteW(None, PCWSTR(verb.as_ptr()), PCWSTR(f.as_ptr()), if args.is_empty() { PCWSTR::null() } else { PCWSTR(a.as_ptr()) }, PCWSTR::null(), SW_SHOWNORMAL) };
    r.0 as isize > 32
}
pub fn urlenc(s: &str) -> String { s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else if b == b' ' { "+".into() } else { format!("%{b:02X}") }).collect() }
fn fix_url(u: &str) -> String {
    let u = u.trim();
    if u.contains("://") || u.starts_with("mailto:") || u.starts_with("ms-") || u.starts_with("shell:") { u.to_string() } else { format!("https://{u}") }
}
/// Home page of a search template ("https://www.youtube.com/results?search_query={q}" → "https://www.youtube.com/").
fn origin(u: &str) -> String {
    let u = fix_url(u);
    match u.find("://") { Some(i) => { let rest = &u[i + 3..]; let host = rest.split('/').next().unwrap_or(rest); format!("{}://{}/", &u[..i], host) } None => u }
}

/* ------------------------------------------------------------------ keys */
fn vk_of(k: &str) -> Option<u16> {
    let u = k.trim().to_ascii_uppercase();
    Some(match u.as_str() {
        x if x.len() == 1 && x.as_bytes()[0].is_ascii_alphanumeric() => x.as_bytes()[0] as u16,
        x if x.starts_with('F') && x[1..].parse::<u16>().map_or(false, |n| (1..=24).contains(&n)) => 0x6F + x[1..].parse::<u16>().unwrap(),
        "CTRL" => 0x11, "ALT" => 0x12, "SHIFT" => 0x10, "WIN" => 0x5B,
        "ENTER" => 0x0D, "ESC" | "ESCAPE" => 0x1B, "TAB" => 0x09, "SPACE" => 0x20, "BACKSPACE" => 0x08, "DELETE" => 0x2E, "INSERT" => 0x2D,
        "HOME" => 0x24, "END" => 0x23, "PAGEUP" => 0x21, "PAGEDOWN" => 0x22, "LEFT" => 0x25, "UP" => 0x26, "RIGHT" => 0x27, "DOWN" => 0x28,
        "PRINTSCREEN" => 0x2C, "PAUSE" => 0x13,
        _ => return None,
    })
}
fn key_input(vk: u16, up: bool) -> INPUT {
    let ext = matches!(vk, 0x21..=0x28 | 0x2D | 0x2E | 0x5B | 0xAD..=0xB3);
    let mut f = KEYBD_EVENT_FLAGS(0);
    if up { f |= KEYEVENTF_KEYUP; }
    if ext { f |= KEYEVENTF_EXTENDEDKEY; }
    INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(vk), wScan: 0, dwFlags: f, time: 0, dwExtraInfo: 0 } } }
}
/// Press "Ctrl+Shift+T" style combos.
pub fn press_combo(combo: &str) -> bool {
    let keys: Vec<u16> = combo.split('+').filter_map(vk_of).collect();
    if keys.is_empty() { return false; }
    let mut v: Vec<INPUT> = keys.iter().map(|&k| key_input(k, false)).collect();
    v.extend(keys.iter().rev().map(|&k| key_input(k, true)));
    unsafe { SendInput(&v, std::mem::size_of::<INPUT>() as i32) as usize == v.len() }
}
fn tap(vk: u16, times: usize) { for _ in 0..times { let v = [key_input(vk, false), key_input(vk, true)]; unsafe { SendInput(&v, std::mem::size_of::<INPUT>() as i32); } } }
/// Type text into whatever window has the keyboard (works for Thai, emoji …).
pub fn type_text(s: &str) {
    let mut v: Vec<INPUT> = Vec::new();
    for u in s.encode_utf16() {
        if u == '\n' as u16 { v.push(key_input(0x0D, false)); v.push(key_input(0x0D, true)); continue; }
        for up in [false, true] {
            v.push(INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(0), wScan: u, dwFlags: KEYEVENTF_UNICODE | if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, time: 0, dwExtraInfo: 0 } } });
        }
    }
    for chunk in v.chunks(200) { unsafe { SendInput(chunk, std::mem::size_of::<INPUT>() as i32); } std::thread::sleep(std::time::Duration::from_millis(5)); }
}
/// The user may still be holding the talk key's modifiers (Ctrl…): wait until they are up so typed text isn't Ctrl+letters.
fn wait_keys_up() {
    for _ in 0..60 {
        let down = [0x10, 0x11, 0x12, 0x5B, 0x5C, 0x20].iter().any(|&k| unsafe { GetAsyncKeyState(k) } as u16 & 0x8000 != 0);
        if !down { return; }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

/* ------------------------------------------------------------ suite apps */
pub fn suite_root() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.ancestors().skip(1).take(6).find(|p| p.join("prism-voiceopen").exists() || p.join("prism-rec").exists() || p.join("prism-size").exists()).map(|p| p.to_path_buf())
}
pub fn prism_exe(name: &str) -> Option<PathBuf> {
    let r = suite_root()?;
    let p = match name {
        "size" => r.join("prism-size").join("dist").join("win-unpacked").join("Prism Size.exe"),
        "cam" => r.join("prism-cam").join("app").join("bin").join("prism-cam.exe"),
        "player" => r.join("prism-media").join("app").join("prism-player.exe"),
        "studio" => r.join("prism-media").join("app").join("prism-studio.exe"),
        "rec" => r.join("prism-rec").join("app").join("bin").join("prism-rec.exe"),
        _ => return None,
    };
    p.exists().then_some(p)
}
/// Prism Rec's own shortcut for an action (it may have been changed by the user).
fn rec_key(name: &str, def: &str) -> String {
    let p = PathBuf::from(std::env::var("APPDATA").unwrap_or_default()).join("com.prism.rec").join("settings.json");
    let v: serde_json::Value = std::fs::read_to_string(p).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    v["hotkeys"][name].as_str().filter(|s| !s.is_empty()).unwrap_or(def).to_string()
}
fn rec_running() -> bool {
    let cls = w("PrismRecBar");
    unsafe { windows::Win32::UI::WindowsAndMessaging::FindWindowW(PCWSTR(cls.as_ptr()), PCWSTR::null()).is_ok() }
}

/* ----------------------------------------------------------------- run */
/// Run one action. `q` = the spoken search words (search actions only). Returns false when it could not be done.
pub fn run_one(a: &Action, q: &str) -> bool {
    if std::env::var("PRISM_DRYRUN").is_ok() { settings::log(&format!("dry-run: {} {:?} q={q:?}", a.kind, a.target)); return true; }
    match a.kind.as_str() {
        "app" | "file" | "folder" => {
            let t = a.target.trim();
            if t.is_empty() { return false; }
            if t.starts_with("shell:") || t.contains('!') && !t.contains('\\') { return shell(&if t.starts_with("shell:") { t.to_string() } else { format!("shell:AppsFolder\\{t}") }, &a.args); }
            shell(t, &a.args)
        }
        "url" => !a.target.trim().is_empty() && shell(&fix_url(&a.target), ""),
        "search" => {
            let t = a.target.trim();
            if t.is_empty() { return false; }
            if q.trim().is_empty() { return shell(&origin(t), ""); }
            let url = if t.contains("{q}") { t.replace("{q}", &urlenc(q.trim())) } else { format!("{}{}", fix_url(t), urlenc(q.trim())) };
            shell(&fix_url(&url), "")
        }
        "system" => {
            match a.target.as_str() {
                "vol_up" => tap(0xAF, 5), "vol_down" => tap(0xAE, 5), "mute" => tap(0xAD, 1),
                "media_play" => tap(0xB3, 1), "media_next" => tap(0xB0, 1), "media_prev" => tap(0xB1, 1),
                "lock" => unsafe { let _ = windows::Win32::System::Shutdown::LockWorkStation(); },
                "snip" => { wait_keys_up(); press_combo("Win+Shift+S"); }
                "desktop" => { wait_keys_up(); press_combo("Win+D"); }
                "taskview" => { wait_keys_up(); press_combo("Win+Tab"); }
                "close" => { wait_keys_up(); press_combo("Alt+F4"); }
                "taskmgr" => { shell("taskmgr.exe", ""); }
                "settings" => { shell("ms-settings:", ""); }
                "explorer" => { shell("explorer.exe", ""); }
                _ => return false,
            }
            true
        }
        "prism" => {
            let t = a.target.as_str();
            if let Some(app) = t.strip_prefix("open:") { return prism_exe(app).map_or(false, |p| shell(&p.to_string_lossy(), "")); }
            let (key, def) = match t { "rec_toggle" => ("rec", "Ctrl+Alt+R"), "rec_pause" => ("pause", "Ctrl+Alt+P"), "rec_replay" => ("replay", "Ctrl+Alt+S"), "rec_draw" => ("draw", "Ctrl+Alt+D"), _ => return false };
            if !rec_running() {
                // start Prism Rec first (in the tray), then send its shortcut
                let Some(p) = prism_exe("rec") else { return false };
                let _ = std::process::Command::new(p).arg("--bar").spawn();
                for _ in 0..40 { std::thread::sleep(std::time::Duration::from_millis(150)); if rec_running() { break; } }
                std::thread::sleep(std::time::Duration::from_millis(600));
            }
            wait_keys_up();
            press_combo(&rec_key(key, def))
        }
        "type" => { wait_keys_up(); std::thread::sleep(std::time::Duration::from_millis(60)); type_text(&a.target); true }
        "keys" => { wait_keys_up(); press_combo(&a.target) }
        "wait" => { std::thread::sleep(std::time::Duration::from_millis(a.target.trim().parse::<u64>().unwrap_or(500).min(30_000))); true }
        _ => false,
    }
}
/// Run all actions of a command in order (in the background). Calls `done(ok_count, total)` at the end.
pub fn run(c: &Command, q: &str, done: impl FnOnce(usize, usize) + Send + 'static) {
    let acts = c.actions.clone();
    let q = q.to_string();
    let name = c.name.clone();
    std::thread::spawn(move || {
        let _ = unsafe { windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED) };
        let mut ok = 0;
        for (i, a) in acts.iter().enumerate() {
            if i > 0 && a.kind != "wait" { std::thread::sleep(std::time::Duration::from_millis(180)); }
            let r = run_one(a, &q);
            if !r { settings::log(&format!("action failed: {} {} {:?}", name, a.kind, a.target)); }
            ok += r as usize;
        }
        done(ok, acts.len());
    });
}

/* ------------------------------------------------------- first-run set */
fn act(kind: &str, target: &str, label: &str) -> Action { Action { kind: kind.into(), target: target.into(), label: label.into(), ..Default::default() } }
fn cmd(name: &str, phrases: &[&str], actions: Vec<Action>) -> Command {
    Command { id: settings::new_id(), name: name.into(), phrases: phrases.iter().map(|s| s.to_string()).collect(), actions, enabled: true, ..Default::default() }
}
/// Ready-made commands so the app is useful the moment it opens (the user can edit or delete any of them).
pub fn starter_commands() -> Vec<Command> {
    let mut v = vec![
        cmd("YouTube", &["เปิดยูทูป", "ยูทูป", "เปิด YouTube", "YouTube"], vec![act("url", "https://www.youtube.com/", "YouTube")]),
        cmd("ค้นหาใน YouTube", &["หายูทูป", "ค้นยูทูป", "หา YouTube", "ค้นหาใน YouTube"], vec![act("search", "https://www.youtube.com/results?search_query={q}", "YouTube")]),
        cmd("ค้นหาใน Google", &["หากูเกิล", "ค้นหา", "หา Google", "ค้นหาใน Google"], vec![act("search", "https://www.google.com/search?q={q}", "Google")]),
        cmd("Facebook", &["เปิดเฟสบุ๊ค", "เฟสบุ๊ค", "เปิด Facebook", "Facebook"], vec![act("url", "https://www.facebook.com/", "Facebook")]),
        cmd("โฟลเดอร์ดาวน์โหลด", &["เปิดดาวน์โหลด", "โฟลเดอร์ดาวน์โหลด"], vec![act("folder", &PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default()).join("Downloads").to_string_lossy(), "Downloads")]),
        cmd("เครื่องคิดเลข", &["เปิดเครื่องคิดเลข", "เครื่องคิดเลข"], vec![act("app", "calc.exe", "Calculator")]),
        cmd("เพิ่มเสียง", &["เพิ่มเสียง", "เสียงดังขึ้น"], vec![act("system", "vol_up", "")]),
        cmd("ลดเสียง", &["ลดเสียง", "เสียงเบาลง"], vec![act("system", "vol_down", "")]),
        cmd("ปิดเสียง / เปิดเสียง", &["ปิดเสียง", "เปิดเสียง"], vec![act("system", "mute", "")]),
        cmd("เล่น / หยุดเพลง", &["หยุดเพลง", "เล่นเพลง", "เล่นต่อ"], vec![act("system", "media_play", "")]),
        cmd("เพลงถัดไป", &["เพลงถัดไป", "ข้ามเพลง"], vec![act("system", "media_next", "")]),
        cmd("ล็อกหน้าจอ", &["ล็อกจอ", "ล็อคจอ", "ล็อกหน้าจอ"], vec![act("system", "lock", "")]),
        cmd("แคปหน้าจอ", &["แคปจอ", "จับภาพหน้าจอ"], vec![act("system", "snip", "")]),
    ];
    if prism_exe("rec").is_some() {
        v.push(cmd("Prism Rec: เริ่ม / หยุดอัด", &["เริ่มอัด", "หยุดอัด", "อัดหน้าจอ"], vec![act("prism", "rec_toggle", "Prism Rec")]));
        v.push(cmd("Prism Rec: บันทึกย้อนหลัง", &["บันทึกย้อนหลัง", "เซฟย้อนหลัง"], vec![act("prism", "rec_replay", "Prism Rec")]));
    }
    v
}
