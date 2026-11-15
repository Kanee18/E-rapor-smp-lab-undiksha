// Isi rapor: ATL, proyek (UOI), narasi, kehadiran, dan indikator khusus per siswa.
// Isian tersimpan otomatis sesaat setelah berhenti mengetik/mengklik.
import { api } from '../api.js';
import {
  h, autoGrow, avatar, button, icon, classTabs, localGrade, debounce, emptyState, errorText, fitPreview, fmtNum, link, openModal,
  rankMap, ratingControl, searchBox, stamp, swapIn, toast, toolbar,
} from '../dom.js';
import { importFormFlow, openFormBuilder } from '../form-tools.js';
import { appendSentence, openPhraseBank } from '../narrative.js';

export async function render(ctx) {
  const { app, content, actions, setTitle } = ctx;
  setTitle('Isi rapor', 'Penilaian ATL, proyek, dan narasi per siswa. Klik predikat atau ketik angka 0–100; semua tersimpan otomatis. Fasilitator juga bisa mengisi lewat form Excel.');
  const classes = app.classes();
  actions.replaceChildren();
  if (!classes.length) {
    content.appendChild(emptyState('Belum ada siswa', 'Impor data siswa dulu.', button('Ke Data siswa', { onClick: () => app.go('siswa') })));
    return;
  }
  const settings = app.data.settings;
  const ranks = rankMap(settings);
  const cls = app.ensureClass();
  let views = await api.studentViews(cls);
  let selectedId = views.some((v) => v.student.id === app.selectedStudentId) ? app.selectedStudentId : views[0]?.student.id;
  let query = '';

  // ---------- penyimpanan otomatis
  let pending = null; // { id, fields }
  const saveState = h('span', { class: 'save-state', 'aria-live': 'polite' });
  const setSaveState = (kind, text) => { saveState.className = `save-state ${kind}`; saveState.textContent = text; };
  let onSaved = () => {};

  async function saveNow() {
    if (!pending) return true;
    const { id, fields } = pending;
    pending = null;
    setSaveState('saving', 'Menyimpan…');
    try {
      const view = await api.saveReportFields(id, fields);
      views = views.map((v) => (v.student.id === id ? view : v));
      setSaveState('', 'Tersimpan');
      onSaved(view);
      return true;
    } catch (e) {
      setSaveState('error', 'Gagal menyimpan');
      toast(errorText(e), 'error');
      return false;
    }
  }
  const saveSoon = debounce(saveNow, 600);
  const queue = (id, fields) => { pending = { id, fields }; setSaveState('saving', 'Belum tersimpan'); saveSoon(); };
  const flush = async () => { saveSoon.cancel(); return saveNow(); };

  const rerender = async () => { await flush(); content.replaceChildren(); render(ctx); };

  actions.append(
    button('Impor form', { onClick: async () => { await flush(); importFormFlow(async () => { await app.reload(); rerender(); }); } }),
    button('Buat form Excel', { kind: 'primary', onClick: async () => { await flush(); openFormBuilder(app); } }),
  );

  // ---------- kerangka
  const projectPick = h('select', { style: { width: 'auto' }, 'aria-label': 'Proyek' },
    settings.projects.map((p) => h('option', { value: p.id }, projectLabel(p))));
  content.appendChild(toolbar(
    classTabs(classes, cls, (c) => { app.selectedClass = c; rerender(); }, { all: false, counts: app.classCounts() }),
    h('span', { class: 'small muted' }, `Beri proyek ke semua siswa ${cls}`), projectPick,
    button('Terapkan', { kind: 'sm', onClick: async () => {
      await flush();
      try {
        const n = await api.assignProject(cls, projectPick.value);
        toast(n ? `Proyek ditambahkan ke ${n} siswa` : 'Semua siswa sudah punya proyek itu', 'ok');
        app.selectedStudentId = selectedId;
        rerender();
      } catch (e) { toast(errorText(e), 'error'); }
    } })));

  // Daftar siswa dan formulir masing-masing punya scroll sendiri; kepala formulir
  // (nama, status simpan, navigasi) selalu terlihat.
  const listBox = h('div', { class: 'index-list' });
  const docHead = h('div', { class: 'doc-head' });
  const docBody = h('div', { class: 'doc-body' });
  const docBox = h('div', { class: 'sheet doc' }, docHead, docBody);
  const split = content.appendChild(h('div', { class: 'split' },
    h('div', { class: 'sheet index' }, searchBox('Cari siswa', (q) => { query = q; drawList(); }), listBox),
    docBox));
  if (!ctx.entering) swapIn(split);

  const visible = () => views.filter((v) => !query || v.student.name.toLowerCase().includes(query));
  /** Siswa sebelum/sesudah yang sedang dibuka, mengikuti urutan daftar yang tampil. */
  const neighbour = (step) => {
    const list = visible().some((v) => v.student.id === selectedId) ? visible() : views;
    const i = list.findIndex((v) => v.student.id === selectedId);
    return i < 0 ? null : list[i + step] ?? null;
  };
  async function select(id) {
    if (!id || id === selectedId) return;
    await flush();
    selectedId = id;
    app.selectedStudentId = id;
    drawList(true);
    drawDoc();
    swapIn(docBody);
  }

  function drawList(reveal = false) {
    listBox.replaceChildren(...visible().map((v) => {
      const p = v.progress;
      const done = v.issues.length === 0;
      const started = p.atl_done > 0 || p.projects > 0 || p.attendance;
      return h('button', {
        type: 'button', class: `index-item${v.student.id === selectedId ? ' active' : ''}`,
        onclick: () => select(v.student.id),
      },
      avatar(v.student.name),
      h('span', { class: 'nm' }, v.student.name),
      h('span', { class: `dot${done ? ' done' : started ? ' part' : ''}`, title: done ? 'Lengkap' : `${v.issues.length} catatan` }),
      h('span', { class: 'st' }, done ? 'Lengkap' : `ATL ${p.atl_done}/${p.atl_total} · proyek ${p.projects_done}/${p.projects}`));
    }));
    // hanya saat pindah siswa; setelah menyimpan, posisi scroll daftar dibiarkan
    if (reveal) listBox.querySelector('.index-item.active')?.scrollIntoView({ block: 'nearest' });
  }

  function drawDoc() {
    const v = views.find((x) => x.student.id === selectedId);
    if (!v) { docHead.replaceChildren(); docBody.replaceChildren(h('p', { class: 'muted', style: { paddingTop: '24px' } }, 'Pilih siswa di sebelah kiri.')); return; }
    const s = v.student;
    const fields = {
      attendance: s.attendance,
      atl: Object.fromEntries(settings.atl.map((a) => [a.id, { grade: s.atl[a.id]?.grade ?? '', indicator: s.atl[a.id]?.indicator ?? '' }])),
      projects: s.projects.map((p) => ({ ...p })),
      subject_notes: { ...s.subject_notes },
    };
    const changed = () => queue(s.id, structuredClone(fields));

    const issuesBox = h('div', {});
    const drawIssues = (view) => issuesBox.replaceChildren(view.issues.length
      ? h('div', { class: 'note warn', style: { marginTop: '16px' } }, h('b', {}, `${view.issues.length} hal belum lengkap`),
        h('ul', {}, view.issues.map((t) => h('li', {}, t))))
      : h('p', { class: 'note ok', style: { marginTop: '16px' } }, 'Rapor siswa ini sudah lengkap.'));
    onSaved = (view) => {
      if (view.student.id !== selectedId) return;
      drawIssues(view);
      drawList();
    };

    // indikator: teks bawaan + ubah khusus siswa ini
    const indicatorEditor = (defaultText, current, onChange) => {
      const wrap = h('div', { class: 'ind' });
      const show = (editing) => {
        if (editing) {
          const ta = autoGrow(h('textarea', { value: current, placeholder: defaultText, 'aria-label': 'Indikator khusus', oninput: (e) => { current = e.target.value; onChange(current); } }));
          wrap.replaceChildren(ta, link('Pakai indikator bawaan', () => { current = ''; onChange(''); show(false); }, 'muted'));
          ta.focus();
        } else {
          wrap.replaceChildren(
            h('div', { class: current ? 'custom' : '' }, current || defaultText || '—'),
            link(current ? 'Ubah indikator khusus' : 'Ubah untuk siswa ini', () => show(true), 'muted'));
        }
      };
      show(false);
      return wrap;
    };

    // ---------- 1. ATL
    const atlRows = settings.atl.map((a) => h('div', { class: 'rate-row' },
      h('div', { class: 'nm' }, a.name),
      ratingControl(fields.atl[a.id].grade, settings.grading, ranks, (val) => { fields.atl[a.id].grade = val; changed(); }, { label: a.name }),
      indicatorEditor(a.indicator, fields.atl[a.id].indicator, (val) => { fields.atl[a.id].indicator = val; changed(); })));

    // narasi + bank kalimat
    const narrativeField = (p) => {
      const ta = autoGrow(h('textarea', { rows: 3, value: p.narrative, placeholder: 'Narasi dari fasilitator, dalam bahasa Inggris', oninput: (e) => { p.narrative = e.target.value; changed(); } }));
      return h('label', { class: 'field' }, h('span', { class: 'lbl' }, 'Teacher Narrative Feedback'), ta,
        h('div', { class: 'narr-tools' },
          h('span', { class: 'muted' }, 'Bahasa Inggris, 2–4 kalimat: yang sudah baik dan langkah berikutnya.'),
          link('Bank kalimat…', () => openPhraseBank({
            settings, student: s,
            theme: settings.projects.find((pc) => pc.id === p.project_id)?.theme,
            predicate: localGrade(p.understanding, settings.grading).code,
            getText: () => ta.value,
            onPick: (sentence) => { ta.value = appendSentence(ta.value, sentence); ta.dispatchEvent(new Event('input')); },
          }))));
    };

    // ---------- 2. proyek
    const projBox = h('div', {});
    const drawProjects = () => {
      projBox.replaceChildren();
      if (!fields.projects.length) projBox.appendChild(h('p', { class: 'muted', style: { marginTop: '8px' } }, 'Belum ada proyek untuk siswa ini.'));
      fields.projects.forEach((p, i) => {
        projBox.appendChild(h('div', { class: 'project' },
          h('div', { class: 'project-head' },
            h('select', { 'aria-label': 'Proyek', onchange: (e) => { p.project_id = e.target.value; changed(); } },
              settings.projects.map((pc) => h('option', { value: pc.id, selected: pc.id === p.project_id }, projectLabel(pc))),
              settings.projects.some((pc) => pc.id === p.project_id) ? null : h('option', { value: p.project_id, selected: true }, '(proyek sudah dihapus)')),
            h('span', { class: 'spacer' }),
            link('Hapus proyek', () => { fields.projects.splice(i, 1); drawProjects(); changed(); }, 'danger')),
          h('div', { class: 'pair' },
            h('span', { class: 'lbl' }, 'Understanding Level'), h('span', { class: 'lbl' }, 'Inquiry & Research Skills'),
            ratingControl(p.understanding, settings.grading, ranks, (val) => { p.understanding = val; changed(); }, { label: 'Understanding Level' }),
            ratingControl(p.inquiry, settings.grading, ranks, (val) => { p.inquiry = val; changed(); }, { label: 'Inquiry & Research Skills' })),
          narrativeField(p)));
      });
    };
    drawProjects();
    const addProject = link('+ Tambah proyek', () => {
      const used = new Set(fields.projects.map((p) => p.project_id));
      const next = settings.projects.find((p) => !used.has(p.id)) || settings.projects[0];
      if (!next) { toast('Belum ada proyek di Pengaturan', 'error'); return; }
      fields.projects.push({ project_id: next.id, understanding: '', inquiry: '', narrative: '' });
      drawProjects();
      changed();
    });

    // ---------- 3. mapel (nilai dari halaman Nilai mapel)
    const subjectRows = v.subjects.map((sv) => h('div', { class: 'rate-row' },
      h('div', { class: 'nm' }, sv.name),
      h('div', { class: 'score-cell' }, stamp(sv.result, ranks),
        sv.result.score !== null ? `nilai ${fmtNum(sv.result.score)}` : h('span', { class: 'small' }, sv.result.message || '—')),
      indicatorEditor(settings.subjects.find((x) => x.id === sv.subject_id)?.indicator ?? '', fields.subject_notes[sv.subject_id] ?? '',
        (val) => { fields.subject_notes[sv.subject_id] = val; changed(); })));

    const section = (no, title, lede, ...body) => h('section', { class: 'doc-sec' },
      h('h3', {}, h('span', { class: 'no' }, no), title), lede ? h('p', { class: 'lede' }, lede) : null, ...body);

    const prev = neighbour(-1);
    const next = neighbour(1);
    const pos = visible().findIndex((x) => x.student.id === s.id);
    const navBtn = (iconName, label, target) => h('button', {
      type: 'button', class: 'icon-btn', title: target ? `${label}: ${target.student.name}` : label, 'aria-label': label,
      disabled: !target, onclick: () => select(target?.student.id),
    }, icon(iconName));
    docHead.replaceChildren(
      avatar(s.name, 'lg'),
      h('div', { class: 'who' },
        h('h2', {}, s.name),
        h('div', { class: 'meta' }, `${s.class_name} · NIS ${s.nis || '—'} · NISN ${s.nisn || '—'}`)),
      h('div', { class: 'side' }, saveState,
        h('div', { class: 'stepnav' },
          navBtn('up', 'Siswa sebelumnya', prev),
          h('span', { class: 'tabular' }, pos >= 0 ? `${pos + 1}/${visible().length}` : ''),
          navBtn('down', 'Siswa berikutnya', next)),
        button('Pratinjau', { onClick: async () => { await flush(); previewModal(s.id, s.name); } })));
    docBody.replaceChildren(
      issuesBox,
      h('div', { class: 'doc-sec' },
        h('label', { class: 'field', style: { maxWidth: '280px' } }, h('span', { class: 'lbl' }, 'Kehadiran (Attendance)'),
          h('input', { value: fields.attendance, placeholder: 'mis. 98% (Present)', oninput: (e) => { fields.attendance = e.target.value; changed(); } }))),
      section('1', 'Approaches to Learning & Learner Profile', null, atlRows),
      section('2', 'Transdisciplinary Unit of Inquiry', null, projBox, h('div', { style: { marginTop: '12px' } }, addProject)),
      section('3', 'Core Subject Progress & Evaluation', 'Nilai diambil dari halaman Nilai mapel. Indikator boleh diubah khusus untuk siswa ini.', subjectRows),
      h('div', { class: 'doc-foot' },
        h('span', { class: 'small muted' }, 'Semua isian tersimpan otomatis.'),
        next
          ? h('button', { type: 'button', class: 'btn primary', onclick: () => select(next.student.id) }, `Lanjut ke ${next.student.name}`, icon('arrow'))
          : h('span', { class: 'small muted' }, 'Ini siswa terakhir di daftar.')),
    );
    docBody.scrollTop = 0;
    setSaveState('', '');
    drawIssues(v);
  }

  drawList(true);
  drawDoc();
  ctx.onLeave(flush);
}

export function projectLabel(p) {
  return p.theme ? `${p.name} — ${p.theme}` : p.name;
}

async function previewModal(id, name) {
  let html;
  try { html = await api.renderReport([id]); } catch (e) { toast(errorText(e), 'error'); return; }
  const frame = h('iframe', { class: 'desk', style: { height: '68vh' }, title: `Pratinjau rapor ${name}` });
  fitPreview(frame);
  frame.srcdoc = html;
  openModal({
    title: name,
    wide: true,
    body: frame,
    actions: [
      { label: 'Tutup', kind: 'quiet', onClick: (c) => c() },
      { label: 'Cetak / Simpan PDF', kind: 'primary', onClick: () => frame.contentWindow?.print() },
    ],
  });
}
