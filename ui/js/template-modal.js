// Template Excel baku untuk guru: data siswa, nilai mapel, nilai proyek, absensi.
// Susunannya sama untuk SD, SMP, dan SMA (lima kolom identitas yang sama, lalu kolom isian),
// jadi semua file dari guru seragam dan langsung terbaca impor aplikasi.
// Daftar siswa dari aplikasi boleh ikut dicantumkan supaya guru tidak perlu mengetik nama.
import { api } from './api.js';
import { h, errorText, openModal, toast } from './dom.js';

const KINDS = [
  { id: 'students', label: '1 · Data siswa' },
  { id: 'scores', label: '2 · Nilai mapel' },
  { id: 'projects', label: '3 · Nilai proyek' },
  { id: 'attendance', label: '4 · Absensi' },
];
const ID_COLS = ['No', 'Nama Siswa', 'NIS', 'NISN', 'Kelas'];

const INFO = {
  students: {
    what: 'Untuk membuat daftar siswa. Satu baris = satu siswa; satu file boleh berisi beberapa kelas.',
    cols: ['L/P'],
    example: ['L'],
    rules: [
      'Nama Siswa wajib. NIS atau NISN isi minimal salah satu (NISN 10 angka).',
      'Kelas ditulis dengan pola yang sama, mis. I-A, VII-1, X-B. L/P diisi L atau P.',
    ],
    where: 'Data siswa → Impor dari Excel',
  },
  scores: {
    what: 'Satu file untuk satu mata pelajaran. Guru menulis nama mapel di kotak kuning, lalu mengisi PH dan PTS.',
    field: 'Mata pelajaran:',
    cols: ['PH', 'PTS'],
    example: ['88', '92'],
    rules: [
      'PH = rata-rata penilaian harian, PTS = penilaian tengah semester. Angka 0–100.',
      'Nilai akhir dan predikat dihitung aplikasi, tidak perlu rumus di Excel.',
    ],
    where: 'Nilai mapel → Impor nilai dari Excel',
  },
  projects: {
    what: 'Satu file untuk satu proyek. Nama proyek ditulis di kotak kuning.',
    field: 'Proyek:',
    cols: ['Understanding Level', 'Inquiry & Research Skills', 'Teacher Narrative Feedback (English)'],
    example: ['ME', 'AE', '… met the expectations of this unit and …'],
    rules: [
      'Understanding dan Inquiry: EE / ME / AE / BE, huruf awalnya (E, M, A, B), atau angka 0–100.',
      'Narasi bahasa Inggris 2–4 kalimat; boleh kosong lalu dibuat dengan Draf otomatis.',
    ],
    where: 'Isi rapor → Nilai proyek → Impor nilai proyek dari Excel',
  },
  attendance: {
    what: 'Untuk rekap kehadiran satu semester.',
    cols: ['Sakit', 'Izin', 'Alpa'],
    example: ['2', '1', '0'],
    rules: [
      'Isi JUMLAH HARI sakit, izin, dan alpa (angka bulat). Kosong dianggap 0.',
      'Persentase kehadiran, mis. 98% (Present), dihitung aplikasi dari hari efektif.',
    ],
    where: 'Isi rapor → Impor absensi Excel',
  },
};

/**
 * @param {object} app
 * @param {{kind?: string, projectId?: string, classes?: string[]}} [opts]
 */
export function templateModal(app, opts = {}) {
  let kind = opts.kind || 'students';
  const allClasses = app.classes();
  const chosen = new Set(opts.classes?.length ? opts.classes : allClasses);
  const projects = app.data.settings.projects;
  let projectId = opts.projectId || '';
  let withStudents = allClasses.length > 0;

  const modal = openModal({ title: 'Template Excel', body: h('div'), wide: true });

  function draw() {
    const info = INFO[kind];
    const tabs = h('div', { class: 'tabs', role: 'tablist' }, KINDS.map((k) => h('button', {
      type: 'button', role: 'tab', class: `tab${k.id === kind ? ' active' : ''}`, 'aria-selected': String(k.id === kind),
      onclick: () => { kind = k.id; draw(); },
    }, k.label)));

    const options = [];
    if (kind === 'projects') {
      options.push(h('label', { class: 'field', style: { maxWidth: '420px' } }, h('span', { class: 'lbl' }, 'Proyek yang ditulis di kotak kuning'),
        h('select', { onchange: (e) => { projectId = e.target.value; draw(); } },
          h('option', { value: '', selected: !projectId }, '(dikosongkan, diisi guru di Excel)'),
          projects.map((p) => h('option', { value: p.id, selected: p.id === projectId }, p.theme ? `${p.name} — ${p.theme}` : p.name)))));
    }
    if (kind !== 'students' && allClasses.length) {
      options.push(h('div', { class: 'stack', style: { gap: '8px' } },
        h('label', { class: 'check' },
          h('input', { type: 'checkbox', checked: withStudents, onchange: (e) => { withStudents = e.target.checked; draw(); } }),
          'Cantumkan daftar siswa dari aplikasi (guru tidak perlu mengetik nama, NIS, NISN, dan kelas)'),
        withStudents ? h('div', { class: 'tpl-classes', style: { paddingLeft: '26px' } }, allClasses.map((c) => h('label', { class: 'check' },
          h('input', { type: 'checkbox', checked: chosen.has(c), onchange: (e) => { if (e.target.checked) chosen.add(c); else chosen.delete(c); draw(); } }),
          c))) : null));
    }

    modal.setBody(h('div', { class: 'stack' },
      h('p', { class: 'muted', style: { margin: 0 } }, 'Template baku yang sama untuk SD, SMP, dan SMA. Bagikan ke guru; setelah diisi, file langsung terbaca aplikasi tanpa perlu mengatur kolom.'),
      tabs,
      h('p', { style: { margin: 0 } }, info.what),
      ...options,
      h('div', {}, h('div', { class: 'eyebrow', style: { marginBottom: '6px' } }, 'Susunan di Excel'), preview()),
      h('ul', { class: 'hint', style: { margin: 0, paddingLeft: '18px' } },
        h('li', {}, 'Lima kolom pertama sama di semua template. Isi NISN kalau ada, supaya siswa pasti cocok.'),
        info.rules.map((r) => h('li', {}, r)),
        h('li', {}, 'Isi mulai baris 6; jangan mengubah baris judul atau menggabungkan sel.')),
      h('p', { class: 'small muted', style: { margin: 0 } }, 'Setelah diisi, masukkan lewat ', h('b', {}, info.where), '. Petunjuk lengkap ada di sheet "Petunjuk" di dalam file.')));
    modal.setActions([
      { label: 'Tutup', kind: 'quiet', onClick: (c) => c() },
      { label: 'Simpan template…', kind: 'primary', onClick: save },
    ]);
  }

  // ---------------------------------------------------------------- pratinjau susunan
  function sample(n) {
    if (kind === 'students' || !withStudents) return [];
    for (const c of allClasses.filter((x) => chosen.has(x))) {
      const list = app.data.students.filter((s) => s.class_name === c)
        .sort((a, b) => (a.order ?? 1e9) - (b.order ?? 1e9) || a.name.localeCompare(b.name));
      if (list.length) return list.slice(0, n);
    }
    return [];
  }

  function preview() {
    const info = INFO[kind];
    const cols = [...ID_COLS, ...info.cols];
    const students = sample(2);
    const ex = (t, c = '') => h('td', { class: `tpl-ex ${c}` }, t);
    const fixed = (t, c = '') => h('td', { class: `tpl-fixed ${c}` }, t);
    const rows = [];
    for (let i = 0; i < 2; i++) {
      const s = students[i];
      const id = s
        ? [fixed(String(i + 1), 'c'), fixed(s.name), fixed(s.nis || ''), fixed(s.nisn || ''), fixed(s.class_name, 'c')]
        : i === 0
          ? [fixed('1', 'c'), ex('Adi Putra Wijaya'), ex('7001'), ex('0098765401'), ex('IX-1', 'c')]
          : [fixed('2', 'c'), h('td'), h('td'), h('td'), h('td')];
      const vals = info.cols.map((_, k) => (i === 0 ? ex(info.example[k], k === 2 && kind === 'projects' ? '' : 'c') : h('td')));
      rows.push(h('tr', {}, id, vals));
    }
    const p = projects.find((x) => x.id === projectId);
    const fieldValue = kind === 'projects' && p ? (p.theme ? `${p.name} — ${p.theme}` : p.name) : kind === 'scores' ? 'mis. IPA' : '';
    return h('div', { class: 'sheet scroll' },
      h('table', { class: 'tpl-sheet' },
        info.field ? h('tr', { class: 'tpl-field-row' }, h('td', { class: 'tpl-plain' }), h('td', { class: 'tpl-plain r' }, h('b', {}, info.field)),
          h('td', { class: 'tpl-field', colSpan: 3 }, h('span', { class: fieldValue.startsWith('mis.') || !fieldValue ? 'tpl-ex' : '' }, fieldValue || '(diisi guru)')),
          info.cols.map(() => h('td', { class: 'tpl-plain' }))) : null,
        h('tr', {}, cols.map((t) => h('th', {}, t))),
        rows,
        h('tr', {}, h('td', { class: 'tpl-more', colSpan: cols.length }, '…'))));
  }

  async function save() {
    const useStudents = kind !== 'students' && withStudents && allClasses.length > 0;
    if (useStudents && !chosen.size) { toast('Pilih minimal satu kelas', 'error'); return; }
    const classes = allClasses.filter((c) => chosen.has(c));
    try {
      const res = await api.saveTemplate({
        kind,
        with_students: useStudents,
        classes: useStudents && classes.length < allClasses.length ? classes : [],
        project_id: kind === 'projects' && projectId ? projectId : null,
      });
      if (!res) return;
      modal.setBody(h('div', { class: 'stack' },
        h('div', { class: 'note ok' }, h('b', {}, 'Template tersimpan. '),
          res.students ? `Daftar ${res.students} siswa sudah tercantum.` : 'Baris masih kosong, siap diisi guru.'),
        h('p', { class: 'hint', style: { wordBreak: 'break-all' } }, res.path),
        h('p', {}, 'Bagikan file ini ke guru. Setelah diisi, masukkan lewat ', h('b', {}, INFO[kind].where), '.')));
      modal.setActions([
        { label: 'Tampilkan di folder', kind: 'quiet', onClick: () => api.revealPath(res.path).catch((e) => toast(errorText(e), 'error')) },
        { label: 'Template lain', kind: 'quiet', onClick: () => draw() },
        { label: 'Selesai', kind: 'primary', onClick: (c) => c() },
      ]);
    } catch (e) {
      toast(errorText(e), 'error', 6000);
    }
  }

  draw();
}
