// Data siswa: impor Dapodik, tambah/ubah/hapus siswa.
import { api } from '../api.js';
import { h, avatar, button, swapIn, classTabs, confirmBox, emptyState, errorText, link, openModal, searchBox, toast, toolbar } from '../dom.js';
import { importWizard } from '../import-wizard.js';

export async function render(ctx) {
  const { app, content, actions, setTitle } = ctx;
  setTitle('Data siswa', 'Diambil dari ekspor Dapodik. Yang disimpan hanya nama, NIS, NISN, jenis kelamin, dan kelas.');
  const rerender = async () => { await app.reload(); content.replaceChildren(); actions.replaceChildren(); await render(ctx); };

  actions.append(
    button('Tambah siswa', { onClick: () => editStudent(app, null, rerender) }),
    button('Impor dari Excel', { kind: 'primary', onClick: () => importWizard('students', rerender) }),
  );

  const classes = app.classes();
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
