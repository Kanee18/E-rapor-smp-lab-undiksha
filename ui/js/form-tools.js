// Form Excel untuk fasilitator / wali kelas: buat → diisi di Excel → impor kembali.
import { api } from './api.js';
import { h, button, errorText, openModal, toast } from './dom.js';

/** Dialog membuat form isian. */
export function openFormBuilder(app) {
  const s = app.data.settings;
  const classes = app.classes();
  const req = {
    classes: app.selectedClass ? [app.selectedClass] : classes.slice(0, 1),
    include_atl: true,
    project_id: s.projects[0]?.id ?? null,
    only_project_members: false,
    include_attendance: false,
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

  const body = h('div', { class: 'stack' },
    h('p', {}, 'Form berisi daftar siswa dengan kolom yang bisa diisi di Excel. Isian yang sudah ada ikut tercantum, jadi form boleh dibuat ulang kapan saja.'),
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
      check('Kehadiran (untuk wali kelas)', req.include_attendance, (on) => { req.include_attendance = on; })),
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
            h('p', { class: 'note ok' }, h('b', {}, 'Form tersimpan. '), `${saved.students} siswa.`),
            h('p', { class: 'small tabular' }, saved.path),
            h('p', { class: 'muted' }, 'Kirim file ini ke fasilitator. Setelah diisi dan dikirim kembali, gunakan tombol "Impor form".')));
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
  const head = rep.applied
    ? h('p', { class: 'note ok' }, h('b', {}, 'Diterapkan. '), `${rep.changed} isian diperbarui untuk ${rep.students} siswa.`)
    : rep.changed
      ? h('p', {}, `${rep.students} siswa ditemukan · ${rep.values_set} isian terisi · `, h('b', {}, `${rep.changed} berbeda dari data sekarang`), '. Periksa lalu klik Terapkan.')
      : h('p', { class: 'note' }, rep.values_set
        ? 'Tidak ada perubahan: isian di form ini sama dengan data sekarang.'
        : 'Form ini belum diisi (semua kolom isian kosong).');
  return h('div', { class: 'stack' },
    h('p', { class: 'small muted tabular' }, rep.project ? `${rep.project} · ${path}` : path),
    head,
    rep.projects_added && !rep.applied ? h('p', { class: 'small muted' }, `${rep.projects_added} siswa akan diberi proyek ini.`) : null,
    list('Isian lama yang diganti', rep.overwritten, 'warn'),
    list('Tidak terbaca (dilewati)', rep.invalid, 'danger'),
    list('Siswa tidak ditemukan (dilewati)', rep.unknown_rows, 'danger'),
    list('Catatan', rep.notes, 'warn'));
}
