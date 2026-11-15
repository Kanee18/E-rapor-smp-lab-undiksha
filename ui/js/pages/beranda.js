// Ringkasan: banner kemajuan, kartu per langkah, kartu per kelas, dan daftar yang perlu dicek.
import { api } from '../api.js';
import { h, button, icon, link, ring } from '../dom.js';
import { STEP } from '../steps.js';
import { backupStatus, fmtTime, runExternalBackup } from './data.js';

const reduceMotion = () => window.matchMedia('(prefers-reduced-motion: reduce)').matches;

/** Angka menghitung naik dari 0 (dilewati kalau "kurangi animasi" aktif). */
function countUp(el, to, ms = 650) {
  if (reduceMotion() || to <= 0) { el.textContent = String(to); return; }
  const start = performance.now();
  const tick = (now) => {
    const t = Math.min(1, (now - start) / ms);
    el.textContent = String(Math.round(to * (1 - (1 - t) ** 3)));
    if (t < 1) requestAnimationFrame(tick);
  };
  el.textContent = '0';
  requestAnimationFrame(tick);
}

function greeting() {
  const hr = new Date().getHours();
  return hr < 11 ? 'Selamat pagi' : hr < 15 ? 'Selamat siang' : hr < 18 ? 'Selamat sore' : 'Selamat malam';
}

export async function render({ app, content, setTitle }) {
  const s = app.data.settings;
  const views = await api.studentViews(null);
  const total = views.length;

  const fullScores = (v) => v.progress.subjects_total > 0 && v.progress.subjects_done === v.progress.subjects_total;
  const fullAtl = (v) => v.progress.atl_total > 0 && v.progress.atl_done === v.progress.atl_total;
  const fullProj = (v) => v.progress.projects > 0 && v.progress.projects_done === v.progress.projects;
  const ready = (v) => v.issues.length === 0;
  const count = (f, list = views) => list.filter(f).length;
  const nReady = count(ready);

  // ---------- banner: kalimat kemajuan + langkah berikutnya + cincin
  const headline = !total ? 'Mulai dengan data siswa'
    : nReady === total ? 'Semua rapor siap dicetak!'
      : `${nReady} dari ${total} rapor siap dicetak`;
  const nScores = count(fullScores);
  const nIsi = count((v) => fullAtl(v) && fullProj(v));
  const next = !total ? { page: 'siswa', text: 'Impor ekspor Dapodik untuk memasukkan daftar siswa.', cta: 'Impor data siswa' }
    : nScores < total ? { page: 'nilai', text: `Berikutnya: lengkapi nilai mapel. ${total - nScores} siswa belum lengkap.`, cta: 'Buka Nilai mapel' }
      : nIsi < total ? { page: 'isi', text: `Berikutnya: isi ATL dan proyek. ${total - nIsi} siswa belum lengkap.`, cta: 'Buka Isi rapor' }
        : nReady < total ? { page: 'isi', text: `${total - nReady} rapor masih ada catatan kecil, misalnya kehadiran.`, cta: 'Lengkapi' }
          : { page: 'cetak', text: 'Tinggal cetak atau simpan sebagai PDF.', cta: 'Ke Cetak' };
  const term = s.report_subtitle.replace(/^STUDENT PROGRESS REPORT\s*[—-]\s*/i, '') || 'Rapor';
  setTitle(headline, `${greeting()} · ${term} · tahun ajaran ${s.academic_year}`);
  const pct = total ? nReady / total : 0;
  content.appendChild(h('section', { class: 'banner' },
    h('div', { class: 'banner-text' },
      h('span', { class: 'pill' }, `${term} · ${s.academic_year}`),
      h('p', { class: 'greet' }, greeting()),
      h('h1', {}, headline),
      h('p', { class: 'next' }, next.text),
      h('div', { class: 'row' }, h('button', { type: 'button', class: 'btn light', onclick: () => app.go(next.page) }, next.cta, icon('arrow')))),
    ring(pct, { size: 132, stroke: 11, label: `${Math.round(pct * 100)}%`, sub: 'siap cetak' })));

  // ---------- empat kartu angka, satu per langkah
  const stats = [
    { page: 'siswa', v: total, unit: '', t: `${app.classes().length} kelas`, pct: total ? 1 : 0 },
    { page: 'nilai', v: nScores, unit: `/${total}`, t: 'nilai lengkap' },
    { page: 'isi', v: nIsi, unit: `/${total}`, t: 'ATL & proyek lengkap' },
    { page: 'cetak', v: nReady, unit: `/${total}`, t: 'tanpa catatan' },
  ];
  const numbers = [];
  const bars = [];
  content.appendChild(h('section', { class: 'hero-stats' }, stats.map((st) => {
    const step = STEP[st.page];
    const num = h('span', {}, '0');
    const fill = h('i', { style: { width: '0%' } });
    numbers.push([num, st.v]);
    bars.push([fill, st.pct ?? (total ? st.v / total : 0)]);
    return h('a', { class: 'stat', href: `#${st.page}` },
      h('span', { class: 'stat-top' }, h('span', { class: 'no' }, step.n), h('span', { class: 'k' }, step.label), icon('arrow')),
      h('span', { class: 'v' }, num, st.unit ? h('small', {}, st.unit) : null),
      h('span', { class: 't' }, st.t),
      h('span', { class: 'bar' }, fill));
  })));
  numbers.forEach(([el, v]) => countUp(el, v));
  requestAnimationFrame(() => bars.forEach(([el, p]) => { el.style.width = `${Math.round(p * 100)}%`; }));

  if (!total) {
    content.appendChild(h('p', { class: 'muted' }, 'Urutannya: impor ekspor Dapodik di Data siswa, lalu impor rekap nilai dari file yang sama di Nilai mapel.'));
    content.appendChild(dataSection(app));
    return;
  }

  // ---------- kartu per kelas
  const byClass = new Map();
  for (const v of views) {
    const k = v.student.class_name || '(tanpa kelas)';
    if (!byClass.has(k)) byClass.set(k, []);
    byClass.get(k).push(v);
  }
  const mini = (label, n, d) => h('div', { class: 'mini' },
    h('span', { class: 'lbl' }, label),
    h('span', { class: 'track' }, h('i', { style: { width: `${d ? Math.round((n / d) * 100) : 0}%` } })),
    h('span', { class: `val${n === d ? ' done' : ''}` }, n === d ? icon('check') : null, `${n}/${d}`));
  content.appendChild(h('section', {},
    h('div', { class: 'section-title' }, h('h2', {}, 'Per kelas')),
    h('div', { class: 'class-grid' }, [...byClass.entries()].map(([cls, list]) => {
      const homeroom = s.classes.find((c) => c.name === cls)?.homeroom_teacher || '';
      const nr = count(ready, list);
      const p = list.length ? nr / list.length : 0;
      return h('article', { class: 'class-card' },
        h('div', { class: 'head' },
          h('div', {},
            h('span', { class: 'cls' }, cls),
            h('div', { class: 'wk' }, `${list.length} siswa · ${homeroom ? homeroom : 'wali kelas belum diisi'}`)),
          ring(p, { size: 54, stroke: 6, label: `${Math.round(p * 100)}%`, cls: p === 1 ? 'done' : '' })),
        h('div', { class: 'body' },
          mini('Nilai', count(fullScores, list), list.length),
          mini('ATL', count(fullAtl, list), list.length),
          mini('Proyek', count(fullProj, list), list.length),
          mini('Kehadiran', count((v) => v.progress.attendance, list), list.length),
          mini('Siap cetak', nr, list.length),
          h('div', { class: 'foot' }, button('Isi rapor kelas ini', { kind: 'sm', onClick: () => { app.selectedClass = cls; app.go('isi'); } }))));
    }))));

  // ---------- perlu dicek
  const lowest = [...s.grading.bands].sort((a, b) => a.min - b.min)[0];
  const checks = [
    { n: count((v) => v.subjects.some((x) => x.result.status === 'below_scale')), kind: 'danger', page: 'nilai',
      text: `siswa punya nilai akhir di bawah ${lowest ? lowest.min : 'rentang'}. Untuk sementara dicetak ${lowest?.code ?? 'predikat terendah'}; aturannya menunggu keputusan kepala sekolah.` },
    { n: count((v) => v.subjects.some((x) => ['incomplete', 'empty'].includes(x.result.status))), page: 'nilai', text: 'siswa nilai mapelnya belum lengkap.' },
    { n: count((v) => v.progress.projects === 0), page: 'isi', text: 'siswa belum punya data proyek (UOI).' },
    { n: count((v) => !v.progress.attendance), page: 'isi', text: 'siswa kehadirannya belum diisi.' },
    { n: s.classes.filter((c) => !c.homeroom_teacher.trim()).length, page: 'pengaturan', text: 'kelas belum ada nama wali kelasnya.' },
    { n: s.principal_name.trim() ? 0 : 1, page: 'pengaturan', text: 'nama kepala sekolah belum diisi.' },
    { n: s.digital_signature ? (s.principal_signature ? 0 : 1) + s.classes.filter((c) => byClass.has(c.name) && !c.homeroom_signature).length : 0,
      page: 'pengaturan', text: 'tanda tangan digital belum ditambahkan (kepala sekolah/wali kelas). Yang kosong tercetak tanpa tanda tangan.' },
  ].filter((c) => c.n > 0);
  if (checks.length) {
    content.appendChild(h('section', {},
      h('div', { class: 'section-title' }, h('h2', {}, 'Perlu dicek')),
      h('div', { class: 'sheet checklist' }, checks.map((c) => h('div', { class: `item ${c.kind || 'warn'}` },
        h('span', { class: 'ic' }, icon(c.kind === 'danger' ? 'alert' : 'info')), h('span', { class: 'txt' }, h('b', {}, c.n), ' ', c.text),
        h('button', { type: 'button', class: 'link', onclick: () => app.go(c.page) }, 'Buka', icon('arrow')))))));
  }

  content.appendChild(dataSection(app));
}

function dataSection(app) {
  const status = backupStatus(app.data.meta);
  const backup = link('Backup ke flashdisk', async () => { if (await runExternalBackup(app)) app.go('data'); });
  if (status === 'ok') {
    return h('p', { class: 'muted small' }, `Data tersimpan otomatis · backup ke luar terakhir ${fmtTime(app.data.meta.last_external_backup_at)}. `, link('Data & arsip', () => app.go('data')));
  }
  return h('p', { class: 'note warn' },
    h('b', {}, status === 'never' ? 'Data belum pernah di-backup ke luar laptop ini. ' : 'Ada perubahan yang belum di-backup ke flashdisk. '),
    'Kalau laptop rusak, data ikut hilang. ', backup, ' · ', link('Data & arsip', () => app.go('data')));
}
