// Wizard impor Excel: pilih file → pilih sheet & kelas → hasil.
import { api } from './api.js';
import { h, openModal, errorText, toast } from './dom.js';

const KIND = {
  students: { title: 'Impor data siswa', what: 'data siswa (ekspor Dapodik)' },
  scores: { title: 'Impor nilai mapel', what: 'rekap nilai (kolom NAMA SISWA + baris PH/PTS)' },
  attendance: { title: 'Impor absensi (kehadiran)', what: 'rekap absensi (kolom Nama + Sakit/Izin/Alpa atau Kehadiran)' },
};

/**
 * @param {'students'|'scores'|'attendance'} kind
 * @param {() => void} onDone dipanggil setelah impor berhasil
 * @param {object} [app] state aplikasi (dipakai untuk daftar kelas & hari efektif pada impor absensi)
 */
export async function importWizard(kind, onDone, app = null) {
  const info = KIND[kind];
  let path;
  try {
    path = await api.pickExcelFile();
  } catch (e) {
    toast(errorText(e), 'error');
    return;
  }
  if (!path) return;

  const modal = openModal({ title: info.title, body: h('p', { class: 'muted' }, 'Membaca file…'), wide: true });
  let scans;
  try {
    scans = await api.scanExcel(path);
  } catch (e) {
    modal.setBody(h('div', { class: 'note danger' }, errorText(e)));
    return;
  }
  const sheets = scans.filter((s) => s.kind === kind);
  if (!sheets.length) {
    modal.setBody(h('div', { class: 'stack' },
      h('div', { class: 'note warn' }, `Di file ini tidak ditemukan sheet ${info.what}.`),
      h('p', { class: 'hint' }, `Sheet yang ada: ${scans.map((s) => s.name).join(', ') || '—'}`),
    ));
    return;
  }

  let sheet = sheets[0];
  const classBox = h('div', { class: 'grid-3' });
  const classHint = h('div', { class: 'eyebrow', style: { marginBottom: '8px' } }, 'Kelas yang diimpor');
  const chosen = new Set();
  let effectiveDays = app?.data?.settings?.effective_days || 100;
  let emptyAsZero = true;

  function renderClasses() {
    classBox.replaceChildren();
    chosen.clear();
    const namedClasses = sheet.classes.filter((c) => c.name);
    if (namedClasses.length) {
      classHint.textContent = 'Kelas yang diimpor';
      const hasNine = namedClasses.some((c) => c.name.startsWith('IX-'));
      for (const c of namedClasses) {
        const on = kind === 'attendance' && app?.selectedClass && namedClasses.some((x) => x.name === app.selectedClass)
          ? true
          : hasNine ? c.name.startsWith('IX-') : true;
        if (on) chosen.add(c.name);
        classBox.appendChild(
          h('label', { class: 'check' },
            h('input', { type: 'checkbox', checked: on, onchange: (e) => (e.target.checked ? chosen.add(c.name) : chosen.delete(c.name)) }),
            `${c.name} `, h('span', { class: 'muted small' }, `(${c.count} siswa)`)),
        );
      }
    } else if (app?.classes?.().length) {
      // File tidak mencantumkan nama kelas: pilih dari kelas yang sudah ada di aplikasi.
      const totalRows = sheet.classes.reduce((a, c) => a + c.count, 0);
      classHint.textContent = `Cocokkan ${totalRows} baris di file ini ke kelas`;
      const appClasses = app.classes();
      const def = app.selectedClass && appClasses.includes(app.selectedClass) ? app.selectedClass : appClasses[0];
      for (const c of appClasses) {
        const on = c === def || appClasses.length === 1;
        if (on) chosen.add(c);
        classBox.appendChild(
          h('label', { class: 'check' },
            h('input', { type: 'checkbox', checked: on, onchange: (e) => (e.target.checked ? chosen.add(c) : chosen.delete(c)) }),
            c),
        );
      }
    }
  }
  renderClasses();

  const sheetPicker = sheets.length > 1
    ? h('label', { class: 'field' }, h('span', { class: 'lbl' }, 'Sheet'),
      h('select', { onchange: (e) => { sheet = sheets[Number(e.target.value)]; renderClasses(); } },
        sheets.map((s, i) => h('option', { value: String(i) }, s.name))))
    : h('p', {}, 'Sheet: ', h('b', {}, sheet.name));

  const desc = kind === 'scores'
    ? h('p', { class: 'hint' }, 'Mapel yang terbaca: ', h('b', {}, sheet.subjects.join(', ')),
      '. Semua mapel disimpan, jadi mapel baru di rapor tidak perlu impor ulang. Sel kosong di file tidak menghapus nilai yang sudah ada.')
    : kind === 'attendance'
      ? h('div', { class: 'stack', style: { gap: '10px' } },
        h('p', { class: 'hint' }, 'Kolom yang terbaca: ', h('b', {}, sheet.subjects.join(', ') || 'Kehadiran'),
          '. Jumlah hari Sakit (S), Izin (I), dan Alpa (A) otomatis dihitung menjadi persentase kehadiran sesuai template rapor, misalnya ', h('b', {}, '98% (Present)'), '.'),
        h('div', { style: { display: 'flex', alignItems: 'center', gap: '10px', flexWrap: 'wrap' } },
          h('span', { class: 'small', style: { fontWeight: '600' } }, 'Total hari efektif semester:'),
          h('input', {
            type: 'number', min: 1, max: 366, value: effectiveDays,
            style: { width: '82px', minHeight: '32px', padding: '4px 8px' },
            oninput: (e) => { effectiveDays = Number(e.target.value) || 0; },
          }),
          h('span', { class: 'small muted' }, 'hari · rumus: (Hari efektif − (S + I + A)) ÷ Hari efektif × 100%')),
        h('label', { class: 'check small' },
          h('input', { type: 'checkbox', checked: emptyAsZero, onchange: (e) => { emptyAsZero = e.target.checked; } }),
          'Sel S / I / A yang kosong di file dianggap 0 hari (hadir penuh 100%)'))
      : h('p', { class: 'hint' }, 'Yang diambil hanya nama, NIS, NISN, jenis kelamin, dan kelas. Data pribadi lain (NIK, alamat, data orang tua) tidak disimpan. Nilai dan isian rapor siswa yang sudah ada tidak berubah.');

  modal.setBody(h('div', { class: 'stack' },
    h('p', { class: 'hint' }, 'File: ', h('b', {}, path)),
    sheetPicker,
    desc,
    h('div', {}, classHint, classBox),
  ));
  modal.setActions([
    { label: 'Batal', kind: 'quiet', onClick: (c) => c() },
    {
      label: 'Impor', kind: 'primary',
      onClick: async () => {
        const classes = [...chosen];
        if (!classes.length) { toast('Pilih minimal satu kelas', 'error'); return; }
        if (kind === 'attendance' && (effectiveDays < 1 || effectiveDays > 366)) {
          toast('Hari efektif harus antara 1 dan 366 hari', 'error');
          return;
        }
        modal.setActions([{ label: 'Memproses…', disabled: true }]);
        try {
          const rep = kind === 'students'
            ? await api.importStudents(path, sheet.name, classes)
            : kind === 'scores'
              ? await api.importScores(path, sheet.name, classes)
              : await api.importAttendance(path, sheet.name, classes, effectiveDays, emptyAsZero);
          modal.setBody(kind === 'students' ? studentResult(rep) : kind === 'scores' ? scoreResult(rep) : attendanceResult(rep));
          modal.setActions([{ label: 'Selesai', kind: 'primary', onClick: (c) => c() }]);
          onDone?.();
        } catch (e) {
          modal.setBody(h('div', { class: 'note danger' }, errorText(e)));
          modal.setActions([{ label: 'Tutup', onClick: (c) => c() }]);
        }
      },
    },
  ]);
}

function studentResult(rep) {
  const parts = [h('div', { class: 'note ok' },
    h('b', {}, 'Impor selesai. '),
    `${rep.added} siswa baru, ${rep.updated} diperbarui. Kelas: ${rep.classes.join(', ') || '—'}.`)];
  if (rep.rombel_conflicts.length) {
    parts.push(h('div', { class: 'note warn' },
      h('b', {}, `${rep.rombel_conflicts.length} siswa: kolom Rombel di Dapodik berbeda dengan judul kelas di file.`),
      ' Yang dipakai judul kelas (sama dengan rekap nilai). Cek apakah memang benar:',
      h('ul', {}, rep.rombel_conflicts.slice(0, 30).map((t) => h('li', {}, t))),
      rep.rombel_conflicts.length > 30 ? h('p', { class: 'small' }, `…dan ${rep.rombel_conflicts.length - 30} lainnya.`) : null));
  }
  return h('div', { class: 'stack' }, parts);
}

function scoreResult(rep) {
  const parts = [
    h('div', { class: 'note ok' }, h('b', {}, 'Impor selesai. '),
      `${rep.matched} siswa cocok, ${rep.values_set} nilai dimasukkan.`),
  ];
  if (rep.approximate.length) {
    parts.push(h('div', { class: 'note warn' },
      h('b', {}, `${rep.approximate.length} nama dicocokkan secara perkiraan. Tolong dicek:`),
      h('ul', {}, rep.approximate.map((a) =>
        h('li', {}, `${a.class_name}: "${a.from_name}" → ${a.to_name}`, h('span', { class: 'muted' }, a.kind === 'abbreviation' ? ' (singkatan)' : ' (ejaan mirip)')))),
    ));
  }
  if (rep.unmatched.length) {
    parts.push(h('div', { class: 'note danger' },
      h('b', {}, `${rep.unmatched.length} baris tidak dimasukkan:`),
      h('ul', {}, rep.unmatched.map((u) => h('li', {}, `${u.class_name || '(tanpa kelas)'}: ${u.name} — ${u.reason}`))),
      h('p', { class: 'small', style: { marginTop: '6px' } }, 'Tambahkan siswanya di Data Siswa (atau samakan namanya), lalu impor ulang.'),
    ));
  }
  if (rep.invalid_values) {
    parts.push(h('div', { class: 'note warn' }, `${rep.invalid_values} nilai dilewati karena di luar 0–100.`));
  }
  return h('div', { class: 'stack' }, parts);
}

function attendanceResult(rep) {
  const parts = [
    h('div', { class: 'note ok' }, h('b', {}, 'Impor absensi selesai. '),
      `${rep.matched} siswa cocok, ${rep.values_set} kehadiran dihitung otomatis (hari efektif: ${rep.effective_days} hari).`),
  ];
  if (rep.approximate.length) {
    parts.push(h('div', { class: 'note warn' },
      h('b', {}, `${rep.approximate.length} nama dicocokkan secara perkiraan. Tolong dicek:`),
      h('ul', {}, rep.approximate.map((a) =>
        h('li', {}, `${a.class_name}: "${a.from_name}" → ${a.to_name}`, h('span', { class: 'muted' }, a.kind === 'abbreviation' ? ' (singkatan)' : ' (ejaan mirip)')))),
    ));
  }
  if (rep.unmatched.length) {
    parts.push(h('div', { class: 'note danger' },
      h('b', {}, `${rep.unmatched.length} baris tidak dimasukkan:`),
      h('ul', {}, rep.unmatched.map((u) => h('li', {}, `${u.class_name || '(tanpa kelas)'}: ${u.name} — ${u.reason}`))),
    ));
  }
  return h('div', { class: 'stack' }, parts);
}
