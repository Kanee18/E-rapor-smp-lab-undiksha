// Form Excel untuk fasilitator / wali kelas: buat → diisi di Excel → impor kembali.
import { api } from './api.js';
import { h, button, errorText, openModal, toast } from './dom.js';

/** Dialog membuat form isian. */
export function openFormBuilder(app) {
  const s = app.data.settings;
  const classes = app.classes();
  const initialClasses = app.selectedClass ? [app.selectedClass] : classes.slice(0, 1);
  const activeProjInClass = app.data.students
    .find((st) => initialClasses.includes(st.class_name) && st.projects.length)?.projects[0]?.project_id;
  const req = {
    classes: initialClasses,
    include_atl: true,
    project_id: (activeProjInClass && s.projects.some((p) => p.id === activeProjInClass))
      ? activeProjInClass
      : (s.projects[0]?.id ?? null),
    only_project_members: false,
    include_attendance: true,
    effective_days: s.effective_days || 100,
  };
  let withProject = Boolean(req.project_id);

  const countEl = h('p', { class: 'small muted tabular' });
  const update = () => {
    const n = app.data.students.filter((st) => req.classes.includes(st.class_name)
      && (!(withProject && req.only_project_members) || st.projects.some((p) => p.project_id === req.project_id))).length;
    const parts = [req.include_atl && 'ATL', withProject && 'proyek + narasi', req.include_attendance && 'kehadiran'].filter(Boolean);
    countEl.textContent = n
      ? `${n} siswa · kolom: ${parts.join(', ') || '—'}`
      : 'Tidak ada siswa untuk pilihan ini.';
  };
  const check = (label, checked, onChange, extra) => h('label', { class: 'check' },
    h('input', { type: 'checkbox', checked, onchange: (e) => { onChange(e.target.checked); update(); } }), label, extra || null);

  const projectSelect = h('select', { style: { width: 'auto', minWidth: '240px' }, onchange: (e) => { req.project_id = e.target.value; update(); } },
    s.projects.map((p) => h('option', { value: p.id, selected: p.id === req.project_id }, p.theme ? `${p.name} — ${p.theme}` : p.name)));

  const effDaysWrap = h('div', { style: { paddingLeft: '23px', display: 'flex', alignItems: 'center', gap: '8px' } },
    h('span', { class: 'small muted' }, 'Total hari efektif semester:'),
    h('input', {
      type: 'number', min: 1, max: 366, value: req.effective_days,
      style: { width: '78px', minHeight: '30px', padding: '4px 8px' },
      oninput: (e) => { req.effective_days = Number(e.target.value) || 100; },
    }),
    h('span', { class: 'small muted' }, 'hari (S, I, A di Excel otomatis dihitung jadi "98% (Present)")'));

  const body = h('div', { class: 'stack' },
    h('p', {}, 'File Excel ini berisi daftar siswa beserta data yang sudah kamu isi. File ini bisa diisi di Microsoft Excel ATAU dikirim ke guru lain untuk dibuka lewat tombol "Impor form" di halaman Isi rapor mereka (data siswa, nilai, dan isian yang sudah ada otomatis ikut terbawa).'),
    h('div', {}, h('div', { class: 'eyebrow', style: { marginBottom: '8px' } }, 'Kelas'),
      h('div', { class: 'grid-3' }, classes.map((c) => check(c, req.classes.includes(c), (on) => {
        req.classes = on ? [...req.classes, c] : req.classes.filter((x) => x !== c);
      })))),
    h('div', { class: 'stack', style: { gap: '10px' } }, h('div', { class: 'eyebrow' }, 'Kolom yang diisi'),
      check('ATL & Learner Profile (7 atribut)', req.include_atl, (on) => { req.include_atl = on; }),
      s.projects.length
        ? h('div', { class: 'row' }, check('Proyek: Understanding, Inquiry, dan narasi', withProject, (on) => { withProject = on; }), projectSelect)
        : null,
      s.projects.length
        ? h('div', { style: { paddingLeft: '23px' } }, check('Hanya siswa yang sudah diberi proyek ini', req.only_project_members, (on) => { req.only_project_members = on; }))
        : null,
      check('Kehadiran: kolom Sakit (S), Izin (I), Alpa (A) + rumus % otomatis', req.include_attendance, (on) => {
        req.include_attendance = on;
        effDaysWrap.style.display = on ? 'flex' : 'none';
      }),
      effDaysWrap),
    countEl);
  update();

  const modal = openModal({
    title: 'Buat form Excel',
    body,
    actions: [
      { label: 'Batal', kind: 'quiet', onClick: (c) => c() },
      { label: 'Simpan form…', kind: 'primary', onClick: async () => {
        if (!req.classes.length) { toast('Pilih minimal satu kelas', 'error'); return; }
        try {
          const saved = await api.saveForm({ ...req, project_id: withProject ? req.project_id : null });
          if (!saved) return; // dialog dibatalkan
          modal.setBody(h('div', { class: 'stack' },
            h('p', { class: 'note ok' }, h('b', {}, 'Form tersimpan. '), `${saved.students} siswa beserta isiannya sudah disertakan.`),
            h('p', { class: 'small tabular' }, saved.path),
            h('p', { class: 'muted' }, 'Kirim file ini ke guru/fasilitator lain. Guru lain cukup membuka menu Isi rapor lalu klik "Impor form" untuk melihat semua siswa dan isian yang sudah kamu buat (atau mengisinya langsung di Excel).')));
          modal.setActions([
            { label: 'Tampilkan di folder', onClick: () => api.revealPath(saved.path).catch((e) => toast(errorText(e), 'error')) },
            { label: 'Selesai', kind: 'primary', onClick: (c) => c() },
          ]);
        } catch (e) {
          toast(errorText(e), 'error');
        }
      } },
    ],
  });
}

/** Pilih form yang sudah diisi → pratinjau perubahan → terapkan. */
export async function importFormFlow(onDone) {
  let path;
  try { path = await api.pickExcelFile(); } catch (e) { toast(errorText(e), 'error'); return; }
  if (!path) return;
  const modal = openModal({ title: 'Impor form isian', body: h('p', { class: 'muted' }, 'Membaca form…'), wide: true });
  let rep;
  try {
    rep = await api.importForm(path, false);
  } catch (e) {
    modal.setBody(h('p', { class: 'note danger' }, errorText(e)));
    return;
  }
  modal.setBody(summary(rep, path));
  if (!rep.changed) {
    modal.setActions([{ label: 'Tutup', kind: 'primary', onClick: (c) => c() }]);
    return;
  }
  modal.setActions([
    { label: 'Batal', kind: 'quiet', onClick: (c) => c() },
    { label: `Terapkan ${rep.changed} perubahan`, kind: 'primary', onClick: async () => {
      modal.setActions([{ label: 'Menerapkan…', disabled: true }]);
      try {
        const done = await api.importForm(path, true);
        modal.setBody(summary(done, path));
        modal.setActions([{ label: 'Selesai', kind: 'primary', onClick: (c) => c() }]);
        onDone?.();
      } catch (e) {
        modal.setBody(h('p', { class: 'note danger' }, errorText(e)));
        modal.setActions([{ label: 'Tutup', onClick: (c) => c() }]);
      }
    } },
  ]);
}

function list(title, items, kind, max = 40) {
  if (!items.length) return null;
  return h('div', { class: `note ${kind}` }, h('b', {}, title),
    h('ul', {}, items.slice(0, max).map((t) => h('li', {}, t))),
    items.length > max ? h('p', { class: 'small' }, `…dan ${items.length - max} lainnya.`) : null);
}

function summary(rep, path) {
  const addedInfo = rep.students_added
    ? (rep.applied ? `${rep.students_added} siswa baru ditambahkan · ` : `${rep.students_added} siswa baru akan ditambahkan · `)
    : '';
  const head = rep.applied
    ? h('p', { class: 'note ok' }, h('b', {}, 'Diterapkan. '), `${addedInfo}${rep.values_set} isian dimuat untuk ${rep.students} siswa.`)
    : rep.changed
      ? h('p', {}, `${rep.students} siswa di form · ${addedInfo}${rep.values_set} isian terisi · `, h('b', {}, `${rep.changed} perubahan siap dimasukkan`), '. Periksa lalu klik Terapkan.')
      : h('p', { class: 'note' }, rep.values_set
        ? 'Tidak ada perubahan: semua data siswa dan isian di form ini sudah sama dengan data di aplikasi.'
        : 'Form ini belum memiliki perubahan baru.');
  return h('div', { class: 'stack' },
    h('p', { class: 'small muted tabular' }, rep.project ? `${rep.project} · ${path}` : path),
    head,
    rep.projects_added && !rep.applied ? h('p', { class: 'small muted' }, `${rep.projects_added} data proyek siswa ikut disertakan.`) : null,
    list('Isian lama yang diganti', rep.overwritten, 'warn'),
    list('Tidak terbaca (dilewati)', rep.invalid, 'danger'),
    list('Baris tanpa nama siswa (dilewati)', rep.unknown_rows, 'danger'),
    list('Catatan', rep.notes, 'warn'));
}
