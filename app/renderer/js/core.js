// Shared shell pieces (same behaviour as Prism Size): native bridge, theme, bus, toast, menus, modals, floating panels.
import { icon } from './icons.js';
import { T, tr, lang } from './i18n.js';
export { T, tr, lang };

/* ------------------------------------------------------------ native bridge */
// Prism Size talks to Electron through window.prism.invoke/on. Prism Cam runs on Tauri: the same channel names
// ("win:min", "update:check" …) map to Rust commands ("win_min", "update_check" …). In a plain browser
// (development preview) the calls fall back to harmless stubs so the UI still works.
const TA = window.__TAURI__;
export const native = !!TA;
const stub = { 'about:info': { version: 'dev', repo: '', tauri: 'browser', vcam: false } };
export const api = {
  invoke: (ch, a) => (TA ? TA.core.invoke(ch.replace(/[:.]/g, '_'), a === undefined ? {} : { a }) : Promise.resolve(stub[ch] ?? { ok: false, reason: 'browser' })),
  raw: (cmd, bytes, headers) => (TA ? TA.core.invoke(cmd, bytes, { headers }) : Promise.resolve(null)),
  on: (ev, fn) => {
    if (!TA) return () => {};
    let off = null, dead = false;
    TA.event.listen(ev, (e) => fn(e.payload)).then((u) => { if (dead) u(); else off = u; });
    return () => { dead = true; if (off) off(); };
  },
};

const store = (k, d) => { try { const v = localStorage.getItem(k); return v === null ? d : v; } catch { return d; } };
export const recall = store;
export const remember = (k, v) => { try { localStorage.setItem(k, v); return true; } catch { return false; } };

/* -------------------------------------------------------------------- theme */
export const theme = { dark: false, classic: false, ink: '35,42,71', ink2: '74,83,118', paper: '255,255,255' };
export function setTheme(dark) {
  Object.assign(theme, dark
    ? { dark: true, ink: '236,240,255', ink2: '176,186,226', paper: '14,18,40' }
    : { dark: false, ink: '35,42,71', ink2: '74,83,118', paper: '255,255,255' });
  document.documentElement.dataset.theme = dark ? 'dark' : 'light';
  remember('prism.theme', dark ? 'dark' : 'light');
  bus.emit('theme', dark);
}
export const savedTheme = () => store('prism.theme', 'light') === 'dark';
// look: 'glass' (moving refractive background, frosted panels, spring motion) or 'classic' (flat, still, light on the PC)
export function setLook(classic) {
  theme.classic = classic;
  document.documentElement.dataset.look = classic ? 'classic' : 'glass';
  remember('prism.look', classic ? 'classic' : 'glass');
  bus.emit('look', classic);
}
export const savedLook = () => store('prism.look', 'glass') === 'classic';

/* ---------------------------------------------------------------------- bus */
const listeners = new Map();
export const bus = {
  on(ev, fn) { (listeners.get(ev) || listeners.set(ev, []).get(ev)).push(fn); },
  emit(ev, ...a) { for (const fn of listeners.get(ev) || []) { try { fn(...a); } catch (err) { console.error(ev, err); } } },
};

export const esc = (s) => String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
export const clamp = (v, a, b) => Math.max(a, Math.min(b, v));
export const $ = (id) => document.getElementById(id);

/* --------------------------------------------------------------- primitives */
export function toast(msg, opts = {}) {
  const el = document.createElement('div');
  el.className = 'toast glass' + (opts.kind === 'err' ? ' err' : '');
  el.innerHTML = `<i class="t-ico"></i><span class="t-msg">${esc(tr(msg))}</span>`;
  if (opts.action) {
    const b = document.createElement('button');
    b.className = 'pill-btn small'; b.textContent = opts.label || T`ตกลง`;
    b.onclick = () => { opts.action(); close(); };
    el.appendChild(b);
  }
  const close = () => { if (el.isConnected) { el.classList.add('out'); setTimeout(() => el.remove(), 400); } };
  $('toasts').appendChild(el);
  while ($('toasts').children.length > 4) $('toasts').firstChild.remove();
  setTimeout(close, opts.action ? 9000 : 4500);
}

export function ctxMenu(x, y, items, title) {
  const m = $('ctx');
  m.innerHTML = '';
  if (title) { const h = document.createElement('div'); h.className = 'ctx-head'; h.textContent = title; m.appendChild(h); }
  for (const it of items) {
    if (!it) continue;
    if (it === '-') { const s = document.createElement('div'); s.className = 'ctx-sep'; m.appendChild(s); continue; }
    const b = document.createElement('button');
    b.className = 'ctx-item' + (it.danger ? ' danger' : '');
    b.innerHTML = `${icon(it.icon)}<span>${esc(it.label)}</span>`;
    b.onclick = () => { hideCtx(); it.run(); };
    m.appendChild(b);
  }
  m.classList.remove('hidden');
  const r = m.getBoundingClientRect();
  m.style.left = clamp(x, 8, innerWidth - r.width - 8) + 'px';
  m.style.top = clamp(y, 8, innerHeight - r.height - 8) + 'px';
}
export function hideCtx() { $('ctx').classList.add('hidden'); }

function modal(html, onMount) {
  return new Promise((resolve) => {
    const m = $('modal');
    m.innerHTML = `<div class="modal-card glass">${html}</div>`;
    m.classList.remove('hidden');
    const close = (v) => { m.classList.add('hidden'); m.innerHTML = ''; m._esc = null; removeEventListener('keydown', onKey, true); resolve(v); };
    const onKey = (e) => { if (e.key === 'Escape') { e.stopPropagation(); if (m._esc) m._esc(e); else close(null); } }; // a dialog can take over Esc (m._esc)
    addEventListener('keydown', onKey, true);
    m.onpointerdown = (e) => { if (e.target === m) close(null); };
    onMount(m, close);
  });
}
export function htmlBox(html, onMount) { return modal(html, onMount); }
export function confirmBox(title, msg, okLabel = T`ยืนยัน`, danger = false) {
  return modal(
    `<h3>${esc(title)}</h3><p>${esc(msg)}</p><div class="modal-btns"><button class="pill-btn" data-x>${T`ยกเลิก`}</button><button class="pill-btn ${danger ? 'warn' : 'primary'}" data-ok>${esc(okLabel)}</button></div>`,
    (m, close) => {
      m.querySelector('[data-x]').onclick = () => close(false);
      m.querySelector('[data-ok]').onclick = () => close(true);
      m.querySelector('[data-ok]').focus();
    }).then(Boolean);
}
export function promptBox(title, value = '', okLabel = T`ตกลง`) {
  return modal(
    `<h3>${esc(title)}</h3><label class="field"><input type="text" spellcheck="false"></label><div class="modal-btns"><button class="pill-btn" data-x>${T`ยกเลิก`}</button><button class="pill-btn primary" data-ok>${esc(okLabel)}</button></div>`,
    (m, close) => {
      const inp = m.querySelector('input');
      inp.value = value; inp.focus(); inp.select();
      const ok = () => close(inp.value.trim() || null);
      inp.onkeydown = (e) => { if (e.key === 'Enter') ok(); };
      m.querySelector('[data-x]').onclick = () => close(null);
      m.querySelector('[data-ok]').onclick = ok;
    });
}

/* ----------------------------------------------- floating panels: inertia */
// free: panel keeps drifting after release and soft-bounces off window edges · elastic: springs back home
export function floaty(el, handle, { elastic = false } = {}) {
  let x = 0, y = 0, vx = 0, vy = 0, tilt = 0, dragging = false, raf = 0, lx = 0, ly = 0, lt = 0, base = null;
  const apply = () => { el.style.transform = `translate3d(${x.toFixed(2)}px,${y.toFixed(2)}px,0) rotate(${tilt.toFixed(3)}deg)`; };
  const bounds = () => { const r = el.getBoundingClientRect(); base = { l: r.left - x, t: r.top - y, w: r.width, h: r.height }; };
  const loop = () => {
    raf = 0;
    if (dragging) { tilt += (clamp(vx * 0.006, -5, 5) - tilt) * 0.2; apply(); raf = requestAnimationFrame(loop); return; }
    if (elastic) {
      vx += (-x * 0.085 - vx * 0.16); vy += (-y * 0.085 - vy * 0.16);
      x += vx * 0.016 * 9; y += vy * 0.016 * 9;
    } else {
      x += vx * 0.016; y += vy * 0.016;
      vx *= 0.945; vy *= 0.945;
      const m = 10;
      if (base.l + x < m) { x = m - base.l; vx = Math.abs(vx) * 0.55; }
      if (base.t + y < m) { y = m - base.t; vy = Math.abs(vy) * 0.55; }
      if (base.l + x + base.w > innerWidth - m) { x = innerWidth - m - base.w - base.l; vx = -Math.abs(vx) * 0.55; }
      if (base.t + y + base.h > innerHeight - m) { y = innerHeight - m - base.h - base.t; vy = -Math.abs(vy) * 0.55; }
    }
    tilt += (clamp(vx * 0.006, -5, 5) - tilt) * 0.14;
    apply();
    const moving = Math.abs(vx) + Math.abs(vy) > (elastic ? 0.02 : 4) || Math.abs(tilt) > 0.02 || (elastic && Math.abs(x) + Math.abs(y) > 0.3);
    if (moving) raf = requestAnimationFrame(loop);
    else { tilt = 0; if (elastic) { x = 0; y = 0; } apply(); }
  };
  handle.addEventListener('pointerdown', (e) => {
    if (e.button !== 0 || e.target.closest('button, input, select, a, textarea')) return;
    dragging = true; bounds();
    lx = e.clientX; ly = e.clientY; lt = performance.now(); vx = vy = 0;
    handle.setPointerCapture(e.pointerId);
    document.body.classList.add('dragging-floaty');
    handle.style.cursor = 'grabbing';
    if (!raf) raf = requestAnimationFrame(loop);
  });
  handle.addEventListener('pointermove', (e) => {
    if (!dragging) return;
    const now = performance.now(), dt = Math.max(1, now - lt) / 1000;
    const dx = e.clientX - lx, dy = e.clientY - ly;
    x += dx; y += dy;
    vx = vx * 0.6 + (dx / dt) * 0.4; vy = vy * 0.6 + (dy / dt) * 0.4;
    lx = e.clientX; ly = e.clientY; lt = now;
  });
  const end = () => {
    if (!dragging) return;
    dragging = false;
    document.body.classList.remove('dragging-floaty');
    handle.style.cursor = '';
    if (performance.now() - lt > 80) vx = vy = 0;
    if (elastic) { vx *= 0.02; vy *= 0.02; }
    if (!raf) raf = requestAnimationFrame(loop);
  };
  handle.addEventListener('pointerup', end);
  handle.addEventListener('pointercancel', end);
  addEventListener('resize', () => { if (!elastic && (x || y)) { bounds(); if (!raf) raf = requestAnimationFrame(loop); } });
}

/* ------------------------------------------------- drag a number to change it */
// Press on any number field and drag left / right: the value follows the mouse (Shift = ×10, Alt = ÷10).
// A plain click still places the caret for typing. While dragging the field fires "input" (with field._scrub set)
// and one "change" at the end, so callers can preview live and keep a single undo step.
// Fields: <input type="number">, or any input with data-scrub="<step>".
addEventListener('pointerdown', (e) => {
  const inp = e.target && e.target.closest && e.target.closest('input[type=number], input[data-scrub]');
  if (!inp || e.button !== 0 || inp.disabled || inp.readOnly || document.activeElement === inp) return;
  const v0 = parseFloat(inp.value);
  if (!Number.isFinite(v0)) return;
  e.preventDefault();
  const x0 = e.clientX, step = parseFloat(inp.dataset.scrub || inp.step) || 1;
  const dec = Math.min(4, (String(step).split('.')[1] || '').length), min = inp.min !== '' ? parseFloat(inp.min) : -Infinity, max = inp.max !== '' ? parseFloat(inp.max) : Infinity;
  let moved = false;
  const mv = (ev) => {
    const dx = ev.clientX - x0;
    if (!moved) { if (Math.abs(dx) < 3) return; moved = true; inp._scrub = true; document.body.classList.add('scrubbing'); }
    const k = (ev.shiftKey ? 10 : 1) * (ev.altKey ? 0.1 : 1);
    const v = clamp(v0 + Math.round(dx) * step * k, min, max);
    inp.value = (+v.toFixed(dec + (ev.altKey ? 1 : 0))).toString();
    inp.dispatchEvent(new Event('input', { bubbles: true }));
  };
  const up = () => {
    removeEventListener('pointermove', mv, true); removeEventListener('pointerup', up, true); removeEventListener('pointercancel', up, true);
    if (moved) { document.body.classList.remove('scrubbing'); inp.dispatchEvent(new Event('change', { bubbles: true })); inp._scrub = false; }
    else { inp.focus(); inp.select(); }
  };
  addEventListener('pointermove', mv, true); addEventListener('pointerup', up, true); addEventListener('pointercancel', up, true);
}, true);
