// Prism VoiceOpen — main window: Prism suite shell (title bar, language, theme, Info / update), the list of voice
// commands and their editor, history, settings and shortcuts. Listening itself is native (hotkey, glow, engine).
import { api, native, T, lang, theme, setTheme, savedTheme, setLook, savedLook, remember, toast, htmlBox, confirmBox, ctxMenu, hideCtx, esc, $ } from './core.js';
import { icon, hydrateIcons } from './icons.js';
import { translateDom } from './i18n.js';
import { startBackground } from './bg.js';

addEventListener('error', (e) => api.invoke('app:log', 'JS error: ' + e.message + ' @' + (e.filename || '').split('/').pop() + ':' + e.lineno));
addEventListener('unhandledrejection', (e) => api.invoke('app:log', 'JS rejection: ' + (e.reason && (e.reason.stack || e.reason.message) || e.reason)));
let S = { commands: [], hotkeys: {} };   // settings (owned by the Rust side)
let SYS = { mics: [], prism: [] };
let HIST = [];
let phase = 'idle';
const DEF_KEYS = { talk: 'Ctrl+Space', open: 'Ctrl+Alt+O', repeat: '' };
const keyText = (id) => (id === 'talk' ? S.hotkey : (S.hotkeys && S.hotkeys[id] !== undefined ? S.hotkeys[id] : DEF_KEYS[id])) || '';
const hk = (id) => keyText(id) || T`ไม่มีคีย์ลัด`;

/* ------------------------------------------------------------ about/update */
const AUTHOR = 'Thanakorn Damrikitjaroen', MODEL = 'Claude Fable 5.2';
const PURPOSE = 'Prism VoiceOpen opens what you ask for — press a key, say a word, and your programs, links, folders and searches open. Speech is understood on your PC, nothing goes online.';
async function showInfo() {
  const info = await api.invoke('about:info');
  const link = info.repo ? `https://github.com/${info.repo}` : '';
  htmlBox(`
    <div class="about">
      <div class="prism-logo big"><i></i><i></i><i></i><b class="lg"><svg viewBox="18 18 64 64" aria-hidden="true"><rect x="41" y="23" width="18" height="33" rx="9" fill="#fff"/><path d="M32 49 A18 18 0 0 0 68 49" fill="none" stroke="#fff" stroke-width="6" stroke-linecap="round"/><path d="M50 67 V76 M41 77 H59" stroke="#fff" stroke-width="6" stroke-linecap="round"/></svg></b></div>
      <h3>Prism VoiceOpen <small>v${esc(info.version)}</small></h3>
      <p class="about-purpose">${esc(PURPOSE)}</p>
      <dl class="kv">
        <dt>${T`เวอร์ชัน`}</dt><dd>${esc(info.version)} · Tauri ${esc(info.tauri)} · Rust · whisper.cpp${info.model ? ` (${esc(info.model)})` : ''}</dd>
        <dt>${T`ผู้พัฒนา`}</dt><dd>${AUTHOR}</dd>
        <dt>${T`โมเดล AI ที่สร้าง`}</dt><dd>${MODEL}</dd>
        <dt>GitHub</dt><dd>${link ? `<a href="#" id="about-repo">${esc(link)}</a>` : T`<span class="muted">ยังไม่ได้ตั้งค่าลิงก์</span>`}</dd>
      </dl>
      <div class="about-update" id="about-update"><span class="muted">${link ? T`กดเพื่อตรวจว่ามีเวอร์ชันใหม่บน GitHub หรือไม่` : T`ใส่ชื่อ repository ในไฟล์ prism.config.json (เช่น "owner/prism") เพื่อเปิดใช้การอัปเดต`}</span></div>
      <div class="modal-btns"><button class="pill-btn" data-x>${T`ปิด`}</button><button class="pill-btn primary" id="about-check">${icon('refresh')}<span>${T`ตรวจหาอัปเดต`}</span></button></div>
    </div>`, (m, close) => {
    const box = m.querySelector('#about-update'), btn = m.querySelector('#about-check');
    const say = (html) => { box.innerHTML = html; };
    const why = (r) => ({
      norepo: T`ยังไม่ได้ตั้งค่าลิงก์ GitHub — ใส่ชื่อ repository ในไฟล์ prism.config.json`,
      notfound: T`ไม่พบ repository นี้บน GitHub`, noversion: T`repository นี้ยังไม่มีเลขเวอร์ชัน`,
      network: T`เชื่อมต่อ GitHub ไม่ได้ — ตรวจอินเทอร์เน็ตแล้วลองใหม่`, uptodate: T`เป็นเวอร์ชันล่าสุดอยู่แล้ว`,
      extract: T`แตกไฟล์อัปเดตไม่สำเร็จ`, layout: T`ไฟล์อัปเดตมีโครงสร้างไม่ถูกต้อง`, notprism: T`ไฟล์ที่ดาวน์โหลดมาไม่ใช่โปรแกรม Prism`,
      build: T`คอมไพล์เวอร์ชันใหม่ไม่สำเร็จ — ต้องติดตั้ง Rust (cargo) ในเครื่องนี้`, busy: T`รอให้ฟังคำสั่งเสร็จก่อน แล้วค่อยอัปเดต`,
    }[r.reason] || T`เกิดข้อผิดพลาดระหว่างอัปเดต`);
    m.querySelector('[data-x]').onclick = () => close(null);
    if (link) m.querySelector('#about-repo').onclick = (e) => { e.preventDefault(); api.invoke('about:openRepo'); };
    const off = api.on('update:step', ({ step, f: frac }) => {
      const label = { download: T`กำลังดาวน์โหลด`, extract: T`กำลังแตกไฟล์`, copy: T`กำลังติดตั้ง`, build: T`กำลังคอมไพล์โปรแกรม (Rust)` }[step] || step;
      say(`<span>${label}${step === 'download' && frac ? ` ${Math.round(frac * 100)}%` : '…'}</span><div class="progress indet"><i></i></div>`);
    });
    const apply = async () => {
      btn.disabled = true;
      say(T`<span>กำลังดาวน์โหลด…</span><div class="progress indet"><i></i></div>`);
      const r = await api.invoke('update:apply');
      if (!r.ok) { say(`<span class="bad">${esc(why(r))}</span>`); btn.disabled = false; return; }
      say(`<span class="good">${T`ติดตั้งเวอร์ชัน ${esc(r.version)} แล้ว — เริ่มโปรแกรมใหม่เพื่อใช้งาน`}</span>`);
      btn.innerHTML = T`<span>เริ่มโปรแกรมใหม่</span>`; btn.disabled = false;
      btn.onclick = () => { off(); api.invoke('app:relaunch'); };
    };
    btn.onclick = async () => {
      btn.disabled = true;
      say(T`<span>กำลังตรวจสอบ…</span><div class="progress indet"><i></i></div>`);
      const r = await api.invoke('update:check');
      btn.disabled = false;
      if (!r.ok) { say(`<span class="bad">${esc(why(r))}</span>`); return; }
      if (!r.hasUpdate) { say(`<span class="good">${T`เป็นเวอร์ชันล่าสุดอยู่แล้ว (v${esc(r.current)})`}</span>`); return; }
      say(`<span class="good">${T`มีเวอร์ชันใหม่ v${esc(r.latest)} (ตอนนี้ใช้ v${esc(r.current)})`}</span>`);
      btn.innerHTML = `${icon('download')}<span>${T`อัปเดตเลย`}</span>`;
      btn.onclick = apply;
    };
  });
}

/* ---------------------------------------------------------------- helpers */
const set = async (patch) => { S = await api.invoke('settings:set', patch); render(); };
const LOC = () => (lang === 'en' ? 'en-GB' : 'th-TH');
const fmtWhen = (ms) => {
  const d = new Date(ms), today = new Date();
  const t = d.toLocaleTimeString(LOC(), { hour: '2-digit', minute: '2-digit' });
  return d.toDateString() === today.toDateString() ? t : d.toLocaleDateString(LOC(), { day: 'numeric', month: 'short' }) + ' ' + t;
};
const kbds = (text) => (text ? text.split('+').map((k) => `<kbd>${esc(k)}</kbd>`).join('') : `<kbd class="none">${T`ไม่มี`}</kbd>`);
const hostOf = (u) => { try { return new URL(u.includes('://') ? u : 'https://' + u).hostname.replace(/^www\./, ''); } catch { return u; } };

/* ---------------------------------------------------------- action kinds */
const KINDS = [
  ['app', 'apps', 'โปรแกรม'], ['url', 'globe', 'ลิงก์เว็บ'], ['search', 'search', 'ค้นหา'], ['folder', 'folder', 'โฟลเดอร์'], ['file', 'file', 'ไฟล์'],
  ['system', 'sliders', 'ควบคุมเครื่อง'], ['prism', 'sparkle', 'แอป Prism'], ['type', 'type', 'พิมพ์ข้อความ'], ['keys', 'keyboard', 'กดคีย์'], ['wait', 'clock', 'รอ'],
];
const kindIcon = (k) => (KINDS.find((x) => x[0] === k) || [0, 'bolt'])[1];
const SYSTEM = [
  ['vol_up', 'เพิ่มเสียง'], ['vol_down', 'ลดเสียง'], ['mute', 'ปิด / เปิดเสียง'], ['media_play', 'เล่น / หยุดเพลง'], ['media_next', 'เพลงถัดไป'], ['media_prev', 'เพลงก่อนหน้า'],
  ['lock', 'ล็อกหน้าจอ'], ['snip', 'แคปหน้าจอ (เลือกพื้นที่)'], ['desktop', 'ไปที่หน้าเดสก์ท็อป'], ['taskview', 'ดูหน้าต่างทั้งหมด'], ['close', 'ปิดหน้าต่างที่ใช้อยู่'],
  ['explorer', 'เปิด File Explorer'], ['taskmgr', 'เปิด Task Manager'], ['settings', 'เปิดการตั้งค่า Windows'],
];
const PRISM = [
  ['rec_toggle', 'Prism Rec: เริ่ม / หยุดอัด', 'rec'], ['rec_pause', 'Prism Rec: พัก / อัดต่อ', 'rec'], ['rec_replay', 'Prism Rec: บันทึกย้อนหลัง', 'rec'], ['rec_draw', 'Prism Rec: วาดบนจอ', 'rec'],
  ['open:rec', 'เปิด Prism Rec', 'rec'], ['open:size', 'เปิด Prism Size', 'size'], ['open:cam', 'เปิด Prism Cam', 'cam'], ['open:player', 'เปิด Prism Player', 'player'], ['open:studio', 'เปิด Prism Studio', 'studio'],
];
const SEARCHES = [
  ['YouTube', 'https://www.youtube.com/results?search_query={q}'], ['Google', 'https://www.google.com/search?q={q}'], ['Google Maps', 'https://www.google.com/maps/search/{q}'],
  ['Google รูปภาพ', 'https://www.google.com/search?tbm=isch&q={q}'], ['Wikipedia', 'https://th.wikipedia.org/w/index.php?search={q}'], ['Shopee', 'https://shopee.co.th/search?keyword={q}'],
  ['Lazada', 'https://www.lazada.co.th/catalog/?q={q}'], ['Pantip', 'https://pantip.com/search?q={q}'], ['Spotify', 'https://open.spotify.com/search/{q}'], ['Netflix', 'https://www.netflix.com/search?q={q}'],
  ['Google แปลภาษา', 'https://translate.google.com/?sl=auto&tl=en&text={q}'], ['Facebook', 'https://www.facebook.com/search/top?q={q}'],
];
function describe(a) {
  const lab = (list, v) => T((list.find((x) => x[0] === v) || [0, v])[1]);
  switch (a.kind) {
    case 'app': return T`เปิด ${a.label || a.target.split('\\').pop()}`;
    case 'url': return T`เปิด ${a.label || hostOf(a.target)}`;
    case 'search': return T`ค้นหาใน ${a.label || hostOf(a.target)} — คำที่พูดต่อท้าย`;
    case 'folder': return T`เปิดโฟลเดอร์ ${a.label || a.target.split('\\').pop()}`;
    case 'file': return T`เปิดไฟล์ ${a.label || a.target.split('\\').pop()}`;
    case 'system': return lab(SYSTEM, a.target);
    case 'prism': return lab(PRISM, a.target);
    case 'type': return T`พิมพ์ “${a.target.length > 28 ? a.target.slice(0, 28) + '…' : a.target}”`;
    case 'keys': return T`กด ${a.target}`;
    case 'wait': return T`รอ ${(+a.target || 500) / 1000} วินาที`;
    default: return a.kind;
  }
}
// picture for an action: real app icon when we have one
function actPic(a, big) {
  if ((a.kind === 'app' || a.kind === 'file' || a.kind === 'folder') && a.icon) return `<img class="${big ? 'pic' : 'mini'}" src="http://pvo.localhost/icon/${esc(a.icon)}.png" alt="" onerror="this.replaceWith(Object.assign(document.createElement('span'),{className:'${big ? 'pic ph' : 'mini ph'}',innerHTML:this.dataset.f}))" data-f="${esc(icon(kindIcon(a.kind)))}">`;
  return `<span class="${big ? 'pic ph' : 'mini ph'} k-${a.kind}">${icon(kindIcon(a.kind))}</span>`;
}

/* ------------------------------------------------------------------ views */
function showView(v) {
  document.querySelectorAll('.view').forEach((el) => el.classList.toggle('active', el.id === 'view-' + v));
  document.querySelectorAll('.dock-btn[data-view]').forEach((b) => b.classList.toggle('active', b.dataset.view === v));
  remember('prism.view', v);
  if (v !== 'settings') micTest.stop();
  if (v === 'history') loadHist();
  stopListeningKey();
}

/* --------------------------------------------------------------- render */
function render() {
  document.querySelectorAll('#mode-chips .chip').forEach((c) => c.classList.toggle('on', c.dataset.mode === S.mode));
  document.querySelectorAll('#quick-chips .chip').forEach((c) => c.classList.toggle('on', !!S[c.dataset.toggle]));
  renderHero();
  // settings controls
  document.querySelectorAll('[data-set]').forEach((el) => {
    const k = el.dataset.set, v = S[k];
    if (el.classList.contains('tabs')) el.querySelectorAll('.tab').forEach((t) => t.classList.toggle('active', String(v) === t.dataset.v));
    else if (el.classList.contains('glow-swatches')) el.querySelectorAll('button').forEach((b) => b.classList.toggle('on', b.dataset.v === v));
    else if (el.type === 'checkbox') el.checked = !!v;
    else if (document.activeElement !== el) el.value = v ?? '';
  });
  document.querySelectorAll('.range-val').forEach((b) => {
    const k = b.dataset.for, v = +S[k];
    b.textContent = k === 'strict' ? Math.round(v * 100) + '%' : k === 'mic_gain' ? '×' + v.toFixed(1) : Math.round(v * 100) + '%';
  });
  $('strict-warn').classList.toggle('hidden', !(+S.strict < 0.65));
  $('glow-preview').dataset.glow = S.glow;
  $('glow-preview').style.setProperty('--gs', S.glow_size);
  renderSamples($('wake-samples'), '__wake', S.wake_samples || [], S.wake_phrase);
  renderCmds();
  renderKeys();
}
function renderHero() {
  const busy = phase !== 'idle';
  document.body.classList.toggle('listening', phase === 'listen' || phase === 'train');
  document.body.classList.toggle('thinking', phase === 'think');
  const h = $('hero-title'), p = $('hero-sub');
  h.textContent = { listen: T`กำลังฟัง… พูดได้เลย`, train: T`กำลังฟังเสียงตัวอย่าง…`, think: T`กำลังฟังว่าพูดอะไร…` }[phase] || T`พร้อมฟังคำสั่ง`;
  const k = S.hotkey || 'Ctrl+Space';
  p.innerHTML = busy ? esc(T`กดปุ่มไมค์อีกครั้งเมื่อพูดจบ · Esc ยกเลิก`)
    : S.mode === 'hold' ? T`กด ${kbds(k)} ค้างไว้ แล้วพูด — ปล่อยเมื่อพูดจบ`
    : S.mode === 'tap' ? T`แตะ ${kbds(k)} แล้วพูด — หยุดพูดแล้วจะทำงานเอง`
    : T`กด ${kbds(k)} ค้างไว้แล้วพูด หรือแตะครั้งเดียวแล้วพูด`;
  $('hero-key').innerHTML = listening === 'talk' ? `<kbd>${T`กดคีย์ใหม่…`}</kbd>` : kbds(S.hotkey);
  $('hero-key').classList.toggle('listening', listening === 'talk');
  $('set-key').innerHTML = $('hero-key').innerHTML;
  $('set-key').classList.toggle('listening', listening === 'talk');
  const dev = SYS.device === 'gpu' ? T`การ์ดจอ` : 'CPU';
  const eng = !SYS.engine ? `<span class="bad">${T`ไม่พบ — ติดตั้งโปรแกรมใหม่`}</span>` : `${esc(SYS.model.replace(/^ggml-|\.bin$/g, ''))} · ${dev} · ${SYS.loaded ? `<span class="good">${T`พร้อม`}</span>` : T`โหลดเมื่อใช้`}`;
  $('kv-engine').innerHTML = eng;
  $('set-engine').innerHTML = !SYS.engine ? T`ไม่พบไฟล์ตัวฟังเสียงในโฟลเดอร์ runtime` : `${esc(SYS.model)} · ${SYS.device === 'gpu' ? T`ใช้การ์ดจอคำนวณ` : T`ใช้ CPU คำนวณ`} · ${SYS.loaded ? T`โหลดอยู่ในหน่วยความจำ พร้อมใช้ทันที` : T`ยังไม่ได้โหลด — จะโหลดเองตอนพูดครั้งแรก`}`;
  const mic = (SYS.mics || []).find((m) => m.id === S.mic_id);
  $('kv-mic').textContent = mic ? mic.name : (SYS.mics || []).length ? T`ค่าเริ่มต้นของ Windows` : T`ไม่พบไมโครโฟน`;
  $('kv-mic').title = $('kv-mic').textContent;
  $('btn-talk').classList.toggle('on', busy);
}

/* ------------------------------------------------------------- commands */
function renderCmds() {
  const q = $('cmd-search').value.trim().toLowerCase();
  const all = S.commands || [];
  const list = all.filter((c) => !q || c.name.toLowerCase().includes(q) || c.phrases.some((p) => p.toLowerCase().includes(q)) || c.actions.some((a) => describe(a).toLowerCase().includes(q)));
  $('cmd-count').textContent = all.length ? T`${all.length} คำสั่ง · เปิดใช้ ${all.filter((c) => c.enabled).length}` : '';
  const box = $('cmds');
  if (!all.length) {
    box.innerHTML = `<div class="empty"><div class="big-ico">${icon('wave')}</div><b>${T`ยังไม่มีคำสั่ง`}</b><p>${T`กด “เพิ่มคำสั่ง” แล้วตั้งว่าพูดคำไหน ให้เปิดอะไร`}</p><button class="pill-btn primary" data-act="add">${icon('plus')}<span>${T`เพิ่มคำสั่ง`}</span></button></div>`;
    return;
  }
  if (!list.length) { box.innerHTML = `<div class="empty"><b>${T`ไม่พบคำสั่งที่ตรงกับ “${esc(q)}”`}</b></div>`; return; }
  box.innerHTML = list.map((c, i) => card(c, i)).join('') + (q ? '' : `<button class="cmd-add glass" data-act="add">${icon('plus')}<span>${T`เพิ่มคำสั่ง`}</span></button>`);
}
function card(c, i) {
  const a0 = c.actions[0] || { kind: 'bolt' };
  const more = c.actions.length > 1 ? `<span class="more">+${c.actions.length - 1}</span>` : '';
  return `
  <div class="cmd glass${c.enabled ? '' : ' off'}" data-id="${esc(c.id)}" style="--i:${Math.min(i, 14)}">
    <div class="cmd-top">
      ${actPic(a0, true)}
      <div class="cmd-name"><b title="${esc(c.name)}">${esc(c.name)}</b><span>${c.actions.map((a) => esc(describe(a))).join(' → ') || T`ยังไม่ได้ตั้งว่าให้ทำอะไร`}</span></div>
      <label class="switch" title="${T`เปิด / ปิดคำสั่งนี้`}"><input type="checkbox" data-act="toggle" ${c.enabled ? 'checked' : ''}><i></i></label>
    </div>
    <div class="phrases">${c.phrases.map((p) => `<span class="phrase">“${esc(p)}”${c.actions.some((a) => a.kind === 'search') ? ` <em>${T`+ คำค้น`}</em>` : ''}</span>`).join('') || `<span class="muted">${T`ยังไม่มีคำพูด`}</span>`}${more}</div>
    <div class="cmd-foot">
      ${c.samples.length ? `<span class="badge ok" title="${T`จำเสียงของคุณไว้ ${c.samples.length} ครั้ง`}">${icon('mic')}${c.samples.length}</span>` : ''}
      ${c.uses ? `<span class="uses">${T`ใช้ ${c.uses} ครั้ง`}</span>` : ''}
      <i class="grow"></i>
      <button class="icon-btn" data-act="run" title="${T`ลองทำเลย (ไม่ต้องพูด)`}">${icon('play')}</button>
      <button class="icon-btn" data-act="edit" title="${T`แก้ไข`}">${icon('edit')}</button>
      <button class="icon-btn" data-act="dup" title="${T`ทำสำเนา`}">${icon('copy')}</button>
      <button class="icon-btn danger" data-act="del" title="${T`ลบ`}">${icon('trash')}</button>
    </div>
  </div>`;
}
async function cmdAct(a, c) {
  if (a === 'add') return edit(null);
  if (!c) return;
  if (a === 'edit') return edit(c);
  if (a === 'run') { await api.invoke('cmd:run', { id: c.id }); toast(T`กำลังทำ “${c.name}”`); return; }
  if (a === 'toggle') { await save({ ...c, enabled: !c.enabled }); return; }
  if (a === 'dup') { const r = await api.invoke('cmd:save', { ...c, id: '', samples: [], uses: 0, name: c.name + T` (สำเนา)` }); S = r.settings; render(); setTimeout(() => edit(S.commands.find((x) => x.id === r.id)), 50); return; }
  if (a === 'del') {
    const index = S.commands.findIndex((x) => x.id === c.id);
    S = await api.invoke('cmd:delete', c.id); render();
    toast(T`ลบ “${c.name}” แล้ว`, { label: T`เลิกทำ`, action: async () => { S = await api.invoke('cmd:restore', { cmd: c, index }); render(); } });
  }
}
async function save(c) { const r = await api.invoke('cmd:save', c); if (r.ok) { S = r.settings; render(); } return r; }
function wireCmds() {
  const box = $('cmds');
  box.onclick = (e) => {
    const b = e.target.closest('[data-act]'); if (!b) return;
    if (b.dataset.act === 'toggle' && e.target.tagName !== 'INPUT') return;
    const el = e.target.closest('.cmd');
    cmdAct(b.dataset.act, el && S.commands.find((x) => x.id === el.dataset.id));
  };
  box.ondblclick = (e) => { const el = e.target.closest('.cmd'); if (el && !e.target.closest('button, label')) edit(S.commands.find((x) => x.id === el.dataset.id)); };
  box.oncontextmenu = (e) => {
    const el = e.target.closest('.cmd'); if (!el) return;
    e.preventDefault();
    const c = S.commands.find((x) => x.id === el.dataset.id); if (!c) return;
    ctxMenu(e.clientX, e.clientY, [
      { icon: 'play', label: T`ลองทำเลย`, run: () => cmdAct('run', c) },
      { icon: 'edit', label: T`แก้ไข`, run: () => cmdAct('edit', c) },
      { icon: 'mic', label: T`สอนด้วยเสียง`, run: () => edit(c, { train: true }) },
      { icon: 'copy', label: T`ทำสำเนา`, run: () => cmdAct('dup', c) },
      { icon: c.enabled ? 'eyeoff' : 'eye', label: c.enabled ? T`ปิดคำสั่งนี้ไว้ก่อน` : T`เปิดคำสั่งนี้`, run: () => cmdAct('toggle', c) },
      '-',
      { icon: 'trash', label: T`ลบ`, danger: true, run: () => cmdAct('del', c) },
    ], c.name);
  };
  $('cmd-search').oninput = renderCmds;
  $('btn-add').onclick = $('dock-add').onclick = () => edit(null);
}

/* ---------------------------------------------------------------- editor */
let APPS = null;
async function edit(orig, opts = {}) {
  const c = JSON.parse(JSON.stringify(orig || { id: '', name: '', phrases: [], actions: [], enabled: true, samples: [] }));
  let dirty = false;
  const isNew = !orig;
  await htmlBox(`
    <div class="editor" id="ed">
      <div class="ed-head"><h3>${isNew ? T`เพิ่มคำสั่งใหม่` : T`แก้ไขคำสั่ง`}</h3><span class="muted">${T`ลากโปรแกรม ไฟล์ โฟลเดอร์ หรือลิงก์มาวางในหน้าต่างนี้ได้เลย`}</span></div>
      <div class="ed-body">
        <div class="ed-col">
          <label class="ed-label">${T`ชื่อคำสั่ง`}</label>
          <label class="field wide"><input id="ed-name" type="text" spellcheck="false" placeholder="${T`เช่น เปิดยูทูป`}"></label>
          <label class="ed-label">${T`คำที่พูด`} <small class="muted" id="ed-ph-tip">${T`ใส่ได้หลายแบบ — พิมพ์แล้วกด Enter`}</small></label>
          <div class="chips-input field wide" id="ed-phrases"><input id="ed-ph" type="text" spellcheck="false" placeholder="${T`เช่น เปิดยูทูป`}"></div>
          <label class="ed-label">${T`สอนด้วยเสียง`} <small class="muted">${T`พูดคำสั่ง 2–3 ครั้ง โปรแกรมจะจำเสียงคุณ แม่นแม้สำเนียงหรือคำแปลก`}</small></label>
          <div class="samples-row" id="ed-samples"></div>
        </div>
        <div class="ed-col">
          <label class="ed-label">${T`ให้ทำอะไร`} <small class="muted">${T`ทำตามลำดับจากบนลงล่าง`}</small></label>
          <div class="acts" id="ed-acts"></div>
          <div class="kind-grid" id="ed-kinds">${KINDS.map(([k, ic, lb]) => `<button class="kind" data-k="${k}">${icon(ic)}<span>${T(lb)}</span></button>`).join('')}</div>
        </div>
      </div>
      <div class="modal-btns"><button class="pill-btn" id="ed-try">${icon('play')}<span>${T`ลองทำ`}</span></button><i class="grow"></i><button class="pill-btn" data-x>${T`ยกเลิก`}</button><button class="pill-btn primary" id="ed-save">${icon('check')}<span>${T`บันทึก`}</span></button></div>
      <div class="picker hidden" id="ed-picker"></div>
    </div>`, (m, close) => {
    const card = m.querySelector('.modal-card'); card.classList.add('wide-card');
    const name = m.querySelector('#ed-name'), ph = m.querySelector('#ed-ph');
    name.value = c.name;
    let autoName = !c.name;
    name.oninput = () => { c.name = name.value; autoName = !name.value; dirty = true; };
    const suggestName = () => { if (!autoName) return; const a = c.actions.find((x) => x.label); name.value = c.name = a ? (a.kind === 'search' ? T`ค้นหาใน ${a.label}` : a.label) : (c.phrases[0] || ''); };
    // phrases as chips
    const drawPh = () => {
      const box = m.querySelector('#ed-phrases');
      box.querySelectorAll('.chip-p').forEach((x) => x.remove());
      c.phrases.forEach((p, i) => { const s = document.createElement('span'); s.className = 'chip-p'; s.innerHTML = `${esc(p)}<button data-i="${i}" title="${T`ลบ`}">${icon('x')}</button>`; box.insertBefore(s, ph); });
      const search = c.actions.some((a) => a.kind === 'search');
      m.querySelector('#ed-ph-tip').textContent = search ? T`คำนำหน้า — สิ่งที่พูดต่อจากนี้จะถูกเอาไปค้นหา เช่น “หายูทูป แมวน่ารัก”` : T`ใส่ได้หลายแบบ — พิมพ์แล้วกด Enter`;
    };
    const addPh = () => { const v = ph.value.trim(); if (v && !c.phrases.includes(v)) { c.phrases.push(v); dirty = true; if (autoName && !c.actions.some((a) => a.label)) suggestName(); } ph.value = ''; drawPh(); };
    ph.onkeydown = (e) => { if (e.key === 'Enter' || e.key === ',') { e.preventDefault(); addPh(); } if (e.key === 'Backspace' && !ph.value && c.phrases.length) { c.phrases.pop(); drawPh(); } };
    ph.onblur = addPh;
    m.querySelector('#ed-phrases').onclick = (e) => { const b = e.target.closest('button[data-i]'); if (b) { c.phrases.splice(+b.dataset.i, 1); dirty = true; drawPh(); } else ph.focus(); };
    drawPh();

    // actions
    const actsBox = m.querySelector('#ed-acts');
    const drawActs = () => {
      if (!c.actions.length) { actsBox.innerHTML = `<div class="acts-empty muted">${icon('bolt')}<span>${T`เลือกด้านล่างว่าจะให้เปิดอะไร`}</span></div>`; return; }
      actsBox.innerHTML = c.actions.map((a, i) => `<div class="act" data-i="${i}">${actPic(a, false)}<div class="act-body">${actEditor(a, i)}</div>
        <div class="act-tools">${i > 0 ? `<button class="icon-btn" data-a="up" title="${T`เลื่อนขึ้น`}">${icon('up')}</button>` : ''}<button class="icon-btn" data-a="try" title="${T`ลองอันนี้`}">${icon('play')}</button><button class="icon-btn danger" data-a="del" title="${T`เอาออก`}">${icon('x')}</button></div></div>`).join('');
      wireActs();
    };
    const wireActs = () => {
      actsBox.querySelectorAll('.act').forEach((row) => {
        const i = +row.dataset.i, a = c.actions[i];
        row.querySelectorAll('[data-f]').forEach((inp) => {
          const f = inp.dataset.f;
          inp.oninput = inp.onchange = () => {
            dirty = true;
            if (f === 'preset') { const p = SEARCHES.find((x) => x[0] === inp.value); if (p) { a.target = p[1]; a.label = p[0]; } else { a.label = ''; } drawActs(); suggestName(); return; }
            a[f] = inp.value;
            if (a.kind === 'url' && f === 'target') a.label = hostOf(inp.value);
            if (f === 'target' && (a.kind === 'system' || a.kind === 'prism')) { a.label = ''; suggestName2(a); }
          };
        });
        row.querySelectorAll('[data-pick]').forEach((b) => { b.onclick = () => pickFor(i, b.dataset.pick); });
        row.querySelectorAll('[data-keycap]').forEach((b) => { b.onclick = () => captureKeys(b, (combo) => { a.target = combo; dirty = true; drawActs(); }); });
        row.querySelector('[data-a="del"]').onclick = () => { c.actions.splice(i, 1); dirty = true; drawActs(); drawPh(); };
        const up = row.querySelector('[data-a="up"]'); if (up) up.onclick = () => { [c.actions[i - 1], c.actions[i]] = [c.actions[i], c.actions[i - 1]]; drawActs(); };
        row.querySelector('[data-a="try"]').onclick = () => { api.invoke('action:try', { action: a, q: a.kind === 'search' ? T`ทดสอบ` : '' }); toast(T`กำลังลอง: ${describe(a)}`); };
      });
    };
    const suggestName2 = (a) => { if (autoName) { name.value = c.name = describe(a); } };
    const addKind = (k) => {
      const a = { kind: k, target: '', args: '', label: '', icon: '' };
      if (k === 'search') { a.target = SEARCHES[0][1]; a.label = SEARCHES[0][0]; }
      if (k === 'system') a.target = 'vol_up';
      if (k === 'prism') a.target = (PRISM.find((p) => SYS.prism.includes(p[2])) || PRISM[0])[0];
      if (k === 'wait') a.target = '1000';
      c.actions.push(a); dirty = true; drawActs(); drawPh();
      if (k === 'app') pickFor(c.actions.length - 1, 'app');
      else if (k === 'folder' || k === 'file') pickFor(c.actions.length - 1, k);
      else { const inp = actsBox.querySelector(`.act[data-i="${c.actions.length - 1}"] [data-f="target"]`); if (inp && inp.tagName !== 'SELECT') inp.focus(); if (k === 'system' || k === 'prism' || k === 'search') suggestName2(a); }
    };
    m.querySelector('#ed-kinds').onclick = (e) => { const b = e.target.closest('[data-k]'); if (b) addKind(b.dataset.k); };

    // program / file / folder picking
    const picker = m.querySelector('#ed-picker');
    const pickFor = async (i, what) => {
      const a = c.actions[i];
      if (what !== 'app') {
        const r = await api.invoke('pick:path', what);
        if (r.ok) { a.target = r.path; a.label = r.name; a.icon = r.key; if (r.dir && a.kind === 'file') a.kind = 'folder'; dirty = true; suggestName(); drawActs(); }
        else if (!a.target) { c.actions.splice(i, 1); drawActs(); }
        return;
      }
      picker.classList.remove('hidden');
      picker.innerHTML = `<div class="pk-head"><label class="field wide"><span data-icon="search"></span><input id="pk-q" type="text" spellcheck="false" placeholder="${T`ค้นหาโปรแกรม`}"></label>
        <button class="pill-btn" id="pk-browse">${icon('folder')}<span>${T`เลือกไฟล์ .exe เอง`}</span></button><button class="icon-btn" id="pk-x" title="${T`ปิด`}">${icon('x')}</button></div>
        <div class="pk-list" id="pk-list"><div class="muted pk-wait"><div class="progress indet"><i></i></div>${T`กำลังอ่านรายการโปรแกรมในเครื่อง…`}</div></div>`;
      hydrateIcons(picker);
      const q = picker.querySelector('#pk-q'); q.focus();
      const done = () => { picker.classList.add('hidden'); picker.innerHTML = ''; if (!a.target) { c.actions.splice(i, 1); } drawActs(); };
      picker.querySelector('#pk-x').onclick = done;
      picker.querySelector('#pk-browse').onclick = async () => { const r = await api.invoke('pick:path', 'app'); if (r.ok) { a.target = r.path; a.label = r.name; a.icon = r.key; dirty = true; suggestName(); done(); } };
      if (!APPS) APPS = await api.invoke('apps:list', false);
      const draw = () => {
        const s = q.value.trim().toLowerCase();
        const list = (APPS || []).filter((x) => !s || x.name.toLowerCase().includes(s)).slice(0, 400);
        picker.querySelector('#pk-list').innerHTML = list.length ? list.map((x, k) => `<button class="pk-app" data-k="${k}" title="${esc(x.target)}"><img src="${esc(x.icon)}" alt="" loading="lazy" onerror="this.style.visibility='hidden'"><span>${esc(x.name)}</span></button>`).join('') : `<div class="muted pk-wait">${T`ไม่พบโปรแกรม — กด “เลือกไฟล์ .exe เอง”`}</div>`;
        picker.querySelectorAll('.pk-app').forEach((b) => { b.onclick = () => { const x = list[+b.dataset.k]; a.target = x.target; a.label = x.name; a.icon = x.key; dirty = true; suggestName(); done(); }; });
      };
      q.oninput = draw;
      q.onkeydown = (e) => { if (e.key === 'Enter') { const f = picker.querySelector('.pk-app'); if (f) f.click(); } if (e.key === 'Escape') { e.stopPropagation(); done(); } };
      draw();
    };

    // drag & drop (Tauri gives real paths)
    const offDrop = api.on('voice:drop', async ({ paths }) => {
      for (const p of paths || []) {
        const info = await api.invoke('path:info', p);
        c.actions.push({ kind: info.kind, target: info.path, args: '', label: info.kind === 'url' ? hostOf(info.path) : info.name, icon: info.kind === 'url' ? '' : info.key });
      }
      dirty = true; suggestName(); drawActs(); drawPh();
      card.classList.add('dropped'); setTimeout(() => card.classList.remove('dropped'), 600);
    });
    // Ctrl+V a link
    m.addEventListener('paste', (e) => {
      if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA') return;
      const t = (e.clipboardData.getData('text') || '').trim();
      if (/^https?:\/\//i.test(t)) { c.actions.push({ kind: 'url', target: t, args: '', label: hostOf(t), icon: '' }); dirty = true; suggestName(); drawActs(); }
    });

    // voice samples (needs a saved command)
    const sampBox = m.querySelector('#ed-samples');
    const drawSamples = () => renderSamples(sampBox, c.id || '__new', c.samples, c.phrases[0] || c.name, async () => {
      if (!c.id) {
        addPh();
        if (!c.phrases.length && !c.name) { toast(T`ใส่ชื่อหรือคำที่พูดก่อน แล้วค่อยสอนด้วยเสียง`, { kind: 'err' }); return false; }
        const r = await save(c); if (!r.ok) return false; c.id = r.id;
      }
      return c.id;
    });
    drawSamples();
    const offSample = api.on('voice:sample', (s) => { if (s.target === c.id && s.ok) { c.samples.push(s.id); drawSamples(); } else if (s.target === c.id) drawSamples(); });
    const offSet = api.on('voice:settings', (ns) => { const x = (ns.commands || []).find((y) => y.id === c.id); if (x) { c.samples = x.samples; drawSamples(); } });

    drawActs();
    const finish = (v) => { offDrop(); offSample(); offSet(); close(v); };
    const doSave = async () => {
      addPh();
      if (!c.phrases.length) { toast(T`ใส่คำที่จะพูดอย่างน้อย 1 คำ`, { kind: 'err' }); ph.focus(); return; }
      if (!c.actions.length) { toast(T`เลือกว่าจะให้ทำอะไรอย่างน้อย 1 อย่าง`, { kind: 'err' }); return; }
      const bad = c.actions.find((a) => !String(a.target).trim() && a.kind !== 'wait');
      if (bad) { toast(T`ยังไม่ได้ใส่รายละเอียดของ “${describe(bad)}”`, { kind: 'err' }); return; }
      if (!c.name.trim()) c.name = c.phrases[0];
      const r = await save(c);
      if (r.ok) { toast(isNew ? T`เพิ่มคำสั่ง “${c.name}” แล้ว — กด ${S.hotkey} แล้วพูด “${c.phrases[0]}” ได้เลย` : T`บันทึกแล้ว`); finish(true); }
    };
    m.querySelector('#ed-save').onclick = doSave;
    m.querySelector('[data-x]').onclick = async () => { if (dirty && !(await confirmClose())) return; finish(null); };
    m.querySelector('#ed-try').onclick = () => { if (!c.actions.length) return; c.actions.forEach((a, k) => setTimeout(() => api.invoke('action:try', { action: a, q: '' }), k * 250)); };
    m.addEventListener('keydown', (e) => { if ((e.ctrlKey && e.key === 'Enter') || (e.ctrlKey && e.key.toLowerCase() === 's')) { e.preventDefault(); doSave(); } });
    m.onpointerdown = null; // don't lose edits by clicking outside
    m._esc = () => {
      if (!picker.classList.contains('hidden')) { picker.querySelector('#pk-x').click(); return; }
      if (card.querySelector('.mini-confirm')) { card.querySelector('.mini-confirm [data-n]').click(); return; }
      m.querySelector('[data-x]').click();
    };
    (opts.train ? sampBox.querySelector('[data-rec]') || name : c.phrases.length ? name : ph).focus();
    if (opts.train) setTimeout(() => { const b = sampBox.querySelector('[data-rec]'); if (b) b.click(); }, 300);
  });
}
function confirmClose() {
  return new Promise((res) => {
    const m = document.createElement('div');
    m.className = 'mini-confirm glass';
    m.innerHTML = `<b>${T`ยังไม่ได้บันทึก — ทิ้งการแก้ไข?`}</b><div class="modal-btns"><button class="pill-btn" data-n>${T`กลับไปแก้ต่อ`}</button><button class="pill-btn warn" data-y>${T`ทิ้ง`}</button></div>`;
    document.querySelector('#modal .modal-card').appendChild(m);
    m.querySelector('[data-n]').onclick = () => { m.remove(); res(false); };
    m.querySelector('[data-y]').onclick = () => { m.remove(); res(true); };
  });
}
function actEditor(a, i) {
  const sel = (f, list, v) => `<label class="field mini"><select data-f="${f}">${list.map(([k, lb, need]) => `<option value="${esc(k)}" ${k === v ? 'selected' : ''} ${need && !SYS.prism.includes(need) ? 'disabled' : ''}>${esc(T(lb))}${need && !SYS.prism.includes(need) ? ' ' + T`(ไม่พบในเครื่อง)` : ''}</option>`).join('')}</select></label>`;
  switch (a.kind) {
    case 'app': return `<b>${esc(a.label || T`ยังไม่ได้เลือกโปรแกรม`)}</b><small class="path" title="${esc(a.target)}">${esc(a.target)}</small>
      <div class="act-row"><button class="pill-btn small" data-pick="app">${icon('apps')}<span>${T`เปลี่ยนโปรแกรม`}</span></button><label class="field mini grow"><input data-f="args" type="text" spellcheck="false" placeholder="${T`ตัวเลือกเพิ่ม (ไม่ต้องใส่ก็ได้)`}" value="${esc(a.args)}"></label></div>`;
    case 'folder': case 'file': return `<b>${esc(a.label || T`ยังไม่ได้เลือก`)}</b><small class="path" title="${esc(a.target)}">${esc(a.target)}</small><div class="act-row"><button class="pill-btn small" data-pick="${a.kind}">${icon(a.kind)}<span>${T`เลือกใหม่`}</span></button></div>`;
    case 'url': return `<b>${T`เปิดลิงก์`}</b><label class="field mini wide"><input data-f="target" type="text" spellcheck="false" placeholder="https://…" value="${esc(a.target)}"></label>`;
    case 'search': {
      const pre = SEARCHES.find((x) => x[1] === a.target);
      return `<b>${T`ค้นหาคำที่พูดต่อท้าย`}</b><div class="act-row"><label class="field mini"><select data-f="preset">${SEARCHES.map(([n]) => `<option ${pre && pre[0] === n ? 'selected' : ''}>${esc(n)}</option>`).join('')}<option value="" ${pre ? '' : 'selected'}>${T`กำหนดเอง…`}</option></select></label></div>
        ${pre ? '' : `<label class="field mini wide"><input data-f="target" type="text" spellcheck="false" placeholder="https://example.com/search?q={q}" value="${esc(a.target)}"></label><small class="muted">${T`ใส่ {q} ตรงที่ให้แทนด้วยคำที่พูด`}</small>`}`;
    }
    case 'system': return `<b>${T`ควบคุมเครื่อง`}</b>${sel('target', SYSTEM, a.target)}`;
    case 'prism': return `<b>${T`แอป Prism`}</b>${sel('target', PRISM, a.target)}`;
    case 'type': return `<b>${T`พิมพ์ข้อความลงในหน้าต่างที่ใช้อยู่`}</b><label class="field mini wide"><textarea data-f="target" rows="2" spellcheck="false" placeholder="${T`ข้อความที่จะพิมพ์`}">${esc(a.target)}</textarea></label>`;
    case 'keys': return `<b>${T`กดคีย์`}</b><div class="act-row"><button class="key-edit" data-keycap>${a.target ? kbds(a.target) : `<kbd>${T`คลิกแล้วกดคีย์`}</kbd>`}</button></div>`;
    case 'wait': return `<b>${T`รอก่อนทำขั้นต่อไป`}</b>${sel('target', [['500', '0.5 วินาที'], ['1000', '1 วินาที'], ['2000', '2 วินาที'], ['3000', '3 วินาที'], ['5000', '5 วินาที']], String(a.target))}`;
    default: return esc(a.kind);
  }
}
// any key combo for "กดคีย์" actions (no global registration)
function captureKeys(btn, done) {
  btn.innerHTML = `<kbd>${T`กดคีย์…`}</kbd>`; btn.classList.add('listening');
  const on = (e) => {
    e.preventDefault(); e.stopPropagation();
    if (e.key === 'Escape') { off(); btn.classList.remove('listening'); done(btn.dataset.prev || ''); return; }
    const c = comboOf(e, true); if (!c) return;
    off(); btn.classList.remove('listening'); done(c);
  };
  const off = () => removeEventListener('keydown', on, true);
  addEventListener('keydown', on, true);
}

/* ------------------------------------------------------ voice samples UI */
let trainingFor = null;
function renderSamples(box, target, ids, phrase, ensureId) {
  const n = ids.length;
  box.innerHTML = `${ids.map((id, i) => `<span class="sample" data-id="${esc(id)}"><button class="icon-btn" data-play="${esc(id)}" title="${T`ฟังเสียงที่อัดไว้`}">${icon('play')}</button><em>${i + 1}</em><button class="icon-btn danger" data-del="${esc(id)}" title="${T`ลบเสียงนี้`}">${icon('x')}</button></span>`).join('')}
    <button class="pill-btn small${trainingFor === target ? ' rec' : ''}" data-rec>${icon('mic')}<span>${trainingFor === target ? T`กำลังฟัง… พูดเลย` : n ? T`พูดอีกครั้ง (${n}/3)` : T`เริ่มสอน — พูด “${phrase || '…'}”`}</span></button>
    ${n >= 3 ? `<span class="good small-note">${icon('check')}${T`พร้อมใช้`}</span>` : ''}`;
  box.querySelector('[data-rec]').onclick = async () => {
    let id = target;
    if (ensureId) { id = await ensureId(); if (!id) return; }
    trainingFor = id;
    renderSamples(box, id, ids, phrase, ensureId);
    if (!(await api.invoke('train', id))) { trainingFor = null; toast(T`กำลังฟังคำสั่งอื่นอยู่ ลองใหม่อีกครั้ง`, { kind: 'err' }); }
  };
  box.querySelectorAll('[data-play]').forEach((b) => { b.onclick = () => { new Audio(`http://pvo.localhost/voice/${b.dataset.play}.wav`).play().catch(() => {}); }; });
  box.querySelectorAll('[data-del]').forEach((b) => {
    b.onclick = async () => { S = await api.invoke('sample:delete', { target, id: b.dataset.del }); ids.splice(ids.indexOf(b.dataset.del), 1); renderSamples(box, target, ids, phrase, ensureId); render(); };
  });
}

/* ---------------------------------------------------------------- history */
async function loadHist() { HIST = (await api.invoke('history:list')) || []; renderHist(); }
function renderHist() {
  const box = $('hist');
  if (!HIST.length) { box.innerHTML = `<div class="empty"><div class="big-ico">${icon('history')}</div><b>${T`ยังไม่มีประวัติ`}</b><p>${T`กด ${S.hotkey} แล้วพูดคำสั่ง — สิ่งที่ได้ยินจะมาอยู่ตรงนี้`}</p></div>`; return; }
  box.innerHTML = HIST.map((h, i) => `<div class="h-row ${h.ok ? 'ok' : 'bad'}" data-i="${i}">
      <span class="h-ico">${icon(h.ok ? 'check' : 'x')}</span>
      <span class="h-main"><b>${h.heard ? '“' + esc(h.heard) + '”' : `<span class="muted">${h.how === 'voice' ? T`(จำได้จากเสียง)` : T`(ไม่ได้ยินคำชัดๆ)`}</span>`}</b>
        <small>${h.ok ? `→ ${esc(h.name || '')}${h.query ? ` · ${T`ค้นหา`} “${esc(h.query)}”` : ''}${h.how === 'voice' ? ' · ' + T`จากเสียงที่สอนไว้` : ''}` : h.near ? T`ไม่ตรงคำสั่งไหน · ใกล้กับ “${esc(h.near)}”` : T`ไม่ตรงคำสั่งไหน`}</small></span>
      <span class="h-time">${fmtWhen(h.t)}${h.ms ? ` · ${h.ms < 150 ? T`ทันที` : (h.ms / 1000).toFixed(1) + ' ' + T`วิ`}` : ''}</span>
      <span class="h-acts">${h.heard ? `<button class="pill-btn small" data-h="add">${icon('plus')}<span>${T`เพิ่มเป็นคำของคำสั่ง…`}</span></button>` : ''}${h.ok && h.id ? `<button class="icon-btn" data-h="run" title="${T`ทำอีกครั้ง`}">${icon('play')}</button>` : ''}</span>
    </div>`).join('');
}
function wireHist() {
  $('hist').onclick = async (e) => {
    const b = e.target.closest('[data-h]'); if (!b) return;
    const h = HIST[+b.closest('.h-row').dataset.i]; if (!h) return;
    if (b.dataset.h === 'run') { api.invoke('cmd:run', { id: h.id, q: h.query || '' }); return; }
    const r = b.getBoundingClientRect();
    const list = [...S.commands].sort((x, y) => (y.name === h.near) - (x.name === h.near) || (y.uses || 0) - (x.uses || 0));
    ctxMenu(r.left, r.bottom + 6, list.slice(0, 14).map((c) => ({ icon: 'plus', label: c.name, run: async () => {
      const res = await api.invoke('add:phrase', { id: c.id, phrase: h.heard });
      if (res.ok) { S = res.settings; render(); toast(T`ครั้งหน้าพูด “${h.heard}” จะทำ “${c.name}”`); }
    } })), T`เพิ่ม “${h.heard}” ให้คำสั่ง`);
  };
  $('hist-clear').onclick = async () => { if (!HIST.length || !(await confirmBox(T`ล้างประวัติทั้งหมด?`, T`ลบรายการที่ได้ยินทั้งหมด (คำสั่งไม่ถูกลบ)`, T`ล้าง`, true))) return; await api.invoke('history:clear'); loadHist(); };
}
function showLast(h) {
  const el = $('last-heard');
  el.classList.remove('hidden');
  el.className = 'last-heard ' + (h.ok ? 'ok' : 'bad');
  el.innerHTML = `${icon(h.ok ? 'check' : 'x')}<span>${h.ok ? `<b>${esc(h.name)}</b>${h.heard ? ` · “${esc(h.heard)}”` : ''}` : `${T`ไม่ตรงคำสั่ง`}${h.heard ? ` · “${esc(h.heard)}”` : ''}`}</span>${!h.ok && h.heard ? `<button class="pill-btn small" id="lh-add">${icon('plus')}<span>${T`เพิ่มเป็นคำของคำสั่ง…`}</span></button>` : ''}`;
  const b = $('lh-add');
  if (b) b.onclick = () => { HIST.unshift(h); renderHist(); showView('history'); setTimeout(() => { const x = document.querySelector('.h-row [data-h="add"]'); if (x) x.click(); }, 120); };
}

/* -------------------------------------------------------------- settings */
const GLOWS = [['prism', 'Prism'], ['aurora', 'แสงเหนือ'], ['sunset', 'พระอาทิตย์ตก'], ['ocean', 'ทะเล'], ['mono', 'ขาวนวล']];
function fillSelects() {
  $('sel-mic').innerHTML = `<option value="">${T`ค่าเริ่มต้นของ Windows`}</option>` + (SYS.mics || []).map((m) => `<option value="${esc(m.id)}">${esc(m.name)}</option>`).join('');
}
function wireSettings() {
  document.querySelectorAll('[data-set]').forEach((el) => {
    const k = el.dataset.set;
    if (el.classList.contains('tabs')) el.querySelectorAll('.tab').forEach((t) => { t.onclick = () => set({ [k]: isNaN(+t.dataset.v) || k === 'engine_keep' ? t.dataset.v : +t.dataset.v }); });
    else if (el.classList.contains('glow-swatches')) {
      el.innerHTML = GLOWS.map(([v, n]) => `<button data-v="${v}" class="g-${v}" title="${esc(T(n))}"></button>`).join('');
      el.querySelectorAll('button').forEach((b) => { b.onclick = () => set({ [k]: b.dataset.v }); });
    } else if (el.type === 'checkbox') el.onchange = () => { if (k === 'wake_on' && el.checked && !(S.wake_samples || []).length) { el.checked = false; toast(T`สอนคำปลุกด้วยเสียงก่อน (กด “เริ่มสอน” ด้านล่าง)`, { kind: 'err' }); return; } set({ [k]: el.checked }); };
    else if (el.type === 'range') { el.oninput = () => { S[k] = +el.value; render(); }; el.onchange = () => set({ [k]: +el.value }); }
    else if (el.tagName === 'SELECT') el.onchange = () => set({ [k]: el.value });
    else if (el.type === 'text') el.onchange = () => set({ [k]: el.value.trim() || (k === 'wake_phrase' ? 'เฮ้ พริซึม' : '') });
  });
  document.querySelectorAll('#mode-chips .chip').forEach((c) => { c.onclick = () => set({ mode: c.dataset.mode }); });
  document.querySelectorAll('#quick-chips .chip').forEach((c) => {
    c.onclick = () => {
      const k = c.dataset.toggle;
      if (k === 'wake_on' && !S.wake_on && !(S.wake_samples || []).length) { showView('settings'); $('wake-samples').scrollIntoView({ behavior: 'smooth', block: 'center' }); toast(T`สอนคำปลุกด้วยเสียงก่อน (พูด 3 ครั้ง) แล้วค่อยเปิดใช้`); return; }
      set({ [k]: !S[k] });
    };
  });
  $('btn-startup-ask').onclick = askStartup;
  $('btn-warm').onclick = async () => { await api.invoke('engine:warm'); $('set-engine').textContent = T`กำลังโหลด…`; setTimeout(refreshSys, 2500); setTimeout(refreshSys, 6000); };
  $('btn-glow-demo').onclick = () => api.invoke('glow:demo');
  $('glow-preview').onclick = () => api.invoke('glow:demo');
  $('btn-starter').onclick = async () => { const n = S.commands.length; S = await api.invoke('starter:reset'); render(); toast(S.commands.length > n ? T`เพิ่มกลับมา ${S.commands.length - n} คำสั่ง` : T`มีคำสั่งตัวอย่างครบแล้ว`); };
  $('sel-mic').addEventListener('change', () => { if (micTest.on) { micTest.stop(); setTimeout(() => micTest.start(), 400); } });
}
async function refreshSys() { SYS = await api.invoke('sys:info'); renderHero(); }

/* --------------------------------------------------------------- mic test */
const micTest = {
  on: false, hold: 0,
  db: (v) => Math.max(0, Math.min(1, (20 * Math.log10(Math.max(v, 1e-5)) + 60) / 60)),
  hint(t, cls) { const h = $('mic-hint'); h.textContent = t; h.className = cls || ''; },
  start() { this.on = true; this.hold = 0; $('mictest-label').textContent = T`หยุดทดสอบ`; this.hint(T`ลองพูดดูเลย…`); api.invoke('mic:test', true); },
  stop() {
    if (!this.on) return;
    this.on = false; api.invoke('mic:test', false);
    $('mictest-label').textContent = T`เริ่มทดสอบ`; $('mic-bar').style.width = '0'; $('mic-peak').style.left = '0';
    this.hint(T`กดเริ่ม แล้วลองพูดตามปกติ — แถบควรขึ้นถึงช่วงสีเขียว`);
  },
  onEvent(m) {
    if (m.stopped) { this.stop(); return; }
    if (!this.on) return;
    if (m.ok === false) { this.hint(T`เปิดไมโครโฟนไม่ได้ — ลองเลือกไมค์ตัวอื่น`, 'bad'); return; }
    const lv = this.db(m.peak);
    this.hold = Math.max(lv, this.hold - 0.012);
    $('mic-bar').style.width = lv * 100 + '%';
    $('mic-peak').style.left = `calc(${this.hold * 100}% - 2px)`;
    $('mic-meter').classList.toggle('clip', m.peak >= 0.99);
    if (m.peak >= 0.99) this.hint(T`ดังเกินไป — ลดความดังไมค์ลง`, 'bad');
    else if (this.hold >= 0.6) this.hint(T`ระดับเสียงกำลังดี`, 'good');
    else if (this.hold > 0.2) this.hint(T`ค่อนข้างเบา — ลองเพิ่มความดังไมค์`);
  },
  wire() { $('btn-mictest').onclick = () => (this.on ? this.stop() : this.start()); },
};

/* -------------------------------------------------------------- shortcuts */
const KEY_ACTIONS = [['talk', 'พูดคำสั่ง (กดค้าง หรือ แตะ)'], ['open', 'เปิดหน้าต่าง Prism VoiceOpen'], ['repeat', 'ทำคำสั่งล่าสุดอีกครั้ง']];
let keyStatus = {}, listening = null;
function renderKeys() {
  const row = (label, keys) => `<div class="key-row"><span>${label}</span><span>${keys.map((k) => `<kbd>${k}</kbd>`).join('')}</span></div>`;
  $('keys-global').innerHTML = KEY_ACTIONS.map(([id, label]) => {
    const text = keyText(id), bad = text && keyStatus[id] === false;
    return `<div class="key-row"><span>${T(label)}${bad ? `<em class="key-warn">${T`ใช้ไม่ได้ — โปรแกรมอื่นจองคีย์นี้ไว้ ลองคีย์อื่น`}</em>` : ''}</span>
      <button class="key-edit${listening === id ? ' listening' : ''}" data-key="${id}" title="${T`คลิกเพื่อเปลี่ยนคีย์ลัด`}">${listening === id ? `<kbd>${T`กดคีย์ใหม่…`}</kbd>` : kbds(text)}</button></div>`;
  }).join('') + row(T`ยกเลิกระหว่างฟัง`, ['Esc']) + row(T`พูดคำสั่งจากถาดข้างนาฬิกา`, [T`คลิกขวาที่ไอคอน`]);
  $('keys-local').innerHTML = [
    row(T`เพิ่มคำสั่ง`, ['Ctrl', 'N']), row(T`ค้นหาคำสั่ง`, ['Ctrl', 'F']), row(T`เปิดหน้าคีย์ลัด`, ['F1']),
    row(T`บันทึกคำสั่งในหน้าแก้ไข`, ['Ctrl', 'S']), row(T`เพิ่มคำที่พูด`, ['Enter']), row(T`วางลิงก์ในหน้าแก้ไข`, ['Ctrl', 'V']), row(T`ปิดหน้าต่างย่อย`, ['Esc']),
  ].join('');
}
// KeyboardEvent → "Ctrl+Alt+R"; null = only a modifier so far, '' = not allowed (globals need a modifier or F-key)
function comboOf(e, any = false) {
  const c = e.code;
  const named = { Space: 'Space', Insert: 'Insert', Delete: 'Delete', Home: 'Home', End: 'End', PageUp: 'PageUp', PageDown: 'PageDown', Pause: 'Pause', ScrollLock: 'ScrollLock', PrintScreen: 'PrintScreen', ArrowLeft: 'Left', ArrowRight: 'Right', ArrowUp: 'Up', ArrowDown: 'Down', Backquote: 'Backquote', Enter: 'Enter', Tab: 'Tab', Escape: 'Esc', Backspace: 'Backspace' };
  const k = c.startsWith('Key') ? c.slice(3) : c.startsWith('Digit') ? c.slice(5) : /^F\d+$/.test(c) ? c : /^Numpad\d$/.test(c) ? 'Num' + c.slice(6) : named[c];
  if (!k) return null;
  const mods = [e.ctrlKey && 'Ctrl', e.altKey && 'Alt', e.shiftKey && 'Shift', e.metaKey && 'Win'].filter(Boolean);
  if (!any && !mods.some((m) => m !== 'Shift') && !/^F\d+$/.test(k) && !['Pause', 'ScrollLock'].includes(k)) return '';
  return [...mods, k].join('+');
}
async function refreshKeyStatus() { await new Promise((r) => setTimeout(r, 250)); keyStatus = (await api.invoke('hotkeys:status')) || {}; renderKeys(); }
async function setKey(id, text) {
  const all = { talk: S.hotkey, ...DEF_KEYS, ...(S.hotkeys || {}) }; all.talk = S.hotkey;
  if (text) { const clash = Object.keys(all).find((o) => o !== id && all[o] === text); if (clash) { toast(T`${text} ใช้กับ “${T(KEY_ACTIONS.find((a) => a[0] === clash)[1])}” อยู่แล้ว`, { kind: 'err' }); return false; } }
  if (id === 'talk' && !text) { toast(T`คีย์สำหรับพูดต้องมี`, { kind: 'err' }); return false; }
  S = await api.invoke('settings:set', id === 'talk' ? { hotkey: text } : { hotkeys: { ...DEF_KEYS, ...(S.hotkeys || {}), [id]: text } });
  await refreshKeyStatus(); render();
  if (text && keyStatus[id] === false) toast(T`${text} ใช้ไม่ได้ — โปรแกรมอื่นจองไว้`, { kind: 'err' });
  else toast(text ? T`ตั้งคีย์เป็น ${text} แล้ว` : T`ปิดคีย์ลัดนี้แล้ว`);
  return true;
}
function startListeningKey(id) { listening = id; api.invoke('hotkeys:pause', true); render(); }
function stopListeningKey() { if (!listening) return; listening = null; api.invoke('hotkeys:pause', false); render(); }
function wireKeys() {
  $('keys-global').onclick = (e) => { const b = e.target.closest('[data-key]'); if (!b) return; if (listening === b.dataset.key) stopListeningKey(); else startListeningKey(b.dataset.key); };
  $('hero-key').onclick = $('set-key').onclick = () => (listening === 'talk' ? stopListeningKey() : startListeningKey('talk'));
  addEventListener('keydown', async (e) => {
    if (!listening) return;
    e.preventDefault(); e.stopPropagation();
    if (e.key === 'Escape') { stopListeningKey(); return; }
    const id = listening;
    if (e.key === 'Backspace') { listening = null; await setKey(id, ''); api.invoke('hotkeys:pause', false); return; }
    const combo = comboOf(e);
    if (combo === null) return;
    if (combo === '') { toast(T`ต้องกด Ctrl, Alt หรือ Win ร่วมด้วย (หรือใช้ F1–F12)`, { kind: 'err' }); return; }
    listening = null;
    if (!(await setKey(id, combo))) listening = id;
    api.invoke('hotkeys:pause', !!listening);
    render();
  }, true);
  addEventListener('blur', stopListeningKey);
  $('keys-reset').onclick = async () => { listening = null; S = await api.invoke('settings:set', { hotkey: DEF_KEYS.talk, hotkeys: { open: DEF_KEYS.open, repeat: '' } }); await refreshKeyStatus(); render(); toast(T`คืนคีย์ลัดเริ่มต้นแล้ว`); };
}

/* -------------------------------------------------------------------- boot */
/* ------------------------------------------------- open with Windows (first run) */
// Asked once on the first open; "ถามใหม่ทีหลัง" / Esc / click outside = ask again next time. Settings has "ถามอีกครั้ง".
async function askStartup() {
  const yes = await htmlBox(`
    <div class="su-head"><div class="su-ic">${icon('power')}</div><h3>${T`ให้ Prism VoiceOpen เปิดพร้อม Windows ไหม?`}</h3></div>
    <p>${T`เปิดเครื่องปุ๊บ ก็กดคีย์แล้วพูดสั่งงานได้เลย ไม่ต้องหาโปรแกรมมาเปิดก่อน`}</p>
    <ul class="su-list">
      <li>${icon('check')}<span>${T`โปรแกรมจะซ่อนอยู่ที่ถาดข้างนาฬิกา พร้อมฟังคีย์พูดทันที`}</span></li>
      <li>${icon('check')}<span>${T`ไม่มีหน้าต่างเด้งขึ้นมาตอนเปิดเครื่อง — ดับเบิลคลิกไอคอนข้างนาฬิกาเพื่อเปิดหน้าต่างนี้`}</span></li>
      <li>${icon('check')}<span>${T`เปลี่ยนใจได้ทุกเมื่อ ที่ ตั้งค่า → เปิดพร้อม Windows`}</span></li>
    </ul>
    <div class="modal-btns su-btns"><button class="pill-btn" data-later>${T`ถามใหม่ทีหลัง`}</button><i class="su-grow"></i><button class="pill-btn" data-no>${T`ไม่ต้อง`}</button><button class="pill-btn primary" data-yes>${icon('power')}<span>${T`เปิดพร้อม Windows`}</span></button></div>`,
  (m, close) => {
    m.querySelector('[data-yes]').onclick = () => close(true);
    m.querySelector('[data-no]').onclick = () => close(false);
    m.querySelector('[data-later]').onclick = () => close(null);
    m.querySelector('.modal-card').classList.add('su-card');
    m.querySelector('[data-yes]').focus();
  });
  if (yes === null) return;
  await set({ autostart: yes, startup_asked: true });
  toast(yes ? T`เปิดพร้อม Windows แล้ว` : T`ไม่เปิดพร้อม Windows — เปลี่ยนได้ที่ตั้งค่า`);
}

async function boot() {
  translateDom(document.body);
  $('cmd-search').placeholder = T('ค้นหา');
  hydrateIcons();
  setLook(savedLook());
  setTheme(savedTheme());
  startBackground($('bg'));
  $('win-min').onclick = () => api.invoke('win:min');
  $('win-max').onclick = () => api.invoke('win:max');
  // X = hide to the tray; the talk key keeps working. Quit lives in the tray menu.
  $('win-close').onclick = () => { if (native) api.invoke('win:hide'); else window.close(); };
  $('btn-info').onclick = showInfo;
  const themeIcon = () => { $('btn-theme').firstElementChild.innerHTML = icon(theme.dark ? 'day' : 'moon'); };
  // appearance menu under the moon/sun: theme (light / dark) + style (glass / classic) — suite shell, same as Prism Size
  {
    const pop = $('theme-pop'), tbtn = $('btn-theme'), isOpen = () => !pop.classList.contains('hidden');
    const sync = () => {
      pop.querySelectorAll('[data-mode]').forEach((b) => b.classList.toggle('on', (b.dataset.mode === 'dark') === theme.dark));
      pop.querySelectorAll('[data-look]').forEach((b) => b.classList.toggle('on', (b.dataset.look === 'classic') === theme.classic));
    };
    const place = () => { const r = tbtn.getBoundingClientRect(), w = pop.offsetWidth; pop.style.left = Math.max(8, Math.min(r.right - w + 8, innerWidth - w - 8)) + 'px'; pop.style.top = r.bottom + 12 + 'px'; };
    const close = () => { pop.classList.add('hidden'); tbtn.classList.remove('open'); };
    tbtn.onclick = () => { if (isOpen()) { close(); return; } hideCtx(); sync(); pop.classList.remove('hidden'); tbtn.classList.add('open'); place(); };
    pop.querySelectorAll('[data-mode]').forEach((b) => { b.onclick = () => { const d = b.dataset.mode === 'dark'; setTheme(d); themeIcon(); api.invoke('settings:set', { dark: d }); sync(); }; });
    pop.querySelectorAll('[data-look]').forEach((b) => { b.onclick = () => { setLook(b.dataset.look === 'classic'); sync(); }; });
    addEventListener('pointerdown', (e) => { if (isOpen() && !pop.contains(e.target) && !tbtn.contains(e.target)) close(); }, true);
    addEventListener('keydown', (e) => { if (e.key === 'Escape' && isOpen()) { e.stopPropagation(); close(); } }, true);
    addEventListener('resize', () => { if (isOpen()) place(); }); // snap / maximise: follow the button
  }
  themeIcon();
  const ls = $('lang-switch');
  ls.dataset.lang = lang;
  ls.querySelectorAll('button').forEach((b) => {
    b.classList.toggle('on', b.dataset.lang === lang);
    b.onclick = async () => {
      if (b.dataset.lang === lang) return;
      ls.dataset.lang = b.dataset.lang;
      remember('prism.lang', b.dataset.lang);
      await api.invoke('settings:set', { lang: b.dataset.lang });
      setTimeout(() => { if (new URLSearchParams(location.search).has('lang')) location.search = ''; else location.reload(); }, 260);
    };
  });
  addEventListener('pointermove', (e) => {
    const g = e.target.closest && e.target.closest('.glass'); if (!g) return;
    const r = g.getBoundingClientRect();
    g.style.setProperty('--mx', e.clientX - r.left + 'px'); g.style.setProperty('--my', e.clientY - r.top + 'px');
  });
  addEventListener('pointerdown', (e) => { if (!e.target.closest('#ctx')) hideCtx(); }, true);
  addEventListener('contextmenu', (e) => { if (!e.target.closest('input, textarea, .cmd')) e.preventDefault(); });
  addEventListener('keydown', (e) => {
    if (listening) return;
    const modalOpen = !$('modal').classList.contains('hidden');
    if (e.key === 'F1') { e.preventDefault(); showView('keys'); }
    if (e.key === 'Escape') hideCtx();
    if (!modalOpen && e.ctrlKey && e.key.toLowerCase() === 'n') { e.preventDefault(); edit(null); }
    if (!modalOpen && e.ctrlKey && e.key.toLowerCase() === 'f') { e.preventDefault(); showView('home'); $('cmd-search').focus(); }
  });
  document.querySelectorAll('.dock-btn[data-view]').forEach((b) => { b.onclick = () => showView(b.dataset.view); });
  const talk = () => (phase === 'listen' ? api.invoke('listen:stop') : phase === 'idle' ? api.invoke('listen:start') : null);
  $('big-mic').onclick = $('btn-talk').onclick = talk;

  api.invoke('hotkeys:pause', false); // a reload (language switch) while picking a key must not leave the global keys off
  S = await api.invoke('settings:get');
  if (native && (S.lang !== lang || S.dark !== theme.dark)) S = await api.invoke('settings:set', { lang, dark: theme.dark });
  SYS = await api.invoke('sys:info');
  fillSelects(); wireSettings(); wireCmds(); wireHist(); wireKeys(); micTest.wire(); render();
  try { const v = localStorage.getItem('prism.view'); if (v && $('view-' + v)) showView(v); } catch { /* */ }
  HIST = (await api.invoke('history:list')) || [];
  if (HIST[0] && Date.now() - HIST[0].t < 3600e3) showLast(HIST[0]);

  api.on('voice:settings', (s) => { S = s; render(); });
  api.on('voice:state', ({ phase: p }) => { phase = p; if (p !== 'train') { trainingFor = null; } renderHero(); if (p === 'idle') setTimeout(refreshSys, 400); });
  api.on('voice:history', (h) => { HIST.unshift(h); if ($('view-history').classList.contains('active')) renderHist(); showLast(h); });
  api.on('voice:sample', (s) => { trainingFor = null; if (!s.ok) toast(T`ไม่ได้ยินเสียง — ลองพูดใหม่ ใกล้ไมค์ขึ้นอีกนิด`, { kind: 'err' }); render(); });
  api.on('voice:mictest', (m) => micTest.onEvent(m));
  api.on('voice:ran', ({ ok, total }) => { if (ok < total) toast(T`ทำไม่สำเร็จ — ตรวจว่าโปรแกรม / ไฟล์ / ลิงก์ ยังอยู่`, { kind: 'err' }); });
  api.on('voice:wake', () => refreshSys());
  refreshKeyStatus();
  addEventListener('focus', refreshSys);
  setTimeout(refreshSys, 5000); setTimeout(refreshSys, 12000); // the engine loads in the background after start
  api.invoke('app:ready');
  const testActs = String((await api.invoke('about:info')).test || '');
  if (native && !S.startup_asked && !S.autostart && !testActs) setTimeout(askStartup, 900);

  // self-test hook: PRISM_ACT=en,dark,info,view:settings,edit:<n>,new … (native items are handled in Rust)
  const acts = String((await api.invoke('about:info')).test || '').split(',').filter(Boolean);
  for (const a of acts) {
    if (a === 'en' || a === 'th') { if (lang !== a) { remember('prism.lang', a); await api.invoke('settings:set', { lang: a }); location.reload(); return; } }
    else if (a === 'dark' || a === 'light') { setTheme(a === 'dark'); themeIcon(); await api.invoke('settings:set', { dark: a === 'dark' }); }
    else if (a === 'themepop' || a === 'classic' || a === 'glass') { // full press sequence, so the close-on-outside-press listener runs too
      const press = (el) => { el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true })); el.dispatchEvent(new PointerEvent('pointerup', { bubbles: true })); el.click(); };
      if (a === 'themepop') press($('btn-theme'));
      else if (!$('theme-pop').classList.contains('hidden')) press(document.querySelector(`#theme-pop [data-look=${a}]`));
      else setLook(a === 'classic');
    }
    else if (a === 'info') showInfo();
    else if (a === 'updfake') window.__prismUpdFake = true;
    else if (a === 'startup') askStartup();
    else if (a.startsWith('view:')) showView(a.slice(5));
    else if (a === 'new') edit(null);
    else if (a.startsWith('edit:')) edit(S.commands[+a.slice(5)] || S.commands[0]);
    else if (a === 'picker') { edit(null).then(() => {}); setTimeout(() => document.querySelector('.kind[data-k="app"]').click(), 400); }
    else if (a === 'mictest') { showView('settings'); micTest.start(); }
    else if (a.startsWith('scroll:')) { await new Promise((r) => setTimeout(r, 600)); document.querySelector('.view.active').scrollTop = +a.slice(7); }
  }
}
boot();

/* >>> prism-update-nudge (tools/brand/apply-web.js) */
{
  // Checks GitHub quietly a few seconds after start (and every 6 h). New version → dot on Info + a chip that slides out
  // once per version; clicking either opens Info already showing "Update now".
  const btn = document.getElementById('btn-info'), SEEN = 'prism.updSeen';
  let found = null;
  const chipOut = (r) => {
    let seen = null; try { seen = localStorage.getItem(SEEN); } catch {}
    if (seen === r.latest || document.querySelector('.upd-chip')) return;
    const chip = document.createElement('button');
    chip.className = 'upd-chip';
    chip.innerHTML = `<i></i><span>${T`มีเวอร์ชันใหม่ v${esc(r.latest)}`}</span><u>${T`(ตอนนี้ v${esc(r.current)})`}</u><b>${T`อัปเดต`}</b>`;
    document.body.appendChild(chip);
    const place = () => { const b = btn.getBoundingClientRect(); chip.style.top = `${b.bottom + 10}px`; chip.style.right = `${Math.max(8, innerWidth - b.right - 6)}px`; };
    place(); addEventListener('resize', place);
    requestAnimationFrame(() => requestAnimationFrame(() => chip.classList.add('show')));
    const hide = () => { chip.classList.remove('show'); removeEventListener('resize', place); setTimeout(() => chip.remove(), 450); };
    chip.onclick = () => { hide(); btn.click(); };
    setTimeout(hide, 10000);
  };
  const check = async () => {
    let r; try { r = window.__prismUpdFake ? { ok: true, hasUpdate: true, latest: '9.9.9', current: '1.0.0' } : await api.invoke('update:check'); } catch { return; } // __prismUpdFake = test hook (PRISM_ACT updfake)
    if (!r || !r.ok || !r.hasUpdate) return;
    const fresh = !found || found.latest !== r.latest;
    found = r;
    btn.classList.add('has-update');
    if (fresh) btn.classList.remove('seen');
    btn.title = T`มีเวอร์ชันใหม่ v${r.latest} — กดเพื่ออัปเดต`;
    chipOut(r);
  };
  setTimeout(check, 4000);
  setInterval(check, 6 * 3600e3);
  btn.addEventListener('click', () => {
    if (!found) return;
    try { localStorage.setItem(SEEN, found.latest); } catch {}
    btn.classList.add('seen');
    document.querySelector('.upd-chip')?.classList.remove('show');
    const t0 = performance.now();
    const go = () => { const c = document.getElementById('about-check'); if (c && !c.disabled && c.isConnected) c.click(); else if (performance.now() - t0 < 2500) requestAnimationFrame(go); };
    requestAnimationFrame(go);
  });
}
/* <<< prism-update-nudge */
