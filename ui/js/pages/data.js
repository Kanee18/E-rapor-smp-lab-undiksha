// Data & arsip: backup ke flashdisk, pulihkan, semester baru, daftar arsip.
import { api } from '../api.js';
import { h, button, errorText, link, openModal, toast } from '../dom.js';
import { passwordField, showRecoveryCode } from '../lock.js';

/** "2026-11-14T02:30:00Z" → "14 Nov 2026, 10.30" (waktu laptop). */
export function fmtTime(iso) {
  if (!iso) return '—';
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleString('id-ID', { dateStyle: 'medium', timeStyle: 'short' });
}

/** Status backup luar: 'never' | 'stale' | 'ok'. */
export function backupStatus(meta) {
  if (!meta.last_external_backup_at) return 'never';
  if (meta.last_change_at && meta.last_change_at > meta.last_external_backup_at) return 'stale';
  return 'ok';
}

/** Jalankan backup ke flashdisk; mengembalikan true kalau berhasil. */
export async function runExternalBackup(app) {
  try {
    const path = await api.backupExternal();
    if (!path) return false;
    await app.reload();
    toast(`Backup tersimpan: ${path}`, 'ok', 7000);
    return true;
  } catch (e) {
    toast(errorText(e), 'error');
    return false;
  }
}

export async function render(ctx) {
  const { app, content, setTitle } = ctx;
  setTitle('Data & arsip', 'Backup ke flashdisk, memulihkan data, dan memulai semester baru.');
  const meta = app.data.meta;
  const rerender = async () => { await app.reload(); content.replaceChildren(); render(ctx); };
  const archives = await api.listArchives().catch(() => []);

  const status = backupStatus(meta);
  const statusNote = {
    never: h('p', { class: 'note warn' }, h('b', {}, 'Belum pernah di-backup ke luar laptop ini. '), 'Kalau laptop rusak atau hilang, semua data ikut hilang.'),
    stale: h('p', { class: 'note warn' }, h('b', {}, 'Ada perubahan sejak backup terakhir '), `(${fmtTime(meta.last_external_backup_at)}).`),
    ok: h('p', { class: 'note ok' }, `Semua perubahan sudah di-backup · terakhir ${fmtTime(meta.last_external_backup_at)}.`),
  }[status];

  const section = (title, lede, ...body) => h('section', { class: 'set card-set' }, h('h2', {}, title), lede ? h('p', { class: 'lede' }, lede) : null, ...body);

  content.append(
    section('Backup ke flashdisk',
      'Colokkan flashdisk, klik tombol di bawah, lalu pilih flashdisk-nya (atau folder lain, mis. Google Drive). Backup disimpan di folder "E-Raport backup". Biasakan setiap selesai input data.',
      statusNote,
      h('div', { class: 'row', style: { marginTop: '14px' } },
        button('Backup ke flashdisk…', { kind: 'primary', onClick: async () => { if (await runExternalBackup(app)) rerender(); } }),
        button('Buka folder data', { kind: 'quiet', onClick: () => api.openDataFolder().catch((e) => toast(errorText(e), 'error')) })),
      h('p', { class: 'hint', style: { marginTop: '10px' } },
        meta.last_external_backup_dir ? `Tujuan terakhir: ${meta.last_external_backup_dir}. ` : '',
        'Selain itu, salinan harian otomatis tetap disimpan di laptop ini (30 hari).')),

    securitySection(app, rerender, section),

    section('Pulihkan data',
      'Kembalikan data dari file backup: dari flashdisk, backup harian, atau arsip. Data saat ini disalin dulu, jadi langkah ini selalu bisa dibatalkan.',
      button('Pilih file backup…', { onClick: async () => {
        try {
          const path = await api.pickBackupFile();
          if (path) confirmRestore(app, path, rerender);
        } catch (e) { toast(errorText(e), 'error'); }
      } })),

    section('Semester atau tahun ajaran baru',
      'Data semester ini disimpan sebagai arsip. Pengaturan (mapel, indikator, proyek, predikat, wali kelas) dibawa; nilai, ATL, proyek siswa, narasi, dan kehadiran dikosongkan.',
      button('Mulai semester baru…', { onClick: () => newTermModal(app, rerender) })),

    section('Arsip', archives.length ? 'Arsip bisa dipulihkan, misalnya untuk mencetak ulang rapor semester lalu.' : null,
      archives.length
        ? h('div', { class: 'sheet scroll' }, h('table', { class: 'ledger' },
          h('thead', {}, h('tr', {}, ['Arsip', 'Tahun ajaran', 'Sub-judul', 'Siswa', 'Diarsipkan', ''].map((t, i) => h('th', { class: i === 3 ? 'r' : '' }, t)))),
          h('tbody', {}, archives.map((a) => h('tr', {},
            h('td', { class: 'name' }, a.archive_label || a.file),
            h('td', {}, a.academic_year),
            h('td', { class: 'muted' }, a.subtitle),
            h('td', { class: 'r tabular' }, a.students),
            h('td', { class: 'tabular' }, fmtTime(a.archived_at)),
            h('td', { class: 'r' }, link('Pulihkan', () => confirmRestore(app, a.path, rerender))))))))
        : h('p', { class: 'muted' }, 'Belum ada arsip.')),
  );
}

// ---------------------------------------------------------------- kata sandi aplikasi

const AUTO_LOCK = [[0, 'Tidak pernah'], [5, '5 menit'], [10, '10 menit'], [15, '15 menit'], [30, '30 menit'], [60, '1 jam']];

function securitySection(app, rerender, section) {
  const sec = app.security;
  const lede = 'Aplikasi meminta kata sandi setiap dibuka dan terkunci sendiri kalau laptop ditinggal. '
    + 'Ini mencegah orang lain membuka atau mengubah nilai lewat aplikasi di laptop ini. '
    + 'File data dan backup di flashdisk tidak ikut dienkripsi, jadi tetap simpan flashdisk di tempat aman.';
  if (!sec.enabled) {
    return section('Kata sandi aplikasi', lede,
      h('p', { class: 'note' }, 'Belum ada kata sandi: siapa pun yang memakai laptop ini bisa membuka aplikasi.'),
      h('div', { class: 'row', style: { marginTop: '14px' } },
        button('Pasang kata sandi…', { kind: 'primary', onClick: () => passwordModal(app, false, rerender) })));
  }
  const autoLock = h('select', { onchange: async (e) => {
    try { await api.securitySetAutoLock(Number(e.target.value)); await app.refreshSecurity(); toast('Kunci otomatis diperbarui', 'ok'); } catch (err) { toast(errorText(err), 'error'); }
  } }, AUTO_LOCK.map(([v, t]) => h('option', { value: v, selected: sec.auto_lock_minutes === v }, t)));
  return section('Kata sandi aplikasi', lede,
    h('p', { class: 'note ok' }, h('b', {}, 'Kata sandi aktif. '), 'Kunci kapan saja dengan Ctrl+L atau tombol "Kunci aplikasi" di kiri bawah.'),
    h('div', { class: 'sec-row' },
      h('label', { class: 'field' }, h('span', { class: 'lbl' }, 'Kunci otomatis setelah tidak dipakai'), autoLock),
      button('Kunci sekarang', { onClick: () => app.lock() }),
      button('Ganti kata sandi…', { onClick: () => passwordModal(app, true, rerender) }),
      button('Matikan kata sandi…', { kind: 'quiet danger', onClick: () => disableModal(app, rerender) })));
}

/** Pasang (enabled = false) atau ganti (enabled = true) kata sandi. */
function passwordModal(app, enabled, onDone) {
  const cur = enabled ? passwordField('Kata sandi sekarang') : null;
  const pw1 = passwordField('Kata sandi baru', { autocomplete: 'new-password' });
  const pw2 = passwordField('Ulangi kata sandi baru', { autocomplete: 'new-password' });
  const err = h('p', { class: 'lock-err', role: 'alert' });
  openModal({
    title: enabled ? 'Ganti kata sandi' : 'Pasang kata sandi',
    body: h('div', { class: 'stack' },
      cur?.el, pw1.el, pw2.el,
      h('p', { class: 'hint' }, 'Minimal 6 karakter. Gabungan kata yang mudah diingat tapi tidak mudah ditebak, mis. "rapor-kelas9-lab".'),
      err),
    actions: [
      { label: 'Batal', kind: 'quiet', onClick: (c) => c() },
      { label: enabled ? 'Ganti kata sandi' : 'Pasang kata sandi', kind: 'primary', onClick: async (close) => {
        err.textContent = '';
        if (pw1.input.value.length < 6) { err.textContent = 'Kata sandi minimal 6 karakter.'; pw1.input.focus(); return; }
        if (pw1.input.value !== pw2.input.value) { err.textContent = 'Kata sandi baru dan ulangannya tidak sama.'; pw2.input.focus(); return; }
        try {
          const code = await api.securitySetPassword(cur ? cur.input.value : null, pw1.input.value);
          close();
          if (code) await showRecoveryCode(code);
          await app.refreshSecurity();
          toast(enabled ? 'Kata sandi diganti' : 'Kata sandi dipasang. Aplikasi akan meminta kata sandi setiap dibuka.', 'ok', 6000);
          onDone?.();
        } catch (e) { err.textContent = errorText(e); }
      } },
    ],
  });
  setTimeout(() => (cur ?? pw1).input.focus(), 50);
}

function disableModal(app, onDone) {
  const cur = passwordField('Kata sandi sekarang');
  const err = h('p', { class: 'lock-err', role: 'alert' });
  openModal({
    title: 'Matikan kata sandi?',
    body: h('div', { class: 'stack' },
      h('p', { class: 'note warn' }, 'Setelah dimatikan, siapa pun yang memakai laptop ini bisa membuka aplikasi dan mengubah nilai.'),
      cur.el, err),
    actions: [
      { label: 'Batal', kind: 'quiet', onClick: (c) => c() },
      { label: 'Matikan', kind: 'danger solid', onClick: async (close) => {
        try { await api.securityDisable(cur.input.value); close(); await app.refreshSecurity(); toast('Kata sandi dimatikan', 'ok'); onDone?.(); } catch (e) { err.textContent = errorText(e); }
      } },
    ],
  });
  setTimeout(() => cur.input.focus(), 50);
}

async function confirmRestore(app, path, onDone) {
  let info;
  try { info = await api.inspectBackup(path); } catch (e) { toast(errorText(e), 'error'); return; }
  const current = app.data;
  const row = (k, a, b) => h('tr', {}, h('td', { class: 'muted' }, k), h('td', {}, a), h('td', {}, b));
  openModal({
    title: 'Pulihkan data?',
    wide: true,
    body: h('div', { class: 'stack' },
      h('p', { class: 'small muted tabular' }, info.path),
      h('div', { class: 'sheet' }, h('table', { class: 'ledger' },
        h('thead', {}, h('tr', {}, h('th', {}, ''), h('th', {}, 'Dari file ini'), h('th', {}, 'Data sekarang'))),
        h('tbody', {},
          info.archive_label ? row('Arsip', info.archive_label, '—') : null,
          row('Jenjang', info.level || '(data lama)', app.level),
          row('Sekolah', info.school_name || '—', current.settings.school_name),
          row('Siswa', `${info.students} (${info.classes.join(', ') || '—'})`, `${current.students.length}`),
          row('Tahun ajaran', info.academic_year, current.settings.academic_year),
          row('Sub-judul', info.subtitle, current.settings.report_subtitle),
          row('Terakhir diubah', fmtTime(info.last_change_at), fmtTime(current.meta.last_change_at))))),
      info.level && info.level !== app.level
        ? h('p', { class: 'note danger' }, h('b', {}, `Ini backup jenjang ${info.level}, `), `sedangkan yang sedang dibuka jenjang ${app.level}. Pindah dulu ke jenjang ${info.level} (tombol di kiri atas) sebelum memulihkan.`)
        : '',
      h('p', { class: 'note warn' }, 'Data sekarang akan diganti seluruhnya dengan isi file ini. Salinan data sekarang disimpan dulu di folder backup.')),
    actions: [
      { label: 'Batal', kind: 'quiet', onClick: (c) => c() },
      { label: 'Pulihkan', kind: 'danger solid', onClick: async (close) => {
        try {
          const safety = await api.restoreBackup(path);
          close();
          openModal({
            title: 'Data dipulihkan',
            body: h('div', { class: 'stack' },
              h('p', { class: 'note ok' }, `Data sekarang berisi ${info.students} siswa dari ${info.archive_label || info.file}.`),
              h('p', {}, 'Data sebelumnya disimpan di:'), h('p', { class: 'small tabular' }, safety),
              h('p', { class: 'muted' }, 'Untuk kembali ke data sebelumnya, pulihkan file itu.')),
          });
          onDone?.();
        } catch (e) { toast(errorText(e), 'error'); }
      } },
    ],
  });
}

function nextSubtitle(sub) {
  const m = sub.match(/^(.*TERM\s*)(\d+)(.*)$/i);
  return m ? `${m[1]}${Number(m[2]) + 1}${m[3]}` : sub;
}

function newTermModal(app, onDone) {
  const s = app.data.settings;
  const term = (s.report_subtitle.match(/TERM\s*\d+/i) || [''])[0];
  const opts = {
    archive_label: `${s.academic_year}${term ? ` ${term.replace(/term/i, 'Term')}` : ''}`.trim(),
    academic_year: s.academic_year,
    report_subtitle: nextSubtitle(s.report_subtitle),
    keep_students: true,
  };
  let printed = false;
  const field = (label, key, hint) => h('label', { class: 'field' }, h('span', { class: 'lbl' }, label),
    h('input', { value: opts[key], oninput: (e) => { opts[key] = e.target.value; } }), hint ? h('span', { class: 'hint' }, hint) : null);
  const radio = (label, desc, value) => h('label', { class: 'check', style: { alignItems: 'flex-start' } },
    h('input', { type: 'radio', name: 'term-kind', checked: opts.keep_students === value, onchange: () => { opts.keep_students = value; }, style: { marginTop: '3px' } }),
    h('span', {}, h('b', {}, label), h('br'), h('span', { class: 'small muted' }, desc)));

  openModal({
    title: 'Mulai semester baru',
    wide: true,
    body: h('div', { class: 'stack' },
      field('Nama arsip untuk data sekarang', 'archive_label', `${app.data.students.length} siswa beserta semua nilai dan isian akan disimpan dengan nama ini.`),
      h('div', { class: 'stack', style: { gap: '10px' } },
        radio('Ganti term', 'Daftar siswa tetap; hanya nilai dan isian yang dikosongkan.', true),
        radio('Tahun ajaran baru', 'Daftar siswa juga dikosongkan; impor ulang dari Dapodik.', false)),
      h('div', { class: 'grid-2' },
        field('Tahun ajaran', 'academic_year'),
        field('Sub-judul rapor', 'report_subtitle')),
      h('label', { class: 'check' }, h('input', { type: 'checkbox', onchange: (e) => { printed = e.target.checked; } }),
        'Rapor semester ini sudah dicetak atau disimpan sebagai PDF')),
    actions: [
      { label: 'Batal', kind: 'quiet', onClick: (c) => c() },
      { label: 'Arsipkan & mulai baru', kind: 'danger solid', onClick: async (close) => {
        if (!printed) { toast('Centang dulu bahwa rapor semester ini sudah dicetak/disimpan', 'error'); return; }
        if (!opts.archive_label.trim()) { toast('Nama arsip wajib diisi', 'error'); return; }
        try {
          const info = await api.startNewTerm(opts);
          close();
          toast(`Diarsipkan sebagai "${info.archive_label}". Data baru siap diisi.`, 'ok', 7000);
          onDone?.();
        } catch (e) { toast(errorText(e), 'error'); }
      } },
    ],
  });
}
