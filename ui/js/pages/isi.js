// Isi rapor: ATL, proyek (UOI), narasi, kehadiran, dan indikator khusus per siswa.
// Dua tampilan: "Per siswa" (semua isian satu siswa) dan "Nilai proyek" (satu proyek, sekelas).
// (Tampilan "Tabel kelas" dihapus 2026-11: cara kerjanya sama dengan Per siswa dan membingungkan guru.)
// Isian tersimpan otomatis sesaat setelah berhenti mengetik/mengklik.
import { api } from '../api.js';
import {
  h, autoGrow, avatar, button, icon, classTabs, confirmBox, localGrade, debounce, emptyState, errorText, fitPreview, fmtNum, link, openModal,
  isMac, rankMap, ratingControl, searchBox, stamp, swapIn, toast, toolbar,
} from '../dom.js';
import { printOnMac } from './cetak.js';
import { importFormFlow, openFormBuilder } from '../form-tools.js';
import { importWizard } from '../import-wizard.js';
import { appendSentence, displayName, draftNarrative, openPhraseBank } from '../narrative.js';
import { renderProjectMode } from './isi-project.js';

export async function render(ctx) {
  const { app, content, actions, setTitle } = ctx;
  setTitle('Isi rapor', 'Penilaian ATL, proyek, narasi, dan kehadiran. Isi per siswa; nilai proyek sekelas di tab Nilai proyek. Semua tersimpan otomatis.');
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
  let mode = app.isiMode === 'project' ? 'project' : 'student'; // 'student' | 'project'

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
      ...[['student', 'Per siswa'], ['project', 'Nilai proyek']].map(([id, label]) =>
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
    const mainNode = mode === 'project'
      ? renderProjectMode({
        app, settings, ranks, views, cls, content, queue, flush, saveState, rerender, projectLabel,
        setOnSaved: (fn) => { onSaved = fn; },
        makeFields, makeDraft,
      })
      : renderStudentMode();
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

      // defaultText boleh fungsi: indikator ATL bawaan mengikuti predikat yang dipilih (EE/ME/AE/BE)
      const indicatorEditor = (defaultText, current, onChange) => {
        const wrap = h('div', { class: 'ind' });
        const def = () => (typeof defaultText === 'function' ? defaultText() : defaultText);
        let editing = false;
        const show = (edit) => {
          editing = edit;
          if (edit) {
            const ta = autoGrow(h('textarea', { value: current, placeholder: def(), 'aria-label': 'Indikator khusus', oninput: (e) => { current = e.target.value; onChange(current); } }));
            wrap.replaceChildren(ta, link('Pakai indikator bawaan', () => { current = ''; onChange(''); show(false); }, 'muted'));
            ta.focus();
          } else {
            wrap.replaceChildren(
              h('div', { class: current ? 'custom' : '' }, current || def() || '—'),
              link(current ? 'Ubah indikator khusus' : 'Ubah untuk siswa ini', () => show(true), 'muted'));
          }
        };
        show(false);
        wrap.refresh = () => { if (!editing) show(false); };
        return wrap;
      };

      const atlRows = settings.atl.map((a) => {
        const ind = indicatorEditor(() => atlIndicator(a, fields.atl[a.id].grade, settings.grading), fields.atl[a.id].indicator,
          (val) => { fields.atl[a.id].indicator = val; changed(); });
        return h('div', { class: 'rate-row' },
          h('div', { class: 'nm' }, a.name),
          ratingControl(fields.atl[a.id].grade, settings.grading, ranks, (val) => { fields.atl[a.id].grade = val; changed(); ind.refresh(); }, { label: a.name }),
          ind);
      });

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

  drawModeTabs();
  drawStage(false);
  ctx.onLeave(flush);
}

/** Indikator ATL bawaan untuk predikat siswa: teks khusus predikat itu (Pengaturan), atau indikator umum. */
function atlIndicator(a, grade, grading) {
  const code = localGrade(grade, grading).code;
  const t = code ? (a.levels?.[code] || '').trim() : '';
  return t || a.indicator;
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
      { label: 'Cetak / Simpan PDF', kind: 'primary', onClick: () => (isMac ? printOnMac([id]) : frame.contentWindow?.print()) },
    ],
  });
}
