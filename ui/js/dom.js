// Helper kecil untuk membuat elemen dan komponen bersama.
// Semua teks dimasukkan sebagai text node (tidak lewat innerHTML), jadi isi
// dari Excel tidak bisa menyisipkan kode.

const PROPS = new Set(['value', 'checked', 'disabled', 'selected', 'readOnly', 'placeholder', 'type', 'rows', 'min', 'max', 'step', 'title', 'htmlFor', 'id', 'name', 'tabIndex', 'colSpan', 'rowSpan', 'indeterminate']);

export function h(tag, props = {}, ...children) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(props || {})) {
    if (v === undefined || v === null || v === false) continue;
    if (k === 'class') el.className = v;
    else if (k === 'style' && typeof v === 'object') {
      for (const [sk, sv] of Object.entries(v)) {
        if (sk.startsWith('--')) el.style.setProperty(sk, sv); else el.style[sk] = sv;
      }
    }
    else if (k === 'dataset') Object.assign(el.dataset, v);
    else if (k.startsWith('on') && typeof v === 'function') el.addEventListener(k.slice(2).toLowerCase(), v);
    else if (PROPS.has(k)) el[k] = v;
    else el.setAttribute(k, v === true ? '' : String(v));
  }
  append(el, children);
  return el;
}

function append(el, children) {
  for (const c of children) {
    if (c === null || c === undefined || c === false) continue;
    if (Array.isArray(c)) append(el, c);
    else if (c instanceof Node) el.appendChild(c);
    else el.appendChild(document.createTextNode(String(c)));
  }
}

export function clear(el) {
  while (el.firstChild) el.removeChild(el.firstChild);
  return el;
}

// ---------------------------------------------------------------- ikon (SVG statis, dipakai seperlunya)

const ICONS = {
  up: '<path d="m6 15 6-6 6 6"/>',
  down: '<path d="m6 9 6 6 6-6"/>',
  trash: '<path d="M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  arrow: '<path d="M5 12h14M13 6l6 6-6 6"/>',
  home: '<rect x="3.5" y="3.5" width="7" height="7" rx="2"/><rect x="13.5" y="3.5" width="7" height="7" rx="2"/><rect x="3.5" y="13.5" width="7" height="7" rx="2"/><rect x="13.5" y="13.5" width="7" height="7" rx="2"/>',
  users: '<circle cx="9.5" cy="8" r="3.5"/><path d="M3.5 20v-1a4.5 4.5 0 0 1 4.5-4.5h3a4.5 4.5 0 0 1 4.5 4.5v1"/><path d="M15.5 4.7a3.5 3.5 0 0 1 0 6.6M18 14.8a4.5 4.5 0 0 1 2.5 4.2v1"/>',
  chart: '<path d="M4 20h16"/><rect x="5" y="11" width="3.5" height="6" rx="1.2"/><rect x="10.25" y="5" width="3.5" height="12" rx="1.2"/><rect x="15.5" y="8.5" width="3.5" height="8.5" rx="1.2"/>',
  pen: '<path d="M4 20h4.2L19 9.2a2.9 2.9 0 0 0-4.2-4.2L4 15.8z"/><path d="m13.5 6.5 4 4"/>',
  printer: '<path d="M7 9V4h10v5"/><rect x="3.5" y="9" width="17" height="8" rx="2.2"/><path d="M7 14h10v6H7z"/>',
  sliders: '<path d="M4 7h9M17 7h3M4 17h3M11 17h9"/><circle cx="15" cy="7" r="2"/><circle cx="9" cy="17" r="2"/>',
  database: '<ellipse cx="12" cy="6" rx="7.5" ry="2.5"/><path d="M4.5 6v6c0 1.4 3.4 2.5 7.5 2.5s7.5-1.1 7.5-2.5V6"/><path d="M4.5 12v6c0 1.4 3.4 2.5 7.5 2.5s7.5-1.1 7.5-2.5v-6"/>',
  calendar: '<rect x="3.5" y="5" width="17" height="15.5" rx="2.5"/><path d="M3.5 10h17M8 3v4M16 3v4"/>',
  alert: '<path d="M10.3 4.4 2.7 17.6a2 2 0 0 0 1.7 2.9h15.2a2 2 0 0 0 1.7-2.9L13.7 4.4a2 2 0 0 0-3.4 0z"/><path d="M12 9.5v4M12 17h.01"/>',
  info: '<circle cx="12" cy="12" r="8.5"/><path d="M12 11v5M12 8h.01"/>',
  check: '<path d="m5 12.5 4.5 4.5L19 7.5"/>',
  lock: '<rect x="4.5" y="10.5" width="15" height="10" rx="2.5"/><path d="M8 10.5V7.5a4 4 0 0 1 8 0v3"/>',
  eye: '<path d="M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12z"/><circle cx="12" cy="12" r="3"/>',
  eyeOff: '<path d="M4 4l16 16M10.6 6.1A9.8 9.8 0 0 1 12 6c6 0 9.5 6 9.5 6a17 17 0 0 1-2.7 3.4M6.5 7.6A16.6 16.6 0 0 0 2.5 12S6 18 12 18a9.5 9.5 0 0 0 4.4-1.1"/><path d="M9.9 9.9a3 3 0 0 0 4.2 4.2"/>',
};

export function icon(name) {
  const span = document.createElement('span');
  // Isi SVG berasal dari daftar statis di atas, bukan dari data pengguna.
  span.innerHTML = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${ICONS[name] || ''}</svg>`;
  return span.firstChild;
}

/** Ikon di dalam kotak berwarna lembut (warna dari `--c`). */
export function tile(name, color, cls = '') {
  return h('span', { class: `tile ${cls}`.trim(), style: color ? { '--c': color } : null, 'aria-hidden': 'true' }, icon(name));
}

/** Lingkaran inisial siswa (netral; biru hanya untuk siswa yang sedang dibuka). */
export function avatar(name, cls = '') {
  const words = String(name || '?').trim().split(/\s+/).filter(Boolean);
  const initials = ((words[0]?.[0] || '?') + (words.length > 1 ? words[words.length - 1][0] : '')).toUpperCase();
  return h('span', { class: `avatar ${cls}`.trim(), 'aria-hidden': 'true' }, initials);
}

/** Cincin kemajuan (0–1) dengan teks di tengah. */
export function ring(pct, { size = 120, stroke = 10, label = null, sub = null, cls = '' } = {}) {
  const NS = 'http://www.w3.org/2000/svg';
  const r = (size - stroke) / 2;
  const len = 2 * Math.PI * r;
  const svg = document.createElementNS(NS, 'svg');
  svg.setAttribute('viewBox', `0 0 ${size} ${size}`);
  svg.setAttribute('aria-hidden', 'true');
  const circle = (c) => {
    const e = document.createElementNS(NS, 'circle');
    e.setAttribute('cx', size / 2); e.setAttribute('cy', size / 2); e.setAttribute('r', r);
    e.setAttribute('fill', 'none'); e.setAttribute('stroke-width', stroke); e.setAttribute('class', c);
    return e;
  };
  const val = circle('val');
  val.setAttribute('stroke-linecap', 'round');
  val.setAttribute('transform', `rotate(-90 ${size / 2} ${size / 2})`);
  val.style.strokeDasharray = `${len}px`;
  val.style.strokeDashoffset = `${len}px`;
  if (pct <= 0) val.style.opacity = '0';
  svg.append(circle('track'), val);
  requestAnimationFrame(() => requestAnimationFrame(() => { val.style.strokeDashoffset = `${len * (1 - Math.min(1, pct))}px`; }));
  return h('div', { class: `ring ${cls}`.trim(), style: { width: `${size}px`, height: `${size}px` } }, svg,
    label !== null ? h('div', { class: 'ring-label' }, h('b', {}, label), sub ? h('span', {}, sub) : null) : null);
}

// ---------------------------------------------------------------- animasi
// Aturannya satu: pindah halaman → seluruh isi muncul pelan sekali (enterAnim, dari router).
// Isi berganti di halaman yang sama (kelas, siswa) → hanya area data, singkat (swapIn).
// Bilah alat, tab, dan tombol tidak pernah ikut berkedip.
const calm = () => window.matchMedia('(prefers-reduced-motion: reduce)').matches;

export function enterAnim(el) {
  if (!el || calm()) return;
  el.animate([{ opacity: 0, transform: 'translateY(6px)' }, { opacity: 1, transform: 'none' }],
    { duration: 280, easing: 'cubic-bezier(.2, .7, .2, 1)' });
}

export function swapIn(el) {
  if (!el || calm()) return;
  el.animate([{ opacity: 0.2 }, { opacity: 1 }], { duration: 180, easing: 'ease-out' });
}

/** Jenjang dari nama kelas: "I-C" → SD, "IX-1" → SMP, "X-B" → SMA (null kalau tidak jelas). */
export function levelOfClass(name) {
  const left = String(name || '').split('-')[0].trim().toUpperCase();
  const ROMAN = { I: 1, II: 2, III: 3, IV: 4, V: 5, VI: 6, VII: 7, VIII: 8, IX: 9, X: 10, XI: 11, XII: 12 };
  const g = ROMAN[left] ?? (/^\d{1,2}$/.test(left) ? Number(left) : null);
  if (!g) return null;
  return g <= 6 ? 'SD' : g <= 9 ? 'SMP' : g <= 12 ? 'SMA' : null;
}

/** Catatan peringatan kalau kelas yang diimpor tampaknya milik jenjang lain. */
export function levelWarning(classNames, currentLevel) {
  if (!currentLevel) return '';
  const other = classNames.filter((c) => { const l = levelOfClass(c); return l && l !== currentLevel; });
  if (!other.length) return '';
  const lv = levelOfClass(other[0]);
  return h('div', { class: 'note warn small' },
    h('b', {}, `Kelas ${other.slice(0, 4).join(', ')}${other.length > 4 ? '…' : ''} tampaknya kelas ${lv}, `),
    `sedangkan yang sedang dibuka jenjang ${currentLevel}. Kalau benar kelas ${lv}, batalkan dulu lalu pindah ke jenjang ${lv} lewat tombol jenjang di kiri atas.`);
}

export function button(label, { kind = '', iconName, onClick, title, disabled } = {}) {
  return h('button', { type: 'button', class: `btn ${kind}`.trim(), onclick: onClick, title, disabled }, iconName ? icon(iconName) : null, label);
}

/**
 * Tombol dengan daftar pilihan kecil (menu tarik-turun).
 * items: [{ label, onClick, kind?: 'danger', hint? }]
 */
export function menuButton(label, items, { kind = '', iconName } = {}) {
  const wrap = h('div', { class: 'menu-wrap' });
  const list = h('div', { class: 'menu-list', role: 'menu', hidden: true });
  const close = () => { list.hidden = true; trigger.setAttribute('aria-expanded', 'false'); document.removeEventListener('pointerdown', outside, true); };
  const outside = (e) => { if (!wrap.contains(e.target)) close(); };
  const trigger = h('button', {
    type: 'button', class: `btn ${kind}`.trim(), 'aria-haspopup': 'menu', 'aria-expanded': 'false',
    onclick: () => {
      if (!list.hidden) { close(); return; }
      list.hidden = false;
      trigger.setAttribute('aria-expanded', 'true');
      document.addEventListener('pointerdown', outside, true);
      list.querySelector('button')?.focus();
    },
  }, iconName ? icon(iconName) : null, label, icon('down'));
  list.append(...items.map((it) => h('button', {
    type: 'button', role: 'menuitem', class: `menu-item ${it.kind || ''}`.trim(),
    onclick: () => { close(); it.onClick(); },
  }, h('span', {}, it.label), it.hint ? h('span', { class: 'menu-hint' }, it.hint) : null)));
  wrap.addEventListener('keydown', (e) => { if (e.key === 'Escape' && !list.hidden) { e.stopPropagation(); close(); trigger.focus(); } });
  wrap.append(trigger, list);
  return wrap;
}

export function link(label, onClick, kind = '') {
  return h('button', { type: 'button', class: `link ${kind}`.trim(), onclick: onClick }, label);
}

// ---------------------------------------------------------------- toast & modal

export function toast(message, kind = 'info', ms = 3500) {
  const box = document.getElementById('toasts');
  const el = h('div', { class: `toast ${kind}`, role: kind === 'error' ? 'alert' : 'status' }, message);
  box.appendChild(el);
  // Popover = lapisan paling atas: pesan tetap terbaca di atas jendela dialog (showModal) yang sedang terbuka.
  if (box.showPopover) {
    try { if (box.matches(':popover-open')) box.hidePopover(); box.showPopover(); } catch { /* tetap tampil sebagai elemen biasa */ }
  }
  setTimeout(() => {
    el.remove();
    if (!box.childElementCount && box.hidePopover) { try { box.hidePopover(); } catch { /* sudah tertutup */ } }
  }, kind === 'error' ? Math.max(ms, 7000) : ms);
}

export function errorText(e) {
  if (typeof e === 'string') return e;
  return e?.message || JSON.stringify(e);
}

/** Tampilkan modal. `actions`: [{label, kind, onClick(close) }]. */
export function openModal({ title, body, actions = [], wide = false, onClose }) {
  const dlg = h('dialog', { class: `modal${wide ? ' wide' : ''}` });
  const close = () => {
    dlg.close();
    dlg.remove();
    onClose?.();
  };
  const foot = h('div', { class: 'modal-foot' });
  const setActions = (list) => {
    clear(foot);
    for (const a of list) {
      foot.appendChild(button(a.label, { kind: a.kind, onClick: () => a.onClick?.(close), disabled: a.disabled }));
    }
  };
  setActions(actions.length ? actions : [{ label: 'Tutup', onClick: (c) => c() }]);
  const bodyEl = h('div', { class: 'modal-body' }, body);
  dlg.append(h('div', { class: 'modal-head' }, h('h2', {}, title)), bodyEl, foot);
  dlg.addEventListener('cancel', (ev) => { ev.preventDefault(); close(); });
  document.body.appendChild(dlg);
  dlg.showModal();
  return { close, dlg, bodyEl, setActions, setBody: (node) => { clear(bodyEl); append(bodyEl, [node]); } };
}

export function confirmBox(message, { title = 'Konfirmasi', ok = 'Ya, lanjutkan', danger = false } = {}) {
  return new Promise((resolve) => {
    let answered = false;
    openModal({
      title,
      body: h('p', {}, message),
      onClose: () => { if (!answered) resolve(false); },
      actions: [
        { label: 'Batal', kind: 'quiet', onClick: (c) => { answered = true; resolve(false); c(); } },
        { label: ok, kind: danger ? 'danger solid' : 'primary', onClick: (c) => { answered = true; resolve(true); c(); } },
      ],
    });
  });
}

// ---------------------------------------------------------------- angka & predikat

const numberFmt = new Intl.NumberFormat('id-ID', { maximumFractionDigits: 2 });
export const fmtNum = (v) => (v === null || v === undefined ? '' : numberFmt.format(v));

/** "85,5" / "85.5" → 85.5; "" → null; teks lain → NaN. */
export function parseNum(text) {
  const t = String(text ?? '').trim();
  if (!t) return null;
  const v = Number(t.replace(',', '.'));
  return Number.isFinite(v) ? v : NaN;
}

const sortedBands = (grading) => [...grading.bands].sort((a, b) => b.min - a.min);

/** Peringkat predikat (0 = tertinggi) untuk pewarnaan. */
export function rankMap(settings) {
  return Object.fromEntries(sortedBands(settings.grading).map((b, i) => [b.code, i]));
}

/**
 * Kalau `raw` cocok dengan kode predikat (mis. "me" → "ME") atau awalan huruf
 * unik dari satu predikat (mis. "m" → "ME", "e" → "EE"), kembalikan kode resminya.
 * Kalau angka atau teks lain, kembalikan `raw` yang sudah di-trim.
 */
export function expandGradeCode(raw, grading) {
  const t = String(raw ?? '').trim();
  if (!t) return '';
  const bands = sortedBands(grading);
  const upper = t.toUpperCase();
  const exact = bands.find((b) => b.code.toUpperCase() === upper);
  if (exact) return exact.code;
  if (Number.isNaN(parseNum(t))) {
    const matches = bands.filter((b) => b.code.toUpperCase().startsWith(upper));
    if (matches.length === 1) return matches[0].code;
  }
  return t;
}

/**
 * Konversi cepat di tampilan (untuk menyorot tombol saat mengetik angka).
 * Aturannya sama dengan grading.rs; hasil resmi tetap dari Rust.
 */
export function localGrade(raw, grading) {
  const t = String(raw ?? '').trim();
  if (!t) return { status: 'empty' };
  const bands = sortedBands(grading);
  const byCode = bands.find((b) => b.code.toUpperCase() === t.toUpperCase());
  if (byCode) return { status: 'ok', code: byCode.code, fromNumber: false };
  let n = parseNum(t);
  if (n === null || Number.isNaN(n) || n < 0 || n > 100) return { status: 'invalid', fromNumber: true };
  if (grading.round_final) n = Math.floor(Math.round(n * 1e6) / 1e6 + 0.5);
  const hit = bands.find((b) => n + 1e-9 >= b.min);
  if (hit) return { status: 'ok', code: hit.code, score: n, fromNumber: true };
  return { status: 'below_scale', code: bands[bands.length - 1]?.code, score: n, fromNumber: true };
}

/** Stempel predikat dari hasil `Resolved` (Rust) atau `localGrade`. */
export function stamp(result, ranks) {
  if (!result || result.status === 'empty') return h('span', { class: 'stamp none', title: 'Belum diisi' }, '—');
  if (result.status === 'invalid') return h('span', { class: 'stamp invalid', title: result.message || 'Isian tidak terbaca' }, '?');
  if (result.status === 'incomplete') return h('span', { class: 'stamp none', title: result.message }, '…');
  const r = ranks?.[result.code] ?? 5;
  const below = result.status === 'below_scale';
  return h('span', { class: `stamp r${r}${below ? ' below' : ''}`, title: result.message || result.label || '' }, result.code);
}

/**
 * Kontrol penilaian: tombol EE/ME/AE/BE (klik) atau angka 0–100 (ketik).
 * `onChange(nilai)` menerima kode ("ME"), angka sebagai teks ("92"), atau "".
 */
export function ratingControl(value, grading, ranks, onChange, { label } = {}) {
  const bands = sortedBands(grading);
  let current = String(value ?? '').trim();
  const wrap = h('div', { class: 'rating', role: 'group', 'aria-label': label || 'Penilaian' });
  const segs = bands.map((b) => h('button', {
    type: 'button', class: `seg r${ranks[b.code] ?? 5}`, title: `${b.label} (${b.min}–${b.max})`,
    onclick: () => set(current.toUpperCase() === b.code.toUpperCase() ? '' : b.code),
  }, b.code));
  const num = h('input', {
    class: 'rating-num', placeholder: 'angka', inputmode: 'decimal', title: 'Atau ketik nilai 0–100',
    oninput: (e) => { current = e.target.value.trim(); paint(); onChange(current); },
  });
  const initial = localGrade(current, grading);
  if (initial.fromNumber) num.value = current;

  function set(v) {
    current = v;
    num.value = '';
    num.classList.remove('invalid');
    paint();
    onChange(v);
  }
  function paint() {
    const g = localGrade(current, grading);
    segs.forEach((s, i) => {
      const code = bands[i].code;
      s.classList.toggle('on', g.status === 'ok' && !g.fromNumber && g.code === code);
      s.classList.toggle('derived', Boolean(g.fromNumber) && g.code === code);
      s.setAttribute('aria-pressed', String(g.code === code && g.status !== 'invalid'));
    });
    num.classList.toggle('invalid', g.status === 'invalid');
    wrap.classList.toggle('below', g.status === 'below_scale');
    wrap.title = g.status === 'below_scale' ? `Nilai ${g.score} di bawah rentang ${g.code}` : '';
  }
  wrap.append(h('div', { class: 'segs' }, segs), num);
  paint();
  return wrap;
}

// ---------------------------------------------------------------- lain-lain

export function debounce(fn, ms) {
  let t;
  const wrapped = (...args) => {
    clearTimeout(t);
    t = setTimeout(() => fn(...args), ms);
  };
  wrapped.cancel = () => clearTimeout(t);
  return wrapped;
}

/** Tab kelas bergaris bawah. */
export function classTabs(classes, active, onPick, { all = true, counts = {} } = {}) {
  const wrap = h('div', { class: 'tabs', role: 'tablist' });
  const items = all ? [{ name: null, label: 'Semua' }, ...classes.map((c) => ({ name: c, label: c }))] : classes.map((c) => ({ name: c, label: c }));
  for (const it of items) {
    const n = it.name === null ? Object.values(counts).reduce((a, b) => a + b, 0) : counts[it.name];
    wrap.appendChild(h('button', {
      type: 'button', role: 'tab', class: `tab${it.name === active ? ' active' : ''}`, 'aria-selected': String(it.name === active),
      onclick: () => onPick(it.name),
    }, it.label, n !== undefined ? h('span', { class: 'count' }, n) : null));
  }
  return wrap;
}

/** Textarea yang tingginya mengikuti isi (tanpa scrollbar kecil). */
export function autoGrow(ta) {
  let tries = 0;
  const fit = () => {
    if (!ta.isConnected) { if (tries++ < 30) requestAnimationFrame(fit); return; }
    ta.style.height = 'auto';
    ta.style.height = `${ta.scrollHeight + 2}px`;
  };
  ta.classList.add('grow');
  ta.addEventListener('input', fit);
  requestAnimationFrame(fit);
  return ta;
}

export function toolbar(left, ...tools) {
  return h('div', { class: 'toolbar' }, left, tools.length ? h('div', { class: 'tools' }, tools) : null);
}

export function searchBox(placeholder, onInput) {
  return h('input', { type: 'search', class: 'search', placeholder, oninput: (e) => onInput(e.target.value.trim().toLowerCase()) });
}

export function emptyState(title, text, action, iconName = 'users') {
  return h('div', { class: 'sheet empty' }, tile(iconName, null, 'lg'), h('h3', {}, title), h('p', {}, text), action || null);
}

/**
 * Kecilkan tampilan rapor di iframe supaya selebar area pratinjau.
 * Memakai <style media="screen">, jadi hasil cetak tetap ukuran asli (100%).
 */
export function fitPreview(frame) {
  let baseWidth = 0;
  const apply = () => {
    const doc = frame.contentDocument;
    const page = doc?.querySelector('.report');
    if (!page) return;
    let style = doc.getElementById('fit-preview');
    if (!style) {
      baseWidth = page.offsetWidth;
      style = doc.createElement('style');
      style.id = 'fit-preview';
      style.media = 'screen';
      doc.head.appendChild(style);
    }
    if (!baseWidth) baseWidth = page.offsetWidth;
    if (!baseWidth || !frame.clientWidth) return;
    const scale = Math.min(1, (frame.clientWidth - 28) / baseWidth);
    style.textContent = `body { zoom: ${scale.toFixed(3)}; }
      ::-webkit-scrollbar { width: 12px; height: 12px; }
      ::-webkit-scrollbar-track, ::-webkit-scrollbar-corner { background: transparent; }
      ::-webkit-scrollbar-thumb { background: #C4C7CE; border-radius: 99px; border: 3px solid transparent; background-clip: padding-box; }
      ::-webkit-scrollbar-thumb:hover { background-color: #A5A9B2; }`;
  };
  frame.addEventListener('load', () => { baseWidth = 0; apply(); });
  const ro = new ResizeObserver(() => {
    if (!frame.isConnected) { ro.disconnect(); return; }
    apply();
  });
  ro.observe(frame);
}
