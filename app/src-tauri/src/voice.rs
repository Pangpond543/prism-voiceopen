// "Teach by voice": the user says a command 2–3 times; we keep the sound's shape (MFCC frames) and later compare new
// speech with DTW. Works for any language, accent or made-up word, needs no model, and answers in a few ms —
// before Whisper has even started. Also used for the wake word, which listens continuously at almost no CPU.
use crate::audio::RATE;
use crate::settings;
use serde::{Deserialize, Serialize};

const FRAME: usize = 400; // 25 ms
const HOP: usize = 160;   // 10 ms
const NFFT: usize = 512;
const MELS: usize = 26;
const CEPS: usize = 13;

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Sample { pub id: String, pub feats: Vec<Vec<f32>>, pub secs: f32 }

/* ------------------------------------------------------------- speech trim */
/// Cut leading/trailing silence (energy based, adaptive to the room's noise). None = no speech at all.
pub fn trim(s: &[f32]) -> Option<&[f32]> {
    let win = 320; // 20 ms
    if s.len() < win * 5 { return None; }
    let rms: Vec<f32> = s.chunks(win).map(|c| (c.iter().map(|v| v * v).sum::<f32>() / c.len() as f32).sqrt()).collect();
    let mut sorted = rms.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let floor = sorted[sorted.len() / 10].max(0.0015);
    let peak = sorted[sorted.len() - 1];
    let thr = (floor * 3.2).max(peak * 0.08).max(0.006);
    let first = rms.iter().position(|&r| r > thr)?;
    let last = rms.iter().rposition(|&r| r > thr)?;
    if last <= first + 4 { return None; } // < 100 ms of sound
    let a = first.saturating_sub(5) * win;
    let b = ((last + 6) * win).min(s.len());
    Some(&s[a..b])
}

/* ------------------------------------------------------------------- MFCC */
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 { j ^= bit; bit >>= 1; }
        j |= bit;
        if i < j { re.swap(i, j); im.swap(i, j); }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * std::f32::consts::PI / len as f32;
        for i in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (wr, wi) = ((ang * k as f32).cos(), (ang * k as f32).sin());
                let (a, b) = (i + k, i + k + len / 2);
                let (xr, xi) = (re[b] * wr - im[b] * wi, re[b] * wi + im[b] * wr);
                re[b] = re[a] - xr; im[b] = im[a] - xi;
                re[a] += xr; im[a] += xi;
            }
        }
        len <<= 1;
    }
}
fn mel_bank() -> &'static Vec<Vec<(usize, f32)>> {
    static B: std::sync::OnceLock<Vec<Vec<(usize, f32)>>> = std::sync::OnceLock::new();
    B.get_or_init(|| {
        let mel = |f: f32| 2595.0 * (1.0 + f / 700.0).log10();
        let hz = |m: f32| 700.0 * (10f32.powf(m / 2595.0) - 1.0);
        let (lo, hi) = (mel(80.0), mel(7600.0));
        let pts: Vec<usize> = (0..MELS + 2).map(|i| ((hz(lo + (hi - lo) * i as f32 / (MELS + 1) as f32) / RATE as f32) * NFFT as f32).floor() as usize).collect();
        (0..MELS).map(|m| {
            let (a, c, b) = (pts[m], pts[m + 1], pts[m + 2]);
            let mut v = Vec::new();
            for k in a..=b {
                let w = if k <= c { if c == a { 1.0 } else { (k - a) as f32 / (c - a) as f32 } } else if b == c { 1.0 } else { (b - k) as f32 / (b - c) as f32 };
                if w > 0.0 { v.push((k, w)); }
            }
            v
        }).collect()
    })
}
/// MFCC 1..12 + log-energy-free, with per-utterance mean removal (so a different mic / distance matters less).
pub fn features(s: &[f32]) -> Vec<Vec<f32>> {
    let s = match trim(s) { Some(t) => t, None => return Vec::new() };
    let mut pre: Vec<f32> = Vec::with_capacity(s.len());
    let mut last = 0.0;
    for &v in s { pre.push(v - 0.97 * last); last = v; }
    let ham: Vec<f32> = (0..FRAME).map(|i| 0.54 - 0.46 * (2.0 * std::f32::consts::PI * i as f32 / (FRAME - 1) as f32).cos()).collect();
    let bank = mel_bank();
    let mut out = Vec::new();
    let mut i = 0;
    while i + FRAME <= pre.len() {
        let mut re = vec![0f32; NFFT];
        let mut im = vec![0f32; NFFT];
        for k in 0..FRAME { re[k] = pre[i + k] * ham[k]; }
        fft(&mut re, &mut im);
        let pw: Vec<f32> = (0..NFFT / 2 + 1).map(|k| re[k] * re[k] + im[k] * im[k]).collect();
        let lm: Vec<f32> = bank.iter().map(|f| (f.iter().map(|&(k, w)| pw[k] * w).sum::<f32>() + 1e-8).ln()).collect();
        let c: Vec<f32> = (1..CEPS).map(|n| (0..MELS).map(|m| lm[m] * (std::f32::consts::PI * n as f32 * (m as f32 + 0.5) / MELS as f32).cos()).sum::<f32>()).collect();
        out.push(c);
        i += HOP;
    }
    if out.is_empty() { return out; }
    let d = out[0].len();
    let mean: Vec<f32> = (0..d).map(|j| out.iter().map(|f| f[j]).sum::<f32>() / out.len() as f32).collect();
    let sd: Vec<f32> = (0..d).map(|j| (out.iter().map(|f| (f[j] - mean[j]).powi(2)).sum::<f32>() / out.len() as f32).sqrt().max(1e-3)).collect();
    for f in out.iter_mut() { for j in 0..d { f[j] = (f[j] - mean[j]) / sd[j]; } }
    // keep every frame but downsample long utterances so DTW stays fast
    if out.len() > 220 { let k = out.len() as f32 / 220.0; out = (0..220).map(|i| out[(i as f32 * k) as usize].clone()).collect(); }
    out
}

/* -------------------------------------------------------------------- DTW */
/// Average frame distance along the best alignment (lower = more alike). ~0.6–0.9 for the same words by the same
/// person, 1.1+ for different words.
pub fn dtw(a: &[Vec<f32>], b: &[Vec<f32>]) -> f32 {
    if a.is_empty() || b.is_empty() { return f32::MAX; }
    let (n, m) = (a.len(), b.len());
    // very different lengths are never the same phrase
    let ratio = n.max(m) as f32 / n.min(m) as f32;
    if ratio > 2.6 { return f32::MAX; }
    let band = ((n.max(m) as f32) * 0.35).ceil() as usize + (n as isize - m as isize).unsigned_abs();
    let dist = |x: &[f32], y: &[f32]| (x.iter().zip(y).map(|(p, q)| (p - q) * (p - q)).sum::<f32>() / x.len() as f32).sqrt();
    let inf = f32::MAX / 4.0;
    let mut prev = vec![inf; m + 1];
    let mut cur = vec![inf; m + 1];
    let mut plen = vec![0u32; m + 1];
    let mut clen = vec![0u32; m + 1];
    prev[0] = 0.0;
    for i in 1..=n {
        cur.iter_mut().for_each(|v| *v = inf);
        let center = i * m / n;
        let lo = center.saturating_sub(band).max(1);
        let hi = (center + band).min(m);
        for j in lo..=hi {
            let c = dist(&a[i - 1], &b[j - 1]);
            let (mut best, mut bl) = (prev[j - 1], plen[j - 1]);
            if prev[j] < best { best = prev[j]; bl = plen[j]; }
            if cur[j - 1] < best { best = cur[j - 1]; bl = clen[j - 1]; }
            cur[j] = best + c; clen[j] = bl + 1;
        }
        std::mem::swap(&mut prev, &mut cur);
        std::mem::swap(&mut plen, &mut clen);
    }
    if prev[m] >= inf { f32::MAX } else { prev[m] / plen[m].max(1) as f32 }
}

/* ---------------------------------------------------------------- storage */
fn file(id: &str) -> std::path::PathBuf {
    let id: String = id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    settings::voice_dir().join(format!("{id}.json"))
}
pub fn save(samples: &[f32]) -> Option<Sample> {
    let feats = features(samples);
    if feats.len() < 15 { return None; } // < 0.15 s of speech
    let id = format!("s{}", settings::new_id());
    let secs = trim(samples).map(|t| t.len() as f32 / RATE as f32).unwrap_or(0.0);
    let s = Sample { id: id.clone(), feats, secs };
    let _ = std::fs::create_dir_all(settings::voice_dir());
    let _ = std::fs::write(file(&id), serde_json::to_string(&s).unwrap_or_default());
    // keep the sound too, so the user can play it back
    let _ = std::fs::write(settings::voice_dir().join(format!("{id}.wav")), crate::audio::wav_bytes(trim(samples).unwrap_or(samples)));
    Some(s)
}
pub fn load(id: &str) -> Option<Sample> { std::fs::read_to_string(file(id)).ok().and_then(|t| serde_json::from_str(&t).ok()) }
/// Delete sample files that no command / wake word uses any more (deleted commands keep theirs until now so Undo works).
pub fn prune() {
    let st = settings::get();
    if settings::RECOVERED.load(std::sync::atomic::Ordering::Relaxed) { return; }
    let keep: std::collections::HashSet<String> = st.commands.iter().flat_map(|c| c.samples.iter().cloned()).chain(st.wake_samples.iter().cloned()).collect();
    let Ok(rd) = std::fs::read_dir(settings::voice_dir()) else { return };
    let mut n = 0;
    for e in rd.flatten() {
        let p = e.path();
        let ext = p.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
        if ext != "json" && ext != "wav" { continue; }
        let Some(stem) = p.file_stem().map(|x| x.to_string_lossy().to_string()) else { continue };
        if !stem.starts_with('s') || keep.contains(&stem) { continue; }
        if std::fs::remove_file(&p).is_ok() { n += 1; }
    }
    if n > 0 { settings::log(&format!("voice: removed {n} unused sample files")); }
}
pub fn remove(id: &str) { let _ = std::fs::remove_file(file(id)); let _ = std::fs::remove_file(file(id).with_extension("wav")); }

/// Tolerance for a set of samples: how far apart the user's own takes are, plus headroom.
pub fn threshold(samples: &[Sample], sense: f32) -> f32 {
    let mut d = Vec::new();
    for i in 0..samples.len() { for j in i + 1..samples.len() { let x = dtw(&samples[i].feats, &samples[j].feats); if x < 5.0 { d.push(x); } } }
    let base = if d.is_empty() { 0.80 } else { (d.iter().sum::<f32>() / d.len() as f32 * 1.22).clamp(0.62, 0.95) };
    base * sense.clamp(0.5, 1.5)
}

/// Best command by voice. Returns (command id, distance, threshold, runner-up distance).
pub fn best(feats: &[Vec<f32>], cmds: &[settings::Command], sense: f32) -> Option<(String, f32, f32, f32)> {
    if feats.len() < 10 { return None; }
    let mut scored: Vec<(String, f32, f32)> = Vec::new();
    for c in cmds.iter().filter(|c| c.enabled && !c.samples.is_empty()) {
        let ss: Vec<Sample> = c.samples.iter().filter_map(|id| load(id)).collect();
        if ss.is_empty() { continue; }
        let d = ss.iter().map(|s| dtw(feats, &s.feats)).fold(f32::MAX, f32::min);
        scored.push((c.id.clone(), d, threshold(&ss, sense)));
    }
    scored.sort_by(|a, b| a.1.total_cmp(&b.1));
    let first = scored.first()?.clone();
    let second = scored.get(1).map(|s| s.1).unwrap_or(f32::MAX);
    Some((first.0, first.1, first.2, second))
}
