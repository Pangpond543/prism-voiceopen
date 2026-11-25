// Native side, on its own thread: the global push-to-talk key (hold / tap), the glowing screen edge while the mic is
// open, and the small glass caption ("กำลังฟัง…" → "✓ เปิด YouTube"). Glow = four click-through layered strips per
// monitor painted with a flowing Prism gradient whose thickness breathes with your voice. Nothing runs while idle.
use crate::engine::{self, Trigger};
use crate::gfx::{self, rgba, Layer};
use crate::settings::{self, tr};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;
use tiny_skia::*;
use windows::core::PCWSTR;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const WM_CMD: u32 = WM_APP + 2;
pub const WM_SHOWMAIN: u32 = WM_APP + 3;
pub const HOST_CLASS: &str = "PrismVoiceHost";
const HK_ESC: i32 = 9;

static HOST: AtomicIsize = AtomicIsize::new(0);
/// Test hook `lv:<0..1>`: pretend the mic is this loud (f32 bits; negative = use the real mic).
pub static FORCE_LV: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0xBF80_0000);
fn host() -> HWND { HWND(HOST.load(Ordering::Relaxed) as _) }

#[derive(Debug, Clone)]
pub enum Cmd { Listen { train: Option<String> }, Think, Result { ok: bool, title: String, sub: String }, Hide, Reload, HotkeysPause(bool), Dump(String) }
fn queue() -> &'static Mutex<VecDeque<Cmd>> { static Q: OnceLock<Mutex<VecDeque<Cmd>>> = OnceLock::new(); Q.get_or_init(Default::default) }
pub fn send(c: Cmd) { crate::settings::lock(queue()).push_back(c); unsafe { let _ = PostMessageW(Some(host()), WM_CMD, WPARAM(0), LPARAM(0)); } }

pub struct Hooks { pub show_main: fn(), pub emit: fn(&str, serde_json::Value) }
static HOOKS: OnceLock<Hooks> = OnceLock::new();
pub fn set_hooks(h: Hooks) { let _ = HOOKS.set(h); }

#[derive(Clone, Copy, PartialEq, Debug)]
enum Vis { Idle, Listen, Train, Think, Ok, Fail }

struct Strip { hwnd: HWND, layer: Layer }
struct Ui {
    s: f32,
    strips: Vec<Strip>,      // 4 per monitor
    mons: Vec<[i32; 4]>,     // monitors the glow is on
    cap: HWND, cap_layer: Layer, cap_pm: Option<Pixmap>, glow_pms: Vec<(i32, i32, Pixmap)>, // first monitor's strips (test dump)
    vis: Vis, title: String, sub: String, since: Instant, t0: Instant,
    a: f32, lv: f32, hide_at: Option<Instant>, timer: bool,
    key_vk: u32, key_down: Option<Instant>, esc_on: bool, paused: bool,
}
thread_local! { static UI: std::cell::RefCell<Option<Ui>> = const { std::cell::RefCell::new(None) }; }
fn with<R>(f: impl FnOnce(&mut Ui) -> R) -> Option<R> { UI.with(|u| u.try_borrow_mut().ok().and_then(|mut g| g.as_mut().map(f))) }

pub fn start() { std::thread::spawn(|| unsafe { run() }); }
unsafe fn register(name: &str, proc_: WNDPROC) {
    let cls = gfx::wide(name);
    let wc = WNDCLASSEXW { cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32, lpfnWndProc: proc_, lpszClassName: PCWSTR(cls.as_ptr()), hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(), ..Default::default() };
    RegisterClassExW(&wc);
}
unsafe fn run() {
    let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
    register(HOST_CLASS, Some(host_proc));
    register("PrismVoiceGlow", Some(plain_proc));
    register("PrismVoiceCaption", Some(plain_proc));
    let cls = gfx::wide(HOST_CLASS);
    let h = CreateWindowExW(WS_EX_TOOLWINDOW, PCWSTR(cls.as_ptr()), PCWSTR(cls.as_ptr()), WS_POPUP, 0, 0, 1, 1, None, None, None, None).unwrap_or_default();
    HOST.store(h.0 as isize, Ordering::Relaxed);
    let ui = Ui {
        s: gfx::screen_dpi_scale(), strips: Vec::new(), mons: Vec::new(),
        cap: gfx::layered_window("PrismVoiceCaption", WS_EX_TRANSPARENT | WS_EX_NOACTIVATE, true), cap_layer: Layer::default(), cap_pm: None, glow_pms: Vec::new(),
        vis: Vis::Idle, title: String::new(), sub: String::new(), since: Instant::now(), t0: Instant::now(),
        a: 0.0, lv: 0.0, hide_at: None, timer: false, key_vk: 0, key_down: None, esc_on: false, paused: false,
    };
    UI.with(|u| *u.borrow_mut() = Some(ui));
    register_hotkeys(h, true);
    settings::log("overlay: ready");
    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).as_bool() { let _ = TranslateMessage(&msg); DispatchMessageW(&msg); }
}
unsafe extern "system" fn host_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_CMD => {
            loop {
                let c = crate::settings::lock(queue()).pop_front();
                let Some(c) = c else { break };
                with(|u| u.cmd(c));
            }
            LRESULT(0)
        }
        WM_SHOWMAIN => { if let Some(k) = HOOKS.get() { (k.show_main)(); } LRESULT(0) }
        WM_TIMER => { with(|u| u.tick()); LRESULT(0) }
        WM_HOTKEY => { with(|u| u.hotkey(wp.0 as i32)); LRESULT(0) }
        WM_DISPLAYCHANGE => { with(|u| u.s = gfx::screen_dpi_scale()); LRESULT(0) }
        _ => DefWindowProcW(h, m, wp, lp),
    }
}
unsafe extern "system" fn plain_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if m == WM_MOUSEACTIVATE { return LRESULT(MA_NOACTIVATE as isize); }
    if m == WM_NCHITTEST { return LRESULT(-1); } // HTTRANSPARENT
    DefWindowProcW(h, m, wp, lp)
}

/* ------------------------------------------------------------ hotkeys */
pub fn parse_hotkey(t: &str) -> Option<(HOT_KEY_MODIFIERS, u32)> {
    let mut m = HOT_KEY_MODIFIERS(0);
    let mut vk = None;
    for part in t.split('+').map(str::trim).filter(|p| !p.is_empty()) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" => m |= MOD_CONTROL, "alt" => m |= MOD_ALT, "shift" => m |= MOD_SHIFT, "win" => m |= MOD_WIN,
            k => {
                let u = k.to_ascii_uppercase();
                vk = Some(match u.as_str() {
                    x if x.len() == 1 && x.as_bytes()[0].is_ascii_alphanumeric() => x.as_bytes()[0] as u32,
                    x if x.starts_with('F') && x[1..].parse::<u32>().map_or(false, |n| (1..=24).contains(&n)) => 0x6F + x[1..].parse::<u32>().unwrap(),
                    x if x.starts_with("NUM") && x[3..].parse::<u32>().map_or(false, |n| n <= 9) => 0x60 + x[3..].parse::<u32>().unwrap(),
                    "SPACE" => 0x20, "INSERT" => 0x2D, "DELETE" => 0x2E, "HOME" => 0x24, "END" => 0x23, "PAGEUP" => 0x21, "PAGEDOWN" => 0x22,
                    "PAUSE" => 0x13, "SCROLLLOCK" => 0x91, "PRINTSCREEN" => 0x2C, "LEFT" => 0x25, "UP" => 0x26, "RIGHT" => 0x27, "DOWN" => 0x28,
                    "BACKQUOTE" => 0xC0, "NUMADD" => 0x6B, "NUMSUB" => 0x6D, "NUMMUL" => 0x6A, "NUMDIV" => 0x6F,
                    _ => return None,
                });
            }
        }
    }
    vk.map(|v| (m, v))
}
pub fn hotkey_status() -> &'static Mutex<std::collections::BTreeMap<String, bool>> {
    static S: OnceLock<Mutex<std::collections::BTreeMap<String, bool>>> = OnceLock::new();
    S.get_or_init(Default::default)
}
unsafe fn register_hotkeys(h: HWND, on: bool) {
    let mut status = crate::settings::lock(hotkey_status());
    for (name, id) in settings::HOTKEY_NAMES {
        let _ = UnregisterHotKey(Some(h), id);
        if !on { continue; }
        let text = settings::hotkey(name);
        if text.is_empty() { status.insert(name.into(), true); continue; }
        // no MOD_NOREPEAT for the talk key: we poll its state for "hold" anyway, repeats are ignored
        let ok = parse_hotkey(&text).map_or(false, |(m, vk)| RegisterHotKey(Some(h), id, m | MOD_NOREPEAT, vk).is_ok());
        if !ok { settings::log(&format!("hotkey {text} ({name}) could not be registered")); }
        status.insert(name.into(), ok);
    }
}

impl Ui {
    fn dark(&self) -> bool { settings::get().dark }
    fn cmd(&mut self, c: Cmd) {
        let ph = match &c { Cmd::Listen { train: Some(_) } => Some("train"), Cmd::Listen { .. } => Some("listen"), Cmd::Think => Some("think"), Cmd::Result { .. } | Cmd::Hide => Some("idle"), _ => None };
        if let (Some(p), Some(k)) = (ph, HOOKS.get()) { (k.emit)("state", serde_json::json!({ "phase": p })); }
        match c {
            Cmd::Listen { train } => {
                self.vis = if train.is_some() { Vis::Train } else { Vis::Listen };
                self.title = match &train { Some(p) => format!("{} “{}”", tr("สอนเสียง:", "Teach:"), p), None => tr("กำลังฟัง… พูดได้เลย", "Listening… go ahead").into() };
                self.sub = if train.is_some() { tr("พูดคำนี้หนึ่งครั้ง แล้วหยุดรอ · Esc ยกเลิก", "Say it once, then pause · Esc to cancel").into() } else { String::new() };
                self.since = Instant::now(); self.hide_at = None;
                self.open_glow();
                self.esc(true);
            }
            Cmd::Think => { self.vis = Vis::Think; self.title = tr("กำลังฟังว่าพูดอะไร…", "Working out what you said…").into(); self.sub.clear(); self.since = Instant::now(); self.esc(false); }
            Cmd::Result { ok, title, sub } => {
                if self.vis == Vis::Idle { self.open_glow(); }
                self.vis = if ok { Vis::Ok } else { Vis::Fail };
                self.title = title; self.sub = sub; self.since = Instant::now();
                self.hide_at = Some(Instant::now() + std::time::Duration::from_millis(if ok { 1500 } else { 3200 }));
                self.esc(false);
                self.key_down = None;
            }
            Cmd::Hide => { self.hide_at = Some(Instant::now()); self.esc(false); self.key_down = None; }
            Cmd::Reload => unsafe { if !self.paused { register_hotkeys(host(), true); } },
            Cmd::HotkeysPause(p) => unsafe { self.paused = p; register_hotkeys(host(), !p); },
            Cmd::Dump(dir) => self.dump(&dir),
        }
        self.ensure_timer();
    }
    fn esc(&mut self, on: bool) {
        if on == self.esc_on { return; }
        unsafe {
            if on { self.esc_on = RegisterHotKey(Some(host()), HK_ESC, MOD_NOREPEAT, 0x1B).is_ok(); } else { let _ = UnregisterHotKey(Some(host()), HK_ESC); self.esc_on = false; }
        }
    }
    fn ensure_timer(&mut self) {
        if !self.timer { unsafe { SetTimer(Some(host()), 1, 24, None); /* ~40 fps: smooth for a glow, less CPU than 60 */ } self.timer = true; }
    }
    fn hotkey(&mut self, id: i32) {
        match id {
            1 => {
                let st = settings::get();
                if engine::busy() {
                    // a tapped (hands-free) session: pressing again = done talking
                    if self.key_down.is_none() && matches!(self.vis, Vis::Listen | Vis::Train) { engine::finish(); }
                    return;
                }
                let hold = st.mode != "tap";
                if !engine::begin(if hold { Trigger::Hold } else { Trigger::Tap }) { return; }
                if hold {
                    self.key_vk = parse_hotkey(&st.hotkey).map(|k| k.1).unwrap_or(0);
                    self.key_down = Some(Instant::now());
                }
                self.ensure_timer();
            }
            2 => { if let Some(k) = HOOKS.get() { (k.show_main)(); } }
            3 => engine::repeat_last(),
            HK_ESC => { engine::cancel(); self.key_down = None; }
            _ => {}
        }
    }

    /* ------------------------------------------------------------ glow */
    fn open_glow(&mut self) {
        let st = settings::get();
        let mut mons = monitors();
        if !st.glow_all || mons.is_empty() {
            let (cx, cy) = gfx::cursor_pos();
            if let Some(m) = mons.iter().find(|m| cx >= m[0] && cx < m[0] + m[2] && cy >= m[1] && cy < m[1] + m[3]).copied().or(mons.first().copied()) { mons = vec![m]; }
        }
        let need = mons.len() * 4;
        while self.strips.len() < need { self.strips.push(Strip { hwnd: gfx::layered_window("PrismVoiceGlow", WS_EX_TRANSPARENT | WS_EX_NOACTIVATE, true), layer: Layer::default() }); }
        for st in self.strips.iter().skip(need) { unsafe { let _ = ShowWindow(st.hwnd, SW_HIDE); } }
        self.mons = mons;
        if self.a <= 0.001 { self.t0 = Instant::now(); }
    }
    fn tick(&mut self) {
        // hold-to-talk: watch the key
        if let Some(t) = self.key_down {
            let down = self.key_vk != 0 && unsafe { GetAsyncKeyState(self.key_vk as i32) } as u16 & 0x8000 != 0;
            if !down {
                self.key_down = None;
                let held = t.elapsed().as_millis();
                if held < 350 && settings::get().mode == "both" {
                    engine::to_tap(); // only tapped: keep listening until the user stops talking
                    self.sub = tr("หยุดพูดแล้วจะทำงานเอง · Esc ยกเลิก", "Stops when you stop talking · Esc to cancel").into();
                } else { engine::finish(); }
            }
        }
        let target = match self.vis { Vis::Idle => 0.0, _ => if self.hide_at.map_or(false, |h| Instant::now() >= h) { 0.0 } else { 1.0 } };
        let speed = if target > self.a { 0.16 } else { 0.10 };
        self.a += (target - self.a) * speed;
        if (target - self.a).abs() < 0.004 { self.a = target; }
        let raw = crate::audio::level();
        let forced = f32::from_bits(FORCE_LV.load(Ordering::Relaxed));
        let lvn = if forced >= 0.0 { forced.min(1.0) } else { ((20.0 * raw.max(1e-5).log10() + 52.0) / 36.0).clamp(0.0, 1.0) };
        self.lv = if lvn > self.lv { self.lv + (lvn - self.lv) * 0.5 } else { self.lv * 0.9 + lvn * 0.1 };
        if self.a <= 0.0 && target == 0.0 {
            for s in &self.strips { unsafe { let _ = ShowWindow(s.hwnd, SW_HIDE); } }
            unsafe { let _ = ShowWindow(self.cap, SW_HIDE); }
            self.vis = Vis::Idle; self.hide_at = None;
            if self.key_down.is_none() { unsafe { let _ = KillTimer(Some(host()), 1); } self.timer = false; }
            return;
        }
        if self.hint_due() { self.sub = if engine::hold_mode() { format!("{} {} {}", tr("ปล่อย", "Release"), settings::get().hotkey, tr("เมื่อพูดจบ · Esc ยกเลิก", "when you're done · Esc to cancel")) } else { tr("หยุดพูดแล้วจะทำงานเอง · Esc ยกเลิก", "Stops when you stop talking · Esc to cancel").into() }; }
        self.paint_glow();
        if settings::get().caption { self.paint_caption(); } else { unsafe { let _ = ShowWindow(self.cap, SW_HIDE); } }
    }
    fn hint_due(&self) -> bool { matches!(self.vis, Vis::Listen) && self.sub.is_empty() }

    fn palette(&self) -> [(f32, f32, f32); 4] {
        let h = |s: &str| { let c = gfx::hex(s, 1.0); (c.red(), c.green(), c.blue()) };
        let p = match settings::get().glow.as_str() {
            "aurora" => ["#2ee8b0", "#3d8bff", "#9a63ff", "#1fd3e0"],
            "sunset" => ["#ff4f7a", "#ffaa2e", "#ff7440", "#b863ff"],
            "ocean" => ["#1ec4ff", "#4466ff", "#14e3c4", "#7488ff"],
            "mono" => ["#ffffff", "#dfe6ff", "#ffffff", "#e8ecff"],
            _ => ["#7a68ff", "#ff5cbc", "#ff9d5c", "#3fc4ff"],
        };
        [h(p[0]), h(p[1]), h(p[2]), h(p[3])]
    }
    fn paint_glow(&mut self) {
        let st = settings::get();
        let t = self.t0.elapsed().as_secs_f32();
        let since = self.since.elapsed().as_secs_f32();
        let pal = self.palette();
        let size = st.glow_size.clamp(0.5, 1.6);
        let base_s = self.s;
        // state look: thickness, flow speed, brightness, tint
        let (mut k, speed, mut bright, tint): (f32, f32, f32, Option<(f32, f32, f32)>) = match self.vis {
            Vis::Listen | Vis::Train => (0.24 + 0.22 * self.lv + 0.03 * (t * 2.2).sin(), 0.07, 0.86 + 0.14 * self.lv, None),
            Vis::Think => (0.17 + 0.03 * (t * 5.0).sin(), 0.32, 0.85, None),
            Vis::Ok => (0.30, 0.10, 1.0, Some((0.31, 0.89, 0.65))),
            Vis::Fail => (0.26, 0.05, 0.95, Some((1.0, 0.71, 0.28))),
            Vis::Idle => (0.2, 0.05, 0.0, None),
        };
        if matches!(self.vis, Vis::Ok | Vis::Fail) { let p = (since / 0.35).min(1.0); k *= 0.75 + 0.45 * (1.0 - (1.0 - p).powi(3)); bright *= 1.0 + 0.25 * (1.0 - p); }
        let tint_mix = if tint.is_some() { (since / 0.25).min(1.0) * 0.85 } else { 0.0 };
        let amp = self.a * bright;
        // exp lookup table: e[i] = exp(-i/64)
        thread_local! { static E: Vec<f32> = (0..1024).map(|i| (-(i as f32) / 64.0).exp()).collect(); }
        let mut si = 0;
        let mons = self.mons.clone();
        for m in &mons {
            let (mx, my, mw, mh) = (m[0], m[1], m[2], m[3]);
            let big = (120.0 * dpi_at(mx + mw / 2, my + mh / 2, base_s) * size).round() as i32;
            let tt = big.min(mh / 3).max(8);
            let per = (2 * (mw + mh)) as f32;
            let kpx = (tt as f32 * k).max(2.0);
            // colour + thickness for a perimeter position
            let col_at = |p: f32| -> (f32, f32, f32, f32) {
                let u = (p / per * 2.0 + t * speed).rem_euclid(1.0) * 4.0;
                let i = u.floor() as usize % 4;
                let f = u - u.floor();
                let f = f * f * (3.0 - 2.0 * f);
                let (a, b) = (pal[i], pal[(i + 1) % 4]);
                let mut c = (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f, a.2 + (b.2 - a.2) * f);
                if let Some(tc) = tint { c = (c.0 + (tc.0 - c.0) * tint_mix, c.1 + (tc.1 - c.1) * tint_mix, c.2 + (tc.2 - c.2) * tint_mix); }
                let ang = p / per * std::f32::consts::TAU;
                let wave = 1.0 + 0.24 * (ang * 3.0 + t * 1.9).sin() + 0.12 * (ang * 7.0 - t * 3.1).sin();
                (c.0, c.1, c.2, kpx * wave)
            };
            // The glow is one ring cut into 4 windows. Each window is a bit thicker than the glow needs and the glow fades
            // fully to 0 before the window's inner edge (`fade`), so a loud voice never shows a hard rectangular cut.
            // Corners mix the two edges' glows *and colours* so the ring has no seam or square block at the corners.
            let ww = ((tt as f32 * 1.4) as i32).min(mh / 3).max(8);
            let inv_ww = 1.0 / ww as f32;
            // (x, y, w, h, edge): 0 top, 1 right, 2 bottom, 3 left
            let rects = [(mx, my, mw, ww, 0u8), (mx + mw - ww, my + ww, ww, mh - 2 * ww, 1), (mx, my + mh - ww, mw, ww, 2), (mx, my + ww, ww, mh - 2 * ww, 3)];
            for (rx, ry, rw, rh, edge) in rects {
                if rw <= 0 || rh <= 0 { si += 1; continue; }
                let Some(mut pm) = Pixmap::new(rw as u32, rh as u32) else { si += 1; continue };
                let data = pm.data_mut();
                let y0 = ry - my; // this strip's top, in monitor rows
                E.with(|e| {
                    // perimeter position (clockwise from top-left) of the left / right edge at monitor row y
                    let left_p = |y: i32| per - y as f32;
                    let right_p = |y: i32| (mw + y) as f32;
                    // precompute per column (top/bottom) or per row (left/right)
                    let along = if edge == 0 || edge == 2 { rw } else { rh };
                    let cols: Vec<(f32, f32, f32, f32)> = (0..along).map(|i| col_at(match edge {
                        0 => i as f32,
                        1 => right_p(y0 + i),
                        2 => (mw + mh) as f32 + (mw - 1 - i) as f32,
                        _ => left_p(y0 + i),
                    })).collect();
                    // top/bottom strips also carry the corners: the left and right edges' look for each row
                    let (sl, sr): (Vec<_>, Vec<_>) = if edge == 0 || edge == 2 {
                        (0..rh).map(|yy| (col_at(left_p(y0 + yy)), col_at(right_p(y0 + yy)))).unzip()
                    } else { (Vec::new(), Vec::new()) };
                    let edge_line = tt as f32 * 0.03 + 1.0;
                    // glow + bright rim for distance d from an edge whose glow thickness is kk
                    let look = |d: i32, kk: f32| -> (f32, f32) {
                        if d >= ww { return (0.0, 0.0); }
                        let x1 = ((d as f32 / kk) * 64.0) as usize;
                        let x2 = ((d as f32 / edge_line) * 64.0) as usize;
                        // full strength for the inner 40 %, then an S-curve down to exactly 0 at the window edge
                        let f = ((1.0 - d as f32 * inv_ww) / 0.6).min(1.0);
                        let fade = f * f * (3.0 - 2.0 * f);
                        (if x1 < 1024 { e[x1] * fade } else { 0.0 }, if x2 < 1024 { e[x2] } else { 0.0 })
                    };
                    for yy in 0..rh {
                        for xx in 0..rw {
                            let (d, ci) = match edge {
                                0 => (yy, xx),
                                2 => (rh - 1 - yy, xx),
                                1 => (rw - 1 - xx, yy),
                                _ => (xx, yy),
                            };
                            let (r1, g1, b1, k1) = cols[ci as usize];
                            let (s1, l1) = look(d, k1);
                            // the nearer side edge (only inside the corner squares of the top/bottom strips)
                            let (mut r, mut g, mut b) = (r1, g1, b1);
                            let (mut soft, mut line) = (s1, l1);
                            if !sl.is_empty() && (xx < ww || xx >= rw - ww) {
                                let (dside, c2) = if xx < ww { (xx, sl[yy as usize]) } else { (rw - 1 - xx, sr[yy as usize]) };
                                let (s2, l2) = look(dside, c2.3);
                                soft = s1 + s2 - s1 * s2;
                                line = l1.max(l2);
                                let w = s1 + s2 + l1 + l2;
                                if w > 1e-6 {
                                    let (w1, w2) = ((s1 + l1) / w, (s2 + l2) / w);
                                    r = r1 * w1 + c2.0 * w2; g = g1 * w1 + c2.1 * w2; b = b1 * w1 + c2.2 * w2;
                                }
                            }
                            let a = ((0.92 * soft + 0.5 * line) * amp).min(1.0);
                            if a < 0.004 { continue; }
                            let i = ((yy * rw + xx) * 4) as usize;
                            // brighten toward white at the very edge
                            let wmix = line * 0.35;
                            data[i] = ((r + (1.0 - r) * wmix) * a * 255.0) as u8;
                            data[i + 1] = ((g + (1.0 - g) * wmix) * a * 255.0) as u8;
                            data[i + 2] = ((b + (1.0 - b) * wmix) * a * 255.0) as u8;
                            data[i + 3] = (a * 255.0) as u8;
                        }
                    }
                });
                if let Some(s) = self.strips.get_mut(si) {
                    s.layer.push(s.hwnd, &pm, rx, ry);
                    unsafe {
                        if !IsWindowVisible(s.hwnd).as_bool() { let _ = ShowWindow(s.hwnd, SW_SHOWNOACTIVATE); }
                        let _ = SetWindowPos(s.hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                    }
                }
                if si < 4 { if si == 0 { self.glow_pms.clear(); } self.glow_pms.push((rx - mx, ry - my, pm)); }
                si += 1;
            }
        }
    }

    /* --------------------------------------------------------- caption */
    fn paint_caption(&mut self) {
        let (cx0, cy0) = gfx::cursor_pos();
        let s = dpi_at(cx0, cy0, self.s);
        let dark = self.dark();
        let t = self.since.elapsed().as_secs_f32();
        let title_px = 15.5 * s;
        let sub_px = 12.5 * s;
        let sub = ellipsize(&self.sub, sub_px, 520.0 * s);
        let title = ellipsize(&self.title, title_px, 540.0 * s);
        let tw = gfx::text_width(&title, title_px, true).max(if sub.is_empty() { 0.0 } else { gfx::text_width(&sub, sub_px, false) });
        let (pad, ic) = (16.0 * s, 38.0 * s);
        let bars = matches!(self.vis, Vis::Listen | Vis::Train);
        let w = (pad + ic + 14.0 * s + tw + if bars { 58.0 * s } else { 0.0 } + pad).clamp(260.0 * s, 760.0 * s);
        let h = 62.0 * s;
        let m = 22.0 * s; // room for the shadow
        let (pw, ph) = ((w + m * 2.0).ceil() as u32, (h + m * 2.0).ceil() as u32);
        let Some(mut pm) = Pixmap::new(pw, ph) else { return };
        let (x, y) = (m, m);
        gfx::shadow(&mut pm, x, y, w, h, h / 2.0, 16.0 * s, 8.0 * s, if dark { (0, 0, 10) } else { (60, 72, 140) }, if dark { 0.6 } else { 0.32 });
        if let Some(p) = gfx::rrect(x, y, w, h, h / 2.0) {
            let bg = if dark { gfx::lin(x, y, x + w, y + h, &[(0.0, rgba(40, 46, 92, 0.94)), (1.0, rgba(24, 28, 60, 0.94))]) } else { gfx::lin(x, y, x + w, y + h, &[(0.0, rgba(255, 255, 255, 0.96)), (1.0, rgba(244, 246, 255, 0.94))]) };
            pm.fill_path(&p, &gfx::shader_paint(bg), FillRule::Winding, Transform::identity(), None);
            let sheen = gfx::lin(x, y, x, y + h * 0.6, &[(0.0, rgba(255, 255, 255, if dark { 0.10 } else { 0.5 })), (1.0, rgba(255, 255, 255, 0.0))]);
            pm.fill_path(&p, &gfx::shader_paint(sheen), FillRule::Winding, Transform::identity(), None);
        }
        // gradient rim
        let pal = self.palette();
        let c = |i: usize, a: f32| Color::from_rgba(pal[i].0, pal[i].1, pal[i].2, a).unwrap_or(Color::WHITE);
        if let Some(p) = gfx::rrect(x + 0.75 * s, y + 0.75 * s, w - 1.5 * s, h - 1.5 * s, h / 2.0 - 0.75 * s) {
            let rim = gfx::lin(x, y, x + w, y + h, &[(0.0, c(0, 0.9)), (0.35, c(1, 0.75)), (0.7, c(2, 0.6)), (1.0, c(3, 0.9))]);
            pm.stroke_path(&p, &gfx::shader_paint(rim), &gfx::stroke(1.5 * s), Transform::identity(), None);
        }
        // icon disc
        let (cx, cy, r) = (x + pad + ic / 2.0, y + h / 2.0, ic / 2.0);
        let disc = match self.vis {
            Vis::Ok => gfx::lin(cx - r, cy - r, cx + r, cy + r, &[(0.0, rgba(94, 230, 170, 1.0)), (1.0, rgba(54, 196, 160, 1.0))]),
            Vis::Fail => gfx::lin(cx - r, cy - r, cx + r, cy + r, &[(0.0, rgba(255, 196, 92, 1.0)), (1.0, rgba(255, 140, 90, 1.0))]),
            _ => gfx::lin(cx - r, cy - r, cx + r, cy + r, &[(0.0, c(0, 1.0)), (0.6, c(1, 1.0)), (1.0, c(2, 1.0))]),
        };
        if bars {
            // soft halo that grows with the voice
            let hr = r * (1.15 + 0.45 * self.lv);
            if let Some(p) = gfx::circle(cx, cy, hr) { pm.fill_path(&p, &gfx::shader_paint(gfx::radial(cx, cy, hr, &[(0.55, c(1, 0.35)), (1.0, c(0, 0.0))])), FillRule::Winding, Transform::identity(), None); }
        }
        if let Some(p) = gfx::circle(cx, cy, r) { pm.fill_path(&p, &gfx::shader_paint(disc), FillRule::Winding, Transform::identity(), None); }
        let white = rgba(255, 255, 255, 1.0);
        match self.vis {
            Vis::Ok => gfx::icon(&mut pm, "check", cx - 11.0 * s, cy - 11.0 * s, 22.0 * s, white),
            Vis::Fail => gfx::text(&mut pm, "!", cx - gfx::text_width("!", 20.0 * s, true) / 2.0, cy, 20.0 * s, true, white),
            Vis::Think => {
                for i in 0..3 {
                    let ph = t * 7.0 - i as f32 * 0.9;
                    let dy = ph.sin().max(0.0) * 4.5 * s;
                    if let Some(p) = gfx::circle(cx + (i as f32 - 1.0) * 8.0 * s, cy - dy + 1.0 * s, 2.6 * s) { pm.fill_path(&p, &gfx::solid(white), FillRule::Winding, Transform::identity(), None); }
                }
            }
            _ => gfx::icon(&mut pm, "mic", cx - 10.0 * s, cy - 10.0 * s, 20.0 * s, white),
        }
        // text
        let ink = if dark { rgba(236, 240, 255, 1.0) } else { rgba(35, 42, 71, 1.0) };
        let ink2 = if dark { rgba(176, 186, 226, 1.0) } else { rgba(96, 104, 140, 1.0) };
        let tx = x + pad + ic + 14.0 * s;
        if sub.is_empty() { gfx::text(&mut pm, &title, tx, cy, title_px, true, ink); }
        else { gfx::text(&mut pm, &title, tx, cy - 9.5 * s, title_px, true, ink); gfx::text(&mut pm, &sub, tx, cy + 11.0 * s, sub_px, false, ink2); }
        // live level bars
        if bars {
            let bx = x + w - pad - 44.0 * s;
            for i in 0..5 {
                let wob = ((t * 9.0 + i as f32 * 1.3).sin() * 0.5 + 0.5) * 0.35 + 0.65;
                let hh = (4.0 * s + 22.0 * s * self.lv * wob * [0.55, 0.85, 1.0, 0.8, 0.5][i]).min(26.0 * s);
                if let Some(p) = gfx::rrect(bx + i as f32 * 9.0 * s, cy - hh / 2.0, 5.0 * s, hh, 2.5 * s) { pm.fill_path(&p, &gfx::solid(c(i % 4, 0.95)), FillRule::Winding, Transform::identity(), None); }
            }
        }
        // fade + slide with the glow
        if self.a < 0.999 {
            let k = self.a.clamp(0.0, 1.0);
            for px in pm.pixels_mut() { *px = PremultipliedColorU8::from_rgba((px.red() as f32 * k) as u8, (px.green() as f32 * k) as u8, (px.blue() as f32 * k) as u8, (px.alpha() as f32 * k) as u8).unwrap_or(*px); }
        }
        // where: bottom centre of the monitor with the mouse (work area, above the taskbar)
        let wa = work_area_at_cursor();
        let lift = (1.0 - self.a) * 14.0 * s;
        let px = wa[0] + (wa[2] - pw as i32) / 2;
        let py = wa[1] + wa[3] - ph as i32 - (36.0 * s) as i32 + lift as i32;
        self.cap_layer.push(self.cap, &pm, px, py);
        unsafe {
            if !IsWindowVisible(self.cap).as_bool() { let _ = ShowWindow(self.cap, SW_SHOWNOACTIVATE); }
            let _ = SetWindowPos(self.cap, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
        self.cap_pm = Some(pm);
    }

    /// Test hook: our windows are hidden from screen capture, so write their pixels to PNG.
    fn dump(&mut self, dir: &str) {
        let _ = std::fs::create_dir_all(dir);
        let stamp = settings::now_ms();
        if let Some(p) = &self.cap_pm { let _ = p.save_png(std::path::Path::new(dir).join(format!("caption-{stamp}.png"))); }
        if !self.glow_pms.is_empty() {
            // the strips on their own are mostly transparent: put the whole ring over a sample desktop colour
            let w = self.glow_pms.iter().map(|(x, _, p)| x + p.width() as i32).max().unwrap_or(1);
            let h = self.glow_pms.iter().map(|(_, y, p)| y + p.height() as i32).max().unwrap_or(1);
            if let Some(mut bg) = Pixmap::new(w as u32, h as u32) {
                bg.fill(if self.dark() { Color::from_rgba8(24, 26, 34, 255) } else { Color::from_rgba8(236, 238, 244, 255) });
                for (x, y, p) in &self.glow_pms { bg.draw_pixmap(*x, *y, p.as_ref(), &PixmapPaint::default(), Transform::identity(), None); }
                let _ = bg.save_png(std::path::Path::new(dir).join(format!("glow-{stamp}.png")));
            }
        }
        settings::log(&format!("dump: vis={:?} a={:.2} title={:?}", self.vis, self.a, self.title));
    }
}

fn ellipsize(s: &str, px: f32, max: f32) -> String {
    if s.is_empty() || gfx::text_width(s, px, false) <= max { return s.to_string(); }
    let chars: Vec<char> = s.chars().collect();
    let (mut lo, mut hi) = (0, chars.len());
    while lo < hi { let mid = (lo + hi + 1) / 2; let t: String = chars[..mid].iter().collect::<String>() + "…"; if gfx::text_width(&t, px, false) <= max { lo = mid; } else { hi = mid - 1; } }
    chars[..lo].iter().collect::<String>() + "…"
}

fn monitors() -> Vec<[i32; 4]> {
    unsafe extern "system" fn cb(_h: HMONITOR, _dc: HDC, r: *mut RECT, lp: LPARAM) -> windows::core::BOOL {
        let v = &mut *(lp.0 as *mut Vec<[i32; 4]>);
        let r = *r;
        v.push([r.left, r.top, r.right - r.left, r.bottom - r.top]);
        windows::core::BOOL(1)
    }
    let mut v: Vec<[i32; 4]> = Vec::new();
    unsafe { let _ = EnumDisplayMonitors(None, None, Some(cb), LPARAM(&mut v as *mut _ as isize)); }
    v
}
/// Scale (1.0 = 96 dpi) of the monitor that contains this point — monitors can have different scaling.
fn dpi_at(x: i32, y: i32, fallback: f32) -> f32 {
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    unsafe {
        let m = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
        let (mut dx, mut dy) = (0u32, 0u32);
        if GetDpiForMonitor(m, MDT_EFFECTIVE_DPI, &mut dx, &mut dy).is_ok() && dx > 0 { dx as f32 / 96.0 } else { fallback }
    }
}
fn work_area_at_cursor() -> [i32; 4] {
    let (x, y) = gfx::cursor_pos();
    unsafe {
        let m = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if GetMonitorInfoW(m, &mut mi).as_bool() { let r = mi.rcWork; return [r.left, r.top, r.right - r.left, r.bottom - r.top]; }
    }
    [0, 0, 1920, 1040]
}
