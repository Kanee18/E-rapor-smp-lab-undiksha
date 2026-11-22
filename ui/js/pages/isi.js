// Isi rapor: ATL, proyek (UOI), narasi, kehadiran, dan indikator khusus per siswa.
// Tersedia dua tampilan: "Per siswa" (lengkap dengan indikator) dan "Tabel kelas" (cepat seperti spreadsheet).
// Isian tersimpan otomatis sesaat setelah berhenti mengetik/mengklik.
import { api } from '../api.js';
import {
  h, autoGrow, avatar, button, icon, classTabs, confirmBox, expandGradeCode, localGrade, debounce, emptyState, errorText, fitPreview, fmtNum, link, openModal,
  rankMap, ratingControl, searchBox, stamp, swapIn, toast, toolbar,
} from '../dom.js';
import { importFormFlow, openFormBuilder } from '../form-tools.js';
import { importWizard } from '../import-wizard.js';
import { appendSentence, displayName, draftNarrative, openPhraseBank, phraseBankList } from '../narrative.js';

export async function render(ctx) {
  const { app, content, actions, setTitle } = ctx;
  setTitle('Isi rapor', 'Penilaian ATL, proyek, narasi, dan kehadiran. Isi per siswa atau sekaligus satu kelas di Tabel kelas; semua tersimpan otomatis.');
  const classes = app.classes();
  actions.replaceChildren();
  const reloadAll = async () => { await app.reload(); content.replaceChildren(); render(ctx); };
  if (!classes.length) {
    actions.append(
      button('Impor form', { kind: 'primary', onClick: () => importFormFlow(reloadAll) }),
    );
    content.appendChild(emptyState(
      'Belum ada siswa',
      'Impor file Form Excel (.xlsx) yang dikirim oleh guru lain untuk langsung memuat daftar siswa beserta isian rapornya, atau impor data awal dari Dapodik.',
      h('div', { class: 'row', style: { display: 'flex', gap: '10px', justifyContent: 'center', flexWrap: 'wrap' } },
        button('Impor form dari guru lain', { kind: 'primary', onClick: () => importFormFlow(reloadAll) }),
        button('Ke Data siswa', { onClick: () => app.go('siswa') })),
    ));
    return;
  }
  const settings = app.data.settings;
  const ranks = rankMap(settings);
  const cls = app.ensureClass();
  // "Belum lengkap" = isian yang memang diisi guru di halaman ini: ATL, proyek (nilai + narasi), kehadiran.
  // (Nilai mapel di bawah rentang tidak dihitung: itu bukan sesuatu yang bisa dilengkapi di sini.)
  const isiIncomplete = (v) => {
    const p = v.progress;
    return p.atl_done < p.atl_total
      || (settings.projects.length > 0 && (p.projects === 0 || p.projects_done < p.projects))
      || !p.attendance;
  };
  let onlyIncomplete = Boolean(app.isiOnlyIncomplete);
  /** Draf narasi untuk satu proyek siswa; '' kalau Understanding Level belum diisi. */
  const makeDraft = (student, p) => draftNarrative({
    settings, student, projectId: p.project_id, predicate: localGrade(p.understanding, settings.grading).code,
  });
  let views = await api.studentViews(cls);
  let selectedId = views.some((v) => v.student.id === app.selectedStudentId) ? app.selectedStudentId : views[0]?.student.id;
  let mode = app.isiMode || 'student'; // 'student' | 'table'

  // ---------- penyimpanan otomatis (mendukung banyak siswa sekaligus di mode tabel)
  const pending = new Map(); // studentId -> fields
  const saveState = h('span', { class: 'save-state', 'aria-live': 'polite' });
  const setSaveState = (kind, text) => { saveState.className = `save-state ${kind}`; saveState.textContent = text; };
  let onSaved = () => {};

  async function saveNow() {
    if (!pending.size) return true;
    const entries = [...pending.entries()];
    pending.clear();
    setSaveState('saving', 'Menyimpan…');
    for (let k = 0; k < entries.length; k++) {
      const [id, fields] = entries[k];
      try {
        const view = await api.saveReportFields(id, fields);
        views = views.map((v) => (v.student.id === id ? view : v));
        onSaved(view);
      } catch (e) {
        // Kembalikan yang gagal + yang belum sempat disimpan ke antrean (kecuali sudah ada
        // isian yang lebih baru untuk siswa itu), supaya tidak ada isian yang hilang diam-diam.
        for (const [rid, rfields] of entries.slice(k)) if (!pending.has(rid)) pending.set(rid, rfields);
        setSaveState('error', 'Gagal menyimpan');
        toast(`${errorText(e)} — isian belum tersimpan, akan dicoba lagi.`, 'error');
        return false;
      }
    }
    setSaveState(pending.size ? 'saving' : '', pending.size ? 'Belum tersimpan' : 'Tersimpan');
    return true;
  }
  const saveSoon = debounce(saveNow, 500);
  const queue = (id, fields) => { pending.set(id, structuredClone(fields)); setSaveState('saving', 'Belum tersimpan'); saveSoon(); };
  const flush = async () => { saveSoon.cancel(); return saveNow(); };

  const rerender = async () => { await flush(); content.replaceChildren(); render(ctx); };

  actions.append(
    button('Impor absensi Excel', { onClick: async () => { await flush(); importWizard('attendance', async () => { await app.reload(); rerender(); }, app); } }),
    button('Impor form', { onClick: async () => { await flush(); importFormFlow(async () => { await app.reload(); rerender(); }); } }),
    button('Buat form Excel', { kind: 'primary', onClick: async () => { await flush(); openFormBuilder(app); } }),
  );

  // ---------- bilah atas: tab kelas + pemilih mode + terapkan proyek
  const modeBox = h('div', { class: 'tabs', role: 'tablist', 'aria-label': 'Tampilan' });
  const projectPick = h('select', { style: { width: 'auto' }, 'aria-label': 'Proyek' },
    settings.projects.map((p) => h('option', { value: p.id }, projectLabel(p))));

  // Pilih default projectPick dari proyek yang paling banyak dipakai di kelas ini
  const firstProjId = views.find((v) => v.student.projects.length)?.student.projects[0]?.project_id;
  if (firstProjId && settings.projects.some((p) => p.id === firstProjId)) projectPick.value = firstProjId;

  const topBar = toolbar(
    h('div', { style: { display: 'flex', alignItems: 'center', gap: '12px', flexWrap: 'wrap' } },
      classTabs(classes, cls, (c) => { app.selectedClass = c; rerender(); }, { all: false, counts: app.classCounts() }),
      modeBox),
    h('span', { class: 'small muted' }, `Beri proyek ke semua siswa ${cls}`), projectPick,
    button('Terapkan', { kind: 'sm', onClick: async () => {
      await flush();
      try {
        const n = await api.assignProject(cls, projectPick.value);
        toast(n ? `Proyek ditambahkan ke ${n} siswa` : 'Semua siswa sudah punya proyek itu', 'ok');
        app.selectedStudentId = selectedId;
        rerender();
      } catch (e) { toast(errorText(e), 'error'); }
    } }));
  content.appendChild(topBar);

  async function switchMode(nextMode, focusStudentId = null) {
    await flush();
    if (focusStudentId) {
      selectedId = focusStudentId;
      app.selectedStudentId = focusStudentId;
    }
    mode = nextMode;
    app.isiMode = nextMode;
    drawModeTabs();
    drawStage(true);
  }

  function drawModeTabs() {
    modeBox.replaceChildren(
      ...[['student', 'Per siswa'], ['table', 'Tabel kelas']].map(([id, label]) =>
        h('button', {
          type: 'button', role: 'tab', class: `tab${mode === id ? ' active' : ''}`, 'aria-selected': String(mode === id),
          onclick: () => { if (mode !== id) switchMode(id); },
        }, label)),
    );
  }

  function drawStage(animate = false) {
    while (content.lastElementChild && content.lastElementChild !== topBar) {
      content.lastElementChild.remove();
    }
    setSaveState('', '');
    const mainNode = mode === 'table' ? renderTableMode() : renderStudentMode();
    if (animate || !ctx.entering) swapIn(mainNode);
  }

  // ================================================================ MODE 1: PER SISWA
  function renderStudentMode() {
    let query = '';
    const listBox = h('div', { class: 'index-list' });
    const filterBar = h('div', { class: 'index-filter' });
    const docHead = h('div', { class: 'doc-head' });
    const docBody = h('div', { class: 'doc-body' });
    const docBox = h('div', { class: 'sheet doc' }, docHead, docBody);
    const split = content.appendChild(h('div', { class: 'split' },
      h('div', { class: 'sheet index' }, searchBox('Cari siswa', (q) => { query = q; drawList(); }), filterBar, listBox),
      docBox));

    // Siswa yang sedang dibuka tetap tampil walaupun sudah lengkap, supaya tidak "hilang" saat diisi.
    const visible = () => views.filter((v) => (!query || v.student.name.toLowerCase().includes(query))
      && (!onlyIncomplete || isiIncomplete(v) || v.student.id === selectedId));
    /** Siswa berikutnya (urutan kelas, berputar) yang isiannya belum lengkap. */
    const nextIncomplete = () => {
      const i = views.findIndex((v) => v.student.id === selectedId);
      for (let k = 1; k < views.length; k++) {
        const v = views[(i + k + views.length) % views.length];
        if (isiIncomplete(v)) return v;
      }
      return null;
    };
    function drawFilter() {
      const todo = views.filter(isiIncomplete).length;
      filterBar.replaceChildren(h('div', { class: 'segs' },
        [[false, `Semua ${views.length}`], [true, `Belum lengkap ${todo}`]].map(([val, label]) => h('button', {
          type: 'button', class: `seg wide${onlyIncomplete === val ? ' on' : ''}`, 'aria-pressed': String(onlyIncomplete === val),
          onclick: async () => {
            if (onlyIncomplete === val) return;
            await flush();
            onlyIncomplete = val;
            app.isiOnlyIncomplete = val;
            drawList(true);
            drawDoc();
          },
        }, label))));
    }
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
      if (!listBox.children.length) {
        listBox.appendChild(h('p', { class: 'muted small', style: { padding: '14px 10px' } },
          onlyIncomplete ? 'Semua siswa di kelas ini sudah lengkap.' : 'Tidak ada siswa yang cocok.'));
      }
      drawFilter();
      if (reveal) listBox.querySelector('.index-item.active')?.scrollIntoView({ block: 'nearest' });
    }

    function drawDoc() {
      const v = views.find((x) => x.student.id === selectedId);
      if (!v) { docHead.replaceChildren(); docBody.replaceChildren(h('p', { class: 'muted', style: { paddingTop: '24px' } }, 'Pilih siswa di sebelah kiri.')); return; }
      const s = v.student;
      const fields = makeFields(s, settings);
      const changed = () => queue(s.id, fields);

      const issuesBox = h('div', {});
      const drawIssues = (view) => issuesBox.replaceChildren(view.issues.length
        ? h('div', { class: 'note warn', style: { marginTop: '16px' } }, h('b', {}, `${view.issues.length} hal belum lengkap`),
          h('ul', {}, view.issues.map((t) => h('li', {}, t))))
        : h('p', { class: 'note ok', style: { marginTop: '16px' } }, 'Rapor siswa ini sudah lengkap.'));
      onSaved = (view) => {
        if (view.student.id === selectedId) drawIssues(view);
        drawList();
        drawFootText();
      };

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

      const atlRows = settings.atl.map((a) => h('div', { class: 'rate-row' },
        h('div', { class: 'nm' }, a.name),
        ratingControl(fields.atl[a.id].grade, settings.grading, ranks, (val) => { fields.atl[a.id].grade = val; changed(); }, { label: a.name }),
        indicatorEditor(a.indicator, fields.atl[a.id].indicator, (val) => { fields.atl[a.id].indicator = val; changed(); })));

      const narrativeField = (p) => {
        const ta = autoGrow(h('textarea', { rows: 3, value: p.narrative, placeholder: 'Narasi dari fasilitator, dalam bahasa Inggris', oninput: (e) => { p.narrative = e.target.value; changed(); } }));
        return h('label', { class: 'field' }, h('span', { class: 'lbl' }, 'Teacher Narrative Feedback'), ta,
          h('div', { class: 'narr-tools' },
            h('span', { class: 'muted' }, 'Bahasa Inggris, 2–4 kalimat: yang sudah baik dan langkah berikutnya.'),
            h('span', { class: 'row', style: { gap: '14px' } },
            link('Draf otomatis', async () => {
              const draft = makeDraft(s, p);
              if (!draft) { toast('Isi Understanding Level dulu, supaya draf sesuai pencapaian siswa', 'error'); return; }
              if (ta.value.trim() && !(await confirmBox('Ganti narasi yang sudah ada dengan draf baru?', { ok: 'Ganti' }))) return;
              ta.value = draft;
              ta.dispatchEvent(new Event('input'));
              toast('Draf narasi dibuat. Baca dan sesuaikan dengan pengamatan Anda.', 'ok');
            }),
            link('Bank kalimat…', () => openPhraseBank({
              settings, student: s,
              theme: settings.projects.find((pc) => pc.id === p.project_id)?.theme,
              predicate: localGrade(p.understanding, settings.grading).code,
              getText: () => ta.value,
              onPick: (sentence) => { ta.value = appendSentence(ta.value, sentence); ta.dispatchEvent(new Event('input')); },
            })))));
      };

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
      const nextTodo = nextIncomplete();
      const footText = h('span', { class: 'small muted' });
      const drawFootText = () => {
        const todo = views.filter(isiIncomplete).length;
        footText.textContent = todo
          ? `${todo} siswa di ${cls} belum lengkap · tersimpan otomatis`
          : `Semua siswa di ${cls} sudah lengkap · tersimpan otomatis`;
      };
      drawFootText();
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
      const effDays = settings.effective_days || 100;
      const attTextInput = h('input', {
        value: fields.attendance, placeholder: 'mis. 98% (Present)',
        oninput: (e) => { fields.attendance = e.target.value; changed(); },
      });
      const makeSiaField = (label, key) => h('label', { class: 'field', style: { width: '92px' } },
        h('span', { class: 'lbl' }, label),
        h('input', {
          type: 'number', min: 0, max: 366, value: fields[key] ?? '', placeholder: '0',
          oninput: (e) => {
            fields[key] = parseDayInput(e.target.value);
            const calc = calcAttendanceSia(fields.attendance_s, fields.attendance_i, fields.attendance_a, effDays);
            if (calc !== null) {
              fields.attendance = calc;
              attTextInput.value = calc;
            }
            changed();
          },
        }));

      docBody.replaceChildren(
        issuesBox,
        h('div', { class: 'doc-sec' },
          h('div', { style: { display: 'flex', alignItems: 'flex-end', gap: '12px', flexWrap: 'wrap' } },
            makeSiaField('Sakit (S)', 'attendance_s'),
            makeSiaField('Izin (I)', 'attendance_i'),
            makeSiaField('Alpa (A)', 'attendance_a'),
            h('label', { class: 'field', style: { width: '210px' } },
              h('span', { class: 'lbl' }, 'Kehadiran di rapor'),
              attTextInput)),
          h('p', { class: 'small muted', style: { marginTop: '6px' } },
            `Isi jumlah hari S / I / A untuk menghitung otomatis dari ${effDays} hari efektif (`,
            link('ubah hari efektif', async () => { await flush(); editEffectiveDaysDialog(app, rerender); }),
            '), atau ketik langsung teks kehadiran di rapor.')),
        section('1', 'Approaches to Learning & Learner Profile', null, atlRows),
        section('2', 'Transdisciplinary Unit of Inquiry', null, projBox, h('div', { style: { marginTop: '12px' } }, addProject)),
        section('3', 'Core Subject Progress & Evaluation', 'Nilai diambil dari halaman Nilai mapel. Indikator boleh diubah khusus untuk siswa ini.', subjectRows),
        h('div', { class: 'doc-foot' },
          footText,
          h('div', { class: 'row' },
            nextTodo && nextTodo.student.id !== next?.student.id
              ? h('button', { type: 'button', class: 'btn', title: 'Lompat ke siswa berikutnya yang isiannya belum lengkap', onclick: () => select(nextTodo.student.id) },
                `Ke yang belum lengkap: ${displayName(nextTodo.student.name)}`)
              : null,
            next
              ? h('button', { type: 'button', class: 'btn primary', onclick: () => select(next.student.id) }, `Lanjut ke ${next.student.name}`, icon('arrow'))
              : h('span', { class: 'small muted' }, 'Ini siswa terakhir di daftar.'))),
      );
      docBody.scrollTop = 0;
      setSaveState('', '');
      drawIssues(v);
    }

    drawList(true);
    drawDoc();
    return split;
  }

  // ================================================================ MODE 2: TABEL KELAS
  function renderTableMode() {
    const g = settings.grading;
    const hasAtl = settings.atl.length > 0;
    const hasProj = settings.projects.length > 0;

    const effDays = settings.effective_days || 100;
    const legend = h('div', { class: 'legend' },
      ...[...g.bands].sort((a, b) => b.min - a.min).map((b) => h('span', {}, stamp({ status: 'ok', code: b.code }, ranks), `≥ ${b.min}`)),
      h('span', { class: 'muted' }, '· Ketik E/M/A/B atau 0–100 · S/I/A otomatis hitung % dari '),
      link(`${effDays} hari efektif`, async () => { await flush(); editEffectiveDaysDialog(app, rerender); }));

    const rows = [];
    const rowById = new Map();

    // Penyaring baris: pencarian + "hanya yang belum lengkap". Daftar "belum lengkap" diambil saat
    // dicentang, jadi baris tidak tiba-tiba hilang ketika gurunya baru selesai mengisi.
    let searchQ = '';
    let todoSet = null;
    const applyRowFilter = () => {
      for (const r of rows) {
        r.tr.hidden = Boolean((searchQ && !r.view.student.name.toLowerCase().includes(searchQ))
          || (todoSet && !todoSet.has(r.view.student.id)));
      }
    };
    const setOnlyIncomplete = (on) => {
      onlyIncomplete = on;
      app.isiOnlyIncomplete = on;
      todoSet = on ? new Set(rows.filter((r) => isiIncomplete(r.view)).map((r) => r.view.student.id)) : null;
      applyRowFilter();
      onlyLabel.lastChild.textContent = on ? ` Hanya yang belum lengkap (${todoSet.size})` : ' Hanya yang belum lengkap';
    };
    const onlyLabel = h('label', { class: 'check small', title: 'Tampilkan siswa yang ATL, proyek, atau kehadirannya belum lengkap' },
      h('input', { type: 'checkbox', checked: onlyIncomplete, onchange: (e) => setOnlyIncomplete(e.target.checked) }),
      h('span', {}, ' Hanya yang belum lengkap'));
    const draftAllBtn = hasProj
      ? button('Draf narasi kosong…', { kind: 'sm', onClick: () => draftAllDialog() })
      : null;

    const subbar = h('div', { class: 'table-subbar' },
      legend,
      h('div', { style: { display: 'flex', alignItems: 'center', gap: '12px', flexWrap: 'wrap' } },
        saveState,
        onlyLabel,
        draftAllBtn,
        searchBox('Cari siswa', (q) => { searchQ = q; applyRowFilter(); })));

    const head1 = h('tr', {},
      h('th', { class: 'sticky idx', rowSpan: 2 }, 'No'),
      h('th', { class: 'sticky2', rowSpan: 2 }, 'Nama'),
      hasAtl ? h('th', { class: 'group gl', colSpan: settings.atl.length }, '1. ATL & Learner Profile') : null,
      hasProj ? h('th', { class: 'group gl', colSpan: 4 }, '2. Unit of Inquiry (Proyek)') : null,
      h('th', { class: 'group gl', colSpan: 4, title: `Dihitung otomatis dari ${effDays} hari efektif` }, 'Kehadiran'));

    const head2 = h('tr', {},
      settings.atl.map((a, i) => h('th', { class: `sub${i === 0 ? ' gl' : ''}`, title: a.indicator || a.name }, a.name)),
      hasProj ? [
        h('th', { class: 'sub gl' }, 'Proyek'),
        h('th', { class: 'sub' }, 'Understanding'),
        h('th', { class: 'sub' }, 'Inquiry'),
        h('th', { class: 'sub' }, 'Narasi'),
      ] : null,
      h('th', { class: 'sub gl', title: 'Sakit (hari)' }, 'S'),
      h('th', { class: 'sub', title: 'Izin (hari)' }, 'I'),
      h('th', { class: 'sub', title: 'Alpa / Tanpa Keterangan (hari)' }, 'A'),
      h('th', { class: 'sub', title: 'Teks kehadiran yang tercetak di rapor' }, 'Rapor'));

    let sheet;
    function moveFocus(colKey, fromRow, step) {
      let r = fromRow + step;
      while (r >= 0 && r < rows.length) {
        if (!rows[r].tr.hidden) {
          const next = sheet.querySelector(`input[data-row="${r}"][data-col="${colKey}"]`);
          if (next && !next.disabled) {
            next.focus();
            next.select();
            return;
          }
        }
        r += step;
      }
    }

    /** Sel predikat/angka di tabel: ketik E/M/A/B atau kode penuh atau angka 0–100. */
    function makeRateCell(initialVal, rowIdx, colKey, label, onValue, extraClass = '') {
      const stampBox = h('span', { class: 'rate-stamp' });
      const input = h('input', {
        class: 'rate-in', value: initialVal, placeholder: '—', 'aria-label': label,
        dataset: { row: String(rowIdx), col: colKey },
        oninput: (e) => {
          const expanded = expandGradeCode(e.target.value, g);
          paint(expanded);
          onValue(expanded);
        },
        onblur: (e) => {
          const expanded = expandGradeCode(e.target.value, g);
          if (e.target.value.trim()) e.target.value = expanded;
          paint(expanded);
        },
        onkeydown: (e) => {
          if (e.key === 'Enter' || e.key === 'ArrowDown') {
            e.preventDefault();
            const expanded = expandGradeCode(e.target.value, g);
            if (e.target.value.trim()) e.target.value = expanded;
            moveFocus(colKey, rowIdx, 1);
          } else if (e.key === 'ArrowUp') {
            e.preventDefault();
            const expanded = expandGradeCode(e.target.value, g);
            if (e.target.value.trim()) e.target.value = expanded;
            moveFocus(colKey, rowIdx, -1);
          }
        },
      });

      function paint(val) {
        const res = localGrade(val, g);
        const rank = res.code ? (ranks[res.code] ?? 5) : 5;
        const isCode = res.status === 'ok' && !res.fromNumber;
        const isNum = Boolean(res.fromNumber) && (res.status === 'ok' || res.status === 'below_scale');
        input.className = `rate-in${isCode ? ` is-code r${rank}` : ''}${isNum ? ' has-num' : ''}${res.status === 'invalid' ? ' invalid' : ''}`;
        input.title = res.status === 'invalid'
          ? 'Isi kode predikat (EE/ME/AE/BE) atau nilai angka 0–100'
          : res.status === 'below_scale'
            ? `Nilai ${res.score} di bawah rentang ${res.code}`
            : '';
        if (isNum) {
          stampBox.hidden = false;
          stampBox.replaceChildren(stamp(res, ranks));
        } else {
          stampBox.hidden = true;
          stampBox.replaceChildren();
        }
      }
      paint(initialVal);

      return {
        td: h('td', { class: `c${extraClass ? ` ${extraClass}` : ''}` }, h('div', { class: 'cell-rate' }, input, stampBox)),
        setValue: (v) => { input.value = v; paint(v); },
      };
    }

    const tbody = h('tbody', {}, views.map((v, rowIdx) => {
      const s = v.student;
      const fields = makeFields(s, settings);
      let activeProjIdx = 0;
      const changed = () => queue(s.id, fields);

      const dotEl = h('span', { class: 'dot' });
      const paintDot = (view) => {
        const p = view.progress;
        const done = view.issues.length === 0;
        const started = p.atl_done > 0 || p.projects > 0 || p.attendance;
        dotEl.className = `dot${done ? ' done' : started ? ' part' : ''}`;
        dotEl.title = done ? 'Rapor lengkap' : view.issues.join(' · ');
      };
      paintDot(v);

      // 1. Sel ATL
      const atlCells = settings.atl.map((a, i) => makeRateCell(
        fields.atl[a.id].grade,
        rowIdx,
        `atl-${i}`,
        `${a.name} — ${s.name}`,
        (val) => { fields.atl[a.id].grade = val; changed(); },
        i === 0 ? 'gl' : '',
      ));

      // 2. Sel Proyek (Proyek, Understanding, Inquiry, Narasi)
      let projCells = null;
      let refreshProjUI = () => {};
      const getActiveProject = () => fields.projects[activeProjIdx] || fields.projects[0] || null;
      const ensureProject = () => {
        if (!fields.projects.length) {
          const pid = projectPick.value || settings.projects[0]?.id || '';
          if (!pid) return null;
          fields.projects.push({ project_id: pid, understanding: '', inquiry: '', narrative: '' });
          activeProjIdx = 0;
        }
        return getActiveProject();
      };

      if (hasProj) {
        const projSelect = h('select', {
          class: 'cell-select', 'aria-label': `Proyek ${s.name}`,
          onchange: (e) => {
            const val = e.target.value;
            if (!val) {
              if (fields.projects.length) {
                fields.projects.splice(activeProjIdx, 1);
                activeProjIdx = 0;
              }
            } else {
              const existingIdx = fields.projects.findIndex((p) => p.project_id === val);
              if (existingIdx >= 0) {
                activeProjIdx = existingIdx;
              } else {
                const cur = fields.projects[activeProjIdx];
                const curHasData = cur && (cur.understanding.trim() || cur.inquiry.trim() || cur.narrative.trim());
                if (cur && !curHasData) {
                  cur.project_id = val;
                } else {
                  fields.projects.push({ project_id: val, understanding: '', inquiry: '', narrative: '' });
                  activeProjIdx = fields.projects.length - 1;
                }
              }
            }
            refreshProjUI();
            changed();
          },
        });
        const moreBadge = h('span', { class: 'cell-proj-more', hidden: true });

        const uCell = makeRateCell(
          getActiveProject()?.understanding ?? '',
          rowIdx,
          'uoi-u',
          `Understanding Level — ${s.name}`,
          (val) => {
            if (!val && !getActiveProject()) return;
            const p = ensureProject();
            if (p) { p.understanding = val; refreshProjSelect(); changed(); }
          },
        );
        const iCell = makeRateCell(
          getActiveProject()?.inquiry ?? '',
          rowIdx,
          'uoi-i',
          `Inquiry & Research Skills — ${s.name}`,
          (val) => {
            if (!val && !getActiveProject()) return;
            const p = ensureProject();
            if (p) { p.inquiry = val; refreshProjSelect(); changed(); }
          },
        );

        const narrBtn = h('button', {
          type: 'button', class: 'cell-narr',
          onclick: () => openQuickNarrative(rowIdx),
        });

        function refreshProjSelect() {
          const p = getActiveProject();
          const curId = p?.project_id ?? '';
          const known = settings.projects.some((pc) => pc.id === curId);
          const hasProjData = (pid) => fields.projects.some((ep) => ep.project_id === pid && (ep.understanding.trim() || ep.inquiry.trim() || ep.narrative.trim()));
          projSelect.replaceChildren(
            h('option', { value: '', selected: !curId }, curId ? '— Hapus proyek ini —' : '— Belum ada —'),
            ...settings.projects.map((pc) => h('option', { value: pc.id, selected: pc.id === curId },
              `${hasProjData(pc.id) ? '✓ ' : ''}${projectLabel(pc)}`)),
            curId && !known ? h('option', { value: curId, selected: true }, '(proyek dihapus)') : '',
          );
          projSelect.value = curId;
          projSelect.classList.toggle('empty', !curId);
          projSelect.title = p ? (settings.projects.find((pc) => pc.id === curId)?.theme || '') : 'Pilih proyek atau langsung ketik nilai Understanding/Inquiry';
          if (fields.projects.length > 1) {
            moreBadge.hidden = false;
            moreBadge.textContent = `${fields.projects.length} proyek`;
            moreBadge.title = `Siswa ini punya ${fields.projects.length} proyek (${fields.projects.map((ep) => settings.projects.find((pc) => pc.id === ep.project_id)?.name || ep.project_id).join(', ')}). Pilih dari daftar untuk berpindah proyek atau klik nama siswa untuk melihat semua.`;
          } else {
            moreBadge.hidden = true;
          }
        }

        function refreshNarrBtn() {
          const text = getActiveProject()?.narrative?.trim() || '';
          narrBtn.classList.toggle('filled', Boolean(text));
          narrBtn.title = text || 'Klik untuk mengisi narasi & membuka bank kalimat';
          narrBtn.replaceChildren(
            text ? h('span', { class: 'ic' }, icon('check')) : '', // (bukan null: replaceChildren menulisnya sebagai teks)
            h('span', { class: 'txt' }, text || '+ Isi narasi…'),
          );
        }

        refreshProjUI = () => {
          const p = getActiveProject();
          refreshProjSelect();
          uCell.setValue(p?.understanding ?? '');
          iCell.setValue(p?.inquiry ?? '');
          refreshNarrBtn();
        };
        refreshProjUI();

        projCells = [
          h('td', { class: 'gl' }, h('div', { class: 'cell-proj-wrap' }, projSelect, moreBadge)),
          uCell.td,
          iCell.td,
          h('td', {}, narrBtn),
        ];
      }

      // 3. Sel Kehadiran (S, I, A → otomatis hitung % di Rapor)
      const attInput = h('input', {
        class: 'att-in', value: fields.attendance, placeholder: 'mis. 98% (Present)', 'aria-label': `Kehadiran ${s.name}`,
        dataset: { row: String(rowIdx), col: 'att' },
        oninput: (e) => { fields.attendance = e.target.value; changed(); },
        onkeydown: (e) => {
          if (e.key === 'Enter' || e.key === 'ArrowDown') { e.preventDefault(); moveFocus('att', rowIdx, 1); }
          else if (e.key === 'ArrowUp') { e.preventDefault(); moveFocus('att', rowIdx, -1); }
        },
      });
      const makeDayCell = (key, colKey, label, first = false) => h('td', { class: first ? 'c gl' : 'c' },
        h('input', {
          class: 'sia-in', inputmode: 'numeric', value: fields[key] ?? '', placeholder: '·',
          'aria-label': `${label} ${s.name}`,
          dataset: { row: String(rowIdx), col: colKey },
          oninput: (e) => {
            fields[key] = parseDayInput(e.target.value);
            const calc = calcAttendanceSia(fields.attendance_s, fields.attendance_i, fields.attendance_a, effDays);
            if (calc !== null) {
              fields.attendance = calc;
              attInput.value = calc;
            }
            changed();
          },
          onkeydown: (e) => {
            if (e.key === 'Enter' || e.key === 'ArrowDown') { e.preventDefault(); moveFocus(colKey, rowIdx, 1); }
            else if (e.key === 'ArrowUp') { e.preventDefault(); moveFocus(colKey, rowIdx, -1); }
          },
        }));

      const tr = h('tr', {},
        h('td', { class: 'sticky idx' }, rowIdx + 1),
        h('td', { class: 'sticky2 name' },
          h('div', { class: 'cell-who' },
            dotEl,
            h('button', {
              type: 'button', class: 'cell-student-btn', title: 'Buka tampilan lengkap per siswa',
              onclick: () => switchMode('student', s.id),
            }, s.name))),
        atlCells.map((c) => c.td),
        projCells,
        makeDayCell('attendance_s', 'att-s', 'Sakit', true),
        makeDayCell('attendance_i', 'att-i', 'Izin'),
        makeDayCell('attendance_a', 'att-a', 'Alpa'),
        h('td', {}, attInput));

      const rowCtrl = {
        tr,
        view: v,
        fields,
        changed,
        paintDot,
        getActiveProject,
        ensureProject,
        refreshProjUI,
      };
      rows.push(rowCtrl);
      rowById.set(s.id, rowCtrl);
      return tr;
    }));

    onSaved = (view) => {
      const r = rowById.get(view.student.id);
      if (r) {
        r.view = view;
        r.paintDot(view);
      }
    };

    /** Modal cepat untuk mengisi narasi + Bank kalimat langsung dari mode tabel, bisa lanjut antar siswa. */
    function openQuickNarrative(startIdx) {
      let curIdx = startIdx;
      const modal = openModal({ title: 'Narasi proyek', wide: true, body: h('div', {}) });

      function drawModal(idx) {
        curIdx = idx;
        const r = rows[idx];
        const s = r.view.student;
        const hadProject = Boolean(r.getActiveProject());
        const p = r.ensureProject();
        r.refreshProjUI();
        if (p && !hadProject) r.changed(); // proyek baru dibuat → simpan, supaya layar = data
        if (!p) {
          modal.setBody(h('p', { class: 'note warn' }, 'Belum ada proyek di Pengaturan.'));
          return;
        }
        const projCfg = settings.projects.find((pc) => pc.id === p.project_id);
        const phraseBox = h('div', {});
        const ta = autoGrow(h('textarea', {
          rows: 3, value: p.narrative, placeholder: 'Narasi dari fasilitator, dalam bahasa Inggris',
          oninput: (e) => {
            p.narrative = e.target.value;
            r.refreshProjUI();
            r.changed();
          },
        }));

        const drawPhrases = () => {
          const theme = settings.projects.find((pc) => pc.id === p.project_id)?.theme;
          const predicate = localGrade(p.understanding, settings.grading).code;
          phraseBox.replaceChildren(phraseBankList({
            settings, student: s, theme, predicate,
            getText: () => ta.value,
            onPick: (sentence) => {
              ta.value = appendSentence(ta.value, sentence);
              ta.dispatchEvent(new Event('input'));
            },
          }));
        };
        drawPhrases();

        modal.dlg.querySelector('.modal-head h2').textContent = `${s.name} (${idx + 1}/${rows.length})`;
        modal.setBody(h('div', { class: 'stack' },
          h('div', { class: 'project', style: { marginTop: '0' } },
            h('div', { class: 'project-head' },
              h('select', {
                'aria-label': 'Proyek',
                onchange: (e) => {
                  p.project_id = e.target.value;
                  r.refreshProjUI();
                  r.changed();
                  drawPhrases();
                },
              }, settings.projects.map((pc) => h('option', { value: pc.id, selected: pc.id === p.project_id }, projectLabel(pc)))),
              projCfg?.central_idea ? h('span', { class: 'small muted', style: { flex: 1 } }, projCfg.central_idea) : null),
            h('div', { class: 'pair' },
              h('span', { class: 'lbl' }, 'Understanding Level'), h('span', { class: 'lbl' }, 'Inquiry & Research Skills'),
              ratingControl(p.understanding, settings.grading, ranks, (val) => {
                p.understanding = val;
                r.refreshProjUI();
                r.changed();
                drawPhrases();
              }, { label: 'Understanding Level' }),
              ratingControl(p.inquiry, settings.grading, ranks, (val) => {
                p.inquiry = val;
                r.refreshProjUI();
                r.changed();
              }, { label: 'Inquiry & Research Skills' }))),
          h('label', { class: 'field' },
            h('span', { class: 'lbl' }, 'Teacher Narrative Feedback'),
            ta),
          h('div', { class: 'row' },
            button('Draf otomatis', { kind: 'sm', onClick: async () => {
              const draft = makeDraft(s, p);
              if (!draft) { toast('Isi Understanding Level dulu, supaya draf sesuai pencapaian siswa', 'error'); return; }
              if (ta.value.trim() && !(await confirmBox('Ganti narasi yang sudah ada dengan draf baru?', { ok: 'Ganti' }))) return;
              ta.value = draft;
              ta.dispatchEvent(new Event('input'));
            } }),
            h('span', { class: 'small muted' }, 'Disusun dari bank kalimat sesuai Understanding Level; tetap bisa disunting.')),
          phraseBox));

        modal.setActions([
          { label: '← Siswa sebelumnya', kind: 'quiet', disabled: idx <= 0, onClick: () => drawModal(curIdx - 1) },
          { label: 'Siswa berikutnya →', disabled: idx >= rows.length - 1, onClick: () => drawModal(curIdx + 1) },
          { label: 'Selesai', kind: 'primary', onClick: (c) => c() },
        ]);
      }

      drawModal(startIdx);
    }

    sheet = h('div', { class: 'sheet scroll cells-wrap' },
      h('table', { class: 'ledger cells' }, h('thead', {}, head1, head2), tbody));

    // ---------- Tempel dari Excel (Ctrl+V): blok sel menyebar ke bawah/kanan mulai dari sel aktif.
    // Kalau kolom pertama yang ditempel berisi nama siswa, baris dicocokkan lewat nama (urutan di
    // Excel tidak harus sama dengan urutan di sini).
    const colOrder = [...settings.atl.map((_, i) => `atl-${i}`), ...(hasProj ? ['uoi-u', 'uoi-i'] : []), 'att-s', 'att-i', 'att-a', 'att'];
    const normName = (t) => String(t).toLowerCase().normalize('NFKD').replace(/[^a-z0-9 ]/g, ' ').replace(/\s+/g, ' ').trim();
    const rowByName = (raw) => {
      const n = normName(raw);
      if (!n) return -1;
      const exact = rows.findIndex((r) => normName(r.view.student.name) === n);
      if (exact >= 0) return exact;
      const starts = rows.map((r, i) => [r, i]).filter(([r]) => normName(r.view.student.name).startsWith(n));
      return starts.length === 1 ? starts[0][1] : -1;
    };
    sheet.addEventListener('paste', (e) => {
      const t = e.target;
      if (!(t instanceof HTMLInputElement) || t.dataset.row === undefined || !colOrder.includes(t.dataset.col)) return;
      const lines = (e.clipboardData?.getData('text/plain') ?? '').replace(/\r/g, '').split('\n');
      while (lines.length && !lines[lines.length - 1].trim()) lines.pop();
      const grid = lines.map((l) => l.split('\t'));
      if (!grid.length || (grid.length === 1 && grid[0].length === 1)) return; // satu nilai: tempel biasa
      e.preventDefault();
      const startRow = Number(t.dataset.row);
      const startCol = colOrder.indexOf(t.dataset.col);
      const firstCells = grid.map((r) => r[0]).filter((c) => c.trim());
      const byName = grid[0].length > 1 && firstCells.length
        && firstCells.filter((c) => rowByName(c) >= 0).length >= Math.ceil(firstCells.length / 2);
      const visibleRows = rows.map((r, i) => i).filter((i) => i >= startRow && !rows[i].tr.hidden);
      let filled = 0;
      let invalid = 0;
      let extraRows = 0;
      const unknownNames = [];
      grid.forEach((cells, li) => {
        let rowIdx;
        let values = cells;
        if (byName) {
          rowIdx = rowByName(cells[0]);
          values = cells.slice(1);
          if (rowIdx < 0) { if (cells[0].trim()) unknownNames.push(cells[0].trim()); return; }
        } else {
          rowIdx = visibleRows[li];
          if (rowIdx === undefined) { extraRows++; return; }
        }
        values.forEach((raw, ci) => {
          const colKey = colOrder[startCol + ci];
          const v = String(raw).trim();
          if (!colKey || !v) return; // sel kosong di Excel tidak menghapus isian
          const el = sheet.querySelector(`input[data-row="${rowIdx}"][data-col="${colKey}"]`);
          if (!el || el.disabled) return;
          el.value = v;
          el.dispatchEvent(new Event('input'));
          el.dispatchEvent(new Event('blur'));
          filled++;
          if (el.classList.contains('invalid') || (el.classList.contains('sia-in') && parseDayInput(v) === null)) invalid++;
        });
      });
      const parts = [`${filled} sel terisi dari Excel`];
      if (invalid) parts.push(`${invalid} tidak terbaca (ditandai merah)`);
      if (extraRows) parts.push(`${extraRows} baris lebih dari jumlah siswa dilewati`);
      if (unknownNames.length) parts.push(`nama tidak ditemukan: ${unknownNames.slice(0, 3).join(', ')}${unknownNames.length > 3 ? '…' : ''}`);
      toast(parts.join(' · '), invalid || unknownNames.length || extraRows ? 'error' : 'ok', 7000);
    });

    /** Isi narasi yang masih kosong untuk semua siswa yang Understanding Level-nya sudah ada. */
    function draftAllDialog() {
      const targets = [];
      let noPredicate = 0;
      for (const r of rows) {
        if (r.tr.hidden) continue;
        const p = r.getActiveProject();
        if (!p || p.narrative.trim()) continue;
        if (localGrade(p.understanding, settings.grading).code) targets.push(r); else noPredicate++;
      }
      if (!targets.length) {
        toast(noPredicate
          ? `Tidak ada yang bisa dibuatkan draf: ${noPredicate} siswa belum diisi Understanding Level-nya`
          : 'Semua narasi proyek di tabel ini sudah terisi', 'error', 6000);
        return;
      }
      openModal({
        title: 'Draf narasi otomatis',
        body: h('div', { class: 'stack' },
          h('p', {}, `Buat draf narasi untuk ${targets.length} siswa yang narasinya masih kosong. Draf disusun dari bank kalimat sesuai Understanding Level masing-masing (nama, he/she, dan tema proyek otomatis).`),
          noPredicate ? h('p', { class: 'note warn small' }, `${noPredicate} siswa dilewati karena Understanding Level-nya belum diisi.`) : null,
          h('p', { class: 'note small' }, 'Narasi yang sudah ada tidak diubah. Draf tetap perlu dibaca dan disesuaikan guru sebelum dicetak.')),
        actions: [
          { label: 'Batal', kind: 'quiet', onClick: (c) => c() },
          { label: `Buat ${targets.length} draf`, kind: 'primary', onClick: (close) => {
            for (const r of targets) {
              const p = r.getActiveProject();
              p.narrative = makeDraft(r.view.student, p);
              r.refreshProjUI();
              r.changed();
            }
            close();
            toast(`${targets.length} draf narasi dibuat. Klik tombol narasi di tiap baris untuk membaca & menyunting.`, 'ok', 7000);
          } },
        ],
      });
    }

    content.append(subbar, sheet);
    if (onlyIncomplete) setOnlyIncomplete(true);
    return sheet;
  }

  drawModeTabs();
  drawStage(false);
  ctx.onLeave(flush);
}

function makeFields(s, settings) {
  return {
    attendance: s.attendance,
    attendance_s: s.attendance_s ?? null,
    attendance_i: s.attendance_i ?? null,
    attendance_a: s.attendance_a ?? null,
    atl: Object.fromEntries(settings.atl.map((a) => [a.id, { grade: s.atl[a.id]?.grade ?? '', indicator: s.atl[a.id]?.indicator ?? '' }])),
    projects: s.projects.map((p) => ({ ...p })),
    subject_notes: { ...s.subject_notes },
  };
}

function parseDayInput(raw) {
  const t = String(raw ?? '').trim();
  if (!t) return null;
  if (t === '-') return 0;
  const n = Number(t);
  return Number.isFinite(n) && n >= 0 ? Math.min(366, Math.round(n)) : null;
}

function calcAttendanceSia(s, i, a, effectiveDays) {
  if (s === null && i === null && a === null) return null;
  const tot = effectiveDays > 0 ? effectiveDays : 100;
  const absent = (s ?? 0) + (i ?? 0) + (a ?? 0);
  const present = Math.max(0, tot - absent);
  const pct = Math.min(100, Math.max(0, Math.round((present / tot) * 100)));
  return `${pct}% (Present)`;
}

function editEffectiveDaysDialog(app, onSaved) {
  let days = app.data.settings.effective_days || 100;
  openModal({
    title: 'Hari efektif semester',
    body: h('div', { class: 'stack' },
      h('p', {}, 'Dipakai untuk menghitung persentase kehadiran otomatis dari jumlah hari Sakit (S), Izin (I), dan Alpa (A):'),
      h('p', { class: 'small muted' }, 'Rumus: (Hari efektif − (S + I + A)) ÷ Hari efektif × 100% → misalnya "98% (Present)".'),
      h('label', { class: 'field', style: { maxWidth: '180px' } },
        h('span', { class: 'lbl' }, 'Total hari efektif'),
        h('input', {
          type: 'number', min: 1, max: 366, value: days,
          oninput: (e) => { days = Number(e.target.value) || 0; },
        }))),
    actions: [
      { label: 'Batal', kind: 'quiet', onClick: (c) => c() },
      { label: 'Simpan & hitung ulang', kind: 'primary', onClick: async (close) => {
        if (days < 1 || days > 366) { toast('Hari efektif harus 1–366 hari', 'error'); return; }
        try {
          const n = await api.setEffectiveDays(days);
          await app.reload();
          close();
          toast(n ? `Hari efektif diubah ke ${days} hari (${n} siswa dihitung ulang)` : `Hari efektif disimpan: ${days} hari`, 'ok');
          onSaved?.();
        } catch (e) { toast(errorText(e), 'error'); }
      } },
    ],
  });
}

export function projectLabel(p) {
  return p.theme ? `${p.name} — ${p.theme}` : p.name;
}

async function previewModal(id, name) {
  let html;
  try { html = await api.renderReport([id]); } catch (e) { toast(errorText(e), 'error'); return; }
  const frame = h('iframe', { class: 'desk', style: { height: '68vh' }, title: `Pratinjau rapor ${name}`, sandbox: 'allow-same-origin allow-modals' });
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
