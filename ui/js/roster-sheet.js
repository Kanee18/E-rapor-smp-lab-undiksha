// Lembar kelas seperti Excel, langsung di halaman Data siswa (bukan pop-up):
// daftar siswa (nama, NIS, NISN, L/P) + catatan Sakit/Izin/Alpa, tersimpan otomatis.
// Urutan baris = nomor absen (urutan di semua halaman dan di rapor).
import { api } from './api.js';
import { h, button, confirmBox, debounce, errorText, icon, shortcut, toast } from './dom.js';

const COLS = ['name', 'nis', 'nisn', 'gender', 's', 'i', 'a'];
const DAY_COLS = ['s', 'i', 'a'];

const genderOf = (raw) => {
  const t = String(raw ?? '').trim().toUpperCase().replace(/[^A-Z]/g, '');
  if (['L', 'LK', 'LAKILAKI', 'LAKI', 'PRIA', 'M'].includes(t)) return 'L';
  if (['P', 'PR', 'PEREMPUAN', 'WANITA', 'F'].includes(t)) return 'P';
  return '';
};
/** "" → null, "3" → 3, "-" → 0, lainnya → undefined (tidak terbaca). */
const parseDays = (raw) => {
  const t = String(raw ?? '').trim();
  if (!t) return null;
  if (t === '-') return 0;
  return /^\d{1,3}$/.test(t) && Number(t) <= 366 ? Number(t) : undefined;
};
const blank = () => ({ id: null, name: '', nis: '', nisn: '', gender: '', s: '', i: '', a: '' });
const isEmpty = (r) => !r.id && !r.name.trim() && !r.nis.trim() && !r.nisn.trim() && !DAY_COLS.some((k) => String(r[k]).trim());
const badNisn = (v) => v.trim() !== '' && !/^\d{10}$/.test(v.trim());

function attendanceText(r, effDays) {
  const d = DAY_COLS.map((k) => parseDays(r[k]));
  if (d.some((x) => x === undefined) || d.every((x) => x === null)) return '';
  const total = effDays > 0 ? effDays : 100;
  const present = Math.max(0, total - d.reduce((n, x) => n + (x ?? 0), 0));
  return `${Math.round((present / total) * 100)}%`;
}

/**
 * @param {object} app
 * @param {string|null} className  kelas yang dibuka; null = kelas baru
 * @param {{ onClassName?: (name: string) => void }} [opts]
 * @returns {{ node: HTMLElement, status: HTMLElement, beforeClose: (opts?: {force?: boolean}) => Promise<boolean> }}
 */
export function rosterSheet(app, className, { onClassName } = {}) {
  const effDays = app.data.settings.effective_days || 100;
  let savedClass = className || null; // nama kelas yang sudah tersimpan di data
  let cls = className || '';
  const rows = savedClass
    ? app.data.students
      .filter((s) => s.class_name === savedClass)
      .sort((a, b) => (a.order ?? Infinity) - (b.order ?? Infinity) || a.name.localeCompare(b.name, 'id'))
      .map((s) => ({
        id: s.id, name: s.name, nis: s.nis, nisn: s.nisn, gender: s.gender,
        s: s.attendance_s ?? '', i: s.attendance_i ?? '', a: s.attendance_a ?? '',
      }))
    : [];
  rows.push(...Array.from({ length: savedClass ? 3 : 10 }, blank));

  // ---------- simpan otomatis
  const status = h('span', { class: 'save-state', 'aria-live': 'polite' });
  const setStatus = (kind, text) => { status.className = `save-state ${kind}`; status.textContent = text; };
  let dirty = false;
  let saving = null; // Promise yang sedang berjalan
  let problem = ''; // alasan belum bisa disimpan

  function validate() {
    if (!cls.trim()) return 'Isi nama kelas dulu';
    const keep = rows.filter((r) => !isEmpty(r));
    if (!keep.length) return 'Ketik minimal satu nama siswa';
    for (const [n, r] of keep.entries()) {
      if (!r.name.trim()) return `Baris ${n + 1}: nama siswa belum diisi`;
      if (badNisn(r.nisn)) return `Baris ${n + 1} (${r.name.trim()}): NISN harus 10 angka`;
      if (DAY_COLS.some((k) => parseDays(r[k]) === undefined)) return `Baris ${n + 1} (${r.name.trim()}): S/I/A harus angka 0–366`;
    }
    return '';
  }

  async function saveNow() {
    if (saving) { await saving; if (!dirty) return true; }
    if (!dirty) return true;
    problem = validate();
    if (problem) { setStatus('error', `Belum tersimpan: ${problem}`); return false; }
    const keep = rows.filter((r) => !isEmpty(r));
    const payload = keep.map((r) => ({
      id: r.id, name: r.name, nis: r.nis, nisn: r.nisn, gender: r.gender,
      attendance: { s: parseDays(r.s), i: parseDays(r.i), a: parseDays(r.a) },
    }));
    dirty = false;
    setStatus('saving', 'Menyimpan…');
    saving = (async () => {
      try {
        const res = await api.saveClassRoster(savedClass, cls, payload, []);
        // siswa baru mendapat ID → baris yang sama (objeknya) dipakai untuk simpanan berikutnya
        res.ids.forEach((id, k) => { keep[k].id = id; });
        if (savedClass !== res.class_name) {
          savedClass = res.class_name;
          onClassName?.(res.class_name);
        }
        if (cls !== res.class_name) { cls = res.class_name; classInput.value = cls; }
        problem = '';
        setStatus(dirty ? 'saving' : 'ok', dirty ? 'Belum tersimpan' : `Tersimpan · ${keep.length} siswa`);
        return true;
      } catch (e) {
        dirty = true;
        problem = errorText(e);
        setStatus('error', `Belum tersimpan: ${problem}`);
        return false;
      } finally {
        saving = null;
      }
    })();
    const ok = await saving;
    if (ok && dirty) saveSoon();
    return ok;
  }
  const saveSoon = debounce(saveNow, 700);
  const changed = () => { dirty = true; setStatus('saving', 'Belum tersimpan'); saveSoon(); };

  async function beforeClose({ force = false } = {}) {
    saveSoon.cancel();
    const ok = await saveNow();
    if (ok || force) return true;
    if (rows.every(isEmpty) && !savedClass) return true;
    return confirmBox(`Lembar ini belum tersimpan: ${problem}. Tutup tanpa menyimpan perubahan terakhir?`, { ok: 'Tutup', danger: true });
  }

  // ---------- tabel
  const tbody = h('tbody', {});
  const counter = h('span', { class: 'muted small' });

  const focusCell = (i, key) => requestAnimationFrame(() => {
    const el = tbody.querySelector(`[data-row="${i}"][data-key="${key}"]`);
    if (!el) return;
    el.focus();
    if (el.tagName === 'INPUT') el.select();
  });

  function pasteBlock(i, key, text) {
    const lines = text.replace(/\r/g, '').split('\n');
    while (lines.length && !lines[lines.length - 1].trim()) lines.pop();
    const start = COLS.indexOf(key);
    let r = i;
    for (const line of lines) {
      let cells = line.split('\t').map((c) => c.trim());
      // salinan yang ikut kolom "No" di depan (1, 2, 3, …) saat ditempel di kolom Nama
      if (key === 'name' && cells.length > 1 && /^\d{1,3}$/.test(cells[0]) && /[A-Za-z]/.test(cells[1])) cells = cells.slice(1);
      if (r >= rows.length) rows.push(blank());
      cells.forEach((v, k) => {
        const col = COLS[start + k];
        if (col) rows[r][col] = col === 'gender' ? genderOf(v) : v;
      });
      r++;
    }
    while (rows.length - r < 2) rows.push(blank());
    draw();
    changed();
    toast(`${lines.length} baris ditempel`, 'ok');
  }

  function moveRow(i, d) {
    const j = i + d;
    if (j < 0 || j >= rows.length) return;
    [rows[i], rows[j]] = [rows[j], rows[i]];
    draw();
    changed();
    focusCell(j, 'name');
  }

  async function removeRow(i) {
    const r = rows[i];
    if (r.id) {
      const ok = await confirmBox(`Hapus ${r.name || 'siswa ini'} dari kelas? Nilai dan isian rapornya ikut terhapus.`, { title: 'Hapus siswa', ok: 'Hapus', danger: true });
      if (!ok) return;
      saveSoon.cancel();
      if (!(await saveNow()) && dirty) { toast('Perbaiki isian yang belum tersimpan dulu', 'error'); return; }
      try { await api.deleteStudent(r.id); } catch (e) { toast(errorText(e), 'error'); return; }
      rows.splice(rows.indexOf(r), 1);
      toast(`${r.name} dihapus`, 'ok');
    } else {
      rows.splice(i, 1);
    }
    while (rows.length < 2 || !isEmpty(rows[rows.length - 1])) rows.push(blank());
    draw();
    changed();
  }

  function cell(r, i, key, extra = {}) {
    const isDay = DAY_COLS.includes(key);
    const input = h('input', {
      value: String(r[key] ?? ''),
      dataset: { row: String(i), key },
      spellcheck: false,
      autocomplete: 'off',
      ...extra,
      oninput: (e) => {
        r[key] = e.target.value;
        if (key === 'nisn') e.target.classList.toggle('invalid', badNisn(r.nisn));
        if (isDay) { e.target.classList.toggle('invalid', parseDays(r[key]) === undefined); paintAtt(); }
        // baris terakhir mulai diisi → sediakan baris kosong baru di bawahnya
        if (i >= rows.length - 2 && !isEmpty(r)) { rows.push(blank()); appendRow(rows.length - 1); }
        paintNo();
        changed();
      },
      onpaste: (e) => {
        const text = e.clipboardData?.getData('text/plain') ?? '';
        if (!/[\t\n]/.test(text.replace(/\r?\n$/, ''))) return; // satu sel biasa
        e.preventDefault();
        pasteBlock(i, key, text);
      },
      onkeydown: (e) => {
        if (e.key === 'Enter' || (e.key === 'ArrowDown' && !e.altKey)) {
          e.preventDefault();
          if (i === rows.length - 1) { rows.push(blank()); appendRow(rows.length - 1); }
          focusCell(i + 1, key);
        } else if (e.key === 'ArrowUp' && !e.altKey) { e.preventDefault(); focusCell(i - 1, key); }
        else if (e.key === 'ArrowDown' && e.altKey) { e.preventDefault(); moveRow(i, 1); }
        else if (e.key === 'ArrowUp' && e.altKey) { e.preventDefault(); moveRow(i, -1); }
        else if (isDay && (e.key === '+' || e.key === '-') && /^\d*$/.test(String(r[key]))) {
          // + / − menambah atau mengurangi satu hari, praktis saat mencatat absen harian
          e.preventDefault();
          const next = Math.max(0, Math.min(366, (Number(r[key]) || 0) + (e.key === '+' ? 1 : -1)));
          r[key] = next ? String(next) : (e.key === '-' ? '' : '0');
          e.target.value = r[key];
          e.target.dispatchEvent(new Event('input'));
        }
      },
    });
    if (key === 'nisn' && badNisn(r.nisn)) input.classList.add('invalid');
    if (isDay && parseDays(r[key]) === undefined) input.classList.add('invalid');
    return input;
  }

  // nomor & kehadiran digambar ulang tanpa membuat ulang input (fokus tetap)
  function paintNo() {
    let no = 0;
    tbody.querySelectorAll('tr').forEach((tr, i) => { tr.firstChild.textContent = isEmpty(rows[i]) ? '' : ++no; });
    counter.textContent = `${no} siswa`;
  }
  function paintAtt() {
    tbody.querySelectorAll('tr').forEach((tr, i) => {
      const t = attendanceText(rows[i], effDays);
      tr.querySelector('.att-out').textContent = t;
    });
  }

  function rowEl(r, i) {
    return h('tr', {},
      h('td', { class: 'idx' }, ''),
      h('td', { class: 'w-name' }, cell(r, i, 'name', { placeholder: i === 0 && !r.name ? 'Ketik nama siswa…' : '' })),
      h('td', { class: 'w-num' }, cell(r, i, 'nis', { inputmode: 'numeric' })),
      h('td', { class: 'w-nisn' }, cell(r, i, 'nisn', { inputmode: 'numeric', maxlength: 10, title: 'NISN 10 angka (boleh kosong)' })),
      h('td', { class: 'w-lp' }, h('select', {
        dataset: { row: String(i), key: 'gender' }, 'aria-label': 'Jenis kelamin',
        onchange: (e) => { r.gender = e.target.value; changed(); },
      }, [['', '—'], ['L', 'L'], ['P', 'P']].map(([v, t]) => h('option', { value: v, selected: r.gender === v }, t)))),
      DAY_COLS.map((k, n) => h('td', { class: `w-day${n === 0 ? ' gl' : ''}` },
        cell(r, i, k, { inputmode: 'numeric', placeholder: '·', 'aria-label': { s: 'Sakit', i: 'Izin', a: 'Alpa' }[k] }))),
      h('td', { class: 'att-out tabular' }, attendanceText(r, effDays)),
      h('td', { class: 'row-tools' },
        h('button', { type: 'button', class: 'icon-btn', title: 'Naikkan (Alt+↑)', tabIndex: -1, onclick: () => moveRow(i, -1) }, icon('up')),
        h('button', { type: 'button', class: 'icon-btn', title: 'Turunkan (Alt+↓)', tabIndex: -1, onclick: () => moveRow(i, 1) }, icon('down')),
        h('button', { type: 'button', class: 'icon-btn', title: r.id ? 'Hapus siswa ini' : 'Kosongkan baris', tabIndex: -1, onclick: () => removeRow(i) }, icon('trash'))));
  }
  const appendRow = (i) => { tbody.appendChild(rowEl(rows[i], i)); paintNo(); };
  function draw() {
    tbody.replaceChildren(...rows.map(rowEl));
    paintNo();
  }

  const classInput = h('input', {
    value: cls, placeholder: 'mis. VII-1, IX-2, I-A', 'aria-label': 'Nama kelas',
    oninput: (e) => { cls = e.target.value; changed(); },
  });

  draw();
  if (savedClass) setStatus('ok', 'Tersimpan otomatis');
  else setStatus('', 'Mulai dengan nama kelas, lalu ketik nama siswa');

  const node = h('div', { class: 'roster-page' },
    h('div', { class: 'roster-bar' },
      h('label', { class: 'field roster-class' }, h('span', { class: 'lbl' }, 'Nama kelas'), classInput),
      h('p', { class: 'hint' },
        h('b', {}, 'Enter'), ' = baris berikutnya · salin dari Excel lalu ', h('b', {}, shortcut('V')),
        ' (beberapa baris & kolom sekaligus) · di kolom S/I/A tekan ', h('b', {}, '+'), ' / ', h('b', {}, '−'),
        ` untuk menambah/mengurangi satu hari · kehadiran dihitung dari ${effDays} hari efektif.`),
      status),
    h('div', { class: 'sheet scroll roster-sheet' },
      h('table', { class: 'ledger edit roster-table' },
        h('thead', {},
          h('tr', {},
            h('th', { class: 'idx' }, 'No'), h('th', {}, 'Nama lengkap'), h('th', {}, 'NIS'), h('th', {}, 'NISN'), h('th', {}, 'L/P'),
            h('th', { class: 'c gl', title: 'Sakit (hari)' }, 'S'), h('th', { class: 'c', title: 'Izin (hari)' }, 'I'),
            h('th', { class: 'c', title: 'Alpa / tanpa keterangan (hari)' }, 'A'),
            h('th', { title: `Kehadiran di rapor, dari ${effDays} hari efektif` }, 'Hadir'), h('th', {}, ''))),
        tbody),
      h('div', { class: 'add-row' },
        button('Tambah 5 baris', { kind: 'quiet sm', iconName: 'plus', onClick: () => {
          const from = rows.length;
          rows.push(...Array.from({ length: 5 }, blank));
          for (let k = from; k < rows.length; k++) appendRow(k);
          focusCell(from, 'name');
        } }),
        ' ', counter)));

  requestAnimationFrame(() => (savedClass ? focusCell(0, 'name') : classInput.focus()));
  return { node, status, beforeClose };
}
