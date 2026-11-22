// Bank kalimat narasi: pilih kalimat, isian {name}/{their}/{theme} diisi otomatis.
import { h, openModal } from './dom.js';

export const PHRASE_CATEGORIES = (bands, short = false) => [
  ...[...bands].sort((a, b) => b.min - a.min).map((b) => ({ value: b.code, label: `${b.code} — ${short ? b.label.split(' ')[0] : b.label}` })),
  { value: 'next', label: 'Langkah berikutnya' },
  { value: 'general', label: 'Umum' },
];

/** "EDSEL SMITH CAHYADI" → "Edsel Smith Cahyadi"; nama yang sudah rapi dibiarkan. */
export function displayName(name) {
  const t = String(name || '').trim();
  if (t && t === t.toUpperCase()) {
    return t.toLowerCase().replace(/(^|[\s'-])(\p{L})/gu, (m, sep, ch) => sep + ch.toUpperCase());
  }
  return t;
}

const cap = (s) => (s ? s[0].toUpperCase() + s.slice(1) : s);

/**
 * Isi tempat kosong di kalimat sesuai siswa (jenis kelamin dari Dapodik: L/P).
 * `nameUsed`: nama sudah muncul di narasi → {name} diganti kata ganti (he/she)
 * supaya nama lengkap tidak berulang. Kalau jenis kelamin tidak diketahui, nama tetap dipakai.
 */
export function fillPhrase(text, student, theme, { nameUsed = false } = {}) {
  const g = String(student.gender || '').toUpperCase();
  const known = g === 'L' || g === 'P';
  const pron = g === 'L' ? { they: 'he', their: 'his', them: 'him' }
    : g === 'P' ? { they: 'she', their: 'her', them: 'her' }
      : { they: 'they', their: 'their', them: 'them' };
  const out = text.replace(/\{(name|they|their|them|theme)\}/gi, (m, key, offset, whole) => {
    const k = key.toLowerCase();
    let v;
    if (k === 'name' && nameUsed && known) {
      // Posisi objek (langsung setelah sebuah kata, mis. "seeing {name}", "help {name}") → him/her;
      // awal kalimat atau setelah tanda baca (", {name} will…") → he/she.
      const before = whole.slice(0, offset).trimEnd();
      v = before && /[\p{L}]$/u.test(before) ? pron.them : pron.they;
    } else if (k === 'name') v = displayName(student.name);
    else if (k === 'theme') v = theme || 'the unit';
    else v = pron[k];
    return key[0] === key[0].toUpperCase() ? cap(v) : v;
  });
  return cap(out.trim());
}

/** Tambahkan kalimat ke akhir narasi. */
export function appendSentence(narrative, sentence) {
  const base = String(narrative || '').trim();
  if (!base) return sentence;
  return `${base}${/[.!?]$/.test(base) ? '' : '.'} ${sentence}`;
}

/**
 * Daftar kalimat narasi yang bisa diklik untuk ditambahkan.
 * `predicate`: kode Understanding Level siswa (untuk urutan saran).
 */
export function phraseBankList({ settings, student, theme, predicate, getText, onPick }) {
  const bank = settings.narrative_bank || [];
  const cats = PHRASE_CATEGORIES(settings.grading.bands);
  const order = [predicate, 'next', 'general', ...cats.map((c) => c.value)].filter((v, i, a) => v && a.indexOf(v) === i);
  const groups = order
    .map((value) => ({ value, label: cats.find((c) => c.value === value)?.label || value, items: bank.filter((p) => p.category === value) }))
    .filter((g) => g.items.length);
  const other = bank.filter((p) => !order.includes(p.category));
  if (other.length) groups.push({ value: 'other', label: 'Lainnya', items: other });

  return h('div', { class: 'stack' },
    h('p', { class: 'muted small' },
      predicate ? `Understanding Level ${predicate}: kalimat yang cocok ditampilkan paling atas. ` : 'Isi Understanding Level dulu supaya saran kalimat lebih tepat. ',
      'Klik untuk menambahkan ke akhir narasi; kalimat tetap bisa diedit setelahnya.'),
    groups.length
      ? groups.map((g) => h('div', { class: 'phrase-group' }, h('h3', {}, g.label),
        g.items.map((p) => {
          // Kalimat dihitung ulang saat diklik: kalau nama sudah ada di narasi, pakai kata ganti.
          const make = () => fillPhrase(p.text, student, theme, { nameUsed: (getText?.() || '').includes(displayName(student.name)) });
          const textEl = h('span', {}, make());
          const mark = h('span', { class: 'add' }, '+ Tambah');
          const row = h('button', { type: 'button', class: 'phrase', onclick: () => {
            const sentence = make();
            textEl.textContent = sentence;
            onPick(sentence);
            row.classList.add('used');
            mark.textContent = '✓ Ditambahkan';
          } }, textEl, mark);
          return row;
        })))
      : h('p', { class: 'muted' }, 'Bank kalimat kosong. Tambahkan kalimat di Pengaturan → Bank kalimat.'));
}

/**
 * Modal pemilih kalimat. `predicate`: kode Understanding Level siswa (untuk urutan saran).
 * `onPick(kalimat)` dipanggil setiap kalimat diklik; modal tetap terbuka.
 */
export function openPhraseBank(opts) {
  openModal({
    title: `Bank kalimat — ${displayName(opts.student.name)}`,
    wide: true,
    body: phraseBankList(opts),
    actions: [{ label: 'Selesai', kind: 'primary', onClick: (c) => c() }],
  });
}


/** Angka tetap dari teks (supaya draf siswa yang sama selalu sama, tapi beda antarsiswa). */
function seedOf(text) {
  let x = 2166136261;
  for (const ch of String(text)) { x ^= ch.charCodeAt(0); x = Math.imul(x, 16777619) >>> 0; }
  return x;
}

/**
 * Draf narasi offline dari bank kalimat, sesuai predikat Understanding Level:
 * 2 kalimat sesuai predikat (atau 1 + kalimat umum) + 1 kalimat langkah berikutnya.
 * Nama dipakai di kalimat pertama, berikutnya kata ganti (he/she) bila jenis kelamin diketahui.
 * Mengembalikan '' kalau tidak ada predikat atau bank kalimat kosong.
 */
export function draftNarrative({ settings, student, projectId, predicate }) {
  if (!predicate) return '';
  const bank = settings.narrative_bank || [];
  const of = (cat) => bank.filter((p) => p.category === cat && p.text.trim());
  const pred = of(predicate);
  const general = of('general');
  const next = of('next');
  if (!pred.length && !general.length) return '';
  const seed = seedOf(`${student.id}|${projectId}`);
  const pick = (list, k) => list[(seed + k * 7919) % list.length];
  const chosen = [];
  if (pred.length) chosen.push(pick(pred, 0));
  const second = pred.length > 1 ? pred.filter((p) => p !== chosen[0]) : general;
  if (second.length) chosen.push(pick(second, 1));
  if (next.length) chosen.push(pick(next, 2));
  const theme = settings.projects.find((pc) => pc.id === projectId)?.theme;
  return chosen.reduce((text, p, i) => appendSentence(text, fillPhrase(p.text, student, theme, { nameUsed: i > 0 })), '');
}
