// "Pick a program": every app in the Start menu (classic and Store apps), read from the shell's Apps folder — the
// same list Windows shows — with their real icons (PNG, cached on disk, served at http://pvo.localhost/icon/<key>.png).
use crate::settings;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use windows::core::Interface;
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::*;
use windows::Win32::UI::Shell::*;

fn cache() -> &'static Mutex<Option<Vec<Value>>> { static C: OnceLock<Mutex<Option<Vec<Value>>>> = OnceLock::new(); C.get_or_init(|| Mutex::new(None)) }
pub fn icons_dir() -> PathBuf { settings::data_dir().join("icons") }
pub fn key_of(s: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.to_lowercase().hash(&mut h);
    format!("{:016x}", h.finish())
}
fn by_key() -> &'static Mutex<std::collections::HashMap<String, String>> { static M: OnceLock<Mutex<std::collections::HashMap<String, String>>> = OnceLock::new(); M.get_or_init(Default::default) }

/// All Start-menu apps: [{name, target, key, icon}]. Cached after the first call (`fresh` re-reads).
pub fn list(fresh: bool) -> Vec<Value> {
    if !fresh { if let Some(v) = crate::settings::lock(cache()).clone() { return v; } }
    let mut out = Vec::new();
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if let Ok(folder) = SHGetKnownFolderItem::<IShellItem>(&FOLDERID_AppsFolder, KNOWN_FOLDER_FLAG(0), None) {
            if let Ok(en) = folder.BindToHandler::<_, IEnumShellItems>(None, &BHID_EnumItems) {
                loop {
                    let mut items = [None];
                    let mut got = 0u32;
                    if en.Next(&mut items, Some(&mut got)).is_err() || got == 0 { break; }
                    let Some(it) = items[0].take() else { break };
                    let name = it.GetDisplayName(SIGDN_NORMALDISPLAY).ok().and_then(|p| { let s = p.to_string().ok(); CoTaskMemFree(Some(p.0 as _)); s }).unwrap_or_default();
                    let parse = it.GetDisplayName(SIGDN_DESKTOPABSOLUTEPARSING).ok().and_then(|p| { let s = p.to_string().ok(); CoTaskMemFree(Some(p.0 as _)); s }).unwrap_or_default();
                    if name.is_empty() || parse.is_empty() { continue; }
                    let low = name.to_lowercase();
                    if low.contains("uninstall") || low.starts_with("readme") || low.contains("ถอนการติดตั้ง") { continue; }
                    // classic programs have a file path; Store apps have an AppUserModelID
                    let target = if parse.contains(":\\") && std::path::Path::new(&parse).exists() { parse.clone() } else { format!("shell:AppsFolder\\{parse}") };
                    if target.ends_with(".url") || target.ends_with(".chm") || target.ends_with(".txt") || target.ends_with(".pdf") { continue; }
                    let key = key_of(&target);
                    crate::settings::lock(by_key()).insert(key.clone(), target.clone());
                    out.push(json!({ "name": name, "target": target, "key": key, "icon": format!("http://pvo.localhost/icon/{key}.png") }));
                }
            }
        }
    }
    out.sort_by(|a, b| a["name"].as_str().unwrap_or("").to_lowercase().cmp(&b["name"].as_str().unwrap_or("").to_lowercase()));
    out.dedup_by(|a, b| a["name"] == b["name"]);
    settings::log(&format!("apps: {} found", out.len()));
    *crate::settings::lock(cache()) = Some(out.clone());
    out
}
/// Remember a target picked by file dialog / drag-drop so its icon can be served too.
pub fn register(target: &str) -> String { let k = key_of(target); crate::settings::lock(by_key()).insert(k.clone(), target.to_string()); k }

/// PNG bytes of the icon for a key (made once, then read from the disk cache).
pub fn icon_png(key: &str) -> Option<Vec<u8>> {
    let key: String = key.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    let file = icons_dir().join(format!("{key}.png"));
    if let Ok(b) = std::fs::read(&file) { return Some(b); }
    let target = crate::settings::lock(by_key()).get(&key).cloned().or_else(|| {
        // after a restart the map is empty: look the key up among the saved commands
        settings::get().commands.iter().flat_map(|c| c.actions.iter()).map(|a| a.target.clone()).find(|t| key_of(t) == key)
    })?;
    let png = unsafe { render(&target, 64) }?;
    let _ = std::fs::create_dir_all(icons_dir());
    let _ = std::fs::write(&file, &png);
    Some(png)
}

unsafe fn render(target: &str, px: i32) -> Option<Vec<u8>> {
    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    let w = crate::gfx::wide(target);
    let item: IShellItem = SHCreateItemFromParsingName(windows::core::PCWSTR(w.as_ptr()), None).ok()?;
    let f: IShellItemImageFactory = item.cast().ok()?;
    let hbmp = f.GetImage(SIZE { cx: px, cy: px }, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK).ok()?;
    let dc = CreateCompatibleDC(None);
    let mut bmi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: px, biHeight: -px, biPlanes: 1, biBitCount: 32, ..Default::default() }, ..Default::default() };
    let mut buf = vec![0u8; (px * px * 4) as usize];
    let n = GetDIBits(dc, hbmp, 0, px as u32, Some(buf.as_mut_ptr() as _), &mut bmi, DIB_RGB_COLORS);
    let _ = DeleteObject(hbmp.into());
    let _ = DeleteDC(dc);
    if n == 0 { return None; }
    // BGRA (premultiplied, as the shell gives it) → RGBA premultiplied for tiny-skia; no alpha at all = opaque icon
    let has_alpha = buf.chunks_exact(4).any(|p| p[3] != 0);
    for p in buf.chunks_exact_mut(4) { p.swap(0, 2); if !has_alpha { p[3] = 255; } else { p[0] = p[0].min(p[3]); p[1] = p[1].min(p[3]); p[2] = p[2].min(p[3]); } }
    let pm = tiny_skia::Pixmap::from_vec(buf, tiny_skia::IntSize::from_wh(px as u32, px as u32)?)?;
    pm.encode_png().ok()
}

/// Nice name for a file/program path ("C:\…\Spotify.exe" → "Spotify").
pub fn pretty_name(p: &str) -> String {
    let path = std::path::Path::new(p);
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| p.to_string());
    if path.is_dir() { return path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or(stem); }
    stem
}
