// Data siswa: impor Dapodik, tambah/ubah/hapus siswa.
import { api } from '../api.js';
import { h, avatar, button, swapIn, classTabs, confirmBox, emptyState, errorText, link, openModal, searchBox, toast, toolbar } from '../dom.js';
import { importWizard } from '../import-wizard.js';

export async function render(ctx) {
  const { app, content, actions, setTitle } = ctx;
  setTitle('Data siswa', 'Diambil dari ekspor Dapodik. Yang disimpan hanya nama, NIS, NISN, jenis kelamin, dan kelas.');
  const rerender = async () => { await app.reload(); content.replaceChildren(); actions.replaceChildren(); await render(ctx); };

  const classes = app.classes();
  actions.append(
    ...(classes.length ? [button('Hapus kelas', { kind: 'danger', onClick: () => deleteClassesDialog(app, rerender) })] : []),
    button('Tambah siswa', { onClick: () => editStudent(app, null, rerender) }),
    button('Impor dari Excel', { kind: 'primary', onClick: () => importWizard('students', rerender) }),
  );

  if (!app.data.students.length) {
    content.appendChild(emptyState('Belum ada siswa',
      'Pilih file ekspor Dapodik (.xls atau .xlsx). Kelas IX dipilih otomatis; kelas lain bisa dicentang.',
      button('Impor dari Excel', { kind: 'primary', onClick: () => importWizard('students', rerender) })));
    return;
  }

  let filter = app.selectedClass && classes.includes(app.selectedClass) ? app.selectedClass : null;
  let query = '';
  const tabsBox = h('div', {});
  const tableBox = h('div', { class: 'sheet scroll' });

  function draw() {
    tabsBox.replaceChildren(classTabs(classes, filter, (c) => { filter = c; if (c) app.selectedClass = c; draw(); swapIn(tableBox); }, { counts: app.classCounts() }));
    const list = app.data.students
      .filter((s) => (!filter || s.class_name === filter) && (!query || s.name.toLowerCase().includes(query) || s.nis.includes(query) || s.nisn.includes(query)))
      .sort((a, b) => classes.indexOf(a.class_name) - classes.indexOf(b.class_name) || a.name.localeCompare(b.name, 'id'));
    tableBox.replaceChildren(list.length
      ? h('table', { class: 'ledger' },
        h('thead', {}, h('tr', {}, h('th', { class: 'idx' }, 'No'), ['Nama', 'NIS', 'NISN', 'L/P', 'Kelas'].map((t) => h('th', {}, t)), h('th', {}, ''))),
        h('tbody', {}, list.map((s, i) => h('tr', {},
          h('td', { class: 'idx' }, i + 1),
          h('td', { class: 'name' }, h('span', { class: 'who' }, avatar(s.name, 'sm'), s.name)),
          h('td', { class: 'tabular' }, s.nis || '—'),
          h('td', { class: 'tabular' }, s.nisn || '—'),
          h('td', {}, s.gender || '—'),
          h('td', {}, s.class_name),
          h('td', { class: 'row-actions' },
            link('Ubah', () => editStudent(app, s, rerender)),
            link('Hapus', async () => {
              const ok = await confirmBox(`Hapus ${s.name}? Nilai dan isian rapornya ikut terhapus.`, { ok: 'Hapus', danger: true });
              if (!ok) return;
              try { await api.deleteStudent(s.id); toast('Siswa dihapus', 'ok'); rerender(); } catch (e) { toast(errorText(e), 'error'); }
            }, 'danger'))))))
      : h('p', { class: 'muted', style: { padding: '20px' } }, 'Tidak ada siswa yang cocok.'));
    tableBox.scrollTop = 0;
  }
  draw();
  content.append(toolbar(tabsBox, searchBox('Cari nama, NIS, atau NISN', (q) => { query = q; draw(); })), tableBox);
}

function editStudent(app, s, onSaved) {
  const v = { id: s?.id ?? null, name: s?.name ?? '', nis: s?.nis ?? '', nisn: s?.nisn ?? '', gender: s?.gender ?? '', class_name: s?.class_name ?? app.selectedClass ?? '' };
  const field = (label, key, extra = {}) => h('label', { class: 'field' }, h('span', { class: 'lbl' }, label),
    h('input', { value: v[key], oninput: (e) => (v[key] = e.target.value), ...extra }));
  openModal({
    title: s ? 'Ubah data siswa' : 'Tambah siswa',
    body: h('div', { class: 'stack' },
      field('Nama lengkap', 'name'),
      h('div', { class: 'grid-2' }, field('NIS', 'nis'), field('NISN', 'nisn')),
      h('div', { class: 'grid-2' },
        h('label', { class: 'field' }, h('span', { class: 'lbl' }, 'Jenis kelamin'),
          h('select', { onchange: (e) => (v.gender = e.target.value) },
            [['', '—'], ['L', 'Laki-laki'], ['P', 'Perempuan']].map(([val, t]) => h('option', { value: val, selected: v.gender === val }, t)))),
        field('Kelas', 'class_name', { list: 'class-options', placeholder: 'mis. IX-1' })),
      h('datalist', { id: 'class-options' }, app.classes().map((c) => h('option', { value: c })))),
    actions: [
      { label: 'Batal', kind: 'quiet', onClick: (c) => c() },
      { label: 'Simpan', kind: 'primary', onClick: async (close) => {
        try { await api.saveStudentIdentity(v); toast('Tersimpan', 'ok'); close(); onSaved(); } catch (e) { toast(errorText(e), 'error'); }
      } },
    ],
  });
}

/** Dialog hapus kelas: pilih satu/beberapa kelas → konfirmasi → hapus (dengan backup otomatis). */
function deleteClassesDialog(app, onDone) {
  const classes = app.classes();
  const counts = app.classCounts();
  const chosen = new Set(app.selectedClass && classes.includes(app.selectedClass) ? [app.selectedClass] : []);
  const boxes = new Map();
  const summary = h('p', { class: 'hint' });

  const refresh = () => {
    for (const [name, input] of boxes) input.checked = chosen.has(name);
    const n = [...chosen].reduce((a, c) => a + (counts[c] || 0), 0);
    summary.textContent = chosen.size ? `Dipilih: ${chosen.size} kelas, ${n} siswa.` : 'Belum ada kelas yang dipilih.';
  };

  // Pintasan per tingkat, mis. "Semua kelas IX" (IX-1, IX-2, ...).
  const grades = [...new Set(classes.map((c) => c.split('-')[0].trim()))];
  const shortcuts = grades.length > 1 || classes.length > 1
    ? h('div', { class: 'row', style: { display: 'flex', flexWrap: 'wrap', gap: '8px' } },
      grades.map((g) => button(`Semua kelas ${g}`, { kind: 'quiet', onClick: () => {
        const group = classes.filter((c) => c.split('-')[0].trim() === g);
        const allOn = group.every((c) => chosen.has(c));
        for (const c of group) allOn ? chosen.delete(c) : chosen.add(c);
        refresh();
      } })))
    : null;

  const grid = h('div', { class: 'grid-3' }, classes.map((c) => {
    const input = h('input', { type: 'checkbox', onchange: (e) => { e.target.checked ? chosen.add(c) : chosen.delete(c); refresh(); } });
    boxes.set(c, input);
    return h('label', { class: 'check' }, input, `${c} `, h('span', { class: 'muted small' }, `(${counts[c] || 0} siswa)`));
  }));

  refresh();
  openModal({
    title: 'Hapus kelas',
    wide: true,
    body: h('div', { class: 'stack' },
      h('div', { class: 'note warn' },
        'Semua siswa di kelas yang dipilih akan dihapus, termasuk nilai, isian rapor, wali kelas, dan tanda tangannya. ',
        'Sebelum menghapus, data dibackup otomatis ke folder backup.'),
      shortcuts,
      grid,
      summary),
    actions: [
      { label: 'Batal', kind: 'quiet', onClick: (c) => c() },
      { label: 'Hapus kelas terpilih', kind: 'danger solid', onClick: async (close) => {
        const list = classes.filter((c) => chosen.has(c));
        if (!list.length) { toast('Pilih minimal satu kelas', 'error'); return; }
        const n = list.reduce((a, c) => a + (counts[c] || 0), 0);
        const ok = await confirmBox(`Yakin hapus ${list.length} kelas (${list.join(', ')}) beserta ${n} siswanya? Tindakan ini tidak bisa dibatalkan dari layar ini, tapi bisa dipulihkan dari backup.`,
          { title: 'Hapus kelas', ok: 'Ya, hapus', danger: true });
        if (!ok) return;
        try {
          const r = await api.deleteClasses(list);
          close();
          if (list.includes(app.selectedClass)) app.selectedClass = null;
          toast(`${r.classes_deleted || list.length} kelas dan ${r.students_deleted} siswa dihapus`, 'ok', 5000);
          onDone();
        } catch (e) { toast(errorText(e), 'error'); }
      } },
    ],
  });
}
