# E-Raport — National Plus Class

Aplikasi desktop (Windows) untuk merekap nilai siswa ke template rapor
*Student Progress Report* (National Plus Class / IB Enrichment Program).
Dibuat dengan **Tauri 2** (Rust + tampilan HTML/JS biasa, tanpa framework).

Satu laptop operator memegang data. Guru dan fasilitator cukup menyerahkan nilai
dalam format Excel yang sudah biasa dipakai, atau mengisinya langsung di aplikasi.

## Panduan untuk guru

- `panduan/Panduan-Lengkap-E-Raport.pdf`: panduan lengkap (39 halaman, bergambar) untuk semua fitur.
  Sumbernya `Panduan-Lengkap-E-Raport.html`, dengan gambar di `panduan/gambar/` (data siswa fiktif).
- `panduan/Panduan-Guru-E-Raport.pdf`: ringkasan 2 halaman untuk dicetak dan ditempel.
- Video tutorial `Tutorial-E-Raport.mp4` (±26 menit, bernarasi) **tidak disimpan di repo** karena ukurannya besar.
  Bagikan lewat flashdisk, Google Drive, atau YouTube (unlisted).

## Template Excel baku untuk guru

Folder `template-excel/` berisi empat template kosong yang **sama untuk SD, SMP, dan SMA**:
`Template 1 - Data Siswa`, `Template 2 - Nilai Mapel` (satu file per mapel, nama mapel di kotak kuning),
`Template 3 - Nilai Proyek` (satu file per proyek), dan `Template 4 - Absensi`. Lima kolom pertama selalu
No · Nama Siswa · NIS · NISN · Kelas. File yang diisi guru langsung terbaca impor aplikasi
(nama mapel bahasa Indonesia seperti "Matematika" dikenali). Setiap file punya sheet *Petunjuk*.

Template yang sama juga bisa disimpan dari aplikasi (tombol **Template Excel** di Data siswa, Nilai mapel,
dan tab Nilai proyek), opsional dengan daftar siswa sudah tercantum. Kode: `src-tauri/src/templates.rs`,
`ui/js/template-modal.js`. Membuat ulang file di `template-excel/` setelah mengubah susunannya:
`ERAPORT_TEMPLATE_DIR=../template-excel cargo test --lib generate_standard_templates -- --ignored`.

## Alur kerja

1. **Data Siswa**: impor dari ekspor Dapodik (`.xls`/`.xlsx`), **atau ketik langsung di aplikasi**
   (*Buat kelas baru* / *Buka lembar <kelas>*): lembar seperti Excel di dalam halaman (bukan pop-up)
   berisi Nama, NIS, NISN, L/P, dan **Sakit/Izin/Alpa** (+/− menambah/mengurangi satu hari; % kehadiran
   dihitung dari hari efektif). Tersimpan otomatis, juga saat pindah halaman. Enter = baris berikutnya,
   salin-tempel beberapa baris/kolom dari Excel, urutan baris = nomor absen (geser ↑/↓, Alt+↑/↓). *Kelola kelas → Ekspor ke Excel*
   menyimpan daftar siswa (No, Nama, NIS, NISN, L/P, Kelas) yang bisa diimpor lagi, mis. di laptop
   guru lain (`src-tauri/src/roster.rs`). Yang disimpan hanya nama, NIS, NISN, jenis kelamin, dan kelas.
2. **Nilai Mapel**: impor rekap nilai (kolom `NAMA SISWA` + baris `PH`/`PTS`, seperti sheet
   *Data Nilai Kls 7 8,9*). Nama yang disingkat atau salah ketik dicocokkan otomatis
   dan dilaporkan untuk dicek. Nilai juga bisa diedit langsung di tabel.
   File guru yang susunannya lain (judul satu baris "IPA PH", satu sheet per mapel dengan
   NH 1/NH 2/Rata-rata NH/PTS, angka berkoma, kelas di judul atau nama sheet) dibaca lewat
   *Atur kolom sendiri* (`src-tauri/src/score_import.rs`): kolom ditebak, bisa dikoreksi, beberapa
   kolom PH dirata-rata, siswa dicocokkan lewat NISN/NIS lalu nama, ada ringkasan sebelum diterapkan.
   Penulisan kelas yang berbeda ("9.1", "Kelas IX 1") disamakan dengan kelas yang sudah ada ("IX-1").
3. **Isi Rapor** (tab *Per siswa* dan *Nilai proyek*; tab *Tabel kelas* sengaja dihapus supaya
   tidak membingungkan guru): ATL (bagian 1), proyek/UOI + narasi (bagian 2), kehadiran, dan indikator
   khusus siswa (opsional). Predikat diklik (EE/ME/AE/BE) atau diketik sebagai angka (`92`).
   Isian tersimpan otomatis.
   - **Form Excel untuk fasilitator/wali kelas**: tombol *Buat form Excel* membuat file .xlsx
     berisi daftar siswa (kelas & nama terkunci, ID tersembunyi) dengan kolom ATL, proyek +
     narasi, dan/atau kehadiran. Isian yang sudah ada ikut tercantum. Setelah diisi, *Impor form*
     menampilkan pratinjau (berapa isian berubah, isian lama yang tertimpa, isian yang tidak
     terbaca) sebelum diterapkan. Sel kosong tidak menghapus data; urutan baris boleh diubah.
   - **Nilai proyek** (tab di Isi rapor): satu proyek × satu kelas — Understanding, Inquiry, narasi;
     tempel dari Excel, atau *Impor nilai proyek dari Excel* (`src-tauri/src/project_import.rs`): kolom
     ditebak dari judul/isi (EE/E/angka), proyek ditebak dari nama sheet/judul, nama dicocokkan
     (persis → singkatan → nama dipendekkan → mirip), pratinjau perubahan & isian yang tertimpa
     sebelum diterapkan; sel kosong tidak menghapus.
   - **Indikator ATL per predikat**: Pengaturan → ATL, tiap atribut punya teks untuk EE/ME/AE/BE
     (`AtlConfig.levels`); rapor mencetak teks sesuai predikat siswa, kosong = indikator umum.
   - **Kotak identitas siswa**: Pengaturan → Kotak identitas siswa. Label tiap baris (Student Name,
     Grade / Class, Student ID, Homeroom Teacher, Academic Year, Attendance) bebas diganti, bisa
     disembunyikan dan diurutkan; kelas bisa "Grade 9 (IX-1)" atau "IX-1", nomor induk bisa NIS+NISN,
     NIS saja, atau NISN saja (`Settings.info_fields`, template: `st.info`).
   - **Bank kalimat narasi**: kalimat bahasa Inggris siap pakai per predikat + "langkah
     berikutnya"; {name}, {their}, {theme} diisi otomatis (kata ganti dari jenis kelamin).
4. **Cetak Rapor**: pratinjau per kelas, lalu cetak atau *Save as PDF*. Satu siswa per halaman baru.
   - **PDF per siswa**: satu file per siswa (`01 Nama Siswa.pdf`) lewat Microsoft Edge mode
     headless (selalu ada di Windows 10/11), ±12 detik per kelas.
   - **Leger Excel**: sheet *Rekap* (sebaran predikat per mapel/kelas, baris *Semua kelas*), satu sheet
     per kelas, dan sheet *Catatan* (rincian yang belum lengkap per siswa). Sheet kelas siap cetak
     mendatar dalam **2 halaman** (ukuran kertas dari Pengaturan): halaman 1 = identitas + nilai mapel
     (PH, PTS, Akhir, Pred.) + rata-rata kelas; halaman 2 = No/Nama lagi + ATL, proyek, S/I/A/Hadir,
     ringkasan "Belum lengkap". Ada keterangan arti warna, tanda tangan kepala sekolah & wali kelas di
     tiap halaman, judul kolom berulang, dan nomor halaman.
   - **Data siswa format bebas** (SD, SMA, daftar dari guru): kalau file bukan ekspor Dapodik,
     aplikasi menebak baris judul (juga judul 2 baris/sel digabung), kolom Nama, NIS/Induk, NISN,
     L/P, Kelas — dari judul kolom **dan** isinya (10 digit = NISN). Kelas ditebak dari kolom kelas,
     judul tabel ("KELAS : IC"), nama sheet, atau nama file ("KELAS XB") → `I-C`, `X-B`.
     Pratinjau + pilihan kolom bisa dikoreksi sebelum impor (`src-tauri/src/student_flex.rs`).
     Siswa lama dicocokkan lewat NISN, lalu NIS **hanya bila kelas/nama juga cocok** (NIS hanya unik
     per sekolah), lalu nama + kelas.
   - **Jenjang SD / SMP / SMA** (`src-tauri/src/levels.rs`): tiap jenjang punya folder data sendiri
     (siswa, pengaturan/kop/kepala sekolah/mapel/indikator/proyek, wali kelas, tanda tangan, backup,
     arsip). SMP = folder utama `Dokumen\E-Raport` (data lama tidak dipindah), SD = `…\SD`,
     SMA = `…\SMA`; jenjang terakhir diingat di `jenjang.json`. Pemilih jenjang di sidebar; jenjang
     baru bisa menyalin pengaturan (tanpa identitas sekolah/kepala sekolah/kelas). "Pindahkan kelas…"
     memindahkan kelas antar jenjang (keduanya dibackup dulu). Impor & pemulihan memperingatkan bila
     kelas/backup tampaknya milik jenjang lain. Kata sandi aplikasi berlaku untuk semua jenjang.
5. **Data & arsip** — *Kata sandi aplikasi* (opsional): aplikasi meminta kata sandi saat dibuka,
   terkunci otomatis jika tidak dipakai (default 10 menit) dan bisa dikunci dengan Ctrl+L.
   Kata sandi & kode pemulihan disimpan sebagai hash Argon2id di `keamanan.json` (terpisah dari
   `data.json`, tidak ikut backup). Selama terkunci semua perintah data ditolak di sisi Rust;
   5× salah → harus menunggu. Lupa kata sandi → 6 angka dari **Google Authenticator** di HP
   (opsional, dihubungkan lewat kode QR di Data & arsip; TOTP RFC 6238, tanpa internet, jam laptop
   diberi kelonggaran ±1 menit dan jam yang meleset dilaporkan; `src-tauri/src/totp.rs`), atau kode
   pemulihan (ditampilkan sekali saat dipasang; disarankan difoto dengan HP). Catatan: file data/backup **tidak dienkripsi**; kunci ini melindungi pemakaian aplikasi.
   Juga: backup ke flashdisk (aplikasi mengingatkan kalau ada perubahan yang belum
   di-backup), pulihkan dari backup/arsip (data sekarang selalu diamankan dulu), dan *mulai
   semester baru* (data diarsipkan; pengaturan dibawa; nilai & isian dikosongkan).
6. **Pengaturan**: logo sekolah, identitas rapor, kotak identitas siswa, rumus & rentang predikat, daftar mapel (bisa lebih dari 3),
   atribut ATL, 7 proyek, wali kelas, ukuran kertas, dan desain rapor kustom.
   - **Tanda tangan** (wali kelas per kelas + kepala sekolah): pilih *tanda tangan basah*
     (ruang dikosongkan) atau *digital*. Tanda tangan digital bisa dari foto/scan kertas
     (latar kertas dihapus otomatis), tempel screenshot (Ctrl+V), atau digambar langsung.
     Disimpan sebagai PNG transparan di data aplikasi (ikut backup). Template kustom lama
     perlu disalin ulang dari desain bawaan agar memuat tanda tangan.

## Aturan nilai (bisa diubah di Pengaturan)

| Predikat | Rentang |
|---|---|
| EE — Exceeding Expectations | 96–100 |
| ME — Meeting Expectations | 91–95 |
| AE — Approaching Expectations | 86–90 |
| BE — Below Expectations | 80–85 |

- Nilai akhir = 50% PH + 50% PTS, lalu dibulatkan (85,5 → 86).
- Predikat ditentukan dari **batas bawah**, jadi tidak ada nilai yang "jatuh di celah".
- Nilai **di bawah 80** tetap dicetak BE, tetapi ditandai di aplikasi karena aturannya belum ditentukan.
- Rentang predikat dicetak sebagai keterangan di bawah bagian 3 rapor.

## Menjalankan (developer)

Prasyarat: Rust (MSVC), Node.js, Visual Studio Build Tools (C++), dan WebView2 (sudah ada di Windows 10/11).

```bash
npm install          # hanya untuk Tauri CLI
npm run dev          # jalankan aplikasi
npm run build        # buat installer → src-tauri/target/release/bundle/nsis/
cd src-tauri && cargo test   # unit test (termasuk impor file contoh di berkas_atau_template/)
```

Mengecek desain rapor tanpa membuka aplikasi:

```bash
cd src-tauri
cargo run --example contoh_rapor -- "../berkas_atau_template/Contoh rekap nilai siswa.xls" contoh.html IX-1 2
```

Untuk uji coba tanpa mengganggu data asli, jalankan dengan variabel lingkungan
`ERAPORT_DATA_DIR=<folder lain>`.

## Versi MacBook

Aplikasi Mac hanya bisa dibuat di macOS, jadi dibuat di server macOS GitHub:
GitHub → **Actions** → **Build macOS** → **Run workflow** (`.github/workflows/build-macos.yml`).
Setelah ±20 menit, unduh dari bagian *Artifacts*: `E-Raport-macOS` (file `.dmg`, satu file untuk
Mac Intel dan Apple Silicon, macOS 11 ke atas) dan `uji-macos-screenshot` (aplikasi dibuka di
server macOS). Workflow juga menjalankan semua unit test di macOS, termasuk membuat PDF lewat Chrome.

Memasang di MacBook guru:
1. Buka file `.dmg`, seret **E-Raport** ke folder **Applications**.
2. Aplikasi belum ditandatangani Apple (perlu akun Apple Developer, $99/tahun), jadi saat pertama
   dibuka macOS menolak ("tidak dapat memverifikasi…"). Klik **Selesai**, lalu buka
   **Pengaturan Sistem → Privasi & Keamanan**, gulir ke bawah, klik **Tetap Buka** (Open Anyway),
   masukkan kata sandi Mac. Cukup sekali. (Cara cepat lewat Terminal: `xattr -cr /Applications/E-Raport.app`.)
3. Saat diminta akses ke folder **Dokumen**, klik **Izinkan** (data tersimpan di Dokumen/E-Raport).

Perbedaan di Mac: tombol pintas memakai ⌘ (⌘L mengunci aplikasi). **Cetak / Simpan PDF** membuat
PDF lalu membukanya di Preview (⌘P untuk mencetak), karena dialog cetak di dalam WebKit macOS tidak
bisa diandalkan. PDF dibuat lewat Google Chrome/Edge/Chromium/Brave; tanpa itu rapor dibuka di Safari
untuk dicetak dari sana, dan "PDF per siswa" meminta guru memasang Google Chrome.
Data dan backup bisa dipindah antara Windows dan Mac (formatnya sama).

## Data

Semua data ada di **`Dokumen\E-Raport\`**:

- `data.json`: seluruh data (siswa, nilai, isian rapor, pengaturan), ditulis secara atomik.
- `backup\harian-YYYY-MM-DD.json`: salinan otomatis setiap hari aplikasi dibuka (30 hari terakhir).
  `backup\sebelum-pulih-*.json` / `sebelum-semester-baru-*.json`: salinan sebelum data diganti.
- `arsip\<nama>.json`: data semester yang sudah diarsipkan.
- `form\`: form Excel untuk fasilitator.
- Backup ke flashdisk tersimpan di `<flashdisk>\E-Raport backup\`.
- `template-rapor.html` (opsional): desain rapor kustom. Buat lewat Pengaturan → Desain rapor.

Untuk pindah laptop, salin seluruh folder itu.

## Struktur kode

```
src-tauri/src/
  model.rs      struktur data + nilai bawaan (predikat, ATL, mapel, 7 proyek)
  grading.rs    rumus nilai akhir & konversi angka → predikat
  names.rs      pencocokan nama (persis / singkatan / salah ketik)
  importer.rs   baca Excel (calamine), deteksi sheet & kelas, gabung ke data
  forms.rs      form Excel untuk fasilitator (rust_xlsxwriter) + impor kembali
  leger.rs      leger kelas + rekap predikat ke Excel
  roster.rs     daftar kelas yang diisi di aplikasi + ekspor daftar siswa ke Excel
  project_import.rs  impor nilai proyek dari Excel guru (kolom ditebak)
  pdf.rs        PDF per siswa lewat Edge headless
  view.rs       hasil hitungan per siswa (dipakai layar & rapor)
  report.rs     render rapor HTML (minijinja) dari templates/report.html
  store.rs      simpan/muat data.json + backup
  commands.rs   perintah yang dipanggil dari tampilan
ui/             tampilan (HTML/CSS/JS modul, tanpa build step)
```

## Belum diputuskan (perlu konfirmasi kepala sekolah)

1. Nilai di bawah 80: tetap BE, atau ada predikat/aturan lain? Di data contoh, 42 dari 79
   siswa kelas 9 punya minimal satu nilai akhir < 80 (rumus 50% PH + 50% PTS).
2. Sumber nilai angka: rata-rata PH & PTS, bobot lain, atau nilai akhir semester?
3. Proyek: tiap siswa ikut 1 dari 7 proyek, atau rapor memuat ketujuhnya?
   (Aplikasi mendukung keduanya.)
4. Mapel bagian 3: cukup IPA, B. Indonesia, B. Inggris dulu? Apakah satu baris per mapel
   atau dipecah per aspek (Reading/Writing)?
5. Judul/kop rapor: template asli bertuliskan *IB Primary Years Programme / Grade 3*,
   yang tidak cocok untuk SMP. Saat ini dipakai "National Plus Class — IB Enrichment Program".
6. Indikator mapel bawaan masih **contoh** dan perlu diganti dengan indikator resmi sekolah.

## Rencana berikutnya

- Lihat daftar "Belum diputuskan" di atas; sesuaikan aturan setelah ada keputusan kepala sekolah.
