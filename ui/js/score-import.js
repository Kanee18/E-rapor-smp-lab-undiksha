// Impor nilai mapel dari file Excel guru yang susunannya bebas:
// tebakan kolom (Nama, NIS/NISN, Kelas, kolom nilai per mapel · PH/PTS/Nilai akhir) →
// koreksi kalau perlu → periksa (ringkasan perubahan) → terapkan.
import { api } from './api.js';
import { h, errorText, openModal, toast } from './dom.js';

const ID_FIELDS = [
  ['name', 'Nama siswa', true],
  ['nisn', 'NISN (opsional)', false],
  ['nis', 'NIS (opsional)', false],
  ['class', 'Kelas (opsional)', false],
];
const PARTS = [['ph', 'PH'], ['pts', 'PTS'], ['final', 'Nilai akhir']];

/**
 * @param {object} app
 * @param {{ path?: string, scans?: object[], sheet?: string, modal?: object, onDone?: () => void }} [opts]
 *   path/scans/modal dipakai kalau dipanggil dari wizard impor biasa (file sudah dipilih).
 */
export async function importScoreWizard(app, { path, scans, sheet, modal, onDone } = {}) {
  if (!path) {
    try { path = await api.pickExcelFile(); } catch (e) { toast(errorText(e), 'error'); return; }
    if (!path) return;
  }
  modal ??= openModal({ title: 'Impor nilai mapel', wide: true, body: h('p', { class: 'muted' }, 'Membaca file…') });
  if (!scans) {
    try { scans = await api.scanExcel(path); } catch (e) { modal.setBody(h('div', { class: 'note danger' }, errorText(e))); return; }
  }
  const classes = app.classes();
  let sheetName = sheet || scans.find((s) => s.kind === 'scores')?.name || scans[0]?.name;
  let mapping = null;
  let preview = null;
  let error = '';
  let chosen = new Set();
  let classesTouched = false;

  async function load(next) {
    try {
      preview = await api.inspectScoreSheet(path, sheetName, next);
      mapping = preview.mapping;
      error = '';
      if (!classesTouched) {
        // kelas yang disebut file (kolom / judul / nama sheet) dan ada di aplikasi; selain itu kelas yang sedang dibuka
        const fromFile = preview.classes.map((c) => c.name).filter((c) => classes.includes(c));
        chosen = new Set(fromFile.length ? fromFile : app.selectedClass && classes.includes(app.selectedClass) ? [app.selectedClass] : classes);
      }
    } catch (e) {
      preview = null;
      error = errorText(e);
    }
    draw();
  }
  const reload = () => load(mapping);

  const colLabel = (c) => `${c.letter} — ${c.header || '(tanpa judul)'}${c.samples.length ? ` · ${c.samples.join(', ')}` : ''}`;
  const val = (t) => t || h('span', { class: 'muted' }, '—');

  function scoreColumns() {
    const idCols = new Set(ID_FIELDS.map(([k]) => mapping[k]).filter((v) => v != null));
    const mapped = new Map(mapping.scores.map((s) => [s.col, s]));
    // kolom yang isinya angka, atau yang sudah dipilih sebagai nilai
    return preview.columns.filter((c) => !idCols.has(c.index)
      && (mapped.has(c.index) || c.samples.some((t) => /^-?\d+([.,]\d+)?$/.test(t.trim()))));
  }

  function setScore(col, value) {
    const rest = mapping.scores.filter((s) => s.col !== col);
    if (value) {
      const [subject, part] = JSON.parse(value);
      rest.push({ col, subject, part });
      rest.sort((a, b) => a.col - b.col);
    }
    mapping = { ...mapping, scores: rest };
    reload();
  }

  function draw() {
    const sheetPicker = scans.length > 1
      ? h('label', { class: 'field', style: { maxWidth: '320px' } }, h('span', { class: 'lbl' }, 'Sheet'),
        h('select', { onchange: (e) => { sheetName = e.target.value; classesTouched = false; load(null); } },
          scans.map((s) => h('option', { value: s.name, selected: s.name === sheetName }, s.name))))
      : h('p', {}, 'Sheet: ', h('b', {}, sheetName));
    if (!preview) {
      modal.setBody(h('div', { class: 'stack' }, h('p', { class: 'hint' }, 'File: ', h('b', {}, path)), sheetPicker,
        h('div', { class: 'note danger' }, error || 'Membaca…')));
      modal.setActions([{ label: 'Tutup', kind: 'quiet', onClick: (c) => c() }]);
      return;
    }

    const idSelects = ID_FIELDS.map(([key, label, required]) => h('label', { class: 'field' },
      h('span', { class: 'lbl' }, label, required ? ' *' : ''),
      h('select', { onchange: (e) => {
        const v = e.target.value === '' ? null : Number(e.target.value);
        mapping = { ...mapping, [key]: v, scores: mapping.scores.filter((s) => s.col !== v) };
        reload();
      } },
      required ? null : h('option', { value: '', selected: mapping[key] == null }, '(tidak ada)'),
      preview.columns.map((c) => h('option', { value: String(c.index), selected: mapping[key] === c.index }, colLabel(c))))));

    const options = (cur) => [
      h('option', { value: '', selected: !cur }, '— bukan nilai —'),
      ...preview.subjects.map((s) => h('optgroup', { label: s.on_report ? s.name : `${s.name} (belum di rapor)` },
        PARTS.map(([p, t]) => h('option', { value: JSON.stringify([s.key, p]), selected: cur && cur.subject === s.key && cur.part === p }, `${s.name} · ${t}`)))),
    ];
    const cols = scoreColumns();
    const scoreTable = h('div', { class: 'sheet scroll', style: { maxHeight: '260px' } }, h('table', { class: 'ledger' },
      h('thead', {}, h('tr', {}, h('th', { style: { width: '46px' } }, 'Kolom'), h('th', {}, 'Judul di file'), h('th', {}, 'Contoh isi'), h('th', { style: { width: '240px' } }, 'Masuk sebagai'))),
      h('tbody', {}, cols.length
        ? cols.map((c) => {
          const cur = mapping.scores.find((s) => s.col === c.index);
          return h('tr', {},
            h('td', { class: 'tabular muted' }, c.letter),
            h('td', {}, val(c.header)),
            h('td', { class: 'small muted' }, c.samples.join(', ')),
            h('td', {}, h('select', { 'aria-label': `Kolom ${c.letter}`, onchange: (e) => setScore(c.index, e.target.value) }, options(cur))));
        })
        : h('tr', {}, h('td', { colspan: 4, class: 'muted' }, 'Tidak ada kolom berisi angka di sheet ini.')))));

    const classField = mapping.class == null
      ? h('label', { class: 'field', style: { maxWidth: '420px' } },
        h('span', { class: 'lbl' }, 'Kelas untuk semua baris'),
        h('select', { onchange: (e) => {
          mapping = { ...mapping, default_class: e.target.value };
          if (e.target.value) { chosen = new Set([e.target.value]); classesTouched = true; }
          reload();
        } },
        h('option', { value: '', selected: !mapping.default_class }, '(cocokkan ke kelas yang dicentang di bawah)'),
        classes.map((c) => h('option', { value: c, selected: mapping.default_class === c }, c)),
        mapping.default_class && !classes.includes(mapping.default_class)
          ? h('option', { value: mapping.default_class, selected: true }, `${mapping.default_class} (tidak ada di data siswa)`) : null),
        preview.class_source ? h('span', { class: 'hint' }, `Ditebak dari ${preview.class_source}. Ubah kalau keliru.`) : null)
      : null;

    const classBox = h('div', {},
      h('div', { class: 'eyebrow', style: { marginBottom: '8px' } }, 'Cocokkan nama dengan siswa di kelas'),
      h('div', { class: 'grid-3' }, classes.map((c) => h('label', { class: 'check' },
        h('input', { type: 'checkbox', checked: chosen.has(c), onchange: (e) => { classesTouched = true; e.target.checked ? chosen.add(c) : chosen.delete(c); } }), c))));

    const table = h('div', { class: 'sheet scroll', style: { maxHeight: '220px' } }, h('table', { class: 'ledger' },
      h('thead', {}, h('tr', {}, ['Baris', 'Nama', 'Kelas', ...preview.parts].map((t) => h('th', {}, t)))),
      h('tbody', {}, preview.rows.map((r) => h('tr', {},
        h('td', { class: 'tabular muted' }, r.row), h('td', { class: 'name' }, r.name), h('td', {}, val(r.class_name)),
        r.values.map((v) => h('td', { class: 'tabular' }, val(v))))))));

    modal.setBody(h('div', { class: 'stack' },
      h('p', { class: 'hint' }, 'File: ', h('b', {}, path)),
      sheetPicker,
      h('p', { class: 'hint' }, 'Kolom ditebak otomatis dari judul dan isinya. Cek pratinjau di bawah; kalau ada yang keliru, ganti pilihannya. Beberapa kolom PH untuk mapel yang sama (mis. NH 1, NH 2, NH 3) otomatis dirata-rata.'),
      h('div', { class: 'map-grid' }, idSelects),
      h('div', {}, h('div', { class: 'eyebrow', style: { marginBottom: '8px' } }, 'Kolom nilai'), scoreTable),
      classField,
      h('div', {}, h('div', { class: 'eyebrow', style: { marginBottom: '8px' } },
        `Pratinjau — ${preview.total} baris siswa${preview.total > preview.rows.length ? ` (${preview.rows.length} pertama)` : ''}`), table),
      preview.warnings.length ? h('div', { class: 'note warn small' }, h('ul', { style: { margin: 0 } }, preview.warnings.map((w) => h('li', {}, w)))) : null,
      classBox,
      h('p', { class: 'hint' }, 'Sel kosong di Excel tidak menghapus nilai yang sudah ada. Anda akan melihat ringkasan perubahan sebelum diterapkan.')));
    modal.setActions([
      { label: 'Batal', kind: 'quiet', onClick: (c) => c() },
      { label: 'Periksa', kind: 'primary', disabled: !preview.total, onClick: () => check() },
    ]);
  }

  const run = (apply) => api.importScoresMapped(path, sheetName, mapping, classes.filter((c) => chosen.has(c)), apply);

  async function check() {
    if (!chosen.size) { toast('Pilih minimal satu kelas', 'error'); return; }
    modal.setActions([{ label: 'Memeriksa…', disabled: true }]);
    let rep;
    try { rep = await run(false); } catch (e) { toast(errorText(e), 'error'); draw(); return; }
    const list = (title, items, kind = 'warn') => items.length
      ? h('div', { class: `note ${kind} small` }, h('b', {}, `${title} (${items.length})`),
        h('ul', { style: { margin: '6px 0 0' } }, items.slice(0, 12).map((t) => h('li', {}, t)), items.length > 12 ? h('li', {}, `… dan ${items.length - 12} lagi`) : null))
      : null;
    const nothing = rep.values_set === 0;
    modal.setBody(h('div', { class: 'stack' },
      h('p', { class: `note ${nothing ? 'warn' : 'ok'}` },
        h('b', {}, nothing ? 'Tidak ada yang berubah. ' : 'Siap diterapkan. '),
        `${rep.matched} dari ${rep.rows_read} siswa cocok · ${rep.values_set} nilai baru/berubah`,
        rep.unchanged ? ` · ${rep.unchanged} sama seperti yang tersimpan` : '', '.'),
      list('Nilai lama yang akan tertimpa', rep.overwritten),
      list('Nama dicocokkan secara perkiraan, mohon dicek', rep.approximate.map((a) => `${a.class_name}: "${a.from_name}" → ${a.to_name}`)),
      list('Nama tidak ditemukan (dilewati)', rep.unmatched.map((u) => `${u.name} — ${u.reason}`), 'danger'),
      list('Isian bukan angka 0–100 (dilewati)', rep.invalid, 'danger')));
    modal.setActions([
      { label: '← Kembali', kind: 'quiet', onClick: () => draw() },
      { label: nothing ? 'Tutup' : `Terapkan ${rep.values_set} nilai`, kind: 'primary', onClick: async (close) => {
        if (nothing) { close(); return; }
        modal.setActions([{ label: 'Menyimpan…', disabled: true }]);
        try {
          const done = await run(true);
          close();
          toast(`Nilai diimpor: ${done.values_set} nilai untuk ${done.matched} siswa`, 'ok', 6000);
          onDone?.();
        } catch (e) { toast(errorText(e), 'error'); check(); }
      } },
    ]);
  }

  modal.setBody(h('p', { class: 'muted' }, 'Menebak susunan kolom…'));
  await load(null);
}
