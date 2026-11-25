// Tiny sound cues (made in code, no files): mic opens / understood / not understood / cancelled. Soft sine "glass"
// tones with a quick fade so they never sound harsh. Off in Settings → "เสียงแจ้ง".
use std::sync::OnceLock;
use windows::core::PCWSTR;
use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};

#[derive(Clone, Copy)]
pub enum Cue { Open, Ok, Fail, Cancel }

const SR: u32 = 44100;
fn tone(notes: &[(f32, f32, f32)], vol: f32) -> Vec<u8> {
    // notes: (frequency Hz, start s, length s)
    let total = notes.iter().map(|n| n.1 + n.2).fold(0.0, f32::max) + 0.05;
    let n = (total * SR as f32) as usize;
    let mut s = vec![0f32; n];
    for &(f, st, len) in notes {
        let a = (st * SR as f32) as usize;
        let m = (len * SR as f32) as usize;
        for i in 0..m {
            if a + i >= n { break; }
            let t = i as f32 / SR as f32;
            let env = (t / 0.008).min(1.0) * (-(t / len) * 4.2).exp();
            s[a + i] += ((2.0 * std::f32::consts::PI * f * t).sin() + 0.25 * (4.0 * std::f32::consts::PI * f * t).sin()) * env * vol;
        }
    }
    let data: Vec<u8> = s.iter().flat_map(|v| ((v.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()).collect();
    let mut b = Vec::with_capacity(44 + data.len());
    b.extend_from_slice(b"RIFF"); b.extend_from_slice(&(36 + data.len() as u32).to_le_bytes()); b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes()); b.extend_from_slice(&1u16.to_le_bytes()); b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&SR.to_le_bytes()); b.extend_from_slice(&(SR * 2).to_le_bytes()); b.extend_from_slice(&2u16.to_le_bytes()); b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data"); b.extend_from_slice(&(data.len() as u32).to_le_bytes()); b.extend_from_slice(&data);
    b
}
fn bank() -> &'static [Vec<u8>; 4] {
    static B: OnceLock<[Vec<u8>; 4]> = OnceLock::new();
    B.get_or_init(|| [
        tone(&[(880.0, 0.0, 0.16), (1318.5, 0.055, 0.2)], 0.16),                       // open: soft rising pair
        tone(&[(1046.5, 0.0, 0.14), (1318.5, 0.06, 0.14), (1568.0, 0.12, 0.26)], 0.15), // ok: little arpeggio
        tone(&[(523.3, 0.0, 0.16), (415.3, 0.11, 0.24)], 0.17),                        // fail: gentle falling pair
        tone(&[(659.3, 0.0, 0.12)], 0.10),                                             // cancel
    ])
}
pub fn play(c: Cue) {
    if !crate::settings::get().sounds { return; }
    let b = &bank()[c as usize];
    unsafe { let _ = PlaySoundW(PCWSTR(b.as_ptr() as *const u16), None, SND_MEMORY | SND_ASYNC | SND_NODEFAULT); }
}
