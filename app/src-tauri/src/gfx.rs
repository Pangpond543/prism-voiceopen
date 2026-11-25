// Drawing helpers for the native windows (floating bar, overlays): tiny-skia for shapes, GDI for text
// (GDI shapes Thai correctly and we load IBM Plex Sans Thai privately), UpdateLayeredWindow for per-pixel alpha.
use std::collections::HashMap;
use std::sync::Mutex;
use tiny_skia::*;
use windows::core::PCWSTR;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub fn wide(s: &str) -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() }

pub fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color { Color::from_rgba8(r, g, b, (a.clamp(0.0, 1.0) * 255.0) as u8) }
pub fn hex(s: &str, a: f32) -> Color {
    let s = s.trim_start_matches('#');
    let v = u32::from_str_radix(s, 16).unwrap_or(0xffd54a);
    rgba((v >> 16) as u8, (v >> 8) as u8, v as u8, a)
}
pub fn solid(c: Color) -> Paint<'static> { let mut p = Paint::default(); p.set_color(c); p.anti_alias = true; p }
pub fn shader_paint(s: Shader) -> Paint { Paint { shader: s, anti_alias: true, ..Paint::default() } }
pub fn lin(x0: f32, y0: f32, x1: f32, y1: f32, stops: &[(f32, Color)]) -> Shader<'static> {
    LinearGradient::new(Point::from_xy(x0, y0), Point::from_xy(x1, y1), stops.iter().map(|(p, c)| GradientStop::new(*p, *c)).collect(), SpreadMode::Pad, Transform::identity())
        .unwrap_or(Shader::SolidColor(stops[0].1))
}
pub fn radial(cx: f32, cy: f32, r: f32, stops: &[(f32, Color)]) -> Shader<'static> {
    RadialGradient::new(Point::from_xy(cx, cy), 0.0, Point::from_xy(cx, cy), r.max(0.1), stops.iter().map(|(p, c)| GradientStop::new(*p, *c)).collect(), SpreadMode::Pad, Transform::identity())
        .unwrap_or(Shader::SolidColor(stops[0].1))
}
/// Prism gradient (--grad): #8ea2ff → #c79bff → #ff9ec7 → #ffc79a
pub fn grad(x0: f32, x1: f32, y: f32, a: f32) -> Shader<'static> {
    lin(x0, y, x1, y, &[(0.0, rgba(0x8e, 0xa2, 0xff, a)), (0.45, rgba(0xc7, 0x9b, 0xff, a)), (0.8, rgba(0xff, 0x9e, 0xc7, a)), (1.0, rgba(0xff, 0xc7, 0x9a, a))])
}

pub fn rrect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<Path> {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    let k = 0.5523 * r;
    let mut p = PathBuilder::new();
    p.move_to(x + r, y);
    p.line_to(x + w - r, y);
    p.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    p.line_to(x + w, y + h - r);
    p.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    p.line_to(x + r, y + h);
    p.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    p.line_to(x, y + r);
    p.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    p.close();
    p.finish()
}
pub fn circle(cx: f32, cy: f32, r: f32) -> Option<Path> { PathBuilder::from_circle(cx, cy, r.max(0.1)) }
pub fn stroke(w: f32) -> Stroke { Stroke { width: w, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Stroke::default() } }

/// Soft drop shadow made of stacked translucent rounded rects (tiny-skia has no blur).
pub fn shadow(pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, r: f32, spread: f32, dy: f32, color: (u8, u8, u8), alpha: f32) {
    let n = 10;
    for i in 0..n {
        let t = i as f32 / n as f32;
        let g = spread * (1.0 - t);
        if let Some(p) = rrect(x - g, y - g + dy, w + 2.0 * g, h + 2.0 * g, r + g) {
            pm.fill_path(&p, &solid(rgba(color.0, color.1, color.2, alpha / n as f32 * (0.4 + t))), FillRule::Winding, Transform::identity(), None);
        }
    }
}

/* ------------------------------------------------------------- SVG icons */
// The suite icons are 24×24 SVG paths drawn with 1.8px round strokes; this parses the subset they use.
pub fn svg_path(d: &str) -> Option<Path> {
    let mut pb = PathBuilder::new();
    let mut toks: Vec<String> = Vec::new();
    let mut cur = String::new();
    let flush = |cur: &mut String, toks: &mut Vec<String>| { if !cur.is_empty() { toks.push(std::mem::take(cur)); } };
    let mut prev = ' ';
    for c in d.chars() {
        if c.is_ascii_alphabetic() && c != 'e' { flush(&mut cur, &mut toks); toks.push(c.to_string()); }
        else if c == ',' || c.is_whitespace() { flush(&mut cur, &mut toks); }
        else if c == '-' && prev != 'e' { flush(&mut cur, &mut toks); cur.push(c); }
        else if c == '.' && cur.contains('.') { flush(&mut cur, &mut toks); cur.push(c); }
        else { cur.push(c); }
        prev = c;
    }
    flush(&mut cur, &mut toks);
    let (mut x, mut y, mut sx, mut sy) = (0f32, 0f32, 0f32, 0f32);
    let (mut lcx, mut lcy) = (0f32, 0f32);
    let mut cmd = 'M';
    let mut i = 0;
    let num = |i: &mut usize| -> f32 { let v = toks.get(*i).and_then(|s| s.parse().ok()).unwrap_or(0.0); *i += 1; v };
    while i < toks.len() {
        if toks[i].chars().next().map_or(false, |c| c.is_ascii_alphabetic()) { cmd = toks[i].chars().next().unwrap(); i += 1; if cmd == 'Z' || cmd == 'z' { pb.close(); x = sx; y = sy; continue; } }
        let rel = cmd.is_ascii_lowercase();
        let (ox, oy) = if rel { (x, y) } else { (0.0, 0.0) };
        match cmd.to_ascii_uppercase() {
            'M' => { x = ox + num(&mut i); y = oy + num(&mut i); pb.move_to(x, y); sx = x; sy = y; cmd = if rel { 'l' } else { 'L' }; lcx = x; lcy = y; }
            'L' => { x = ox + num(&mut i); y = oy + num(&mut i); pb.line_to(x, y); lcx = x; lcy = y; }
            'H' => { x = (if rel { x } else { 0.0 }) + num(&mut i); pb.line_to(x, y); lcx = x; lcy = y; }
            'V' => { y = (if rel { y } else { 0.0 }) + num(&mut i); pb.line_to(x, y); lcx = x; lcy = y; }
            'C' => {
                let (x1, y1, x2, y2) = (ox + num(&mut i), oy + num(&mut i), ox + num(&mut i), oy + num(&mut i));
                x = ox + num(&mut i); y = oy + num(&mut i);
                pb.cubic_to(x1, y1, x2, y2, x, y); lcx = x2; lcy = y2;
            }
            'S' => {
                let (x1, y1) = (2.0 * x - lcx, 2.0 * y - lcy);
                let (x2, y2) = (ox + num(&mut i), oy + num(&mut i));
                x = ox + num(&mut i); y = oy + num(&mut i);
                pb.cubic_to(x1, y1, x2, y2, x, y); lcx = x2; lcy = y2;
            }
            'Q' => {
                let (x1, y1) = (ox + num(&mut i), oy + num(&mut i));
                x = ox + num(&mut i); y = oy + num(&mut i);
                pb.quad_to(x1, y1, x, y); lcx = x1; lcy = y1;
            }
            'A' => {
                let (rx, ry, rot, large, sweep) = (num(&mut i), num(&mut i), num(&mut i), num(&mut i) != 0.0, num(&mut i) != 0.0);
                let (nx, ny) = (ox + num(&mut i), oy + num(&mut i));
                arc_to(&mut pb, x, y, rx, ry, rot, large, sweep, nx, ny);
                x = nx; y = ny; lcx = x; lcy = y;
            }
            _ => { i += 1; }
        }
    }
    pb.finish()
}
#[allow(clippy::too_many_arguments)]
fn arc_to(pb: &mut PathBuilder, x1: f32, y1: f32, rx: f32, ry: f32, rot: f32, large: bool, sweep: bool, x2: f32, y2: f32) {
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx < 1e-4 || ry < 1e-4 || (x1 == x2 && y1 == y2) { pb.line_to(x2, y2); return; }
    let phi = rot.to_radians();
    let (c, s) = (phi.cos(), phi.sin());
    let dx = (x1 - x2) / 2.0; let dy = (y1 - y2) / 2.0;
    let x1p = c * dx + s * dy; let y1p = -s * dx + c * dy;
    let lam = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lam > 1.0 { let k = lam.sqrt(); rx *= k; ry *= k; }
    let num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
    let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let mut co = (num / den).max(0.0).sqrt();
    if large == sweep { co = -co; }
    let cxp = co * rx * y1p / ry; let cyp = -co * ry * x1p / rx;
    let cx = c * cxp - s * cyp + (x1 + x2) / 2.0; let cy = s * cxp + c * cyp + (y1 + y2) / 2.0;
    let ang = |ux: f32, uy: f32, vx: f32, vy: f32| { let a = (ux * vy - uy * vx).atan2(ux * vx + uy * vy); a };
    let t1 = ang(1.0, 0.0, (x1p - cxp) / rx, (y1p - cyp) / ry);
    let mut dt = ang((x1p - cxp) / rx, (y1p - cyp) / ry, (-x1p - cxp) / rx, (-y1p - cyp) / ry);
    if !sweep && dt > 0.0 { dt -= std::f32::consts::TAU; } else if sweep && dt < 0.0 { dt += std::f32::consts::TAU; }
    let segs = (dt.abs() / (std::f32::consts::FRAC_PI_2)).ceil().max(1.0) as i32;
    let d = dt / segs as f32;
    let k = 4.0 / 3.0 * (d / 4.0).tan();
    let mut t = t1;
    for _ in 0..segs {
        let (ct, st, ct2, st2) = (t.cos(), t.sin(), (t + d).cos(), (t + d).sin());
        let p = |ex: f32, ey: f32| (cx + c * rx * ex - s * ry * ey, cy + s * rx * ex + c * ry * ey);
        let (ax, ay) = p(ct - k * st, st + k * ct);
        let (bx, by) = p(ct2 + k * st2, st2 - k * ct2);
        let (ex, ey) = p(ct2, st2);
        pb.cubic_to(ax, ay, bx, by, ex, ey);
        t += d;
    }
}

pub fn icon_d(name: &str) -> &'static str {
    match name {
        "pause" => "M9 6v12M15 6v12",
        "play" => "M8 5.5v13l10.5-6.5z",
        "pen" => "M4 20l1-4.5L16.5 4a2.1 2.1 0 0 1 3 3L8 18.5zM14.5 6l3 3",
        "marker" => "M14.5 4.5l5 5-8 8H7.5v-4zM7.5 17.5L5 20h4.5M12 7l5 5",
        "arrow" => "M5 19L18.5 5.5M10 5.5h8.5V14",
        "rect" => "M6 5.5h12a2.5 2.5 0 0 1 2.5 2.5v8a2.5 2.5 0 0 1-2.5 2.5H6A2.5 2.5 0 0 1 3.5 16V8A2.5 2.5 0 0 1 6 5.5z",
        "cursor" => "M6 4l12 6.5-5.3 1.6-2.2 5.4zM15 15l4 4",
        "mic" => "M12 3.5a3 3 0 0 1 3 3v5a3 3 0 0 1-6 0v-5a3 3 0 0 1 3-3zM5.5 11.5a6.5 6.5 0 0 0 13 0M12 18v3",
        "micoff" => "M12 3.5a3 3 0 0 1 3 3v5a3 3 0 0 1-6 0v-5a3 3 0 0 1 3-3zM5.5 11.5a6.5 6.5 0 0 0 13 0M12 18v3M4 4l16 16",
        "speaker" => "M4 9.5h3.5L12 5.5v13l-4.5-4H4zM15.5 9a4 4 0 0 1 0 6M18.2 6.5a7.6 7.6 0 0 1 0 11",
        "speakeroff" => "M4 9.5h3.5L12 5.5v13l-4.5-4H4zM16 9.5l5 5M21 9.5l-5 5",
        "screen" => "M5.5 4.5h13a2.5 2.5 0 0 1 2.5 2.5v7a2.5 2.5 0 0 1-2.5 2.5h-13A2.5 2.5 0 0 1 3 14V7a2.5 2.5 0 0 1 2.5-2.5zM9 20h6M12 16.5V20",
        "region" => "M4 9V5.5A1.5 1.5 0 0 1 5.5 4H9M15 4h3.5A1.5 1.5 0 0 1 20 5.5V9M20 15v3.5a1.5 1.5 0 0 1-1.5 1.5H15M9 20H5.5A1.5 1.5 0 0 1 4 18.5V15",
        "sliders" => "M4 7h9M18 7h2M4 17h3M11 17h9M15.5 4.5v5M8.5 14.5v5",
        "undo" => "M8 6.5L4 10.5l4 4M4 10.5h10a5 5 0 0 1 0 10h-2",
        "trash" => "M4.5 7h15M9.5 7V4.5h5V7M6.5 7l.8 12a1.5 1.5 0 0 0 1.5 1.4h6.4a1.5 1.5 0 0 0 1.5-1.4l.8-12",
        "check" => "M5 12.5l4.5 4.5L19 7.5",
        "x" => "M6.5 6.5l11 11M17.5 6.5l-11 11",
        "clock" => "M12 3.5a8.5 8.5 0 1 1 0 17a8.5 8.5 0 1 1 0-17zM12 7.5V12l3 2",
        "folder" => "M3.5 7.5a2 2 0 0 1 2-2h4l2 2.5h7a2 2 0 0 1 2 2v7.5a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z",
        "min" => "M6 12.5h12",
        "replay" => "M4.5 12a7.5 7.5 0 1 0 2.2-5.3M4.5 4v3.5H8M12 8v4l2.5 1.6",
        _ => "M12 12h.01",
    }
}
/// Stroke a suite icon (24×24 design) at (x,y) with size `s` px.
pub fn icon(pm: &mut Pixmap, name: &str, x: f32, y: f32, s: f32, color: Color) {
    thread_local! { static CACHE: std::cell::RefCell<HashMap<String, Option<Path>>> = Default::default(); }
    let path = CACHE.with(|c| c.borrow_mut().entry(name.to_string()).or_insert_with(|| svg_path(icon_d(name))).clone());
    let Some(path) = path else { return };
    let k = s / 24.0;
    let tf = Transform::from_row(k, 0.0, 0.0, k, x, y);
    if name == "play" { pm.fill_path(&path, &solid(color), FillRule::Winding, tf, None); }
    pm.stroke_path(&path, &solid(color), &stroke(1.9), tf, None);
}

/* ------------------------------------------------------------------- text */
static FONTS_LOADED: Mutex<bool> = Mutex::new(false);
pub fn load_fonts() {
    let mut done = FONTS_LOADED.lock().unwrap();
    if *done { return; }
    for data in [&include_bytes!("../assets/IBMPlexSansThai-Regular.ttf")[..], &include_bytes!("../assets/IBMPlexSansThai-SemiBold.ttf")[..]] {
        let mut n = 0u32;
        unsafe { AddFontMemResourceEx(data.as_ptr() as _, data.len() as u32, None, &mut n); }
    }
    *done = true;
}

pub struct TextMask { pub w: i32, pub h: i32, pub a: Vec<u8> }
fn font(px: i32, weight: i32) -> HFONT {
    let face = wide("IBM Plex Sans Thai");
    let mut lf = LOGFONTW { lfHeight: -px, lfWeight: weight, lfQuality: ANTIALIASED_QUALITY, lfCharSet: DEFAULT_CHARSET, ..Default::default() };
    for (i, c) in face.iter().take(31).enumerate() { lf.lfFaceName[i] = *c; }
    unsafe { CreateFontIndirectW(&lf) }
}
/// Render `s` white-on-black with GDI and keep the coverage as an alpha mask.
pub fn text_mask(s: &str, px: f32, bold: bool) -> TextMask {
    thread_local! { static CACHE: std::cell::RefCell<HashMap<(String, i32, bool), std::rc::Rc<TextMask>>> = Default::default(); }
    let key = (s.to_string(), (px * 10.0) as i32, bold);
    if let Some(m) = CACHE.with(|c| c.borrow().get(&key).cloned()) { return TextMask { w: m.w, h: m.h, a: m.a.clone() }; }
    load_fonts();
    let ws: Vec<u16> = s.encode_utf16().collect();
    let m = unsafe {
        let dc = CreateCompatibleDC(None);
        let f = font(px.round() as i32, if bold { 600 } else { 400 });
        let old = SelectObject(dc, f.into());
        let mut sz = SIZE::default();
        let _ = GetTextExtentPoint32W(dc, &ws, &mut sz);
        let (w, h) = (sz.cx.max(1) + 4, (sz.cy.max(1) as f32 * 1.15) as i32 + 2);
        let bmi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, ..Default::default() }, ..Default::default() };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let bmp = CreateDIBSection(Some(dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0).unwrap_or_default();
        let oldb = SelectObject(dc, bmp.into());
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, COLORREF(0x00ffffff));
        let mut wsm = ws.clone();
        let mut rc = RECT { left: 2, top: 1, right: w, bottom: h };
        DrawTextW(dc, &mut wsm, &mut rc, DT_LEFT | DT_TOP | DT_SINGLELINE | DT_NOPREFIX);
        let _ = GdiFlush();
        let px = std::slice::from_raw_parts(bits as *const u8, (w * h * 4) as usize);
        let a: Vec<u8> = px.chunks_exact(4).map(|p| ((p[0] as u16 + p[1] as u16 * 2 + p[2] as u16) / 4) as u8).collect();
        SelectObject(dc, oldb);
        SelectObject(dc, old);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteObject(f.into());
        let _ = DeleteDC(dc);
        TextMask { w, h, a }
    };
    let rc = std::rc::Rc::new(TextMask { w: m.w, h: m.h, a: m.a.clone() });
    CACHE.with(|c| { let mut c = c.borrow_mut(); if c.len() > 300 { c.clear(); } c.insert(key, rc); });
    m
}
pub fn text_width(s: &str, px: f32, bold: bool) -> f32 { (text_mask(s, px, bold).w - 4) as f32 }
/// Draw text with its left edge at x and vertical centre at cy.
pub fn text(pm: &mut Pixmap, s: &str, x: f32, cy: f32, px: f32, bold: bool, color: Color) {
    let m = text_mask(s, px, bold);
    let (ox, oy) = (x.round() as i32 - 2, (cy - m.h as f32 / 2.0).round() as i32);
    let (pw, ph) = (pm.width() as i32, pm.height() as i32);
    let c = color.premultiply().to_color_u8();
    let data = pm.data_mut();
    for yy in 0..m.h {
        let ty = oy + yy;
        if ty < 0 || ty >= ph { continue; }
        for xx in 0..m.w {
            let tx = ox + xx;
            if tx < 0 || tx >= pw { continue; }
            let cov = m.a[(yy * m.w + xx) as usize] as u32;
            if cov == 0 { continue; }
            let i = ((ty * pw + tx) * 4) as usize;
            let sa = c.alpha() as u32 * cov / 255;
            let inv = 255 - sa;
            data[i] = ((c.red() as u32 * cov / 255) + data[i] as u32 * inv / 255).min(255) as u8;
            data[i + 1] = ((c.green() as u32 * cov / 255) + data[i + 1] as u32 * inv / 255).min(255) as u8;
            data[i + 2] = ((c.blue() as u32 * cov / 255) + data[i + 2] as u32 * inv / 255).min(255) as u8;
            data[i + 3] = (sa + data[i + 3] as u32 * inv / 255).min(255) as u8;
        }
    }
}

/* ---------------------------------------------------------- layered window */
/// Per-window DIB cache for UpdateLayeredWindow.
pub struct Layer { dc: HDC, bmp: HBITMAP, old: HGDIOBJ, bits: *mut u8, w: i32, h: i32 }
impl Default for Layer { fn default() -> Self { Layer { dc: HDC::default(), bmp: HBITMAP::default(), old: HGDIOBJ::default(), bits: std::ptr::null_mut(), w: 0, h: 0 } } }
impl Layer {
    fn ensure(&mut self, w: i32, h: i32) {
        if self.w == w && self.h == h && !self.bits.is_null() { return; }
        self.free();
        unsafe {
            self.dc = CreateCompatibleDC(None);
            let bmi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, ..Default::default() }, ..Default::default() };
            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            self.bmp = CreateDIBSection(Some(self.dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0).unwrap_or_default();
            self.old = SelectObject(self.dc, self.bmp.into());
            self.bits = bits as *mut u8;
        }
        self.w = w; self.h = h;
    }
    fn free(&mut self) {
        if self.bits.is_null() { return; }
        unsafe { SelectObject(self.dc, self.old); let _ = DeleteObject(self.bmp.into()); let _ = DeleteDC(self.dc); }
        self.bits = std::ptr::null_mut();
    }
    /// Upload a premultiplied RGBA pixmap and place the window at (x,y).
    pub fn push(&mut self, hwnd: HWND, pm: &Pixmap, x: i32, y: i32) {
        let (w, h) = (pm.width() as i32, pm.height() as i32);
        self.ensure(w, h);
        if self.bits.is_null() { return; }
        unsafe {
            let dst = std::slice::from_raw_parts_mut(self.bits, (w * h * 4) as usize);
            for (d, s) in dst.chunks_exact_mut(4).zip(pm.data().chunks_exact(4)) { d[0] = s[2]; d[1] = s[1]; d[2] = s[0]; d[3] = s[3]; }
            let pt_dst = POINT { x, y };
            let sz = SIZE { cx: w, cy: h };
            let pt_src = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
            let _ = UpdateLayeredWindow(hwnd, None, Some(&pt_dst), Some(&sz), Some(self.dc), Some(&pt_src), COLORREF(0), Some(&blend), ULW_ALPHA);
        }
    }
}
impl Drop for Layer { fn drop(&mut self) { self.free(); } }

/// Create a borderless layered popup owned by our class.
pub fn layered_window(class: &str, ex: WINDOW_EX_STYLE, exclude: bool) -> HWND {
    unsafe {
        let cls = wide(class);
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | ex,
            PCWSTR(cls.as_ptr()), PCWSTR(cls.as_ptr()), WS_POPUP, 0, 0, 10, 10, None, None, None, None,
        ).unwrap_or_default();
        if exclude && std::env::var("PRISM_NOEXCLUDE").is_err() {
            // keep our controls out of the recording (and out of screenshots of the desktop)
            let _ = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
        }
        hwnd
    }
}

/* ----------------------------------------------------------- frosted glass */
/// Backdrop resolution: 1/2 of the screen pixels (fine enough for the refraction to show moving content).
pub const BACK_K: f32 = 2.0;
/// Copy the screen under `rc` (our excluded windows are invisible to this), shrink it and blur it.
pub fn backdrop(x: i32, y: i32, w: i32, h: i32) -> Option<Pixmap> {
    let k = BACK_K as i32;
    let (sw, sh) = ((w / k).max(2), (h / k).max(2));
    unsafe {
        let sdc = GetDC(None);
        let dc = CreateCompatibleDC(Some(sdc));
        let bmi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: sw, biHeight: -sh, biPlanes: 1, biBitCount: 32, ..Default::default() }, ..Default::default() };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let bmp = CreateDIBSection(Some(dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        let old = SelectObject(dc, bmp.into());
        SetStretchBltMode(dc, COLORONCOLOR); // the blur smooths it anyway; HALFTONE costs ~3x
        let _ = StretchBlt(dc, 0, 0, sw, sh, Some(sdc), x, y, w, h, SRCCOPY);
        let _ = GdiFlush();
        let src = std::slice::from_raw_parts(bits as *const u8, (sw * sh * 4) as usize).to_vec();
        SelectObject(dc, old);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(dc);
        ReleaseDC(None, sdc);
        let buf: Vec<[f32; 3]> = src.chunks_exact(4).map(|p| [p[2] as f32, p[1] as f32, p[0] as f32]).collect();
        blurred(buf, sw as usize, sh as usize)
    }
}
/// Full-size BGRA (from Desktop Duplication) → shrunk by BACK_K and blurred, like `backdrop`.
pub fn backdrop_from_bgra(px: &[u8], w: u32, h: u32) -> Option<Pixmap> {
    let k = BACK_K as usize;
    let (w, h) = (w as usize, h as usize);
    let (sw, sh) = ((w / k).max(2), (h / k).max(2));
    let mut buf = vec![[0f32; 3]; sw * sh];
    for y in 0..sh {
        for x in 0..sw {
            let mut a = [0f32; 3];
            for dy in 0..k { for dx in 0..k {
                let (xx, yy) = ((x * k + dx).min(w - 1), (y * k + dy).min(h - 1));
                let i = (yy * w + xx) * 4;
                a[0] += px[i + 2] as f32; a[1] += px[i + 1] as f32; a[2] += px[i] as f32;
            } }
            let n = (k * k) as f32;
            buf[y * sw + x] = [a[0] / n, a[1] / n, a[2] / n];
        }
    }
    blurred(buf, sw, sh)
}
fn blurred(mut buf: Vec<[f32; 3]>, sw: usize, sh: usize) -> Option<Pixmap> {
    for _ in 0..3 { box_blur(&mut buf, sw, sh, 3); }
    let mut pm = Pixmap::new(sw as u32, sh as u32)?;
    for (d, s) in pm.data_mut().chunks_exact_mut(4).zip(buf.iter()) { d[0] = s[0] as u8; d[1] = s[1] as u8; d[2] = s[2] as u8; d[3] = 255; }
    Some(pm)
}
fn box_blur(b: &mut [[f32; 3]], w: usize, h: usize, r: usize) {
    let mut tmp = b.to_vec();
    for y in 0..h {
        for x in 0..w {
            let (mut s, mut n) = ([0f32; 3], 0f32);
            for k in x.saturating_sub(r)..=(x + r).min(w - 1) { let p = b[y * w + k]; s[0] += p[0]; s[1] += p[1]; s[2] += p[2]; n += 1.0; }
            tmp[y * w + x] = [s[0] / n, s[1] / n, s[2] / n];
        }
    }
    for y in 0..h {
        for x in 0..w {
            let (mut s, mut n) = ([0f32; 3], 0f32);
            for k in y.saturating_sub(r)..=(y + r).min(h - 1) { let p = tmp[k * w + x]; s[0] += p[0]; s[1] += p[1]; s[2] += p[2]; n += 1.0; }
            b[y * w + x] = [s[0] / n, s[1] / n, s[2] / n];
        }
    }
}

/// Bend the backdrop like a thick glass pill: a slight lens zoom inside, strong inward bending near the rim and a
/// faint red/blue split there (prism dispersion). `pills` = [x, y, w, h, radius] in backdrop pixels.
pub fn refract(src: &Pixmap, pills: &[[f32; 5]]) -> Pixmap {
    let (w, h) = (src.width() as i32, src.height() as i32);
    let mut out = src.clone();
    let sd = src.data();
    let sample = |x: f32, y: f32, c: usize| -> f32 {
        // bilinear
        let (x, y) = (x.clamp(0.0, (w - 1) as f32), y.clamp(0.0, (h - 1) as f32));
        let (x0, y0) = (x.floor() as i32, y.floor() as i32);
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let g = |xx: i32, yy: i32| sd[((yy * w + xx) * 4) as usize + c] as f32;
        (g(x0, y0) * (1.0 - fx) + g(x1, y0) * fx) * (1.0 - fy) + (g(x0, y1) * (1.0 - fx) + g(x1, y1) * fx) * fy
    };
    let od = out.data_mut();
    for p in pills {
        let [px, py, pw, ph, pr] = *p;
        let (cx, cy, hx, hy) = (px + pw / 2.0, py + ph / 2.0, pw / 2.0, ph / 2.0);
        let band = (ph * 0.42).max(2.0);
        let sdf = |x: f32, y: f32| {
            let qx = (x - cx).abs() - (hx - pr); let qy = (y - cy).abs() - (hy - pr);
            (qx.max(0.0).hypot(qy.max(0.0))) + qx.max(qy).min(0.0) - pr
        };
        for y in py.floor() as i32..(py + ph).ceil() as i32 {
            if y < 0 || y >= h { continue; }
            for x in px.floor() as i32..(px + pw).ceil() as i32 {
                if x < 0 || x >= w { continue; }
                let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                let d = -sdf(fx, fy); // depth inside the pill
                if d < 0.0 { continue; }
                // lens zoom toward the centre
                let (mut sx, mut sy) = (cx + (fx - cx) * 0.93, cy + (fy - cy) * 0.93);
                let mut disp = 0.0;
                if d < band {
                    let t = 1.0 - d / band;
                    let (gx, gy) = (sdf(fx + 0.5, fy) - sdf(fx - 0.5, fy), sdf(fx, fy + 0.5) - sdf(fx, fy - 0.5));
                    let n = gx.hypot(gy).max(1e-3);
                    disp = t * t * band * 1.25;
                    sx -= gx / n * disp; sy -= gy / n * disp;
                }
                let i = ((y * w + x) * 4) as usize;
                let ca = disp * 0.3; // chromatic split grows toward the rim
                od[i] = sample(sx + ca, sy, 0) as u8;
                od[i + 1] = sample(sx, sy, 1) as u8;
                od[i + 2] = sample(sx - ca, sy, 2) as u8;
            }
        }
    }
    out
}

/// The Prism glass sheet: blurred backdrop + tint + top light + rim. `dark` = night glass.
pub fn glass(pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, r: f32, back: Option<&Pixmap>, back_origin: (f32, f32), dark: bool, s: f32) {
    glass_shadow(pm, x, y, w, h, r, dark, s);
    glass_sheet(pm, x, y, w, h, r, back, back_origin, dark, s);
}
pub fn glass_shadow(pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, r: f32, dark: bool, s: f32) {
    shadow(pm, x, y, w, h, r, 14.0 * s, 8.0 * s, if dark { (0, 0, 10) } else { (60, 72, 140) }, if dark { 0.55 } else { 0.30 });
}
/// The glass without its drop shadow (the shadow never changes, so the bar caches it).
#[allow(clippy::too_many_arguments)]
pub fn glass_sheet(pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, r: f32, back: Option<&Pixmap>, back_origin: (f32, f32), dark: bool, s: f32) {
    let Some(path) = rrect(x, y, w, h, r) else { return };
    if let Some(b) = back {
        // the backdrop was captured at reduced size: scale it back up with bilinear filtering (= extra softness)
        let tf = Transform::from_row(BACK_K, 0.0, 0.0, BACK_K, back_origin.0, back_origin.1);
        let p = shader_paint(Pattern::new(b.as_ref(), SpreadMode::Pad, FilterQuality::Bilinear, 1.0, tf));
        pm.fill_path(&path, &p, FillRule::Winding, Transform::identity(), None);
    }
    let tint = if dark {
        lin(x, y, x + w, y + h, &[(0.0, rgba(58, 66, 128, 0.70)), (0.5, rgba(30, 36, 78, 0.62)), (1.0, rgba(48, 56, 110, 0.68))])
    } else {
        lin(x, y, x + w, y + h, &[(0.0, rgba(255, 255, 255, 0.52)), (0.48, rgba(255, 255, 255, 0.30)), (1.0, rgba(255, 255, 255, 0.44))])
    };
    pm.fill_path(&path, &shader_paint(tint), FillRule::Winding, Transform::identity(), None);
    let top = lin(x, y, x, y + h * 0.55, &[(0.0, rgba(255, 255, 255, if dark { 0.10 } else { 0.34 })), (1.0, rgba(255, 255, 255, 0.0))]);
    pm.fill_path(&path, &shader_paint(top), FillRule::Winding, Transform::identity(), None);
    let ra = if dark { 0.45 } else { 1.0 };
    let rim = lin(x, y, x + w, y + h, &[(0.0, rgba(255, 255, 255, ra)), (0.3, rgba(255, 255, 255, 0.25 * ra)), (0.55, rgba(255, 255, 255, 0.08 * ra)), (1.0, rgba(255, 255, 255, 0.85 * ra))]);
    if let Some(p2) = rrect(x + 0.6 * s, y + 0.6 * s, w - 1.2 * s, h - 1.2 * s, r - 0.6 * s) {
        pm.stroke_path(&p2, &shader_paint(rim), &stroke(1.2 * s), Transform::identity(), None);
    }
}

pub fn screen_dpi_scale() -> f32 {
    unsafe { windows::Win32::UI::HiDpi::GetDpiForSystem() as f32 / 96.0 }
}
pub fn cursor_pos() -> (i32, i32) { let mut p = POINT::default(); unsafe { let _ = GetCursorPos(&mut p); } (p.x, p.y) }
