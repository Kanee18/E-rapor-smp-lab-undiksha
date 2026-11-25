// Wizard impor nilai proyek dari file Excel guru:
// pilih file → cek tebakan kolom + proyek + kelas → periksa (pratinjau perubahan) → terapkan.
import { api } from './api.js';
import { h, errorText, openModal, toast } from './dom.js';

const FIELDS = [
  ['name', 'Nama siswa', true],
  ['class', 'Kelas (opsional)', false],
  ['understanding', 'Understanding Level', false],
  ['inquiry', 'Inquiry & Research Skills', false],
  ['narrative', 'Narasi / feedback', false],
];

/**
 * @param {object} app
 * @param {{ projectId: string, cls: string, onDone: (projectId: string) => void }} opts
 */
export async function importProjectWizard(app, { projectId, cls, onDone }) {
  let path;
  try { path = await api.pickExcelFile(); } catch (e) { toast(errorText(e), 'error'); return; }
  if (!path) return;
  const settings = app.data.settings;
  const classes = app.classes();
  const modal = openModal({ title: 'Impor nilai proyek', wide: true, body: h('p', { class: 'muted' }, 'Membaca file…') });
  let scans;
  try { scans = await api.scanExcel(path); } catch (e) { modal.setBody(h('div', { class: 'note danger' }, errorText(e))); return; }

  let sheetName = scans[0]?.name;
  let mapping = null;
  let preview = null;
  let error = '';
  let pid = projectId;
  let pidTouched = false;
  const chosen = new Set(cls ? [cls] : classes);

  async function load(next) {
    try {
      preview = await api.inspectProjectSheet(path, sheetName, next);
      mapping = preview.mapping;
      error = '';
      if (!pidTouched && preview.guessed_project) pid = preview.guessed_project;
      // file punya kolom kelas: pilih kelas-kelas yang muncul di file
      const fileClasses = [...new Set(preview.rows.map((r) => r.class_name).filter((c) => classes.includes(c)))];
      if (mapping.class != null && fileClasses.length) { chosen.clear(); fileClasses.forEach((c) => chosen.add(c)); }
    } catch (e) {
      preview = null;
      error = errorText(e);
    }
    draw();
  }

  const colLabel = (c) => `${c.letter} — ${c.header || '(tanpa judul)'}${c.samples.length ? ` · ${c.samples.join(', ')}` : ''}`;
  const val = (t) => t || h('span', { class: 'muted' }, '—');

  function draw() {
    const sheetPicker = scans.length > 1
      ? h('label', { class: 'field', style: { maxWidth: '320px' } }, h('span', { class: 'lbl' }, 'Sheet'),
        h('select', { onchange: (e) => { sheetName = e.target.value; load(null); } },
          scans.map((s) => h('option', { value: s.name, selected: s.name === sheetName }, s.name))))
      : h('p', {}, 'Sheet: ', h('b', {}, sheetName));
    if (!preview) {
      modal.setBody(h('div', { class: 'stack' }, h('p', { class: 'hint' }, 'File: ', h('b', {}, path)), sheetPicker,
        h('div', { class: 'note danger' }, error || 'Membaca…')));
      modal.setActions([{ label: 'Tutup', kind: 'quiet', onClick: (c) => c() }]);
      return;
    }
    const selects = FIELDS.map(([key, label, required]) => h('label', { class: 'field' },
      h('span', { class: 'lbl' }, label, required ? ' *' : ''),
      h('select', { onchange: (e) => { mapping = { ...mapping, [key]: e.target.value === '' ? null : Number(e.target.value) }; load(mapping); } },
        required ? null : h('option', { value: '', selected: mapping[key] == null }, '(tidak ada)'),
        preview.columns.map((c) => h('option', { value: String(c.index), selected: mapping[key] === c.index }, colLabel(c))))));

    const projectField = h('label', { class: 'field', style: { maxWidth: '420px' } },
      h('span', { class: 'lbl' }, 'Masukkan ke proyek *'),
      h('select', { onchange: (e) => { pid = e.target.value; pidTouched = true; } },
        settings.projects.map((p) => h('option', { value: p.id, selected: p.id === pid }, p.theme ? `${p.name} — ${p.theme}` : p.name))),
      preview.guessed_project && !pidTouched ? h('span', { class: 'hint' }, 'Ditebak dari nama sheet / judul tabel. Ubah kalau keliru.') : null);

    const classBox = h('div', {},
      h('div', { class: 'eyebrow', style: { marginBottom: '8px' } }, 'Cocokkan nama dengan siswa di kelas'),
      h('div', { class: 'grid-3' }, classes.map((c) => h('label', { class: 'check' },
        h('input', { type: 'checkbox', checked: chosen.has(c), onchange: (e) => (e.target.checked ? chosen.add(c) : chosen.delete(c)) }), c))));

    const table = h('div', { class: 'sheet scroll', style: { maxHeight: '240px' } }, h('table', { class: 'ledger' },
      h('thead', {}, h('tr', {}, ['Baris', 'Nama', 'Kelas', 'Understanding', 'Inquiry', 'Narasi'].map((t) => h('th', {}, t)))),
      h('tbody', {}, preview.rows.map((r) => h('tr', {},
        h('td', { class: 'tabular muted' }, r.row), h('td', { class: 'name' }, r.name), h('td', {}, val(r.class_name)),
        h('td', {}, val(r.understanding)), h('td', {}, val(r.inquiry)),
        h('td', { class: 'small' }, val(r.narrative.length > 60 ? `${r.narrative.slice(0, 60)}…` : r.narrative)))))));

    modal.setBody(h('div', { class: 'stack' },
      h('p', { class: 'hint' }, 'File: ', h('b', {}, path)),
      sheetPicker,
      h('p', { class: 'hint' }, 'Kolom ditebak otomatis dari judul dan isinya. Cek pratinjau; kalau ada kolom yang keliru, ganti pilihannya. Predikat boleh ditulis EE/ME/AE/BE, E/M/A/B, atau angka 0–100.'),
      h('div', { class: 'map-grid' }, selects),
      projectField,
      h('div', {}, h('div', { class: 'eyebrow', style: { marginBottom: '8px' } },
        `Pratinjau — ${preview.total} baris siswa${preview.total > preview.rows.length ? ` (${preview.rows.length} pertama)` : ''}`), table),
      preview.warnings.length ? h('div', { class: 'note warn small' }, h('ul', { style: { margin: 0 } }, preview.warnings.map((w) => h('li', {}, w)))) : null,
      classBox,
      h('p', { class: 'hint' }, 'Sel kosong di Excel tidak menghapus isian yang sudah ada. Anda akan melihat ringkasan perubahan sebelum diterapkan.')));
    modal.setActions([
      { label: 'Batal', kind: 'quiet', onClick: (c) => c() },
      { label: 'Periksa', kind: 'primary', onClick: () => check() },
    ]);
  }

  async function run(apply) {
    return api.importProjectScores(path, sheetName, mapping, pid, classes.filter((c) => chosen.has(c)), apply);
  }

  async function check() {
    if (!chosen.size) { toast('Pilih minimal satu kelas', 'error'); return; }
    modal.setActions([{ label: 'Memeriksa…', disabled: true }]);
    let rep;
    try { rep = await run(false); } catch (e) { toast(errorText(e), 'error'); draw(); return; }
    const proj = settings.projects.find((p) => p.id === pid);
    const list = (title, items, kind = 'warn') => items.length
      ? h('div', { class: `note ${kind} small` }, h('b', {}, `${title} (${items.length})`),
        h('ul', { style: { margin: '6px 0 0' } }, items.slice(0, 12).map((t) => h('li', {}, t)), items.length > 12 ? h('li', {}, `… dan ${items.length - 12} lagi`) : null))
      : null;
    const nothing = rep.changed_cells === 0 && rep.new_entries === 0;
    modal.setBody(h('div', { class: 'stack' },
      h('p', { class: `note ${nothing ? 'warn' : 'ok'}` },
        h('b', {}, nothing ? 'Tidak ada yang berubah. ' : 'Siap diterapkan. '),
        `${rep.matched} dari ${rep.rows_read} siswa cocok · ${rep.changed_cells} isian berubah · ${rep.new_entries} siswa baru masuk ${proj?.name || 'proyek ini'}.`),
      list('Isian lama yang akan tertimpa', rep.overwritten),
      list('Nama dicocokkan secara perkiraan, mohon dicek', rep.approximate.map((a) => `${a.class_name}: "${a.from_name}" → ${a.to_name}`)),
      list('Nama tidak ditemukan (dilewati)', rep.unmatched.map((u) => `${u.name} — ${u.reason}`), 'danger'),
      list('Isian tidak terbaca (dilewati)', rep.invalid, 'danger'),
      list(`Sudah punya proyek lain; setelah ini punya dua proyek di rapor`, rep.other_project)));
    modal.setActions([
      { label: '← Kembali', kind: 'quiet', onClick: () => draw() },
      { label: nothing ? 'Tutup' : `Terapkan ${rep.changed_cells} isian`, kind: 'primary', onClick: async (close) => {
        if (nothing) { close(); return; }
        modal.setActions([{ label: 'Menyimpan…', disabled: true }]);
        try {
          const done = await run(true);
          close();
          toast(`Nilai proyek diimpor: ${done.changed_cells} isian, ${done.matched} siswa`, 'ok', 6000);
          onDone(pid);
        } catch (e) { toast(errorText(e), 'error'); check(); }
      } },
    ]);
  }

  modal.setBody(h('p', { class: 'muted' }, 'Menebak susunan kolom…'));
  await load(null);
}
