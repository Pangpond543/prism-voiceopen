// Heard text → command. Whisper's Thai spelling wobbles (ยูทูป / ยูทูบ / ยูธูป, extra polite words, spaces), so both sides
// are reduced to a "sound skeleton" first (tone marks dropped, letters that sound the same merged), then compared with
// an approximate substring search: "ช่วยเปิดยูทูบหน่อยครับ" still finds "เปิดยูทูป".
use crate::settings::Command;

/// Letters that sound alike in Thai → one representative. Tone marks, silent markers and punctuation are dropped.
fn fold(c: char) -> Option<char> {
    Some(match c {
        '\u{0E48}'..='\u{0E4B}' | '\u{0E4C}' | '\u{0E47}' | '\u{0E4D}' | '\u{0E4E}' | '\u{0E3A}' | 'ๆ' | 'ฯ' => return None,
        'ท' | 'ธ' | 'ฒ' | 'ฑ' | 'ฐ' | 'ถ' => 'ท',
        'ส' | 'ศ' | 'ษ' | 'ซ' => 'ส',
        'ค' | 'ฆ' | 'ข' | 'ฅ' | 'ฃ' => 'ค',
        'พ' | 'ภ' | 'ผ' => 'พ',
        'ฟ' | 'ฝ' => 'ฟ',
        'ต' | 'ฏ' => 'ต',
        'ด' | 'ฎ' => 'ด',
        'น' | 'ณ' => 'น',
        'ล' | 'ฬ' | 'ร' => 'ล', // many speakers say ร as ล
        'ย' | 'ญ' => 'ย',
        'ช' | 'ฌ' | 'ฉ' => 'ช',
        'ห' | 'ฮ' => 'ห',
        'บ' | 'ป' => 'บ', // final ป/บ sound the same (ยูทูป / ยูทูบ)
        'ใ' => 'ไ',
        // short / long vowels are often confused by speech-to-text (ยุทุป / ยูทูป)
        'ุ' | 'ู' => 'ุ',
        'ิ' | 'ี' => 'ิ',
        'ึ' | 'ื' => 'ึ',
        'ั' => 'ะ',
        'ำ' => 'ำ',
        c if c.is_whitespace() => return None,
        c if c.is_ascii_punctuation() || "“”‘’…–—«»。、，！？".contains(c) => return None,
        c => c.to_lowercase().next().unwrap_or(c),
    })
}
/// (folded chars, index of each in the original char list)
pub fn norm(s: &str) -> (Vec<char>, Vec<usize>) {
    let mut out = Vec::new();
    let mut idx = Vec::new();
    for (i, c) in s.chars().enumerate() {
        if let Some(f) = fold(c) {
            // "ะ" after a consonant is often dropped in speech-to-text — ignore it
            if f == 'ะ' { continue; }
            out.push(f); idx.push(i);
        }
    }
    (out, idx)
}

/// Smallest edit distance from `p` to any substring of `t` (Sellers). Returns (distance, end index in t, start index).
fn approx_find(p: &[char], t: &[char]) -> (usize, usize, usize) {
    let (n, m) = (p.len(), t.len());
    if n == 0 { return (0, 0, 0); }
    if m == 0 { return (n, 0, 0); }
    // dp over t, with start positions tracked
    let mut prev: Vec<(usize, usize)> = (0..=m).map(|j| (0, j)).collect(); // (cost, start)
    let mut cur = vec![(0usize, 0usize); m + 1];
    for i in 1..=n {
        cur[0] = (i, 0);
        for j in 1..=m {
            let sub = prev[j - 1].0 + if p[i - 1] == t[j - 1] { 0 } else { 1 };
            let del = prev[j].0 + 1;
            let ins = cur[j - 1].0 + 1;
            cur[j] = if sub <= del && sub <= ins { (sub, prev[j - 1].1) } else if del <= ins { (del, prev[j].1) } else { (ins, cur[j - 1].1) };
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    let mut best = (usize::MAX, 0, 0);
    for j in 0..=m { if prev[j].0 < best.0 { best = (prev[j].0, j, prev[j].1); } }
    best
}

pub struct Hit { pub id: String, pub score: f32, pub query: String, pub phrase: String }

const POLITE: [&str; 14] = ["ครับผม", "ครับ", "ค่ะ", "คะ", "จ้า", "จ้ะ", "หน่อย", "ให้หน่อย", "ด้วย", "นะ", "please", "นะคะ", "นะครับ", "ที"];
/// Text after the trigger words = the search query ("หายูทูป แมวน่ารัก ครับ" → "แมวน่ารัก").
fn query_after(text: &str, end_norm: usize, idx: &[usize]) -> String {
    let chars: Vec<char> = text.chars().collect();
    let from = if end_norm == 0 { 0 } else { idx.get(end_norm - 1).map(|i| i + 1).unwrap_or(chars.len()) };
    // skip combining marks that belong to the last matched letter
    let mut from = from;
    while from < chars.len() && matches!(chars[from], '\u{0E31}' | '\u{0E34}'..='\u{0E3A}' | '\u{0E47}'..='\u{0E4E}') { from += 1; }
    let mut q: String = chars[from.min(chars.len())..].iter().collect();
    q = q.trim().trim_matches(|c: char| c.is_ascii_punctuation() || c == ' ' || "“”".contains(c)).to_string();
    for _ in 0..3 {
        let before = q.clone();
        for p in POLITE { if let Some(r) = q.strip_suffix(p) { q = r.trim_end().to_string(); } }
        for p in ["ว่า", "เรื่อง", "คำว่า"] { if let Some(r) = q.strip_prefix(p) { q = r.trim_start().to_string(); } }
        if q == before { break; }
    }
    q.trim().to_string()
}

/// Score every enabled command against the heard text. Best first.
pub fn rank(text: &str, cmds: &[Command]) -> Vec<Hit> {
    let (t, idx) = norm(text);
    let mut hits = Vec::new();
    if t.is_empty() { return hits; }
    for c in cmds.iter().filter(|c| c.enabled) {
        let mut best: Option<Hit> = None;
        for ph in c.phrases.iter().filter(|p| !p.trim().is_empty()) {
            let (p, _) = norm(ph);
            if p.is_empty() { continue; }
            let (d, end, start) = approx_find(&p, &t);
            let mut score = 1.0 - d as f32 / p.len() as f32;
            // extra words around the phrase cost a little (but a search command expects them after)
            let extra = t.len().saturating_sub(p.len()) as f32;
            if !c.is_search() {
                score -= (extra / (p.len() as f32 + 12.0)).min(0.18) * 0.5;
                // a short phrase found (approximately) inside a long sentence is weak evidence: the user's log had
                // "สปอติฟไฟฟังสิ่งเริ่มเป็นข้าไว้" (they said "Spotify") start a screen recording via "เริ่มอัด" at 0.58
                let cover = p.len() as f32 / t.len() as f32;
                if cover < 0.4 { score -= (0.4 - cover) * if d > 0 { 0.9 } else { 0.4 }; }
            }
            else { score -= (start as f32 / (p.len() as f32 + 6.0)).min(0.2) * 0.5; }
            // short phrases (2–3 letters) must be nearly exact
            if p.len() <= 3 && d > 0 { score -= 0.25; }
            let query = if c.is_search() { query_after(text, end, &idx) } else { String::new() };
            if best.as_ref().map_or(true, |b| score > b.score) { best = Some(Hit { id: c.id.clone(), score, query, phrase: ph.clone() }); }
        }
        if let Some(b) = best { hits.push(b); }
    }
    hits.sort_by(|a, b| b.score.total_cmp(&a.score));
    hits
}

fn lev(a: &[char], b: &[char]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, x) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, y) in b.iter().enumerate() { cur.push((prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + usize::from(x != y))); }
        prev = cur;
    }
    prev[b.len()]
}
/// 0…1: how alike two heard texts sound (same Thai "sound skeleton" as `rank`).
pub fn similar(a: &str, b: &str) -> f32 {
    let (a, _) = norm(a);
    let (b, _) = norm(b);
    if a.is_empty() || b.is_empty() { return 0.0; }
    1.0 - lev(&a, &b) as f32 / a.len().max(b.len()) as f32
}
/// Wake-word check on the words Whisper heard: best likeness to the wake phrase or to what Whisper heard for the user's
/// own wake samples (it spells "เฮ้ พริซึม" as "ให้ไปเสริม", "เอพิซึม" …). The start of the text is also tried, for
/// "เฮ้ พริซึม เปิดยูทูป" said in one breath. Tested on the user's real samples + log (11-19): own samples ≥ 0.55,
/// every false wake-up ≤ 0.43 → `WAKE_OK` = 0.5.
pub const WAKE_OK: f32 = 0.5;
pub fn wake_score(text: &str, refs: &[String]) -> f32 {
    let (t, idx) = norm(text);
    if t.is_empty() { return 0.0; }
    let chars: Vec<char> = text.chars().collect();
    let mut best = 0f32;
    for r in refs {
        best = best.max(similar(text, r));
        let n = norm(r).0.len();
        if n > 0 && t.len() > n + 2 {
            let cut = idx.get(n + 1).copied().unwrap_or(chars.len());
            best = best.max(similar(&chars[..cut].iter().collect::<String>(), r) - 0.05);
        }
    }
    best
}

/// Words for Whisper's prompt (not used: it caused false triggers — see pitfalls).
#[allow(dead_code)]
pub fn prompt(cmds: &[Command], wake: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    let mut by_use: Vec<&Command> = cmds.iter().filter(|c| c.enabled).collect();
    by_use.sort_by(|a, b| b.uses.cmp(&a.uses));
    for c in by_use { for p in &c.phrases { let p = p.trim(); if !p.is_empty() && !words.iter().any(|w| w == p) { words.push(p.to_string()); } } }
    let _ = wake;
    let mut s = String::new();
    for w in words { if s.chars().count() + w.chars().count() > 220 { break; } if !s.is_empty() { s.push_str(", "); } s.push_str(&w); }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Replay a saved history against a settings file: `PVO_SETTINGS=… PVO_HISTORY=… cargo test replay -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn replay() {
        let s: crate::settings::Settings = serde_json::from_str(&std::fs::read_to_string(std::env::var("PVO_SETTINGS").unwrap()).unwrap()).unwrap();
        let h: Vec<serde_json::Value> = serde_json::from_str(&std::fs::read_to_string(std::env::var("PVO_HISTORY").unwrap()).unwrap()).unwrap();
        for x in h {
            let heard = x["heard"].as_str().unwrap_or("");
            if heard.is_empty() { continue; }
            let r = rank(heard, &s.commands);
            let now = r.first().filter(|b| b.score >= s.strict).map(|b| s.commands.iter().find(|c| c.id == b.id).unwrap().name.clone());
            let was = if x["ok"] == true && x["how"] == "text" { x["name"].as_str().map(|n| n.to_string()) } else { None };
            let (bid, bsc) = r.first().map(|b| (b.id.as_str(), b.score)).unwrap_or(("", 0.0));
            println!("{} | {heard:?} was={was:?} now={now:?} best={bid} {bsc:.2}", if was == now { "same" } else { "DIFF" });
        }
    }
    fn cmd(id: &str, phrases: &[&str], search: bool) -> Command {
        Command { id: id.into(), name: id.into(), phrases: phrases.iter().map(|s| s.to_string()).collect(), enabled: true,
            actions: vec![crate::settings::Action { kind: if search { "search".into() } else { "url".into() }, ..Default::default() }], ..Default::default() }
    }
    #[test]
    fn matches() {
        let cs = vec![cmd("yt", &["เปิดยูทูป"], false), cmd("ytq", &["หายูทูป"], true), cmd("calc", &["เปิดเครื่องคิดเลข"], false)];
        let r = rank("ช่วยเปิดยูทูบหน่อยครับ", &cs);
        assert_eq!(r[0].id, "yt", "{}", r[0].score);
        assert!(r[0].score > 0.8);
        let r = rank("หายูทูป แมวน่ารัก ครับ", &cs);
        assert_eq!(r[0].id, "ytq");
        assert_eq!(r[0].query, "แมวน่ารัก");
        let r = rank("เปิดเครื่องคิดเลข", &cs);
        assert_eq!(r[0].id, "calc");
        // real Whisper-small output for TTS speech
        let r = rank("หายุทุปแมว น่ารัก", &cs);
        assert_eq!(r[0].id, "ytq", "{}", r[0].score);
        assert_eq!(r[0].query, "แมว น่ารัก");
        let cs2 = vec![cmd("down", &["ลดเสียง"], false), cmd("up", &["เพิ่มเสียง"], false)];
        let r = rank("หลัดเสียง", &cs2);
        assert_eq!(r[0].id, "down", "{}", r[0].score);
        assert!(r[0].score > 0.72, "{}", r[0].score);
        let r = rank("วันนี้อากาศดี", &cs);
        assert!(r[0].score < 0.6, "{}", r[0].score);
        // real false trigger from the user's log (strictness 0.55): must not start a recording any more
        let rec = vec![cmd("rec", &["เริ่มอัด", "หยุดอัด", "อัดหน้าจอ"], false), cmd("lock", &["ล็อกจอ", "ล็อคจอ", "ล็อกหน้าจอ"], false)];
        let r = rank("สปอติฟไฟฟังสิ่งเริ่มเป็นข้าไว้", &rec);
        assert!(r[0].score < 0.5, "{} {}", r[0].id, r[0].score);
        // ...while an exact phrase inside a polite sentence still works
        let r = rank("ช่วยเริ่มอัดหน้าจอให้หน่อยนะครับ", &rec);
        assert_eq!(r[0].id, "rec"); assert!(r[0].score >= 0.72, "{}", r[0].score);
        let r = rank("ล็อกจอเลย", &rec);
        assert_eq!(r[0].id, "lock"); assert!(r[0].score >= 0.72, "{}", r[0].score);
        let r = rank("ล็อคจอ", &rec);
        assert!(r[0].score > 0.9, "{}", r[0].score);
        // other phrases from the user's history that ran nothing must still run nothing at 0.55
        for t in ["ลักหนัดเถอะ", "ลักแน่เจ้า", "เฉพาติฟาย", "โปรดติดตามต่อไป", "ต้องตื่นลายมาก"] {
            let r = rank(t, &rec);
            assert!(r[0].score < 0.55, "{t}: {} {}", r[0].id, r[0].score);
        }
    }
    /// Real data (2026-11-25): what Whisper (Typhoon Turbo + VAD) heard for the user's 13 wake samples, and what it heard
    /// after each of the user's false wake-ups on 11-19 (music, talk, commands said without the wake word).
    #[test]
    fn wake_words() {
        let own = ["ให้ปิดซึ่ม", "ให้ปีซึ่ม", "เฮ้ยพี่ซึ่ม", "ให้ไปเสริม", "ให้ไปเสริม", "ให้ปีซึม", "ให้ไปซึม", "ไอ้พี่ซึ่ม", "ให้ไปเสิร์ฟ", "เอฟิเซ็นต์", "ให้ไปซึ่ง", "ให้อีกปีเซ็นต์", "เอพิซึม"];
        for (i, w) in own.iter().enumerate() {
            let mut refs: Vec<String> = vec!["เฮ้ พริซึม".into()];
            refs.extend(own.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, s)| s.to_string()));
            let s = wake_score(w, &refs);
            assert!(s >= WAKE_OK, "own sample {w} = {s}");
        }
        let refs: Vec<String> = std::iter::once("เฮ้ พริซึม".to_string()).chain(own.iter().map(|s| s.to_string())).collect();
        assert!(wake_score("เฮ้ พริซึม เปิดยูทูป", &refs) >= WAKE_OK);
        assert!(wake_score("Hey Prism", &["hey prism".to_string()]) >= WAKE_OK);
        for t in ["โอเค", "ล๊อคหน้าจอ", "กลับมาก่อน", "บรรยายธรรม ลอคนาจอง", "ลอกนัดเจ้า", "ลักจ้า", "ไปดิทฟูก", "เปิดอยู่ทุก", "ก็",
                  "สวัสดีกว่า", "Lock", "A/B Zoom...A/B Zoom...", "เฮ้ย มาเจอกันไงว่า", "อันนี้ล่ะ", "Capture", "Facebook", "เพิ่มเสียง",
                  "ให้พิ่ง", "เล่นเพลง", "หยุดเพลง", "ดีหน้าต่าง", "ปิดหน้าตั้ง", "เจ้า", "เปิดเล็ก", "โปรดติดตามต่อไป", "สวัสดีครับ", ""] {
            let s = wake_score(t, &refs);
            assert!(s < WAKE_OK, "false wake {t:?} = {s}");
        }
    }
}
