// Microphone → 16 kHz mono float (what Whisper and the voice matcher want). WASAPI shared mode; Windows converts the
// format for us when it can, otherwise we take the mix format and resample here. Capture only runs while listening
// (or while the wake word is on), so the "mic in use" icon is honest.
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::*;

pub const RATE: u32 = 16000;
pub static LEVEL: AtomicU32 = AtomicU32::new(0); // last ~30 ms RMS (f32 bits), for the glow / meter
pub fn level() -> f32 { f32::from_bits(LEVEL.load(Ordering::Relaxed)) }

pub type Queue = Arc<Mutex<VecDeque<f32>>>;

/// A running capture. Drop (or `end`) stops it.
pub struct Cap { stop: Arc<AtomicBool>, pub q: Queue, th: Option<std::thread::JoinHandle<()>>, pub failed: Arc<AtomicBool> }
impl Cap {
    pub fn start(mic_id: &str, gain: f32) -> Cap {
        let stop = Arc::new(AtomicBool::new(false));
        let failed = Arc::new(AtomicBool::new(false));
        let q: Queue = Arc::new(Mutex::new(VecDeque::with_capacity(RATE as usize * 4)));
        let (s2, q2, f2, id) = (stop.clone(), q.clone(), failed.clone(), mic_id.to_string());
        let th = std::thread::spawn(move || {
            if let Err(e) = unsafe { capture(&id, gain, &s2, &q2) } {
                crate::settings::log(&format!("mic capture failed: {e}"));
                f2.store(true, Ordering::Relaxed);
            }
        });
        Cap { stop, q, th: Some(th), failed }
    }
    pub fn take(&self) -> Vec<f32> { crate::settings::lock(&self.q).drain(..).collect() }
    pub fn end(&mut self) { self.stop.store(true, Ordering::Relaxed); if let Some(t) = self.th.take() { let _ = t.join(); } LEVEL.store(0, Ordering::Relaxed); }
}
impl Drop for Cap { fn drop(&mut self) { self.end(); } }

#[repr(C)]
#[derive(Clone, Copy)]
struct WaveExt { fmt: WAVEFORMATEX, valid_bits: u16, mask: u32, sub: windows::core::GUID }
const FLOAT_SUB: windows::core::GUID = windows::core::GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71);
const PCM_SUB: windows::core::GUID = windows::core::GUID::from_u128(0x00000001_0000_0010_8000_00aa00389b71);

unsafe fn device(mic_id: &str) -> windows::core::Result<IMMDevice> {
    let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
    if !mic_id.is_empty() {
        let w = crate::gfx::wide(mic_id);
        if let Ok(d) = en.GetDevice(PCWSTR(w.as_ptr())) { return Ok(d); }
    }
    en.GetDefaultAudioEndpoint(eCapture, eCommunications).or_else(|_| en.GetDefaultAudioEndpoint(eCapture, eConsole))
}

unsafe fn capture(mic_id: &str, gain: f32, stop: &AtomicBool, q: &Queue) -> windows::core::Result<()> {
    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    let dev = device(mic_id)?;
    let client: IAudioClient = dev.Activate(CLSCTX_ALL, None)?;
    let want = WaveExt {
        fmt: WAVEFORMATEX { wFormatTag: 0xFFFE, nChannels: 1, nSamplesPerSec: RATE, nAvgBytesPerSec: RATE * 4, nBlockAlign: 4, wBitsPerSample: 32, cbSize: 22 },
        valid_bits: 32, mask: 4, sub: FLOAT_SUB,
    };
    if client.Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | 0x0800_0000, 1_000_000, 0, &want.fmt, None).is_ok() {
        return run(client, 1, RATE, 'f', gain, stop, q);
    }
    let client2: IAudioClient = dev.Activate(CLSCTX_ALL, None)?;
    let mix = client2.GetMixFormat()?;
    let m = *mix;
    let float = m.wFormatTag == 3 || (m.wFormatTag == 0xFFFE && (*(mix as *const WaveExt)).sub == FLOAT_SUB);
    let pcm16 = m.wBitsPerSample == 16 && (m.wFormatTag == 1 || (m.wFormatTag == 0xFFFE && (*(mix as *const WaveExt)).sub == PCM_SUB));
    client2.Initialize(AUDCLNT_SHAREMODE_SHARED, 0, 1_000_000, 0, mix, None)?;
    CoTaskMemFree(Some(mix as _));
    run(client2, m.nChannels as usize, m.nSamplesPerSec, if float && m.wBitsPerSample == 32 { 'f' } else if pcm16 { 's' } else { 'x' }, gain, stop, q)
}

unsafe fn run(client: IAudioClient, ch: usize, rate: u32, kind: char, gain: f32, stop: &AtomicBool, q: &Queue) -> windows::core::Result<()> {
    if kind == 'x' { crate::settings::log("mic: unsupported device format"); return Err(windows::core::Error::from_hresult(windows::core::HRESULT(-1))); }
    let cap: IAudioCaptureClient = client.GetService()?;
    client.Start()?;
    let step = rate as f64 / RATE as f64;
    let mut pos = 0f64;
    let mut acc = 0f32;
    let mut acc_n = 0usize;
    let mut sq = 0f64;
    let mut sq_n = 0usize;
    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(8));
        loop {
            // an error here = the mic was unplugged / disabled (AUDCLNT_E_DEVICE_INVALIDATED): report it instead of
            // silently delivering nothing forever (the wake word would stay deaf until a restart)
            let n = cap.GetNextPacketSize()?;
            if n == 0 { break; }
            let (mut data, mut frames, mut flags) = (std::ptr::null_mut(), 0u32, 0u32);
            cap.GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
            let silent = flags & 2 != 0;
            let mut out: Vec<f32> = Vec::with_capacity(frames as usize);
            for i in 0..frames as usize {
                // mono mix of the device channels
                let mut v = 0f32;
                if !silent {
                    for c in 0..ch {
                        v += if kind == 'f' { *(data as *const f32).add(i * ch + c) } else { *(data as *const i16).add(i * ch + c) as f32 / 32768.0 };
                    }
                    v /= ch as f32;
                }
                if rate == RATE { out.push(v); continue; }
                // box-filter downsample (good enough for speech): average every `step` input samples
                acc += v; acc_n += 1; pos += 1.0;
                if pos >= step { out.push(acc / acc_n as f32); pos -= step; acc = 0.0; acc_n = 0; }
            }
            let _ = cap.ReleaseBuffer(frames);
            for v in out.iter_mut() { *v = if v.is_finite() { (*v * gain).clamp(-1.0, 1.0) } else { 0.0 }; sq += (*v * *v) as f64; sq_n += 1; }
            if sq_n >= 480 { LEVEL.store(((sq / sq_n as f64).sqrt() as f32).to_bits(), Ordering::Relaxed); sq = 0.0; sq_n = 0; }
            let mut g = crate::settings::lock(q);
            g.extend(out);
            let limit = RATE as usize * 40;
            while g.len() > limit { g.pop_front(); }
        }
    }
    let _ = client.Stop();
    Ok(())
}

pub fn mics() -> Vec<(String, String)> {
    let mut v = Vec::new();
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let Ok(en) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) else { return v };
        let Ok(col) = en.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE) else { return v };
        for i in 0..col.GetCount().unwrap_or(0) {
            let Ok(d) = col.Item(i) else { continue };
            let id = d.GetId().map(|p: PWSTR| { let s = p.to_string().unwrap_or_default(); CoTaskMemFree(Some(p.0 as _)); s }).unwrap_or_default();
            let name = d.OpenPropertyStore(STGM_READ).and_then(|ps| ps.GetValue(&windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName))
                .ok().and_then(|pv| windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc(&pv).ok().map(|p| { let s = p.to_string().unwrap_or_default(); CoTaskMemFree(Some(p.0 as _)); s }))
                .unwrap_or_default();
            v.push((id, name));
        }
    }
    v
}

/// 16-bit mono WAV bytes (for Whisper and for "play back my sample").
pub fn wav_bytes(s: &[f32]) -> Vec<u8> {
    let data: Vec<u8> = s.iter().flat_map(|v| ((v.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()).collect();
    let mut b = Vec::with_capacity(44 + data.len());
    b.extend_from_slice(b"RIFF"); b.extend_from_slice(&(36 + data.len() as u32).to_le_bytes()); b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes()); b.extend_from_slice(&1u16.to_le_bytes()); b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&RATE.to_le_bytes()); b.extend_from_slice(&(RATE * 2).to_le_bytes()); b.extend_from_slice(&2u16.to_le_bytes()); b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data"); b.extend_from_slice(&(data.len() as u32).to_le_bytes()); b.extend_from_slice(&data);
    b
}
/// Read a 16-bit PCM WAV (any rate / channels) → 16 kHz mono (test hook + voice samples).
pub fn read_wav(bytes: &[u8]) -> Option<Vec<f32>> {
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" { return None; }
    let mut i = 12;
    let (mut ch, mut rate, mut bits) = (1usize, RATE, 16u16);
    while i + 8 <= bytes.len() {
        let id = &bytes[i..i + 4];
        let len = u32::from_le_bytes(bytes[i + 4..i + 8].try_into().ok()?) as usize;
        let body = &bytes[i + 8..(i + 8 + len).min(bytes.len())];
        if id == b"fmt " && body.len() >= 16 {
            ch = u16::from_le_bytes([body[2], body[3]]) as usize;
            rate = u32::from_le_bytes(body[4..8].try_into().ok()?);
            bits = u16::from_le_bytes([body[14], body[15]]);
        } else if id == b"data" {
            if bits != 16 || ch == 0 { return None; }
            let frames: Vec<f32> = body.chunks_exact(2 * ch).map(|f| (0..ch).map(|c| i16::from_le_bytes([f[c * 2], f[c * 2 + 1]]) as f32 / 32768.0).sum::<f32>() / ch as f32).collect();
            if rate == RATE { return Some(frames); }
            let step = rate as f64 / RATE as f64;
            let n = (frames.len() as f64 / step) as usize;
            return Some((0..n).map(|k| { let p = k as f64 * step; let a = p as usize; let t = (p - a as f64) as f32; let b = (a + 1).min(frames.len() - 1); frames[a] * (1.0 - t) + frames[b] * t }).collect());
        }
        i += 8 + len + (len & 1);
    }
    None
}
