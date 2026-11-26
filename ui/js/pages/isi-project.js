// Isi rapor → tampilan "Nilai proyek": satu proyek, satu kelas, seperti lembar Excel.
// Kolom: Understanding Level, Inquiry & Research Skills, Teacher Narrative Feedback.
// Bisa diisi langsung, ditempel dari Excel (Ctrl+V), atau diimpor dari file Excel guru.
import { h, autoGrow, button, expandGradeCode, link, localGrade, searchBox, shortcut, stamp, toast } from '../dom.js';
import { importProjectWizard } from '../project-import.js';
import { templateModal } from '../template-modal.js';

/**
 * @param {object} env
 *  app, settings, ranks, views, cls, content, queue(id, fields), flush(), saveState,
 *  setOnSaved(fn), makeFields(student, settings), makeDraft(student, project), rerender(), projectLabel(p)
 */
export function renderProjectMode(env) {
  const { app, settings, ranks, views, cls, content, queue, flush, saveState, setOnSaved, makeFields, makeDraft, rerender, projectLabel } = env;
  const g = settings.grading;
  if (!settings.projects.length) {
    const note = h('p', { class: 'note warn' }, 'Belum ada proyek. Tambahkan dulu di Pengaturan → Proyek (UOI).');
    content.append(note);
    return note;
  }

  // proyek terpilih: yang terakhir dipakai, atau yang paling banyak dipakai di kelas ini
  const counts = new Map();
  for (const v of views) for (const p of v.student.projects) counts.set(p.project_id, (counts.get(p.project_id) || 0) + 1);
  const mostUsed = [...counts.entries()].sort((a, b) => b[1] - a[1]).map(([id]) => id).find((id) => settings.projects.some((p) => p.id === id));
  let pid = settings.projects.some((p) => p.id === app.isiProject) ? app.isiProject : (mostUsed || settings.projects[0].id);

  let query = '';
  let onlyMembers = Boolean(app.isiProjectOnlyMembers);
  const rows = [];
  const tbody = h('tbody', {});
  const statLine = h('span', { class: 'small muted' });

  const projectSelect = h('select', {
    'aria-label': 'Proyek', style: { width: 'auto', maxWidth: '360px' },
    onchange: async (e) => { await flush(); pid = e.target.value; app.isiProject = pid; drawRows(); },
  }, settings.projects.map((p) => h('option', { value: p.id, selected: p.id === pid }, projectLabel(p))));

  const legend = h('div', { class: 'legend' },
    ...[...g.bands].sort((a, b) => b.min - a.min).map((b) => h('span', {}, stamp({ status: 'ok', code: b.code }, ranks), `≥ ${b.min}`)),
    h('span', { class: 'muted' }, '· ketik E/M/A/B atau angka 0–100'));

  const membersLabel = h('label', { class: 'check small', title: 'Sembunyikan siswa yang belum punya proyek ini' },
    h('input', { type: 'checkbox', checked: onlyMembers, onchange: (e) => { onlyMembers = e.target.checked; app.isiProjectOnlyMembers = onlyMembers; applyFilter(); } }),
    ' Hanya peserta proyek ini');

  const row = (...kids) => h('div', { style: { display: 'flex', alignItems: 'center', gap: '12px', flexWrap: 'wrap' } }, ...kids);
  const subbar = h('div', { class: 'table-subbar' },
    row(h('span', { style: { fontWeight: 600 } }, 'Proyek'), projectSelect, statLine),
    row(saveState,
      button('Template Excel', { kind: 'sm quiet', onClick: () => templateModal(app, { kind: 'projects', projectId: pid, classes: [cls] }) }),
      button('Impor nilai proyek dari Excel…', { kind: 'sm primary', onClick: async () => {
        await flush();
        importProjectWizard(app, { projectId: pid, cls, onDone: async (usedPid) => { app.isiProject = usedPid; await app.reload(); rerender(); } });
      } })));
  const sub2 = h('div', { class: 'table-subbar proj-sub2' },
    legend,
    row(membersLabel,
      button('Draf narasi kosong…', { kind: 'sm', onClick: () => draftEmpty() }),
      searchBox('Cari siswa', (q) => { query = q; applyFilter(); })));

  function moveFocus(col, from, step) {
    for (let r = from + step; r >= 0 && r < rows.length; r += step) {
      if (rows[r].tr.hidden) continue;
      const el = tbody.querySelector(`[data-row="${r}"][data-col="${col}"]`);
      if (el) { el.focus(); if (el.select) el.select(); return; }
    }
  }

  function rateCell(row, idx, col, key, label) {
    const stampBox = h('span', { class: 'rate-stamp' });
    const input = h('input', {
      class: 'rate-in', placeholder: '—', 'aria-label': `${label} — ${row.student.name}`,
      dataset: { row: String(idx), col },
      oninput: (e) => { const v = expandGradeCode(e.target.value, g); paint(v); row.set(key, v); },
      onblur: (e) => { const v = expandGradeCode(e.target.value, g); if (e.target.value.trim()) e.target.value = v; paint(v); },
      onkeydown: (e) => {
        if (e.key === 'Enter' || e.key === 'ArrowDown') { e.preventDefault(); e.target.dispatchEvent(new Event('blur')); moveFocus(col, idx, 1); }
        else if (e.key === 'ArrowUp') { e.preventDefault(); e.target.dispatchEvent(new Event('blur')); moveFocus(col, idx, -1); }
      },
    });
    function paint(val) {
      const res = localGrade(val, g);
      const rank = res.code ? (ranks[res.code] ?? 5) : 5;
      const isCode = res.status === 'ok' && !res.fromNumber;
      const isNum = Boolean(res.fromNumber) && (res.status === 'ok' || res.status === 'below_scale');
      input.className = `rate-in${isCode ? ` is-code r${rank}` : ''}${isNum ? ' has-num' : ''}${res.status === 'invalid' ? ' invalid' : ''}`;
      input.title = res.status === 'invalid' ? 'Isi kode predikat (EE/ME/AE/BE) atau nilai angka 0–100' : '';
      stampBox.hidden = !isNum;
      stampBox.replaceChildren(isNum ? stamp(res, ranks) : '');
    }
    return {
      td: h('td', { class: 'c' }, h('div', { class: 'cell-rate' }, input, stampBox)),
      set: (v) => { input.value = v; paint(v); },
      input,
    };
  }

  function makeRow(v, idx) {
    const s = v.student;
    const fields = makeFields(s, settings);
    let entry = fields.projects.find((p) => p.project_id === pid) || null;
    let created = false;
    const otherNames = fields.projects
      .filter((p) => p.project_id !== pid)
      .map((p) => settings.projects.find((pc) => pc.id === p.project_id)?.name || '(proyek dihapus)');
    const row = {
      student: s,
      member: () => Boolean(entry),
      get entry() { return entry; },
      set(key, val) {
        if (!entry) {
          if (!String(val).trim()) return;
          entry = { project_id: pid, understanding: '', inquiry: '', narrative: '' };
          fields.projects.push(entry);
          created = true;
        }
        entry[key] = val;
        // baris yang baru dibuat lalu dikosongkan lagi: siswa tidak jadi ikut proyek ini
        if (created && !entry.understanding.trim() && !entry.inquiry.trim() && !entry.narrative.trim()) {
          fields.projects.splice(fields.projects.indexOf(entry), 1);
          entry = null;
          created = false;
        }
        paintRow();
        queue(s.id, fields);
        drawStats();
      },
    };
    const u = rateCell(row, idx, 'u', 'understanding', 'Understanding Level');
    const i = rateCell(row, idx, 'i', 'inquiry', 'Inquiry & Research Skills');
    const ta = autoGrow(h('textarea', {
      rows: 1, class: 'narr-in', placeholder: 'Narasi (bahasa Inggris)…', 'aria-label': `Narasi — ${s.name}`,
      dataset: { row: String(idx), col: 'n' },
      oninput: (e) => row.set('narrative', e.target.value),
    }));
    const dot = h('span', { class: 'dot' });
    const paintRow = () => {
      const done = entry && localGrade(entry.understanding, g).code && localGrade(entry.inquiry, g).code && entry.narrative.trim();
      dot.className = `dot${done ? ' done' : entry ? ' part' : ''}`;
      dot.title = done ? 'Lengkap' : entry ? 'Belum lengkap' : 'Belum ikut proyek ini';
    };
    u.set(entry?.understanding ?? '');
    i.set(entry?.inquiry ?? '');
    ta.value = entry?.narrative ?? '';
    paintRow();
    row.ta = ta;
    row.cells = { u, i };
    row.tr = h('tr', {},
      h('td', { class: 'sticky idx' }, idx + 1),
      h('td', { class: 'sticky2 name' }, h('div', { class: 'cell-who' }, dot, h('span', {}, s.name))),
      u.td, i.td,
      h('td', { class: 'narr-td' }, ta),
      h('td', { class: 'small muted', title: otherNames.length ? 'Siswa ini juga ikut proyek lain; semua proyeknya tercetak di rapor' : '' },
        otherNames.length ? `juga: ${otherNames.join(', ')}` : ''));
    return row;
  }

  function drawStats() {
    const members = rows.filter((r) => r.member());
    const graded = members.filter((r) => localGrade(r.entry.understanding, g).code && localGrade(r.entry.inquiry, g).code);
    statLine.textContent = `${members.length} dari ${rows.length} siswa ${cls} ikut · ${graded.length} sudah dinilai`;
  }

  function applyFilter() {
    for (const r of rows) {
      r.tr.hidden = Boolean((query && !r.student.name.toLowerCase().includes(query)) || (onlyMembers && !r.member()));
    }
  }

  function drawRows() {
    rows.length = 0;
    views.forEach((v, idx) => rows.push(makeRow(v, idx)));
    tbody.replaceChildren(...rows.map((r) => r.tr));
    applyFilter();
    drawStats();
  }

  function draftEmpty() {
    let n = 0;
    let noGrade = 0;
    for (const r of rows) {
      if (r.tr.hidden || !r.entry || r.entry.narrative.trim()) continue;
      const draft = makeDraft(r.student, r.entry);
      if (!draft) { noGrade++; continue; }
      r.ta.value = draft;
      r.ta.dispatchEvent(new Event('input'));
      n++;
    }
    toast(n
      ? `${n} draf narasi dibuat${noGrade ? ` · ${noGrade} siswa dilewati (Understanding belum diisi)` : ''}. Baca dan sesuaikan sebelum dicetak.`
      : noGrade ? `Isi Understanding Level dulu (${noGrade} siswa)` : 'Tidak ada narasi kosong untuk peserta proyek ini', n ? 'ok' : 'error', 7000);
  }

  setOnSaved(() => {});
  drawRows();

  const sheet = h('div', { class: 'sheet scroll cells-wrap' },
    h('table', { class: 'ledger cells proj-table' },
      h('thead', {}, h('tr', {},
        h('th', { class: 'sticky idx' }, 'No'), h('th', { class: 'sticky2' }, 'Nama'),
        h('th', { class: 'c' }, 'Understanding Level'), h('th', { class: 'c' }, 'Inquiry & Research Skills'),
        h('th', {}, 'Teacher Narrative Feedback'), h('th', {}, ''))),
      tbody));

  // Tempel dari Excel: blok sel mulai dari sel aktif; kalau kolom pertama berisi nama siswa,
  // baris dicocokkan lewat nama (urutan di Excel boleh berbeda).
  const colOrder = ['u', 'i', 'n'];
  const norm = (t) => String(t).toLowerCase().normalize('NFKD').replace(/[^a-z0-9 ]/g, ' ').replace(/\s+/g, ' ').trim();
  const byName = (raw) => {
    const n = norm(raw);
    if (!n) return -1;
    const exact = rows.findIndex((r) => norm(r.student.name) === n);
    if (exact >= 0) return exact;
    const starts = rows.map((r, k) => [r, k]).filter(([r]) => norm(r.student.name).startsWith(n));
    return starts.length === 1 ? starts[0][1] : -1;
  };
  sheet.addEventListener('paste', (e) => {
    const t = e.target;
    if (!(t instanceof HTMLElement) || t.dataset.row === undefined || !colOrder.includes(t.dataset.col)) return;
    const lines = (e.clipboardData?.getData('text/plain') ?? '').replace(/\r/g, '').split('\n');
    while (lines.length && !lines[lines.length - 1].trim()) lines.pop();
    const grid = lines.map((l) => l.split('\t'));
    if (!grid.length || (grid.length === 1 && grid[0].length === 1)) return;
    e.preventDefault();
    const startRow = Number(t.dataset.row);
    const startCol = colOrder.indexOf(t.dataset.col);
    const firsts = grid.map((r) => r[0]).filter((c) => c.trim());
    const named = grid[0].length > 1 && firsts.length && firsts.filter((c) => byName(c) >= 0).length >= Math.ceil(firsts.length / 2);
    const visible = rows.map((r, k) => k).filter((k) => k >= startRow && !rows[k].tr.hidden);
    let filled = 0;
    let bad = 0;
    const unknown = [];
    grid.forEach((cells, li) => {
      let r;
      let values = cells;
      if (named) {
        r = byName(cells[0]);
        values = cells.slice(1);
        if (r < 0) { if (cells[0].trim()) unknown.push(cells[0].trim()); return; }
      } else {
        r = visible[li];
        if (r === undefined) return;
      }
      values.forEach((raw, ci) => {
        const col = colOrder[startCol + ci];
        const v = String(raw).trim();
        if (!col || !v) return; // sel kosong di Excel tidak menghapus isian
        const row = rows[r];
        if (col === 'n') { row.ta.value = v; row.ta.dispatchEvent(new Event('input')); }
        else {
          const cell = row.cells[col];
          cell.input.value = v;
          cell.input.dispatchEvent(new Event('input'));
          cell.input.dispatchEvent(new Event('blur'));
          if (cell.input.classList.contains('invalid')) bad++;
        }
        filled++;
      });
    });
    const parts = [`${filled} sel terisi dari Excel`];
    if (bad) parts.push(`${bad} tidak terbaca (ditandai merah)`);
    if (unknown.length) parts.push(`nama tidak ditemukan: ${unknown.slice(0, 3).join(', ')}${unknown.length > 3 ? '…' : ''}`);
    toast(parts.join(' · '), bad || unknown.length ? 'error' : 'ok', 7000);
  });

  const hint = h('p', { class: 'small muted', style: { margin: '-8px 0 0' } },
    `Tip: blok kolom nilai di Excel (boleh beserta kolom nama di depannya) lalu ${shortcut('V')} di sel pertama. `,
    link('Atur indikator & proyek di Pengaturan', () => app.go('pengaturan')), '.');
  content.append(subbar, sub2, sheet, hint);
  return sheet;
}
