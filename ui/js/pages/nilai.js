// Nilai mapel: tabel PH / PTS / nilai akhir / predikat per kelas, terasa seperti spreadsheet.
// Kolom mapel mengikuti Pengaturan → Mapel (bisa lebih dari 3).
import { api } from '../api.js';
import { h, button, swapIn, classTabs, emptyState, errorText, fmtNum, parseNum, rankMap, stamp, toast, toolbar } from '../dom.js';
import { importWizard } from '../import-wizard.js';
import { templateModal } from '../template-modal.js';

export async function render(ctx) {
  const { app, content, actions, setTitle } = ctx;
  const g = app.data.settings.grading;
  setTitle('Nilai mapel', `Nilai akhir = ${g.weight_ph}% PH + ${g.weight_pts}% PTS${g.round_final ? ', dibulatkan' : ''}, lalu menjadi predikat. Kolom Akhir boleh diisi manual untuk menggantikan rumus.`);
  const rerender = async () => { await app.reload(); content.replaceChildren(); actions.replaceChildren(); await render(ctx); };
  actions.append(
    button('Template Excel', { kind: 'quiet', onClick: () => templateModal(app, { kind: 'scores' }) }),
    button('Impor nilai dari Excel', { kind: 'primary', onClick: () => importWizard('scores', rerender, app) }));

  const classes = app.classes();
  if (!classes.length) {
    content.appendChild(emptyState('Belum ada siswa', 'Impor data siswa dulu, lalu impor rekap nilai di sini.',
      button('Ke Data siswa', { onClick: () => app.go('siswa') })));
    return;
  }
  const subjects = app.data.settings.subjects;
  if (!subjects.length) {
    content.appendChild(h('p', { class: 'note warn' }, 'Belum ada mapel. Tambahkan di Pengaturan → Mapel.'));
    return;
  }

  const cls = app.ensureClass();
  const ranks = rankMap(app.data.settings);
  const legend = h('div', { class: 'legend' }, [...g.bands].sort((a, b) => b.min - a.min)
    .map((b) => h('span', {}, stamp({ status: 'ok', code: b.code }, ranks), `≥ ${b.min}`)));
  content.appendChild(toolbar(
    classTabs(classes, cls, (c) => { app.selectedClass = c; rerender(); }, { all: false, counts: app.classCounts() }),
    legend));

  const views = await api.studentViews(cls);
  const head1 = h('tr', {},
    h('th', { class: 'sticky idx', rowSpan: 2 }, 'No'),
    h('th', { class: 'sticky2', rowSpan: 2 }, 'Nama'),
    subjects.map((s) => h('th', { class: 'group gl', colSpan: 4, title: s.source_key ? `Kolom sumber: ${s.source_key}` : 'Sumber nilai belum diatur' },
      s.name, s.source_key ? null : h('div', { class: 'small', style: { color: 'var(--warn)', fontFamily: 'var(--sans)' } }, 'sumber nilai belum diatur'))));
  const head2 = h('tr', {}, subjects.map(() => [
    h('th', { class: 'sub r gl' }, 'PH'), h('th', { class: 'sub r' }, 'PTS'), h('th', { class: 'sub r' }, 'Akhir'), h('th', { class: 'sub c' }, 'Pred.')]));

  const body = h('tbody', {}, views.map((v, rowIdx) => h('tr', {},
    h('td', { class: 'sticky idx' }, rowIdx + 1),
    h('td', { class: 'sticky2 name' }, v.student.name),
    v.subjects.map((sv, colIdx) => scoreCells(v.student.id, sv, rowIdx, colIdx, ranks)))));
  const sheet = content.appendChild(h('div', { class: 'sheet scroll cells-wrap' },
    h('table', { class: 'ledger cells' }, h('thead', {}, head1, head2), body)));
  if (!ctx.entering) swapIn(sheet); // ganti kelas: hanya tabel yang berganti
}

function scoreCells(studentId, sv, rowIdx, colIdx, ranks) {
  const disabled = !sv.source_key;
  const mk = (val, part, label) => h('input', {
    value: fmtNum(val), disabled, inputmode: 'decimal', 'aria-label': `${label} ${sv.name}`,
    dataset: { row: String(rowIdx), col: `${colIdx}-${part}` },
    onkeydown: (e) => {
      if (e.key !== 'Enter') return;
      e.preventDefault();
      const next = document.querySelector(`input[data-row="${rowIdx + 1}"][data-col="${colIdx}-${part}"]`);
      e.target.blur();
      next?.focus();
      next?.select();
    },
  });
  const ph = mk(sv.ph, 'ph', 'PH');
  const pts = mk(sv.pts, 'pts', 'PTS');
  const manual = mk(sv.manual, 'akhir', 'Nilai akhir');
  const stampCell = h('td', { class: 'c' }, stamp(sv.result, ranks));
  const paintFinal = () => {
    manual.classList.toggle('manual', sv.manual !== null);
    manual.placeholder = sv.manual === null && sv.result.score !== null ? fmtNum(sv.result.score) : '';
    manual.title = sv.manual !== null ? 'Nilai akhir manual (kosongkan untuk kembali ke rumus)' : 'Hasil rumus. Ketik angka untuk menggantinya.';
  };
  paintFinal();

  async function save(e) {
    // Hanya kolom yang diubah yang dibaca dari teks; kolom lain memakai nilai
    // asli (tampilan dibulatkan 2 desimal, jadi teksnya tidak selalu persis).
    const changed = parseNum(e.target.value);
    if (Number.isNaN(changed) || (changed !== null && (changed < 0 || changed > 100))) {
      e.target.classList.add('invalid');
      toast('Nilai harus angka 0–100', 'error');
      return;
    }
    const vals = [sv.ph, sv.pts, sv.manual];
    vals[[ph, pts, manual].indexOf(e.target)] = changed;
    try {
      const view = await api.setScore(studentId, sv.source_key, vals[0], vals[1], vals[2]);
      const fresh = view.subjects.find((x) => x.subject_id === sv.subject_id);
      if (!fresh) return;
      Object.assign(sv, fresh);
      e.target.classList.remove('invalid');
      paintFinal();
      stampCell.replaceChildren(stamp(sv.result, ranks));
    } catch (err) {
      e.target.classList.add('invalid');
      toast(errorText(err), 'error');
    }
  }
  for (const i of [ph, pts, manual]) i.addEventListener('change', save);

  return [h('td', { class: 'r gl' }, ph), h('td', { class: 'r' }, pts), h('td', { class: 'r' }, manual), stampCell];
}
