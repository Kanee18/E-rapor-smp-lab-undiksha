// Navigasi antar halaman + data global aplikasi.
import { api } from './api.js';
import { h, clear, enterAnim, icon, isMac, shortcut, tile, toast, errorText, openModal } from './dom.js';
import { STEP } from './steps.js';
import { showLockScreen } from './lock.js';
import { backupStatus } from './pages/data.js';
import * as beranda from './pages/beranda.js';
import * as siswa from './pages/siswa.js';
import * as nilai from './pages/nilai.js';
import * as isi from './pages/isi.js';
import * as cetak from './pages/cetak.js';
import * as pengaturan from './pages/pengaturan.js';
import * as dataPage from './pages/data.js';

// Menu: Ringkasan, lalu langkah kerja berurutan, lalu alat lain.
const page = (id, mod, extra = {}) => ({ id, mod, ...STEP[id], ...extra });
const PAGES = [
  page('beranda', beranda, { hero: true }),
  { group: 'Langkah kerja' },
  page('siswa', siswa),
  page('nilai', nilai),
  page('isi', isi),
  page('cetak', cetak),
  { group: 'Lainnya' },
  page('pengaturan', pengaturan),
  page('data', dataPage),
];

export const app = {
  data: null,
  dataDir: '',
  customTemplate: false,
  /** Kelas yang sedang dipilih; dipakai bersama oleh semua halaman. */
  selectedClass: null,
  selectedStudentId: null,

  /** Jenjang yang sedang dibuka ("SD" | "SMP" | "SMA") dan daftar semua jenjang. */
  level: 'SMP',
  levels: [],

  async reload() {
    const r = await api.getData();
    this.data = r.data;
    this.dataDir = r.data_dir;
    this.customTemplate = r.custom_template;
    this.level = r.level || 'SMP';
    this.levels = await api.listLevels().catch(() => []);
    if (r.warning) {
      openModal({ title: 'Perhatian', body: h('p', { class: 'note warn' }, r.warning) });
    }
    renderRail();
    return this.data;
  },

  /** Nama kelas yang ada (dari pengaturan + data siswa), urut alami. */
  classes() {
    const names = new Set(this.data.settings.classes.map((c) => c.name));
    for (const s of this.data.students) if (s.class_name) names.add(s.class_name);
    return [...names].sort(compareClass);
  },

  classCounts() {
    const counts = {};
    for (const s of this.data.students) counts[s.class_name] = (counts[s.class_name] || 0) + 1;
    return counts;
  },

  /** Pastikan `selectedClass` valid; default kelas pertama. */
  ensureClass() {
    const list = this.classes();
    if (!list.includes(this.selectedClass)) this.selectedClass = list[0] ?? null;
    return this.selectedClass;
  },

  go(pageId) {
    location.hash = pageId;
  },

  /** Status kunci aplikasi terakhir: { enabled, locked, auto_lock_minutes, problem }. */
  security: { enabled: false, locked: false, auto_lock_minutes: 0 },
  async refreshSecurity() {
    this.security = await api.securityStatus();
    renderRail();
    return this.security;
  },
  /** Kunci sekarang (isian yang belum tersimpan diamankan dulu). */
  lock: () => lockApp(),
  /** Pesan yang ditampilkan setelah aplikasi dibuka lagi. */
  notice: null,
};

const ROMAN = { I: 1, V: 5, X: 10, L: 50 };
function roman(s) {
  let total = 0, prev = 0;
  for (const ch of [...s.toUpperCase()].reverse()) {
    const v = ROMAN[ch];
    if (!v) return NaN;
    if (v < prev) total -= v; else { total += v; prev = v; }
  }
  return total;
}
function compareClass(a, b) {
  const [ga, na] = a.split('-');
  const [gb, nb] = b.split('-');
  const x = (roman(ga) || Number(ga) || 999) - (roman(gb) || Number(gb) || 999);
  if (x) return x;
  const y = (Number(na) || 999) - (Number(nb) || 999);
  return y || a.localeCompare(b);
}

// ---------------------------------------------------------------- rail

function renderNav(activeId) {
  const nav = clear(document.getElementById('nav'));
  for (const p of PAGES) {
    if (p.group) { nav.appendChild(h('div', { class: 'group', role: 'presentation' }, p.group)); continue; }
    nav.appendChild(h('a', { href: `#${p.id}`, class: p.id === activeId ? 'active' : '', 'aria-current': p.id === activeId ? 'page' : false },
      tile(p.icon), h('span', {}, p.label), p.n ? h('span', { class: 'n' }, Number(p.n)) : null));
  }
}

function renderLevels() {
  const box = document.getElementById('level-switch');
  box.replaceChildren(...app.levels.map((l) => h('button', {
    type: 'button', role: 'radio', 'aria-checked': String(l.active), class: `lv${l.active ? ' on' : ''}`,
    title: l.exists ? `${l.school} · ${l.students} siswa` : `${l.school} · belum ada data`,
    onclick: () => switchLevel(l),
  }, l.label)));
}

/** Pindah jenjang: simpan isian halaman ini dulu; jenjang baru boleh menyalin pengaturan. */
async function switchLevel(target) {
  if (target.active || locking) return;
  if (leaveHandler && (await leaveHandler()) === false) return;
  let copy = false;
  if (!target.exists) {
    copy = await new Promise((resolve) => {
      let picked = null;
      openModal({
        title: `Mulai jenjang ${target.label}`,
        body: h('div', { class: 'stack' },
          h('p', {}, `Jenjang ${target.label} belum punya data. Data ${target.label} disimpan terpisah dari ${app.level}: siswa, kop, kepala sekolah, mapel, indikator, proyek, wali kelas, tanda tangan, backup, dan arsip.`),
          h('p', { class: 'note small' }, `Salin pengaturan dari ${app.level} = predikat, mapel & indikator, ATL, proyek, bank kalimat, judul rapor, dan logo ikut disalin (bisa diubah nanti). Nama sekolah, kepala sekolah, tanda tangan, dan kelas tidak disalin.`)),
        onClose: () => resolve(picked),
        actions: [
          { label: 'Batal', kind: 'quiet', onClick: (c) => { picked = null; c(); } },
          { label: 'Pengaturan bawaan', onClick: (c) => { picked = false; c(); } },
          { label: `Salin pengaturan dari ${app.level}`, kind: 'primary', onClick: (c) => { picked = true; c(); } },
        ],
      });
    });
    if (copy === null) return;
  }
  try {
    await api.switchLevel(target.id, copy);
  } catch (e) {
    toast(errorText(e), 'error');
    return;
  }
  leaveHandler = null;
  currentId = null;
  app.selectedClass = null;
  app.selectedStudentId = null;
  await app.reload();
  toast(`Sekarang membuka data ${app.levels.find((l) => l.active)?.school || target.label}`, 'ok');
  if (location.hash === '#beranda' || !location.hash) route(); else location.hash = 'beranda';
}

function renderRail() {
  renderLevels();
  const s = app.data?.settings;
  if (!s) return;
  document.getElementById('rail-meta').textContent = 'National Plus Class';
  const status = backupStatus(app.data.meta);
  const backupText = { ok: 'Backup aman', stale: 'Ada perubahan belum di-backup', never: 'Belum pernah di-backup' }[status];
  clear(document.getElementById('rail-foot')).append(
    h('a', { class: 'rail-card', href: '#data', title: `Folder data: ${app.dataDir}` },
      h('div', { class: 'rc-year' }, icon('calendar'), h('b', {}, s.academic_year || '—')),
      h('div', { class: 'rc-sub' }, `${app.data.students.length} siswa · ${app.classes().length} kelas`),
      h('div', { class: `rc-backup ${status}` }, backupText)),
  );
  if (app.security.enabled) {
    document.getElementById('rail-foot').appendChild(
      h('button', { type: 'button', class: 'rail-lock', title: `Kunci aplikasi (${shortcut('L')})`, onclick: () => lockApp() },
        icon('lock'), h('span', {}, 'Kunci aplikasi'), h('kbd', {}, shortcut('L'))));
  }
}

// ---------------------------------------------------------------- router

let currentId = null;
let suppressNext = false;
/** Dipanggil sebelum pindah halaman / menutup jendela. Return false = batal. */
let leaveHandler = null;

function setTitle(title, desc = '') {
  document.getElementById('page-title').textContent = title;
  document.getElementById('page-desc').textContent = desc;
  document.title = `${title} — E-Raport ${app.level}`;
}

async function route() {
  if (suppressNext) { suppressNext = false; return; }
  const id = location.hash.slice(1) || 'beranda';
  const page = PAGES.find((p) => p.id === id) || PAGES[0];
  if (leaveHandler && currentId !== page.id) {
    const ok = await leaveHandler();
    if (ok === false) {
      suppressNext = true;
      location.hash = currentId;
      return;
    }
  }
  currentId = page.id;
  leaveHandler = null;
  renderNav(page.id);
  // penanda posisi langkah: ▬ ▬ ▭ ▭ Langkah 2 dari 4
  const stepEl = document.getElementById('page-step');
  stepEl.hidden = !page.n;
  if (page.n) {
    const k = Number(page.n);
    stepEl.replaceChildren(...[1, 2, 3, 4].map((i) => h('i', { class: i < k ? 'done' : i === k ? 'on' : '' })),
      h('span', {}, `Langkah ${k} dari 4`));
  }
  const mainEl = document.querySelector('main');
  mainEl.classList.toggle('hero-page', !!page.hero);
  mainEl.classList.remove('scrolled');
  const content = clear(contentEl);
  content.classList.toggle('fill', !!page.fill);
  content.scrollTop = 0;
  scrollHook = null;
  const actions = clear(document.getElementById('page-actions'));
  setTitle(page.label);
  // Halaman boleh mendaftarkan ulang handler setiap kali digambar ulang.
  const onLeave = (fn) => { if (currentId === page.id) leaveHandler = fn; };
  const onScroll = (fn) => { if (currentId === page.id) scrollHook = fn; };
  // Isi disembunyikan selama digambar (sebagian bisa muncul sebelum data selesai dimuat),
  // lalu muncul sekaligus dengan satu animasi. `entering` = false untuk gambar ulang di halaman.
  const token = ++routeToken;
  content.style.opacity = '0';
  const ctx = { app, content, actions, setTitle, onLeave, onScroll, entering: true };
  try {
    await page.mod.render(ctx);
  } catch (e) {
    console.error(e);
    content.appendChild(h('p', { class: 'note danger' }, `Terjadi kesalahan: ${errorText(e)}`));
  }
  ctx.entering = false;
  if (token === routeToken) {
    content.style.opacity = '';
    enterAnim(content);
  }
}
let routeToken = 0;

// Satu-satunya scroll halaman ada di #content; judul halaman tetap di atas dan
// mendapat garis tipis begitu isi mulai bergulir di bawahnya.
const contentEl = document.getElementById('content');
let scrollHook = null;
contentEl.addEventListener('scroll', () => {
  document.querySelector('main').classList.toggle('scrolled', contentEl.scrollTop > 2);
  scrollHook?.();
}, { passive: true });

window.addEventListener('hashchange', route);
window.addEventListener('unhandledrejection', (ev) => {
  toast(errorText(ev.reason), 'error');
});

// Simpan isian yang belum tersimpan sebelum jendela ditutup.
window.__TAURI__.window.getCurrentWindow().onCloseRequested(async (event) => {
  if (!leaveHandler) return;
  try {
    if ((await leaveHandler()) === false) event.preventDefault();
  } catch {
    /* tetap tutup */
  }
});

// ---------------------------------------------------------------- kunci aplikasi

const wait = (ms) => new Promise((r) => setTimeout(r, ms));
let locking = false;

/** Amankan isian, kunci di Rust, bersihkan tampilan, tunggu dibuka, lalu gambar ulang. */
async function lockApp() {
  if (locking || !app.security.enabled) return;
  locking = true;
  try {
    // isian yang sedang diketik (mis. sel nilai) tersimpan saat kehilangan fokus
    document.activeElement?.blur?.();
    await wait(250);
    if (leaveHandler) await Promise.race([leaveHandler({ force: true }), wait(3000)]).catch(() => {});
    await api.securityLock();
  } catch (e) {
    console.error(e);
  }
  // tidak ada data yang tertinggal di layar atau di memori tampilan
  document.querySelectorAll('dialog').forEach((d) => d.remove());
  clear(contentEl);
  clear(document.getElementById('page-actions'));
  clear(document.getElementById('rail-foot'));
  app.data = null;
  leaveHandler = null;
  currentId = null;
  await showLockScreen(await api.securityStatus().catch(() => ({})));
  locking = false;
  await openAfterUnlock();
}

async function openAfterUnlock() {
  await app.refreshSecurity();
  try {
    await app.reload();
  } catch (e) {
    contentEl.appendChild(h('p', { class: 'note danger' }, `Data tidak bisa dimuat: ${errorText(e)}`));
    return;
  }
  lastActive = Date.now();
  route();
  if (app.notice) { toast(app.notice, 'error', 9000); app.notice = null; }
  optimizeStoredLogos();
}

/** Perkecil logo lama di data.json yang ukurannya terlalu besar supaya halaman Cetak tidak berat. */
async function optimizeStoredLogos() {
  const s = app.data?.settings;
  if (!s) return;
  let changed = false;
  const copy = structuredClone(s);
  for (const key of ['logo', 'university_logo']) {
    const v = copy[key];
    if (typeof v === 'string' && v.startsWith('data:image/') && v.length > 25_000) {
      const smaller = await pengaturan.shrinkLogo(v, 220);
      if (smaller && smaller.length < v.length) {
        copy[key] = smaller;
        changed = true;
      }
    }
  }
  if (changed) {
    try {
      await api.saveSettings(copy);
      await app.reload();
    } catch {
      /* abaikan bila gagal */
    }
  }
}

// kunci otomatis kalau tidak ada aktivitas
let lastActive = Date.now();
for (const ev of ['pointerdown', 'pointermove', 'keydown', 'wheel']) {
  window.addEventListener(ev, () => { lastActive = Date.now(); }, { passive: true, capture: true });
}
const idleCheck = () => {
  const s = app.security;
  if (!s.enabled || s.locked || locking || !s.auto_lock_minutes) return;
  if (Date.now() - lastActive > s.auto_lock_minutes * 60_000) lockApp();
};
setInterval(idleCheck, 15_000);
document.addEventListener('visibilitychange', () => { if (!document.hidden) idleCheck(); });
window.addEventListener('keydown', (e) => {
  if ((isMac ? e.metaKey : e.ctrlKey) && !e.altKey && e.key.toLowerCase() === 'l' && app.security.enabled && !locking) {
    e.preventDefault();
    lockApp();
  }
});

(async function start() {
  try {
    app.security = await api.securityStatus();
  } catch (e) {
    contentEl.appendChild(h('p', { class: 'note danger' }, `Aplikasi tidak bisa dimulai: ${errorText(e)}`));
    return;
  }
  if (app.security.locked) {
    locking = true;
    await showLockScreen(app.security);
    locking = false;
  }
  await openAfterUnlock();
})();
