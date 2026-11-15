// Identitas tiap halaman: label, nomor langkah, dan ikon garis (satu warna).
// `fill`: halaman kerja yang mengisi tinggi layar; tabel/daftarnya punya scroll sendiri
// dan judul + tab kelas tetap terlihat.
export const STEP = {
  beranda: { label: 'Ringkasan', icon: 'home' },
  siswa: { n: '01', label: 'Data siswa', icon: 'users', fill: true },
  nilai: { n: '02', label: 'Nilai mapel', icon: 'chart', fill: true },
  isi: { n: '03', label: 'Isi rapor', icon: 'pen', fill: true },
  cetak: { n: '04', label: 'Cetak', icon: 'printer', fill: true },
  pengaturan: { label: 'Pengaturan', icon: 'sliders' },
  data: { label: 'Data & arsip', icon: 'database' },
};
