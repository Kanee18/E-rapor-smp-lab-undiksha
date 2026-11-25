// Editor tanda tangan digital: dari foto/gambar (latar kertas dihapus otomatis),
// tempel screenshot (Ctrl+V), atau gambar langsung dengan mouse/touchpad/pen.
// Hasil akhir: PNG berlatar transparan, dipotong rapat, tinggi maks. 200 px.
import { api } from './api.js';
import { h, errorText, isMac, openModal, toast } from './dom.js';

const MAX_WORK = 1400; // sisi terpanjang saat mengolah foto (cukup tajam, tetap cepat)
const OUT_H = 200;
const OUT_W = 640;
const INKS = {
  asli: null,
  hitam: [28, 28, 34],
  biru: [24, 48, 150],
};

// ---------------------------------------------------------------- pengolahan gambar

function loadImage(src) {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () => reject(new Error('Gambar tidak bisa dibuka.'));
    img.src = src;
  });
}

/** Gambar → kanvas kerja (diperkecil, diputar kelipatan 90°). */
function workCanvas(img, rotation) {
  const scale = Math.min(1, MAX_WORK / Math.max(img.naturalWidth, img.naturalHeight));
  const w = Math.max(1, Math.round(img.naturalWidth * scale));
  const hh = Math.max(1, Math.round(img.naturalHeight * scale));
  const swap = rotation % 180 !== 0;
  const c = document.createElement('canvas');
  c.width = swap ? hh : w;
  c.height = swap ? w : hh;
  const g = c.getContext('2d', { willReadFrequently: true });
  g.translate(c.width / 2, c.height / 2);
  g.rotate((rotation * Math.PI) / 180);
  g.drawImage(img, -w / 2, -hh / 2, w, hh);
  return c;
}

function hasTransparency(d) {
  let n = 0;
  let total = 0;
  for (let i = 3; i < d.length; i += 4 * 5) { total++; if (d[i] < 250) n++; }
  return n > total * 0.02;
}

/**
 * Pisahkan tinta dari kertas. Terang kertas dihitung per area (rata-rata sekitar
 * tiap titik), jadi foto dengan cahaya tidak merata tetap bersih.
 * `sensitivity` 0..1: makin tinggi, goresan tipis/pudar makin ikut terambil.
 */
function extractInk(src, { sensitivity, ink }) {
  const { width: w, height: hh } = src;
  const d = src.getContext('2d', { willReadFrequently: true }).getImageData(0, 0, w, hh).data;
  const out = new ImageData(w, hh);
  const o = out.data;
  const color = INKS[ink];

  if (hasTransparency(d)) {
    // sudah transparan (mis. dari aplikasi gambar): pakai apa adanya
    for (let i = 0; i < d.length; i += 4) {
      o[i] = color ? color[0] : d[i];
      o[i + 1] = color ? color[1] : d[i + 1];
      o[i + 2] = color ? color[2] : d[i + 2];
      o[i + 3] = d[i + 3];
    }
  } else {
    const gray = new Float32Array(w * hh);
    for (let p = 0, i = 0; p < gray.length; p++, i += 4) gray[p] = 0.299 * d[i] + 0.587 * d[i + 1] + 0.114 * d[i + 2];
    // integral image untuk rata-rata kotak yang cepat
    const W = w + 1;
    const sum = new Float64Array(W * (hh + 1));
    for (let y = 0; y < hh; y++) {
      let row = 0;
      for (let x = 0; x < w; x++) {
        row += gray[y * w + x];
        sum[(y + 1) * W + x + 1] = sum[y * W + x + 1] + row;
      }
    }
    const r = Math.max(8, Math.round(Math.max(w, hh) / 24));
    const t0 = 0.3 - sensitivity * 0.24;
    for (let y = 0; y < hh; y++) {
      const y0 = Math.max(0, y - r);
      const y1 = Math.min(hh, y + r + 1);
      for (let x = 0; x < w; x++) {
        const x0 = Math.max(0, x - r);
        const x1 = Math.min(w, x + r + 1);
        const bg = (sum[y1 * W + x1] - sum[y0 * W + x1] - sum[y1 * W + x0] + sum[y0 * W + x0]) / ((y1 - y0) * (x1 - x0));
        const p = y * w + x;
        const dark = (bg - gray[p]) / Math.max(bg, 40);
        let a = Math.min(1, Math.max(0, (dark - t0) / 0.16));
        if (gray[p] > 238) a = 0;
        const i = p * 4;
        if (color) { o[i] = color[0]; o[i + 1] = color[1]; o[i + 2] = color[2]; } else {
          // warna asli sedikit dipertegas supaya tidak pucat di kertas
          o[i] = d[i] * 0.72; o[i + 1] = d[i + 1] * 0.72; o[i + 2] = d[i + 2] * 0.72;
        }
        o[i + 3] = Math.round(a * 255);
      }
    }
  }
  const c = document.createElement('canvas');
  c.width = w;
  c.height = hh;
  c.getContext('2d').putImageData(out, 0, 0);
  return c;
}

/**
 * Potong rapat ke goresan lalu perkecil. Baris/kolom dengan sangat sedikit titik
 * tinta (bintik debu) diabaikan saat mencari batas. Null kalau tidak ada tinta.
 */
function finalize(canvas) {
  const { width: w, height: hh } = canvas;
  const d = canvas.getContext('2d', { willReadFrequently: true }).getImageData(0, 0, w, hh).data;
  const rows = new Uint32Array(hh);
  const cols = new Uint32Array(w);
  let total = 0;
  for (let y = 0; y < hh; y++) {
    for (let x = 0; x < w; x++) {
      if (d[(y * w + x) * 4 + 3] > 110) { rows[y]++; cols[x]++; total++; }
    }
  }
  if (total < 30) return null;
  const minRow = Math.max(1, Math.round(w * 0.002));
  const minCol = Math.max(1, Math.round(hh * 0.004));
  const first = (arr, min) => arr.findIndex((v) => v >= min);
  const last = (arr, min) => arr.length - 1 - [...arr].reverse().findIndex((v) => v >= min);
  let top = first(rows, minRow);
  let bottom = last(rows, minRow);
  let left = first(cols, minCol);
  let right = last(cols, minCol);
  if (top < 0 || left < 0 || bottom <= top || right <= left) return null;
  const pad = Math.round(Math.max(right - left, bottom - top) * 0.03) + 2;
  top = Math.max(0, top - pad); left = Math.max(0, left - pad);
  bottom = Math.min(hh - 1, bottom + pad); right = Math.min(w - 1, right + pad);
  const cw = right - left + 1;
  const ch = bottom - top + 1;
  const scale = Math.min(1, OUT_H / ch, OUT_W / cw);
  const out = document.createElement('canvas');
  out.width = Math.max(1, Math.round(cw * scale));
  out.height = Math.max(1, Math.round(ch * scale));
  const g = out.getContext('2d');
  g.imageSmoothingQuality = 'high';
  g.drawImage(canvas, left, top, cw, ch, 0, 0, out.width, out.height);
  return out.toDataURL('image/png');
}

// ---------------------------------------------------------------- tampilan

/** Pratinjau seperti di rapor: gambar sedikit menimpa garis nama. */
function paperPreview(src, name, title) {
  return h('div', { class: 'sig-paper' },
    h('div', { class: 'sig-space' }, src ? h('img', { src, alt: 'Tanda tangan' }) : h('span', { class: 'muted small' }, 'Pratinjau muncul di sini')),
    h('div', { class: 'sig-name' }, name || '(nama belum diisi)'),
    h('div', { class: 'sig-role' }, title));
}

function segmented(options, value, onPick) {
  const wrap = h('div', { class: 'segs', role: 'radiogroup' });
  const draw = () => wrap.replaceChildren(...options.map(([v, label]) => h('button', {
    type: 'button', class: `seg wide${v === value ? ' on' : ''}`, role: 'radio', 'aria-checked': String(v === value),
    onclick: () => { value = v; draw(); onPick(v); },
  }, label)));
  draw();
  return wrap;
}

/**
 * Buka editor. `onSave(dataUrl)` dipanggil dengan PNG hasil akhir.
 * @param {{ role: string, name: string, title: string, current?: string, onSave: (url: string) => void }} opt
 */
export function openSignatureEditor({ role, name, title, current = '', onSave }) {
  let mode = 'foto';
  let ink = 'asli';
  let sensitivity = 0.55;
  let rotation = 0;
  let sourceImg = null;
  // hasil per mode; selama belum ada yang baru, pratinjau menampilkan tanda tangan lama
  let photoResult = '';
  let drawResult = '';
  const result = () => (mode === 'foto' ? photoResult : drawResult);
  const strokes = [];

  const body = h('div', { class: 'sig-editor' });
  const preview = h('div', {});
  const showPreview = () => preview.replaceChildren(paperPreview(result() || current, name, title));

  // ---------- foto / gambar / screenshot
  let processTimer = 0;
  const process = () => {
    clearTimeout(processTimer);
    processTimer = setTimeout(() => {
      if (!sourceImg) return;
      photoResult = finalize(extractInk(workCanvas(sourceImg, rotation), { sensitivity, ink })) || '';
      if (!photoResult) toast('Goresan tanda tangan tidak terdeteksi. Coba naikkan kepekaan atau pakai foto yang lebih terang.', 'error', 6000);
      showPreview();
    }, 60);
  };
  const useSource = async (dataUrl) => {
    try {
      sourceImg = await loadImage(dataUrl);
      rotation = 0;
      mode = 'foto';
      draw();
      process();
    } catch (e) { toast(errorText(e), 'error'); }
  };
  const pickFile = async () => {
    try {
      const path = await api.pickImageFile('Pilih foto atau gambar tanda tangan');
      if (!path) return;
      await useSource(await api.loadSignatureSource(path));
    } catch (e) { toast(errorText(e), 'error'); }
  };
  // Ctrl+V: tempel screenshot (Win+Shift+S) atau gambar yang disalin
  const onPaste = (ev) => {
    const item = [...(ev.clipboardData?.items || [])].find((it) => it.type.startsWith('image/'));
    if (!item) return;
    ev.preventDefault();
    const file = item.getAsFile();
    const reader = new FileReader();
    reader.onload = () => useSource(String(reader.result));
    reader.readAsDataURL(file);
  };
  window.addEventListener('paste', onPaste);

  // ---------- gambar langsung
  const pad = h('canvas', { class: 'sig-pad', 'aria-label': 'Area menggambar tanda tangan' });
  let dpr = 1;
  const strokeColor = () => (INKS[ink] ? `rgb(${INKS[ink].join(',')})` : 'rgb(24,28,40)');
  const drawStroke = (g, s) => {
    g.strokeStyle = strokeColor();
    g.fillStyle = strokeColor();
    g.lineCap = 'round';
    g.lineJoin = 'round';
    const p = s.pts;
    if (p.length === 1) { g.beginPath(); g.arc(p[0].x, p[0].y, p[0].w / 2, 0, Math.PI * 2); g.fill(); return; }
    for (let i = 1; i < p.length; i++) {
      const a = p[i - 1];
      const b = p[i];
      const c = p[i + 1];
      g.lineWidth = (a.w + b.w) / 2;
      g.beginPath();
      // kurva halus lewat titik tengah antarsegmen
      g.moveTo(i === 1 ? a.x : (a.x + b.x) / 2, i === 1 ? a.y : (a.y + b.y) / 2);
      if (c) g.quadraticCurveTo(b.x, b.y, (b.x + c.x) / 2, (b.y + c.y) / 2); else g.lineTo(b.x, b.y);
      g.stroke();
    }
  };
  const redraw = () => {
    const g = pad.getContext('2d');
    g.clearRect(0, 0, pad.width, pad.height);
    strokes.forEach((s) => drawStroke(g, s));
  };
  const sizePad = () => {
    dpr = window.devicePixelRatio || 1;
    const r = pad.getBoundingClientRect();
    if (!r.width) return;
    pad.width = Math.round(r.width * dpr);
    pad.height = Math.round(r.height * dpr);
    redraw();
  };
  let active = null;
  let lastT = 0;
  const pointFrom = (ev, prev) => {
    const r = pad.getBoundingClientRect();
    const x = (ev.clientX - r.left) * dpr;
    const y = (ev.clientY - r.top) * dpr;
    const base = 2.4 * dpr;
    let w;
    if (ev.pointerType === 'pen' && ev.pressure > 0) w = base * (0.45 + ev.pressure * 1.1);
    else if (prev) {
      // mouse/touchpad: makin cepat makin tipis, seperti pena
      const dt = Math.max(1, ev.timeStamp - lastT);
      const v = Math.hypot(x - prev.x, y - prev.y) / dpr / dt;
      w = prev.w * 0.7 + base * Math.min(1.25, Math.max(0.55, 1.3 - v * 0.35)) * 0.3;
    } else w = base;
    lastT = ev.timeStamp;
    return { x, y, w };
  };
  pad.addEventListener('pointerdown', (ev) => {
    ev.preventDefault();
    try { pad.setPointerCapture(ev.pointerId); } catch { /* pointer sudah dilepas */ }
    active = { pts: [pointFrom(ev, null)] };
    strokes.push(active);
    redraw();
  });
  pad.addEventListener('pointermove', (ev) => {
    if (!active) return;
    // titik-titik antara (lebih halus); kalau browser tidak memberi, pakai event itu sendiri
    const list = ev.getCoalescedEvents?.() ?? [];
    for (const e of list.length ? list : [ev]) active.pts.push(pointFrom(e, active.pts[active.pts.length - 1]));
    redraw();
  });
  const endStroke = () => {
    if (!active) return;
    active = null;
    drawResult = finalize(pad) || '';
    showPreview();
  };
  pad.addEventListener('pointerup', endStroke);
  pad.addEventListener('pointercancel', endStroke);
  const ro = new ResizeObserver(() => sizePad());

  // ---------- susunan
  function draw() {
    const inkPick = h('div', { class: 'row' }, h('span', { class: 'lbl small' }, 'Warna tinta'),
      segmented([['asli', 'Asli'], ['hitam', 'Hitam'], ['biru', 'Biru']], ink, (v) => { ink = v; if (mode === 'foto') process(); else { redraw(); if (strokes.length) drawResult = finalize(pad) || ''; showPreview(); } }));
    let panel;
    if (mode === 'foto') {
      panel = sourceImg
        ? h('div', { class: 'stack' },
          h('div', { class: 'sig-controls' },
            h('label', { class: 'field' }, h('span', { class: 'lbl' }, 'Kepekaan'),
              h('input', { type: 'range', min: 0, max: 1, step: 0.01, value: String(sensitivity), oninput: (e) => { sensitivity = Number(e.target.value); process(); } }),
              h('span', { class: 'hint' }, 'Geser ke kanan kalau goresan terputus, ke kiri kalau ada bintik/bayangan.')),
            inkPick,
            h('div', { class: 'row' },
              h('button', { type: 'button', class: 'btn sm', onclick: () => { rotation = (rotation + 90) % 360; process(); } }, 'Putar 90°'),
              h('button', { type: 'button', class: 'btn sm quiet', onclick: pickFile }, 'Ganti foto…'))))
        : h('div', { class: 'sig-drop' },
          h('button', { type: 'button', class: 'btn primary', onclick: pickFile }, 'Pilih foto atau gambar…'),
          h('p', { class: 'small muted' }, 'atau tekan ', h('kbd', {}, isMac ? '⌘' : 'Ctrl'), isMac ? '' : '+', h('kbd', {}, 'V'), ' untuk menempel screenshot'),
          h('ul', { class: 'hint' },
            h('li', {}, 'Tanda tangan di kertas putih polos dengan pulpen hitam/biru.'),
            h('li', {}, 'Foto dari atas, cahaya terang, tanda tangan memenuhi sebagian besar foto.'),
            h('li', {}, 'Latar kertas dihapus otomatis; hanya goresan yang tersimpan.')));
    } else {
      panel = h('div', { class: 'stack' },
        h('div', { class: 'sig-pad-wrap' }, pad, h('span', { class: 'sig-pad-hint' }, 'Tanda tangan di atas garis'),
          h('div', { class: 'sig-pad-tools' },
            h('button', { type: 'button', class: 'btn sm', onclick: () => { strokes.pop(); redraw(); drawResult = strokes.length ? finalize(pad) || '' : ''; showPreview(); } }, 'Urungkan'),
            h('button', { type: 'button', class: 'btn sm', onclick: () => { strokes.length = 0; redraw(); drawResult = ''; showPreview(); } }, 'Ulangi'))),
        inkPick,
        h('p', { class: 'hint' }, 'Pakai mouse, touchpad, atau pen. Dengan pen/layar sentuh, tebal goresan mengikuti tekanan.'));
    }
    body.replaceChildren(
      segmented([['foto', 'Dari foto / gambar'], ['gambar', 'Gambar langsung']], mode, (v) => { mode = v; draw(); }),
      h('div', { class: 'sig-grid' }, panel, h('div', { class: 'stack', style: { gap: '6px' } }, h('span', { class: 'lbl small' }, 'Pratinjau di rapor'), preview)));
    showPreview();
    if (mode === 'gambar') { ro.observe(pad); requestAnimationFrame(sizePad); } else ro.unobserve(pad);
  }

  draw();
  openModal({
    title: `Tanda tangan ${role.charAt(0).toLowerCase()}${role.slice(1)}`,
    wide: true,
    body,
    onClose: () => { window.removeEventListener('paste', onPaste); ro.disconnect(); clearTimeout(processTimer); },
    actions: [
      { label: 'Batal', kind: 'quiet', onClick: (c) => c() },
      { label: 'Simpan tanda tangan', kind: 'primary', onClick: (close) => {
        const r = result();
        if (!r) {
          if (current) { close(); return; } // tidak ada yang diubah
          toast(mode === 'foto' ? 'Pilih foto atau tempel screenshot tanda tangan dulu' : 'Gambar tanda tangan dulu', 'error');
          return;
        }
        onSave(r);
        close();
      } },
    ],
  });
}

// untuk pengujian otomatis
export const _internal = { extractInk, finalize, workCanvas, loadImage };
