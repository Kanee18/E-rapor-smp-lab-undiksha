// Pengaturan: identitas rapor, rumus & predikat, mapel, ATL, proyek, wali kelas, desain.
// Perubahan baru tersimpan setelah klik "Simpan" di bar bawah.
import { api } from '../api.js';
import { h, autoGrow, button, confirmBox, debounce, errorText, icon, link, toast } from '../dom.js';
import { PHRASE_CATEGORIES } from '../narrative.js';
import { openSignatureEditor } from '../signature.js';

const newId = (prefix) => `${prefix}${Date.now().toString(36)}${Math.random().toString(36).slice(2, 5)}`;

export async function render(ctx) {
  const { app, content, setTitle, onLeave, onScroll } = ctx;
  setTitle('Pengaturan', 'Berlaku untuk semua rapor.');
  let draft = structuredClone(app.data.settings);
  let dirty = false;

  const saveBar = h('div', { class: 'save-bar', role: 'region', 'aria-label': 'Simpan perubahan' },
    h('span', {}, 'Ada perubahan yang belum disimpan'), h('span', { class: 'spacer' }),
    button('Batalkan', { kind: 'quiet', onClick: () => { draft = structuredClone(app.data.settings); setDirty(false); draw(); } }),
    button('Simpan', { kind: 'primary', onClick: save }));
  const setDirty = (v) => { dirty = v; saveBar.classList.toggle('show', v); };
  const markDirty = () => setDirty(true);

  async function save() {
    try {
      await api.saveSettings(draft);
      await app.reload();
      draft = structuredClone(app.data.settings);
      setDirty(false);
      toast('Pengaturan tersimpan', 'ok');
      draw();
    } catch (e) {
      toast(errorText(e), 'error');
    }
  }

  onLeave(async (opts) => (dirty
    ? (opts?.force
      // dikunci otomatis: tidak bisa bertanya, perubahan dibatalkan dan diberitahukan setelah dibuka
      ? ((app.notice = 'Perubahan pengaturan yang belum disimpan dibatalkan karena aplikasi terkunci.'), true)
      : confirmBox('Ada perubahan pengaturan yang belum disimpan. Tinggalkan tanpa menyimpan?', { ok: 'Tinggalkan', danger: true }))
    : true));

  // ---------- isian terikat ke objek
  const text = (label, obj, key, { multiline = false, hint, placeholder, rows = 2 } = {}) => h('label', { class: 'field' },
    h('span', { class: 'lbl' }, label),
    multiline
      ? h('textarea', { rows, value: obj[key] ?? '', placeholder, oninput: (e) => { obj[key] = e.target.value; markDirty(); } })
      : h('input', { value: obj[key] ?? '', placeholder, oninput: (e) => { obj[key] = e.target.value; markDirty(); } }),
    hint ? h('span', { class: 'hint' }, hint) : null);

  const num = (label, obj, key, onChange) => h('label', { class: 'field' }, h('span', { class: 'lbl' }, label),
    h('input', { type: 'number', min: 0, max: 100, step: 'any', value: String(obj[key]), oninput: (e) => {
      const v = Number(e.target.value);
      obj[key] = Number.isFinite(v) ? v : 0;
      markDirty();
      onChange?.();
    } }));

  const check = (label, obj, key) => h('label', { class: 'check' },
    h('input', { type: 'checkbox', checked: Boolean(obj[key]), onchange: (e) => { obj[key] = e.target.checked; markDirty(); } }), label);

  /** Tabel daftar yang bisa ditambah/hapus/diurutkan; sel bisa langsung diedit. */
  function listEditor(items, columns, { addLabel, make, confirmDelete, fixed = false } = {}) {
    const wrap = h('div', { class: 'sheet' });
    const cellInput = (item, c) => {
      if (c.type === 'readonly') return h('div', { class: 'name', style: { padding: '5px 7px' } }, item[c.key]);
      const common = { placeholder: c.placeholder, 'aria-label': c.label, oninput: (e) => { item[c.key] = c.type === 'number' ? Number(e.target.value) : e.target.value; markDirty(); } };
      if (c.type === 'select') {
        return h('select', { 'aria-label': c.label, onchange: (e) => { item[c.key] = e.target.value; markDirty(); } },
          c.options.map((o) => h('option', { value: o.value, selected: item[c.key] === o.value }, o.label)),
          c.options.some((o) => o.value === item[c.key]) ? null : h('option', { value: item[c.key], selected: true }, item[c.key] || '—'));
      }
      if (c.type === 'textarea') return autoGrow(h('textarea', { rows: 1, value: item[c.key] ?? '', ...common }));
      if (c.type === 'number') return h('input', { type: 'number', step: 'any', min: 0, max: 100, value: String(item[c.key]), ...common });
      return h('input', { value: item[c.key] ?? '', list: c.list, ...common });
    };
    const drawTable = () => {
      const move = (i, d) => { [items[i + d], items[i]] = [items[i], items[i + d]]; markDirty(); drawTable(); };
      wrap.replaceChildren(
        h('table', { class: 'ledger edit' },
          h('thead', {}, h('tr', {}, columns.map((c) => h('th', { style: c.width ? { width: c.width } : {} }, c.label)), fixed ? null : h('th', {}, ''))),
          h('tbody', {}, items.map((item, i) => h('tr', {},
            columns.map((c) => h('td', {}, cellInput(item, c))),
            fixed ? null : h('td', { class: 'tight' },
              h('button', { type: 'button', class: 'icon-btn', title: 'Naik', 'aria-label': 'Naik', disabled: i === 0, onclick: () => move(i, -1) }, icon('up')),
              h('button', { type: 'button', class: 'icon-btn', title: 'Turun', 'aria-label': 'Turun', disabled: i === items.length - 1, onclick: () => move(i, 1) }, icon('down')),
              h('button', { type: 'button', class: 'icon-btn', title: 'Hapus', 'aria-label': 'Hapus', onclick: async () => {
                if (confirmDelete && !(await confirmBox(confirmDelete(item), { ok: 'Hapus', danger: true }))) return;
                items.splice(i, 1); markDirty(); drawTable();
              } }, icon('trash'))))))),
        addLabel ? h('div', { class: 'add-row' }, link(`+ ${addLabel}`, () => { items.push(make()); markDirty(); drawTable(); })) : '');
    };
    drawTable();
    return wrap;
  }

  const sections = [];
  const set = (id, title, lede, ...body) => {
    sections.push({ id, title });
    return h('section', { class: 'set', id: `set-${id}` }, h('h2', {}, title), lede ? h('p', { class: 'lede' }, lede) : null, ...body);
  };

  // ---------- tanda tangan: pilih mode, lalu kelola gambar per orang
  function signatureSection(d) {
    const choice = (value, label, desc) => h('label', { class: `choice${d.digital_signature === value ? ' on' : ''}` },
      h('input', { type: 'radio', name: 'sig-mode', checked: d.digital_signature === value, onchange: () => { d.digital_signature = value; markDirty(); draw(); } }),
      h('span', {}, h('b', {}, label), h('span', { class: 'small muted' }, desc)));
    const card = (role, person, title, img, setImg) => h('div', { class: `sig-card${img ? ' has' : ''}` },
      h('div', { class: 'sig-thumb' }, img ? h('img', { src: img, alt: `Tanda tangan ${role}` }) : h('span', { class: 'small muted' }, 'Belum ada tanda tangan')),
      h('div', { class: 'sig-meta' }, h('b', {}, role), h('span', { class: 'small muted' }, person || 'Nama belum diisi')),
      h('div', { class: 'row' },
        button(img ? 'Ganti…' : 'Tambah tanda tangan…', { kind: 'sm', onClick: () => openSignatureEditor({
          role, name: person, title, current: img,
          onSave: (url) => { setImg(url); markDirty(); draw(); toast('Tanda tangan siap. Klik Simpan di bawah untuk menyimpan.', 'ok'); },
        }) }),
        img ? button('Hapus', { kind: 'sm quiet', onClick: () => { setImg(''); markDirty(); draw(); } }) : null));
    return h('div', { class: 'stack', style: { gap: '16px' } },
      h('div', { class: 'choices' },
        choice(false, 'Tanda tangan basah', 'Ruang tanda tangan dikosongkan; ditandatangani dengan pulpen setelah dicetak.'),
        choice(true, 'Tanda tangan digital', 'Gambar tanda tangan langsung tercetak di setiap rapor.')),
      d.digital_signature
        ? h('div', { class: 'stack', style: { gap: '12px' } },
          h('div', { class: 'sig-cards' },
            card('Kepala sekolah', d.principal_name, d.principal_title, d.principal_signature, (v) => { d.principal_signature = v; }),
            d.classes.map((c) => card(`Wali kelas ${c.name}`, c.homeroom_teacher, d.homeroom_title, c.homeroom_signature, (v) => { c.homeroom_signature = v; }))),
          d.classes.length ? null : h('p', { class: 'muted small' }, 'Kartu wali kelas muncul setelah data siswa diimpor.'),
          h('p', { class: 'hint' }, 'Tanda tangan yang belum ada dibiarkan kosong di rapor. Gambar tanda tangan ikut tersimpan di data aplikasi dan backup, jadi simpan flashdisk backup di tempat aman.'))
        : (d.principal_signature || d.classes.some((c) => c.homeroom_signature)
          ? h('p', { class: 'hint' }, 'Gambar tanda tangan yang sudah ditambahkan tetap disimpan, tetapi tidak dicetak selama mode ini dipilih.')
          : null));
  }

  function draw() {
    sections.length = 0;
    const d = draft;
    const sum = h('span', { class: 'hint' });
    const updateSum = () => { sum.textContent = `Total bobot ${d.grading.weight_ph + d.grading.weight_pts}%, dihitung sebagai rata-rata berbobot.`; };
    updateSum();

    // Logo & posisinya langsung disimpan, tapi HANYA bagian itu: diterapkan ke pengaturan yang
    // tersimpan, bukan ke draf. Perubahan lain di halaman ini yang belum diklik Simpan tetap
    // berstatus belum tersimpan (tidak ikut tersimpan diam-diam).
    const saveLogoSettings = async (patch, msg) => {
      try {
        await api.saveSettings({ ...structuredClone(app.data.settings), ...patch });
        await app.reload();
        for (const k of Object.keys(patch)) d[k] = app.data.settings[k];
        toast(msg, 'ok');
        draw();
      } catch (e) {
        toast(errorText(e), 'error');
      }
    };

    const normPos = (p, def) => (['left', 'right', 'top'].includes(p) ? p : def);
    const setLogoPos = (posKey, otherPosKey, otherDef, newPos, label) => {
      const curOther = normPos(d[otherPosKey], otherDef);
      const patch = { [posKey]: newPos };
      // Kalau pindah ke Kiri/Kanan yang sedang ditempati logo satunya, tukar otomatis supaya tidak menumpuk.
      if ((newPos === 'left' || newPos === 'right') && curOther === newPos) {
        patch[otherPosKey] = newPos === 'left' ? 'right' : 'left';
      }
      const posName = { left: 'sebelah kiri judul', right: 'sebelah kanan judul', top: 'di atas judul' }[newPos];
      saveLogoSettings(patch, `Posisi ${label.toLowerCase()} dipindah ke ${posName}`);
    };

    const POS_LABELS = [['left', 'Kiri'], ['right', 'Kanan'], ['top', 'Atas']];

    const logoPicker = (label, key, posKey, defPos, otherPosKey, otherDef) => {
      const hasLogo = typeof d[key] === 'string' && d[key].startsWith('data:image/');
      const curPos = normPos(d[posKey], defPos);
      const posName = { left: 'Sebelah kiri judul', right: 'Sebelah kanan judul', top: 'Di atas judul' }[curPos];
      return h('div', { class: 'logo-box' },
        h('div', { class: 'preview' }, hasLogo ? h('img', { src: d[key], alt: label }) : 'Belum ada logo'),
        h('div', { class: 'stack', style: { gap: '7px' } },
          h('div', { style: { display: 'flex', alignItems: 'center', gap: '8px', flexWrap: 'wrap' } },
            h('b', { style: { fontSize: '13.5px' } }, label),
            h('span', { class: 'small muted' }, `· ${posName}`)),
          h('div', { class: 'row', style: { display: 'flex', alignItems: 'center', gap: '8px', flexWrap: 'wrap' } },
            h('span', { class: 'small muted' }, 'Posisi:'),
            h('div', { class: 'segs' }, POS_LABELS.map(([val, textLbl]) =>
              h('button', {
                type: 'button',
                class: `seg wide${curPos === val ? ' on' : ''}`,
                style: { height: '26px', padding: '0 10px', fontSize: '12px' },
                onclick: () => { if (curPos !== val) setLogoPos(posKey, otherPosKey, otherDef, val, label); },
              }, textLbl)))),
          h('div', { class: 'row', style: { display: 'flex', alignItems: 'center', gap: '8px', flexWrap: 'wrap' } },
            button(hasLogo ? 'Ganti logo…' : 'Pilih logo…', { kind: 'sm', onClick: async () => {
              try {
                const path = await api.pickImageFile(`Pilih ${label.toLowerCase()}`);
                if (!path) return;
                const rawUrl = await api.loadLogo(path);
                const resized = await shrinkLogo(rawUrl, 220);
                await saveLogoSettings({ [key]: resized }, `${label} disimpan`);
              } catch (e) { toast(errorText(e), 'error'); }
            } }),
            hasLogo ? button('Hapus', { kind: 'sm quiet', onClick: () => saveLogoSettings({ [key]: 'none' }, `${label} dihapus`) }) : null,
            !hasLogo && key === 'logo'
              ? button('Pakai logo bawaan', { kind: 'sm quiet', onClick: () => saveLogoSettings({ logo: '' }, 'Logo sekolah bawaan dipasang') })
              : null)));
    };

    const schoolPos = normPos(d.logo_position, 'left');
    const univPos = normPos(d.university_logo_position, 'right');
    const hasImg = (v) => typeof v === 'string' && v.startsWith('data:image/');
    const logosAt = (pos) => [
      schoolPos === pos && hasImg(d.logo) ? { src: d.logo, alt: 'Logo sekolah' } : null,
      univPos === pos && hasImg(d.university_logo) ? { src: d.university_logo, alt: 'Logo kampus' } : null,
    ].filter(Boolean);

    const headerPreview = h('div', { class: 'header-preview' },
      h('div', { class: 'hp-top-bar' },
        h('span', { class: 'small muted' }, 'Pratinjau posisi logo di kepala rapor:'),
        button('⇄ Tukar posisi Kiri ↔ Kanan', { kind: 'sm', onClick: () => {
          const nextSchool = schoolPos === 'left' ? 'right' : schoolPos === 'right' ? 'left' : 'right';
          const nextUniv = univPos === 'right' ? 'left' : univPos === 'left' ? 'right' : 'left';
          saveLogoSettings(
            { logo_position: nextSchool, university_logo_position: nextUniv },
            'Posisi logo sekolah & kampus ditukar',
          );
        } })),
      logosAt('top').length
        ? h('div', { class: 'hp-top-logos' }, logosAt('top').map((l) => h('img', { src: l.src, alt: l.alt })))
        : null,
      h('div', { class: 'hp-grid' },
        h('div', { class: 'hp-slot' },
          logosAt('left').length
            ? logosAt('left').map((l) => h('img', { src: l.src, alt: l.alt }))
            : h('span', { class: 'hp-empty' }, 'Kiri kosong')),
        h('div', { class: 'hp-text' },
          h('div', { class: 'hp-school' }, d.school_name || 'NAMA SEKOLAH'),
          h('div', { class: 'hp-title' }, (d.report_title || 'JUDUL RAPOR').split('\n').filter(Boolean).map((line, i) => [i > 0 ? h('br') : null, line])),
          h('div', { class: 'hp-sub' }, d.report_subtitle || '')),
        h('div', { class: 'hp-slot' },
          logosAt('right').length
            ? logosAt('right').map((l) => h('img', { src: l.src, alt: l.alt }))
            : h('span', { class: 'hp-empty' }, 'Kanan kosong'))));

    const body = [
      set('identitas', 'Identitas rapor', 'Atur gambar dan posisi logo sekolah maupun logo universitas/kampus (kiri, kanan, atau atas), teks kepala rapor, serta tanda tangan.',
        h('div', { class: 'grid-2', style: { marginBottom: '14px' } },
          logoPicker('Logo sekolah', 'logo', 'logo_position', 'left', 'university_logo_position', 'right'),
          logoPicker('Logo universitas / kampus', 'university_logo', 'university_logo_position', 'right', 'logo_position', 'left')),
        headerPreview,
        h('div', { class: 'grid-2' },
          text('Nama sekolah', d, 'school_name', { hint: 'Baris kecil di atas judul.' }),
          text('Judul rapor', d, 'report_title', { multiline: true, hint: 'Enter untuk baris baru.' }),
          text('Sub-judul', d, 'report_subtitle', { placeholder: 'STUDENT PROGRESS REPORT — TERM 1' }),
          text('Tahun ajaran', d, 'academic_year', { placeholder: '2026/2027' }),
          text('Tempat & tanggal', d, 'place_date', { placeholder: 'Singaraja, 30 November 2026' }),
          text('Nama kepala sekolah', d, 'principal_name', { hint: 'Kosong = garis titik-titik.' }),
          text('Jabatan kepala sekolah', d, 'principal_title'),
          text('Jabatan wali kelas', d, 'homeroom_title'),
          h('label', { class: 'field' }, h('span', { class: 'lbl' }, 'Ukuran kertas'),
            h('select', { onchange: (e) => { d.paper_size = e.target.value; markDirty(); } },
              [['A4', 'A4 — 210 × 297 mm'], ['F4', 'F4 / Folio — 215 × 330 mm'], ['Letter', 'Letter — 216 × 279 mm']]
                .map(([v, t]) => h('option', { value: v, selected: d.paper_size === v }, t)))),
          h('div', { style: { alignSelf: 'end', paddingBottom: '6px' } }, check('Tampilkan angka di samping predikat mapel, mis. "ME (92)"', d, 'show_numeric_score')))),

      set('penilaian', 'Rumus & predikat',
        'Nilai akhir mapel = rata-rata berbobot PH dan PTS. Predikat ditentukan dari batas bawah: nilai ≥ batas bawah EE menjadi EE, dan seterusnya. Nilai di bawah predikat terendah tetap memakai predikat terendah tapi ditandai. Rentangnya dicetak sebagai keterangan di rapor.',
        h('div', { class: 'grid-3', style: { marginBottom: '16px' } },
          num('Bobot PH (%)', d.grading, 'weight_ph', updateSum),
          num('Bobot PTS (%)', d.grading, 'weight_pts', updateSum),
          h('div', { style: { alignSelf: 'end', paddingBottom: '6px' } }, check('Bulatkan nilai akhir (85,5 → 86)', d.grading, 'round_final'))),
        h('div', { style: { margin: '-8px 0 16px' } }, sum),
        listEditor(d.grading.bands, [
          { key: 'code', label: 'Kode', width: '90px' },
          { key: 'label', label: 'Keterangan di rapor' },
          { key: 'min', label: 'Batas bawah', type: 'number', width: '120px' },
          { key: 'max', label: 'Batas atas', type: 'number', width: '120px' },
        ], { addLabel: 'Tambah predikat', make: () => ({ code: '', label: '', min: 0, max: 0 }) })),

      set('mapel', 'Mapel',
        'Satu baris = satu baris di bagian 3 rapor; jumlahnya bebas. "Sumber nilai" adalah judul kolom di file rekap, misalnya IPA, B. INDO, atau MAT.',
        h('datalist', { id: 'score-columns' }, app.data.score_columns.map((c) => h('option', { value: c }))),
        listEditor(d.subjects, [
          { key: 'name', label: 'Nama di rapor', width: '220px' },
          { key: 'source_key', label: 'Sumber nilai', width: '140px', list: 'score-columns', placeholder: 'mis. MAT' },
          { key: 'indicator', label: 'Indikator', type: 'textarea' },
        ], { addLabel: 'Tambah mapel', make: () => ({ id: newId('m'), name: '', source_key: '', indicator: '' }) }),
        app.data.score_columns.length
          ? h('p', { class: 'hint', style: { marginTop: '8px' } }, `Kolom yang sudah diimpor: ${app.data.score_columns.join(', ')}.`)
          : null),

      set('atl', 'ATL & Learner Profile', 'Atribut dan indikator di tabel bagian 1. Penilaiannya diisi per siswa di Isi rapor.',
        listEditor(d.atl, [
          { key: 'name', label: 'Atribut', width: '220px' },
          { key: 'indicator', label: 'Indikator', type: 'textarea' },
        ], { addLabel: 'Tambah atribut', make: () => ({ id: newId('a'), name: '', indicator: '' }) })),

      set('proyek', 'Proyek (UOI)', 'Tema, central idea, dan lines of inquiry (satu baris = satu poin). Kolom fasilitator hanya catatan, tidak dicetak.',
        listEditor(d.projects, [
          { key: 'name', label: 'Nama', width: '110px' },
          { key: 'theme', label: 'Tema', width: '160px' },
          { key: 'central_idea', label: 'Central idea', type: 'textarea' },
          { key: 'lines_of_inquiry', label: 'Lines of inquiry', type: 'textarea', rows: 3 },
          { key: 'facilitators', label: 'Fasilitator', width: '140px', type: 'textarea' },
        ], {
          addLabel: 'Tambah proyek',
          make: () => ({ id: newId('p'), name: `Project ${d.projects.length + 1}`, theme: '', central_idea: '', lines_of_inquiry: '', facilitators: '' }),
          confirmDelete: (p) => `Hapus ${p.name}? Siswa yang sudah diberi proyek ini akan menampilkan "(proyek dihapus)".`,
        })),

      set('kalimat', 'Bank kalimat narasi',
        'Kalimat siap pakai untuk narasi guru, dipilih lewat "Bank kalimat…" di Isi rapor. Isian otomatis: {name} = nama siswa, {their} = his/her, {they} = he/she, {them} = him/her (dari jenis kelamin), {theme} = tema proyek.',
        listEditor(d.narrative_bank, [
          { key: 'category', label: 'Kelompok', type: 'select', width: '200px', options: PHRASE_CATEGORIES(d.grading.bands, true) },
          { key: 'text', label: 'Kalimat (bahasa Inggris)', type: 'textarea' },
        ], { addLabel: 'Tambah kalimat', make: () => ({ id: newId('k'), category: 'general', text: '' }) })),

      set('kelas', 'Wali kelas', 'Kelas muncul otomatis dari data siswa.',
        d.classes.length
          ? listEditor(d.classes, [
            { key: 'name', label: 'Kelas', type: 'readonly', width: '120px' },
            { key: 'homeroom_teacher', label: 'Nama wali kelas, lengkap dengan gelar', placeholder: 'mis. Ni Ketut …, S.Pd.' },
          ], { fixed: true })
          : h('p', { class: 'muted' }, 'Belum ada kelas. Impor data siswa dulu.')),

      set('ttd', 'Tanda tangan', 'Setiap rapor memuat dua tanda tangan: wali kelas (berbeda per kelas) dan kepala sekolah.',
        signatureSection(d)),

      set('desain', 'Desain rapor', null, templateSection(app, () => draw())),
    ];

    // daftar isi: klik untuk melompat; bagian yang sedang dibaca ditandai saat menggulir
    let jumping = null;
    const tocButtons = sections.map((s, i) => h('button', { type: 'button', onclick: () => {
      markToc(i);
      jumping = i;
      setTimeout(() => { jumping = null; }, 1500);
      document.getElementById(`set-${s.id}`)?.scrollIntoView({ behavior: 'smooth', block: 'start' });
    } }, s.title));
    const markToc = (i) => tocButtons.forEach((b, j) => { b.classList.toggle('active', i === j); b.setAttribute('aria-current', i === j ? 'true' : 'false'); });
    const toc = h('nav', { class: 'toc', 'aria-label': 'Bagian pengaturan' }, tocButtons);
    content.replaceChildren(h('div', { class: 'settings' }, toc, h('div', {}, body, saveBar)));
    const spy = debounce(() => {
      const limit = content.getBoundingClientRect().top + 120;
      let current = 0;
      sections.forEach((s, i) => { if ((document.getElementById(`set-${s.id}`)?.getBoundingClientRect().top ?? Infinity) <= limit) current = i; });
      if (content.scrollTop + content.clientHeight >= content.scrollHeight - 4) current = sections.length - 1;
      // setelah lompat lewat daftar isi, tanda tetap di bagian yang diklik
      if (jumping !== null) { const j = jumping; jumping = null; markToc(j); return; }
      markToc(current);
    }, 60);
    onScroll(spy);
    markToc(0);
  }

  draw();
}

export function shrinkLogo(dataUrl, maxPx = 220) {
  return new Promise((resolve) => {
    const img = new Image();
    img.onload = () => {
      if (img.width <= maxPx && img.height <= maxPx && dataUrl.length < 25_000) {
        resolve(dataUrl);
        return;
      }
      const scale = Math.min(1, maxPx / Math.max(img.width, img.height));
      const w = Math.max(1, Math.round(img.width * scale));
      const h = Math.max(1, Math.round(img.height * scale));
      const canvas = document.createElement('canvas');
      canvas.width = w;
      canvas.height = h;
      const ctx = canvas.getContext('2d');
      ctx.drawImage(img, 0, 0, w, h);
      const png = canvas.toDataURL('image/png');
      const webp = canvas.toDataURL('image/webp', 0.9);
      resolve(webp.startsWith('data:image/webp') && webp.length < png.length ? webp : png);
    };
    img.onerror = () => resolve(dataUrl);
    img.src = dataUrl;
  });
}

function templateSection(app, redraw) {
  const active = app.customTemplate;
  const run = (fn, msg) => async () => {
    try { const p = await fn(); toast(msg(p), 'ok', 6000); await app.reload(); redraw(); } catch (e) { toast(errorText(e), 'error'); }
  };
  return h('div', { class: 'stack' },
    h('p', { class: `note ${active ? 'warn' : ''}` }, active
      ? 'Sedang memakai desain kustom: template-rapor.html di folder data.'
      : 'Sedang memakai desain bawaan, mengikuti template docx dari sekolah.'),
    h('p', { class: 'hint' },
      'Untuk mengubah tampilan rapor (logo, warna, susunan), salin desain bawaan ke folder data lalu edit template-rapor.html (HTML dengan isian {{ … }}). Aplikasi otomatis memakai file itu.'),
    h('div', { class: 'row' },
      active
        ? button('Kembali ke desain bawaan', { onClick: run(api.resetTemplate, (p) => `Desain kustom disimpan sebagai ${p}`) })
        : button('Salin desain bawaan ke folder data', { onClick: run(api.exportTemplate, (p) => `Disalin ke ${p}`) }),
      button('Buka folder data', { kind: 'quiet', onClick: () => api.openDataFolder().catch((e) => toast(errorText(e), 'error')) })));
}
