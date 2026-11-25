// "Open with Windows": HKCU\...\Run entry + Task Manager's on/off flag (StartupApproved\Run).
// Same file in Prism VoiceOpen and Prism Rec — keep them identical.
use windows::core::PCWSTR;
use windows::Win32::System::Registry::*;

const RUN: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const APPROVED: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run";

fn w(s: &str) -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() }
fn open(path: &str, access: REG_SAM_FLAGS) -> Option<HKEY> {
    let p = w(path);
    let mut h = HKEY::default();
    unsafe { if RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(p.as_ptr()), None, access, &mut h).is_ok() { Some(h) } else { None } }
}
fn read(path: &str, name: &str) -> Option<Vec<u8>> {
    let h = open(path, KEY_READ)?;
    let n = w(name);
    let mut size = 0u32;
    let mut out = None;
    unsafe {
        if RegQueryValueExW(h, PCWSTR(n.as_ptr()), None, None, None, Some(&mut size)).is_ok() {
            let mut buf = vec![0u8; size as usize];
            if RegQueryValueExW(h, PCWSTR(n.as_ptr()), None, None, Some(buf.as_mut_ptr()), Some(&mut size)).is_ok() { buf.truncate(size as usize); out = Some(buf); }
        }
        let _ = RegCloseKey(h);
    }
    out
}
fn del(path: &str, name: &str) {
    if let Some(h) = open(path, KEY_SET_VALUE) { let n = w(name); unsafe { let _ = RegDeleteValueW(h, PCWSTR(n.as_ptr())); let _ = RegCloseKey(h); } }
}
fn command(flag: &str) -> String { format!("\"{}\" {flag}", std::env::current_exe().unwrap_or_default().display()) }

/// Turn it on / off. On also clears a "Disabled" mark Task Manager may have left, so Windows really runs it.
pub fn set(name: &str, flag: &str, on: bool) {
    if on {
        if let Some(h) = open(RUN, KEY_SET_VALUE) {
            let n = w(name);
            let v = w(&command(flag));
            unsafe {
                let bytes = std::slice::from_raw_parts(v.as_ptr() as *const u8, v.len() * 2);
                let _ = RegSetValueExW(h, PCWSTR(n.as_ptr()), None, REG_SZ, Some(bytes));
                let _ = RegCloseKey(h);
            }
        }
    } else {
        del(RUN, name);
    }
    del(APPROVED, name);
}

/// Is it really on right now? (Run entry present and not switched off in Task Manager → Startup apps.)
pub fn active(name: &str) -> bool {
    if read(RUN, name).is_none() { return false; }
    // StartupApproved: first byte 2 / 6 = enabled, 3 / 7 = disabled by the user
    !matches!(read(APPROVED, name), Some(b) if b.first().map_or(false, |x| x & 1 == 1))
}

/// At launch: follow what Windows says (the user may have switched us off in Task Manager), and if we are on,
/// rewrite the entry so it points at this exe (the folder may have moved). Returns the real state.
pub fn sync(name: &str, flag: &str, wanted: bool) -> bool {
    if !wanted { return false; }
    if !active(name) { return false; }
    let cur = read(RUN, name).map(|b| String::from_utf16_lossy(&b.chunks(2).map(|c| u16::from_le_bytes([c[0], *c.get(1).unwrap_or(&0)])).collect::<Vec<_>>()).trim_end_matches('\0').to_string()).unwrap_or_default();
    if cur != command(flag) { set(name, flag, true); }
    true
}
