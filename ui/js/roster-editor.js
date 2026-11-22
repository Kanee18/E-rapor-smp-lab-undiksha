// Buat / ubah daftar siswa satu kelas langsung di aplikasi, seperti mengisi tabel Excel.
// Urutan baris = nomor absen (urutan tampil di semua halaman dan di rapor).
import { api } from './api.js';
import { h, button, confirmBox, errorText, icon, openModal, toast } from './dom.js';

const COLS = ['name', 'nis', 'nisn', 'gender'];

const genderOf = (raw) => {
  const t = String(raw ?? '').trim().toUpperCase().replace(/[^A-Z]/g, '');
  if (['L', 'LK', 'LAKILAKI', 'LAKI', 'PRIA', 'M'].includes(t)) return 'L';
  if (['P', 'PR', 'PEREMPUAN', 'WANITA', 'F'].includes(t)) return 'P';
  return '';
};
const blank = () => ({ id: null, name: '', nis: '', nisn: '', gender: '', removed: false });
const isEmpty = (r) => !r.id && !r.name.trim() && !r.nis.trim() && !r.nisn.trim();
const badNisn = (v) => v.trim() !== '' && !/^\d{10}$/.test(v.trim());

/**
 * @param {object} app
 * @param {string|null} className  kelas yang diedit; null = buat kelas baru
 * @param {(cls: string) => void} onSaved
 */
export function rosterEditor(app, className, onSaved) {
  const original = className || null;
  const existing = original
    ? app.data.students
      .filter((s) => s.class_name === original)
      .sort((a, b) => (a.order ?? Infinity) - (b.order ?? Infinity) || a.name.localeCompare(b.name, 'id'))
      .map((s) => ({ id: s.id, name: s.name, nis: s.nis, nisn: s.nisn, gender: s.gender, removed: false }))
    : [];
  const rows = existing.length ? [...existing, blank()] : Array.from({ length: 5 }, blank);
  const snapshot = JSON.stringify(existing);
  let cls = original || '';
  let dirty = false;

  const tbody = h('tbody', {});
  const counter = h('span', { class: 'muted small' });

  const focusCell = (i, key) => {
    requestAnimationFrame(() => {
      const el = tbody.querySelector(`[data-row="${i}"][data-key="${key}"]`);
      if (!el) return;
      el.focus();
      if (el.tagName === 'INPUT') el.setSelectionRange(el.value.length, el.value.length);
    });
  };

  /** Tempel beberapa baris/kolom dari Excel mulai dari sel (i, key). */
  function pasteBlock(i, key, text) {
    const lines = text.replace(/\r/g, '').split('\n');
    while (lines.length && !lines[lines.length - 1].trim()) lines.pop();
    const start = COLS.indexOf(key);
    let r = i;
    for (const line of lines) {
      let cells = line.split('\t').map((c) => c.trim());
      // salinan yang ikut kolom "No" di depan (1, 2, 3, …) saat ditempel di kolom Nama
      if (key === 'name' && cells.length > 1 && /^\d{1,3}$/.test(cells[0]) && /[A-Za-z]/.test(cells[1])) cells = cells.slice(1);
      while (r < rows.length && rows[r].removed) r++;
      if (r >= rows.length) rows.push(blank());
      cells.forEach((v, k) => {
        const col = COLS[start + k];
        if (!col) return;
        rows[r][col] = col === 'gender' ? genderOf(v) : v;
      });
      r++;
    }
    if (!rows.length || !isEmpty(rows[rows.length - 1])) rows.push(blank());
    dirty = true;
    draw();
    toast(`${lines.length} baris ditempel`, 'ok');
  }

  function moveRow(i, d) {
    const j = i + d;
    if (j < 0 || j >= rows.length) return;
    [rows[i], rows[j]] = [rows[j], rows[i]];
    dirty = true;
    draw();
    focusCell(j, 'name');
  }

  function removeRow(i) {
    const r = rows[i];
    if (r.id) r.removed = !r.removed; else rows.splice(i, 1);
    if (!rows.length) rows.push(blank());
    dirty = true;
    draw();
  }

  function cellInput(r, i, key, extra = {}) {
    const input = h('input', {
      value: r[key],
      dataset: { row: String(i), key },
      disabled: r.removed,
      spellcheck: false,
      ...extra,
      oninput: (e) => {
        r[key] = e.target.value;
        dirty = true;
        if (key === 'nisn') e.target.classList.toggle('invalid', badNisn(r.nisn));
        if (i === rows.length - 1 && !isEmpty(r)) { rows.push(blank()); draw(); focusCell(i, key); }
        updateCount();
      },
      onpaste: (e) => {
        const text = e.clipboardData?.getData('text/plain') ?? '';
        if (!/[\t\n]/.test(text.replace(/\r?\n$/, ''))) return; // satu sel biasa
        e.preventDefault();
        pasteBlock(i, key, text);
      },
      onkeydown: (e) => {
        if (e.key === 'Enter') {
          e.preventDefault();
          if (i === rows.length - 1) { rows.push(blank()); draw(); }
          focusCell(i + 1, key);
        } else if (e.key === 'ArrowDown' && e.altKey) { e.preventDefault(); moveRow(i, 1); }
        else if (e.key === 'ArrowUp' && e.altKey) { e.preventDefault(); moveRow(i, -1); }
      },
    });
    if (key === 'nisn' && badNisn(r.nisn)) input.classList.add('invalid');
    return input;
  }

  function updateCount() {
    const live = rows.filter((r) => !r.removed && !isEmpty(r)).length;
    const gone = rows.filter((r) => r.removed).length;
    counter.textContent = `${live} siswa${gone ? ` · ${gone} akan dihapus` : ''}`;
  }

  function draw() {
    let no = 0;
    tbody.replaceChildren(...rows.map((r, i) => {
      const shown = r.removed || isEmpty(r) ? '' : ++no;
      return h('tr', { class: r.removed ? 'removed' : '' },
        h('td', { class: 'idx' }, shown),
        h('td', { class: 'w-name' }, cellInput(r, i, 'name', { placeholder: i === 0 ? 'Nama lengkap siswa' : '' })),
        h('td', { class: 'w-num' }, cellInput(r, i, 'nis', { inputmode: 'numeric' })),
        h('td', { class: 'w-nisn' }, cellInput(r, i, 'nisn', { inputmode: 'numeric', maxlength: 10, title: 'NISN 10 angka (boleh kosong)' })),
        h('td', { class: 'w-lp' }, h('select', {
          disabled: r.removed,
          dataset: { row: String(i), key: 'gender' },
          onchange: (e) => { r.gender = e.target.value; dirty = true; },
        }, [['', '—'], ['L', 'L'], ['P', 'P']].map(([v, t]) => h('option', { value: v, selected: r.gender === v }, t)))),
        h('td', { class: 'row-tools' },
          r.removed
            ? h('button', { type: 'button', class: 'link', onclick: () => removeRow(i) }, 'Batalkan')
            : [
              h('button', { type: 'button', class: 'icon-btn', title: 'Naikkan (Alt+↑)', disabled: i === 0, onclick: () => moveRow(i, -1) }, icon('up')),
              h('button', { type: 'button', class: 'icon-btn', title: 'Turunkan (Alt+↓)', disabled: i === rows.length - 1, onclick: () => moveRow(i, 1) }, icon('down')),
              h('button', { type: 'button', class: 'icon-btn', title: r.id ? 'Hapus siswa ini' : 'Hapus baris', onclick: () => removeRow(i) }, icon('trash')),
            ]));
    }));
    updateCount();
  }

  const classInput = h('input', {
    value: cls,
    placeholder: 'mis. VII-1, IX-2, I-A',
    oninput: (e) => { cls = e.target.value; dirty = true; },
  });

  draw();
  const modal = openModal({
    title: original ? `Daftar siswa kelas ${original}` : 'Buat kelas baru',
    wide: true,
    body: h('div', { class: 'stack roster' },
      h('div', { class: 'roster-head' },
        h('label', { class: 'field' }, h('span', { class: 'lbl' }, 'Nama kelas'), classInput),
        h('p', { class: 'hint' },
          'Isi seperti tabel Excel. ', h('b', {}, 'Enter'), ' pindah ke baris berikutnya. Bisa juga menyalin kolom dari Excel lalu ',
          h('b', {}, 'Ctrl+V'), ' di kolom Nama: beberapa baris sekaligus. Urutan baris = nomor absen; geser dengan ↑ ↓.')),
      h('div', { class: 'sheet' },
        h('table', { class: 'ledger edit roster-table' },
          h('thead', {}, h('tr', {},
            h('th', { class: 'idx' }, 'No'), h('th', {}, 'Nama lengkap'), h('th', {}, 'NIS'), h('th', {}, 'NISN'), h('th', {}, 'L/P'), h('th', {}, ''))),
          tbody),
        h('div', { class: 'add-row' },
          button('Tambah baris', { kind: 'quiet sm', iconName: 'plus', onClick: () => { rows.push(blank()); dirty = true; draw(); focusCell(rows.length - 1, 'name'); } }),
          ' ', counter))),
    actions: [
      { label: 'Batal', kind: 'quiet', onClick: async (close) => {
        const changed = dirty && (JSON.stringify(rows.filter((r) => !isEmpty(r)).map(({ removed, ...r }) => r)) !== snapshot || rows.some((r) => r.removed) || cls !== (original || ''));
        if (changed && !(await confirmBox('Perubahan di daftar ini belum disimpan. Tutup tanpa menyimpan?', { ok: 'Tutup', danger: true }))) return;
        close();
      } },
      { label: 'Simpan daftar', kind: 'primary', onClick: (close) => save(close) },
    ],
  });
  if (!original) requestAnimationFrame(() => classInput.focus());
  else focusCell(0, 'name');

  async function save(close) {
    if (!cls.trim()) { toast('Isi nama kelas dulu, mis. VII-1', 'error'); classInput.focus(); return; }
    const keep = rows.filter((r) => !r.removed && !isEmpty(r));
    const remove = rows.filter((r) => r.removed).map((r) => r.id);
    const unnamed = keep.findIndex((r) => !r.name.trim());
    if (unnamed >= 0) { toast(`Baris ${unnamed + 1}: nama siswa belum diisi`, 'error'); return; }
    const wrong = keep.findIndex((r) => badNisn(r.nisn));
    if (wrong >= 0) { toast(`Baris ${wrong + 1} (${keep[wrong].name}): NISN harus 10 angka, atau kosongkan`, 'error'); return; }
    if (!keep.length) { toast('Daftar masih kosong. Isi minimal satu nama siswa.', 'error'); return; }
    if (remove.length) {
      const names = rows.filter((r) => r.removed).map((r) => r.name).join(', ');
      const ok = await confirmBox(`${remove.length} siswa akan dihapus beserta nilai dan isian rapornya: ${names}. Data dibackup otomatis dulu.`,
        { title: 'Hapus siswa', ok: 'Ya, simpan & hapus', danger: true });
      if (!ok) return;
    }
    try {
      const r = await api.saveClassRoster(original, cls, keep.map(({ removed, ...x }) => x), remove);
      const parts = [r.added && `${r.added} baru`, r.updated && `${r.updated} diperbarui`, r.removed && `${r.removed} dihapus`].filter(Boolean);
      toast(`Kelas ${r.class_name} tersimpan${parts.length ? `: ${parts.join(', ')}` : ''}`, 'ok', 5000);
      close();
      onSaved(r.class_name);
    } catch (e) { toast(errorText(e), 'error', 7000); }
  }
  return modal;
}
