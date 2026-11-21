// Cetak: pilih siswa → pratinjau → cetak / simpan PDF.
import { api } from '../api.js';
import { h, button, swapIn, classTabs, debounce, emptyState, errorText, fitPreview, openModal, toast, toolbar } from '../dom.js';

export async function render(ctx) {
  const { app, content, actions, setTitle } = ctx;
  setTitle('Cetak', 'Satu siswa per lembar baru. Di dialog cetak, pilih printer atau Simpan sebagai PDF.');
  const classes = app.classes();
  if (!classes.length) {
    content.appendChild(emptyState('Belum ada siswa', 'Impor data siswa dulu.', button('Ke Data siswa', { onClick: () => app.go('siswa') })));
    return;
  }
  const cls = app.ensureClass();
  const views = await api.studentViews(cls);
  const selected = new Set(views.map((v) => v.student.id));

  const frame = h('iframe', { class: 'desk', title: 'Pratinjau rapor' });
  fitPreview(frame);
  actions.appendChild(button('Leger Excel…', { onClick: () => openLegerModal(app) }));
  actions.appendChild(button('PDF per siswa…', { onClick: () => {
    const ids = views.map((v) => v.student.id).filter((id) => selected.has(id));
    if (!ids.length) { toast('Pilih minimal satu siswa', 'error'); return; }
    exportPdfFlow(ids);
  } }));
  actions.appendChild(button('Cetak / Simpan PDF', { kind: 'primary', onClick: () => {
    if (!selected.size) { toast('Pilih minimal satu siswa', 'error'); return; }
    frame.contentWindow?.focus();
    frame.contentWindow?.print();
  } }));

  const runRefresh = async () => {
    const ids = views.map((v) => v.student.id).filter((id) => selected.has(id));
    if (!ids.length) {
      frame.srcdoc = '<p style="font:14px IBM Plex Sans,Segoe UI,sans-serif;padding:28px;color:#7B828C">Belum ada siswa yang dipilih.</p>';
      return;
    }
    try { frame.srcdoc = await api.renderReport(ids); } catch (e) { toast(errorText(e), 'error'); }
  };
  const refresh = debounce(runRefresh, 160);

  const countText = h('span', { class: 'small muted tabular' });
  const all = h('input', { type: 'checkbox', checked: true, 'aria-label': 'Pilih semua' });
  const boxes = [];
  const syncAll = () => {
    all.checked = selected.size === views.length;
    all.indeterminate = selected.size > 0 && selected.size < views.length;
    countText.textContent = `${selected.size} dari ${views.length}`;
  };
  all.addEventListener('change', () => {
    views.forEach((v, i) => { boxes[i].checked = all.checked; if (all.checked) selected.add(v.student.id); else selected.delete(v.student.id); });
    syncAll();
    refresh();
  });

  const list = h('div', { class: 'list' }, views.map((v) => {
    const cb = h('input', { type: 'checkbox', checked: true, onchange: (e) => {
      if (e.target.checked) selected.add(v.student.id); else selected.delete(v.student.id);
      syncAll();
      refresh();
    } });
    boxes.push(cb);
    return h('label', {}, cb, h('span', {}, v.student.name),
      v.issues.length ? h('span', { class: 'issues', title: v.issues.join('\n') }, `${v.issues.length} catatan`) : h('span', {}));
  }));
  syncAll();

  const incomplete = views.filter((v) => v.issues.length).length;
  let printLayout;
  content.append(
    toolbar(classTabs(classes, cls, (c) => { app.selectedClass = c; content.replaceChildren(); actions.replaceChildren(); render(ctx); }, { all: false, counts: app.classCounts() })),
    printLayout = h('div', { class: 'print-layout' },
      h('div', { class: 'print-side' },
        h('div', { class: 'sheet picklist' },
          h('label', { class: 'head check' }, all, h('b', {}, 'Pilih semua'), h('span', { class: 'spacer' }), countText),
          list),
        incomplete
          ? h('p', { class: 'note warn small' }, `${incomplete} rapor masih punya catatan. Arahkan kursor ke tulisan "catatan" untuk melihat detailnya, atau lengkapi di Isi rapor.`)
          : h('p', { class: 'note ok small' }, 'Semua rapor di kelas ini sudah lengkap.'),
        h('p', { class: 'small muted' }, `Kertas ${app.data.settings.paper_size}, skala 100%.`)),
      frame),
  );
  if (!ctx.entering) swapIn(printLayout);
  runRefresh();
}

/** Leger kelas ke Excel: pilih kelas → simpan. */
function openLegerModal(app) {
  const classes = app.classes();
  const counts = app.classCounts();
  const chosen = new Set(classes);
  const modal = openModal({
    title: 'Leger kelas ke Excel',
    body: h('div', { class: 'stack' },
      h('p', {}, 'Satu sheet per kelas berisi nilai, predikat, ATL, proyek, kehadiran, dan status rapor setiap siswa. Sheet "Rekap" berisi sebaran predikat per mapel, termasuk berapa nilai yang di bawah rentang.'),
      h('div', {}, h('div', { class: 'eyebrow', style: { marginBottom: '8px' } }, 'Kelas'),
        h('div', { class: 'grid-3' }, classes.map((c) => h('label', { class: 'check' },
          h('input', { type: 'checkbox', checked: true, onchange: (e) => (e.target.checked ? chosen.add(c) : chosen.delete(c)) }),
          c, h('span', { class: 'muted small' }, `(${counts[c] || 0})`)))))),
    actions: [
      { label: 'Batal', kind: 'quiet', onClick: (c) => c() },
      { label: 'Simpan leger…', kind: 'primary', onClick: async () => {
        if (!chosen.size) { toast('Pilih minimal satu kelas', 'error'); return; }
        try {
          const saved = await api.saveLeger(classes.filter((c) => chosen.has(c)));
          if (!saved) return;
          modal.setBody(h('div', { class: 'stack' },
            h('p', { class: 'note ok' }, h('b', {}, 'Leger tersimpan. '), `${saved.students} siswa.`),
            h('p', { class: 'small tabular' }, saved.path)));
          modal.setActions([
            { label: 'Tampilkan di folder', onClick: () => api.revealPath(saved.path).catch((e) => toast(errorText(e), 'error')) },
            { label: 'Selesai', kind: 'primary', onClick: (c) => c() },
          ]);
        } catch (e) { toast(errorText(e), 'error'); }
      } },
    ],
  });
}

/** Simpan rapor tiap siswa sebagai PDF terpisah (lewat Edge), dengan kemajuan. */
async function exportPdfFlow(ids) {
  let folder;
  try { folder = await api.pickFolder('Pilih folder untuk menyimpan PDF rapor'); } catch (e) { toast(errorText(e), 'error'); return; }
  if (!folder) return;
  const fill = h('i', { style: { width: '0%', transition: 'width .2s' } });
  const status = h('p', { class: 'small muted tabular' }, `Menyiapkan ${ids.length} PDF…`);
  const modal = openModal({
    title: 'Membuat PDF per siswa',
    body: h('div', { class: 'stack' }, status, h('span', { class: 'meter', style: { display: 'block', height: '4px' } }, fill),
      h('p', { class: 'hint' }, 'Satu file per siswa, siap dikirim ke orang tua. Biasanya kurang dari satu menit per kelas.')),
    actions: [{ label: 'Memproses…', disabled: true }],
  });
  const unlisten = await window.__TAURI__.event.listen('pdf-progress', (e) => {
    const { done, total, name } = e.payload;
    status.textContent = `${done} dari ${total} · ${name}`;
    fill.style.width = `${Math.round((done / total) * 100)}%`;
  });
  try {
    const res = await api.exportPdfs(ids, folder);
    modal.setBody(h('div', { class: 'stack' },
      h('p', { class: res.failed.length ? 'note warn' : 'note ok' }, h('b', {}, `${res.files} PDF tersimpan. `),
        res.failed.length ? `${res.failed.length} gagal.` : ''),
      h('p', { class: 'small tabular' }, res.folder),
      res.failed.length ? h('ul', { class: 'small' }, res.failed.map((f) => h('li', {}, f))) : null));
    modal.setActions([
      res.first_file ? { label: 'Tampilkan di folder', onClick: () => api.revealPath(res.first_file).catch((e) => toast(errorText(e), 'error')) } : null,
      { label: 'Selesai', kind: 'primary', onClick: (c) => c() },
    ].filter(Boolean));
  } catch (e) {
    modal.setBody(h('p', { class: 'note danger' }, errorText(e)));
    modal.setActions([{ label: 'Tutup', onClick: (c) => c() }]);
  } finally {
    unlisten();
  }
}
