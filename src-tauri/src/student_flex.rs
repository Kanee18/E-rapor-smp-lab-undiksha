//! Impor data siswa dari Excel berformat bebas: daftar nilai guru (SD), daftar kelas
//! sederhana (SMA), dsb. — bukan hanya ekspor Dapodik.
//!
//! Alurnya: tebak susunan tabel (baris judul, kolom Nama/NIS/NISN/L-P/Kelas, kelas) →
//! tampilkan pratinjau → pengguna boleh mengoreksi pilihan kolom/kelas → impor.
//! Penebakan memakai judul kolom DAN isinya (mis. "NISN / Induk" yang digabung dua kolom:
//! kolom berisi 10 digit = NISN, kolom berisi 3–9 digit = nomor induk).

use crate::importer::{cell, cell_text, count_classes, format_nisn, header, read_grid, ClassCount, Grid, StudentRow};
use crate::model::class_sort_key;
use crate::names::normalize_name;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Pilihan kolom (indeks 0-based) dan kelas bawaan. Bisa ditebak otomatis atau diubah pengguna.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StudentMapping {
    /// Baris judul kolom (0-based). Data dibaca mulai baris sesudahnya.
    pub header_row: usize,
    pub name: Option<usize>,
    pub nis: Option<usize>,
    pub nisn: Option<usize>,
    pub gender: Option<usize>,
    pub class: Option<usize>,
    /// Kelas untuk baris yang tidak punya kolom/judul kelas sendiri (mis. "X-B").
    pub default_class: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ColumnInfo {
    pub index: usize,
    /// Huruf kolom Excel ("A", "B", …).
    pub letter: String,
    pub header: String,
    pub samples: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FlexPreview {
    pub columns: Vec<ColumnInfo>,
    pub mapping: StudentMapping,
    /// Asal tebakan kelas: "kolom kelas", "judul tabel", "nama sheet", "nama file", atau "" (belum ada).
    pub class_source: String,
    pub total: usize,
    /// Beberapa baris pertama untuk pratinjau.
    pub rows: Vec<StudentRow>,
    pub classes: Vec<ClassCount>,
    pub warnings: Vec<String>,
}

// ---------------------------------------------------------------- nama kelas

const ROMAN_GRADES: [(&str, u32); 12] = [
    ("XII", 12), ("VIII", 8), ("VII", 7), ("III", 3), ("XI", 11), ("IX", 9),
    ("IV", 4), ("VI", 6), ("II", 2), ("X", 10), ("V", 5), ("I", 1),
];

fn is_grade(g: &str) -> bool {
    ROMAN_GRADES.iter().any(|(r, _)| *r == g) || g.parse::<u32>().is_ok_and(|n| (1..=12).contains(&n))
}

/// "IC" → "I-C", "XB" → "X-B", "x b" → "X-B", "7A" → "7-A", "IX-1" → "IX-1", "XI" → "XI".
/// `None` kalau bukan nama kelas (tingkat harus I–XII atau 1–12, bagian maks. 6 huruf/angka).
pub fn canonical_class(token: &str) -> Option<String> {
    let s: String = token
        .to_uppercase()
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '.' && *c != '_')
        .collect();
    let s = s.strip_prefix("KELAS").unwrap_or(&s).trim_start_matches([':', '-']).to_string();
    if s.is_empty() {
        return None;
    }
    let section_ok = |rest: &str| rest.len() <= 6 && rest.chars().all(|c| c.is_ascii_alphanumeric());
    let join = |g: &str, rest: &str| if rest.is_empty() { g.to_string() } else { format!("{g}-{rest}") };
    if let Some((g, rest)) = s.split_once('-') {
        return (is_grade(g) && !rest.is_empty() && section_ok(rest)).then(|| join(g, rest));
    }
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    if !digits.is_empty() {
        let rest = &s[digits.len()..];
        return (is_grade(&digits) && section_ok(rest)).then(|| join(&digits, rest));
    }
    ROMAN_GRADES
        .iter()
        .find(|(r, _)| s.starts_with(r) && section_ok(&s[r.len()..]))
        .map(|(r, _)| join(r, &s[r.len()..]))
}

/// Cari nama kelas di teks yang memuat kata "KELAS", mis. "KELAS :  IC TH. 2026/2027" → "I-C",
/// "KELAS XB" → "X-B", "Kelas X B" → "X-B".
pub fn class_from_text(text: &str) -> Option<String> {
    let up = text.to_uppercase();
    let at = up.find("KELAS")?;
    let rest = up[at + 5..].trim_start_matches(|c: char| c.is_whitespace() || c == ':' || c == '-' || c == '.');
    let tokens: Vec<&str> = rest
        .split(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == '(' || c == ')')
        .filter(|t| !t.is_empty())
        .collect();
    let first = tokens.first()?.trim_end_matches(['.', ':']);
    // "X B": tingkat dan bagian ditulis terpisah
    if let Some(second) = tokens.get(1) {
        let second = second.trim_end_matches(['.', ':']);
        let stop = ["TH", "TAHUN", "TA", "SEMESTER", "SMT", "TP"];
        if is_grade(first) && second.len() <= 2 && second.chars().all(|c| c.is_ascii_alphanumeric()) && !stop.contains(&second) {
            return canonical_class(&format!("{first}{second}"));
        }
    }
    canonical_class(first)
}

// ---------------------------------------------------------------- penebakan kolom

pub(crate) fn is_name_header(h: &str) -> bool {
    const NOT_STUDENT: [&str; 21] = [
        "AYAH", "IBU", "WALI", "ORANG TUA", "ORTU", "SEKOLAH", "GURU", "MAPEL", "PELAJARAN", "BIDANG", "KELAS", "ROMBEL",
        "KEPALA", "PARENT", "FATHER", "MOTHER", "TEACHER", "SCHOOL", "SUBJECT", "CLASS", "FILE",
    ];
    let named = matches!(h, "NAMA" | "NAME" | "STUDENT NAME" | "NAMA MURID" | "FULL NAME" | "PESERTA DIDIK")
        || h.starts_with("NAMA ")
        || h.ends_with(" NAMA")
        || h.ends_with(" NAME");
    named && !NOT_STUDENT.iter().any(|w| h.contains(w))
}

/// Apakah baris sesudah judul (`hr`) adalah lanjutan judul (sel digabung, mis. "PH | PTS"),
/// bukan baris pemisah kelas ("KELAS IX-1") atau baris data.
pub(crate) fn has_sub_header(grid: &Grid, hr: usize, name_col: usize) -> bool {
    let Some(row) = grid.get(hr + 1) else { return false };
    if !cell_text(cell(grid, hr + 1, name_col)).is_empty() {
        return false;
    }
    let texts: Vec<String> = row
        .iter()
        .filter_map(|d| match d {
            calamine::Data::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
            _ => None,
        })
        .collect();
    let numbers = row.iter().filter(|d| matches!(d, calamine::Data::Float(_) | calamine::Data::Int(_))).count();
    match texts.as_slice() {
        [] => false,
        // satu teks saja yang berupa nama kelas = pemisah kelas
        [only] if numbers == 0 => class_from_text(only).is_none() && canonical_class(only).is_none_or(|c| !c.contains('-')),
        _ => numbers <= texts.len(),
    }
}

/// Tingkat + bagian kelas untuk membandingkan penulisan yang berbeda:
/// "IX-1", "9-1", "9.1", "Kelas IX 1", "ix.1" → (9, "1"); "9A" / "IX-A" → (9, "A").
pub fn class_key(raw: &str) -> Option<(u32, String)> {
    let up = raw.trim().to_uppercase();
    let t = up.strip_prefix("KELAS").unwrap_or(&up).trim().replace(['.', '/', '_'], "-");
    let t: String = {
        // "IX 1" → "IX-1"
        let parts: Vec<&str> = t.split_whitespace().collect();
        if parts.len() == 2 && is_grade(parts[0]) { format!("{}-{}", parts[0], parts[1]) } else { parts.join("") }
    };
    let c = canonical_class(&t)?;
    let (g, rest) = c.split_once('-').unwrap_or((c.as_str(), ""));
    let grade = ROMAN_GRADES.iter().find(|(r, _)| *r == g).map(|(_, n)| *n).or_else(|| g.parse().ok())?;
    Some((grade, rest.to_string()))
}

/// Nama kelas di file → nama kelas yang sudah ada di aplikasi kalau penulisannya setara
/// ("9.1" → "IX-1"). Kalau tidak ada yang setara, nama dari file (dirapikan) dipakai.
pub fn resolve_class(existing: &[String], raw: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() || existing.iter().any(|c| c == raw) {
        return raw.to_string();
    }
    if let Some(key) = class_key(raw) {
        let hits: Vec<&String> = existing.iter().filter(|c| class_key(c).as_ref() == Some(&key)).collect();
        if hits.len() == 1 {
            return hits[0].clone();
        }
    }
    raw.to_string()
}

/// Isi sel kelas → nama kelas rapi: "IX-1", "Kelas 9.1" → "9-1", "9A" → "9-A".
pub fn tidy_class(raw: &str) -> String {
    let t = raw.trim();
    canonical_class(t)
        .filter(|c| c.contains('-') || !t.contains(['.', ' ', '/']))
        .or_else(|| {
            class_key(t).map(|(g, rest)| {
                let roman = ROMAN_GRADES.iter().find(|(_, n)| *n == g).map(|(r, _)| *r).unwrap_or("");
                // ikut gaya penulisan file: angka tetap angka, romawi tetap romawi
                let up = t.to_uppercase();
                let grade = if up.trim_start_matches("KELAS").trim().starts_with(|c: char| c.is_ascii_digit()) { g.to_string() } else { roman.to_string() };
                if rest.is_empty() { grade } else { format!("{grade}-{rest}") }
            })
        })
        .unwrap_or_else(|| crate::importer::normalize_class(t))
}

/// Semua nama kelas yang sudah ada di aplikasi (dari siswa dan Pengaturan).
pub fn known_classes(data: &crate::model::AppData) -> Vec<String> {
    let mut v: Vec<String> = data
        .students
        .iter()
        .map(|s| s.class_name.clone())
        .chain(data.settings.classes.iter().map(|c| c.name.clone()))
        .filter(|c| !c.is_empty())
        .collect();
    v.sort();
    v.dedup();
    v
}

pub(crate) fn col_letter(mut c: usize) -> String {
    let mut out = Vec::new();
    loop {
        out.push((b'A' + (c % 26) as u8) as char);
        if c < 26 {
            break;
        }
        c = c / 26 - 1;
    }
    out.into_iter().rev().collect()
}

/// Jenis kelamin ke "L"/"P" (kosong kalau tidak dikenali).
pub fn normalize_gender(raw: &str) -> String {
    match normalize_name(raw).as_str() {
        "L" | "LK" | "LAKI LAKI" | "LAKI" | "LELAKI" | "PRIA" | "COWOK" | "M" | "MALE" | "BOY" => "L".into(),
        "P" | "PR" | "PEREMPUAN" | "WANITA" | "CEWEK" | "F" | "FEMALE" | "GIRL" => "P".into(),
        _ => String::new(),
    }
}

/// Baris yang jelas bukan siswa: rekap, tanda tangan, catatan.
pub(crate) fn is_footer_row(name: &str) -> bool {
    let n = normalize_name(name);
    const STARTS: [&str; 14] = [
        "JUMLAH", "TOTAL", "RATA", "KETERANGAN", "MENGETAHUI", "KEPALA", "WALI KELAS", "GURU", "CATATAN", "NIP",
        "LAKI LAKI", "PEREMPUAN", "SINGARAJA", "TTD",
    ];
    STARTS.iter().any(|s| n == *s || n.starts_with(&format!("{s} "))) || name.contains(':')
}

struct Profile {
    filled: usize,
    nisn_like: usize,
    nis_like: usize,
    gender_like: usize,
    small_ints: usize,
}

/// Ciri isi satu kolom pada baris-baris data (dipakai untuk menebak kolom tanpa judul jelas).
fn profile(grid: &Grid, rows: &[usize], c: usize) -> Profile {
    let mut p = Profile { filled: 0, nisn_like: 0, nis_like: 0, gender_like: 0, small_ints: 0 };
    for &r in rows {
        let d = cell(grid, r, c);
        let t = cell_text(d);
        if t.is_empty() {
            continue;
        }
        p.filled += 1;
        let digits = t.chars().all(|ch| ch.is_ascii_digit());
        let nisn = format_nisn(d);
        if nisn.len() == 10 && nisn.chars().all(|ch| ch.is_ascii_digit()) && t.len() >= 9 {
            p.nisn_like += 1;
        } else if digits && (3..=9).contains(&t.len()) {
            p.nis_like += 1;
        }
        if digits && t.len() <= 3 {
            p.small_ints += 1;
        }
        if !normalize_gender(&t).is_empty() {
            p.gender_like += 1;
        }
    }
    p
}

impl Profile {
    fn mostly(&self, n: usize) -> bool {
        self.filled >= 2 && n * 10 >= self.filled * 7
    }
}

/// Tebak susunan tabel. Mengembalikan pilihan kolom + asal tebakan kelas.
pub fn guess_mapping(grid: &Grid, sheet_name: &str, file_stem: &str) -> Option<(StudentMapping, String)> {
    // 1. baris judul = baris pertama yang punya kolom Nama
    let (hr, name_col) = (0..grid.len().min(30)).find_map(|r| {
        (0..grid[r].len()).find(|&c| is_name_header(&header(cell(grid, r, c)))).map(|c| (r, c))
    })?;
    // judul dua baris (sel digabung): baris bawah kosong di kolom Nama tapi berisi teks di kolom lain
    let sub = has_sub_header(grid, hr, name_col);
    let width = grid.iter().skip(hr).take(60).map(|r| r.len()).max().unwrap_or(0);
    let col_header = |c: usize| {
        let a = header(cell(grid, hr, c));
        let b = if sub { header(cell(grid, hr + 1, c)) } else { String::new() };
        [a, b].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ")
    };
    let data_rows: Vec<usize> = (hr + 1..grid.len())
        .filter(|&r| {
            let n = cell_text(cell(grid, r, name_col));
            !n.is_empty() && !is_footer_row(&n) && !is_name_header(&normalize_name(&n))
        })
        .take(200)
        .collect();

    let mut m = StudentMapping { header_row: if sub { hr + 1 } else { hr }, name: Some(name_col), ..Default::default() };
    for c in 0..width {
        if c == name_col {
            continue;
        }
        let h = col_header(c);
        if h.contains("NISN") {
            m.nisn.get_or_insert(c);
        } else if matches!(h.as_str(), "NIS" | "NIPD" | "NO INDUK" | "NOMOR INDUK" | "INDUK" | "NIS LOKAL" | "STUDENT ID" | "ID SISWA" | "STUDENT NUMBER")
            || (h.contains("INDUK") && !h.contains("NISN"))
        {
            m.nis.get_or_insert(c);
        } else if matches!(h.as_str(), "L P" | "LP" | "JK" | "J K" | "JENIS KELAMIN" | "KELAMIN" | "GENDER" | "SEX") {
            m.gender.get_or_insert(c);
        } else if matches!(h.as_str(), "KELAS" | "KLS" | "ROMBEL" | "ROMBEL SAAT INI" | "ROMBONGAN BELAJAR" | "CLASS") {
            m.class.get_or_insert(c);
        }
    }

    // 2. periksa & lengkapi lewat isi kolom
    let used = |m: &StudentMapping, c: usize| [m.name, m.nis, m.nisn, m.gender, m.class].contains(&Some(c));
    let prof = |c: usize| profile(grid, &data_rows, c);
    // kolom berjudul NISN tapi isinya bukan 10 digit (mis. judul "NISN / Induk" menaungi dua kolom)
    if let Some(c) = m.nisn {
        let p = prof(c);
        if !p.mostly(p.nisn_like) {
            if let Some(alt) = (0..width).find(|&x| !used(&m, x) && { let q = prof(x); q.mostly(q.nisn_like) }) {
                if m.nis.is_none() && p.mostly(p.nis_like) {
                    m.nis = Some(c);
                }
                m.nisn = Some(alt);
            }
        }
    }
    if m.nisn.is_none() {
        m.nisn = (0..width).find(|&c| !used(&m, c) && { let p = prof(c); p.mostly(p.nisn_like) });
    }
    if m.nis.is_none() {
        // nomor induk: angka 3–9 digit, bukan kolom nomor urut (1, 2, 3, …)
        m.nis = (0..width).find(|&c| !used(&m, c) && { let p = prof(c); p.mostly(p.nis_like) && !p.mostly(p.small_ints) });
    }
    if m.gender.is_none() {
        m.gender = (0..width).find(|&c| !used(&m, c) && { let p = prof(c); p.mostly(p.gender_like) });
    }

    // 3. kelas: kolom → judul di atas tabel → nama sheet → nama file
    let mut source = String::new();
    if m.class.is_some() {
        source = "kolom kelas".into();
    } else if let Some(c) = (0..hr).flat_map(|r| grid[r].iter()).find_map(|d| class_from_text(&cell_text(d))) {
        m.default_class = c;
        source = "judul tabel".into();
    } else if let Some(c) = (sheet_name.trim().chars().count() <= 8).then(|| canonical_class(sheet_name)).flatten() {
        m.default_class = c;
        source = "nama sheet".into();
    } else if let Some(c) = class_from_text(file_stem).or_else(|| canonical_class(file_stem)) {
        m.default_class = c;
        source = "nama file".into();
    }
    Some((m, source))
}

// ---------------------------------------------------------------- pembacaan

/// Baca baris siswa sesuai pilihan kolom.
pub fn parse_mapped(grid: &Grid, m: &StudentMapping) -> Result<Vec<StudentRow>, String> {
    let name_col = m.name.ok_or("Pilih kolom Nama dulu")?;
    let fallback_class = canonical_class(&m.default_class).unwrap_or_else(|| m.default_class.trim().to_uppercase());
    let mut section: Option<String> = None;
    let mut out = Vec::new();
    for r in m.header_row + 1..grid.len() {
        let name = cell_text(cell(grid, r, name_col)).split_whitespace().collect::<Vec<_>>().join(" ");
        if name.is_empty() {
            // baris pemisah kelas (mis. "KELAS IX-2" / "VII-1") — hanya teks, bukan nomor urut kosong
            let first = cell(grid, r, 0);
            if matches!(first, calamine::Data::String(_)) {
                let t = cell_text(first);
                if let Some(c) = class_from_text(&t).or_else(|| canonical_class(&t).filter(|c| c.contains('-'))) {
                    section = Some(c);
                }
            }
            continue;
        }
        if is_footer_row(&name) || is_name_header(&normalize_name(&name)) || name.chars().filter(|c| c.is_alphabetic()).count() < 2 {
            continue;
        }
        let class_name = m
            .class
            .map(|c| cell_text(cell(grid, r, c)))
            .filter(|t| !t.is_empty())
            .map(|t| tidy_class(&t))
            .or_else(|| section.clone())
            .unwrap_or_else(|| fallback_class.clone());
        let order = out.len() as u32 + 1;
        out.push(StudentRow {
            order,
            name,
            nis: m.nis.map(|c| cell_text(cell(grid, r, c))).unwrap_or_default(),
            nisn: m.nisn.map(|c| format_nisn(cell(grid, r, c))).unwrap_or_default(),
            gender: m.gender.map(|c| normalize_gender(&cell_text(cell(grid, r, c)))).unwrap_or_default(),
            class_name,
            rombel_conflict: None,
        });
    }
    if out.is_empty() {
        return Err("Tidak ada baris siswa di kolom Nama yang dipilih".into());
    }
    Ok(out)
}

fn file_stem(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

/// Tebak (atau pakai pilihan pengguna) lalu kembalikan pratinjau.
pub fn inspect(path: &Path, sheet: &str, mapping: Option<StudentMapping>) -> Result<FlexPreview, String> {
    let grid = read_grid(path, sheet)?;
    let guessed = guess_mapping(&grid, sheet, &file_stem(path));
    let (m, source) = match (mapping, guessed) {
        (Some(m), g) => {
            let src = if m.class.is_some() {
                "kolom kelas".to_string()
            } else if g.as_ref().is_some_and(|(gm, _)| !m.default_class.trim().is_empty() && gm.default_class == m.default_class) {
                g.map(|(_, s)| s).unwrap_or_default()
            } else if m.default_class.trim().is_empty() {
                String::new()
            } else {
                "diisi manual".into()
            };
            (m, src)
        }
        (None, Some(g)) => g,
        (None, None) => {
            return Err("Kolom Nama tidak ditemukan di sheet ini. Pastikan ada judul kolom \"Nama\" / \"Nama Siswa\".".into())
        }
    };

    let hr = m.header_row.min(grid.len().saturating_sub(1));
    let width = grid.iter().skip(hr.saturating_sub(1)).take(60).map(|r| r.len()).max().unwrap_or(0);
    let columns = (0..width)
        .map(|c| {
            let titles: Vec<String> = (hr.saturating_sub(1)..=hr).map(|r| cell_text(cell(&grid, r, c))).filter(|t| !t.is_empty()).collect();
            let samples = (hr + 1..grid.len())
                .map(|r| cell_text(cell(&grid, r, c)))
                .filter(|t| !t.is_empty())
                .take(3)
                .map(|t| t.chars().take(28).collect())
                .collect();
            ColumnInfo { index: c, letter: col_letter(c), header: titles.join(" / "), samples }
        })
        .collect();

    let rows = parse_mapped(&grid, &m)?;
    let mut warnings = Vec::new();
    let no_class = rows.iter().filter(|r| r.class_name.is_empty()).count();
    if no_class > 0 {
        warnings.push(format!("{no_class} siswa belum punya kelas. Isi \"Kelas\" di atas sebelum mengimpor."));
    }
    if m.nis.is_none() && m.nisn.is_none() {
        warnings.push("File ini tidak punya NIS/NISN. Siswa dikenali lewat nama + kelas; impor ulang memperbarui siswa yang sama.".into());
    }
    let mut classes = count_classes(rows.iter().map(|r| r.class_name.clone()).filter(|c| !c.is_empty()));
    classes.sort_by_key(|c| class_sort_key(&c.name));
    Ok(FlexPreview {
        columns,
        mapping: m,
        class_source: source,
        total: rows.len(),
        rows: rows.into_iter().take(8).collect(),
        classes,
        warnings,
    })
}

pub fn read_mapped(path: &Path, sheet: &str, m: &StudentMapping) -> Result<Vec<StudentRow>, String> {
    let rows = parse_mapped(&read_grid(path, sheet)?, m)?;
    if rows.iter().any(|r| r.class_name.is_empty()) {
        return Err("Ada siswa yang belum punya kelas. Isi kelas dulu.".into());
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use calamine::Data;

    #[test]
    fn class_names_from_various_writings() {
        assert_eq!(canonical_class("IC").as_deref(), Some("I-C"));
        assert_eq!(canonical_class("XB").as_deref(), Some("X-B"));
        assert_eq!(canonical_class("x b").as_deref(), Some("X-B"));
        assert_eq!(canonical_class("IXA").as_deref(), Some("IX-A"));
        assert_eq!(canonical_class("VIIIB").as_deref(), Some("VIII-B"));
        assert_eq!(canonical_class("IX-1").as_deref(), Some("IX-1"));
        assert_eq!(canonical_class("7A").as_deref(), Some("7-A"));
        assert_eq!(canonical_class("XI").as_deref(), Some("XI"));
        assert_eq!(canonical_class("XII IPA1").as_deref(), Some("XII-IPA1"));
        assert_eq!(canonical_class("26"), None);
        assert_eq!(canonical_class("NAMA"), None);
        assert_eq!(class_from_text("KELAS :  IC TH. 2026/2027").as_deref(), Some("I-C"));
        assert_eq!(class_from_text("KELAS XB").as_deref(), Some("X-B"));
        assert_eq!(class_from_text("Kelas X B").as_deref(), Some("X-B"));
        assert_eq!(class_from_text("Data Siswa SD Lab"), None);
    }

    #[test]
    fn gender_variants() {
        for (raw, want) in [("L", "L"), ("Laki-laki", "L"), ("laki laki", "L"), ("P", "P"), ("Perempuan", "P"), ("?", "")] {
            assert_eq!(normalize_gender(raw), want, "{raw}");
        }
    }

    fn s(v: &str) -> Data {
        Data::String(v.into())
    }

    #[test]
    fn sd_grade_book_with_merged_two_row_header() {
        let e = || Data::Empty;
        // mirip "DAFTAR NILAI PRESTASI" SD: judul 2 baris, "NISN / Induk" menaungi 2 kolom
        let grid: Grid = vec![
            vec![s("SD LABORATORIUM UNDIKSHA")],
            vec![s("DAFTAR NILAI PRESTASI"), e(), e(), e(), e(), e(), s("KELAS :  IC TH. 2026/2027")],
            vec![s("Nomor"), e(), e(), s("L/P"), s("BUKU"), s("NAMA SISWA"), s("I")],
            vec![s("Abs."), s("NISN / Induk"), e(), e(), e(), e(), e()],
            vec![Data::Float(1.0), s("3195900273"), Data::Float(3941.0), s("P"), e(), s("Anak Agung Ayu Trisha"), e()],
            vec![Data::Float(2.0), Data::Float(199886731.0), Data::Float(3942.0), s("L"), e(), s("Gede Aksa"), e()],
            vec![Data::Float(3.0), e(), e(), e(), e(), e(), e()],
        ];
        let (m, src) = guess_mapping(&grid, "IC", "Data Siswa SD").unwrap();
        assert_eq!((m.header_row, m.name, m.nisn, m.nis, m.gender), (3, Some(5), Some(1), Some(2), Some(3)));
        assert_eq!((m.default_class.as_str(), src.as_str()), ("I-C", "judul tabel"));
        let rows = parse_mapped(&grid, &m).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].nisn, "3195900273");
        assert_eq!(rows[1].nisn, "0199886731"); // nol di depan dikembalikan
        assert_eq!(rows[0].nis, "3941");
        assert_eq!(rows[0].gender, "P");
        assert_eq!(rows[0].class_name, "I-C");
    }

    #[test]
    fn simple_class_list_class_from_file_name() {
        let grid: Grid = vec![
            vec![s("NO."), s("NAMA"), s("L / P")],
            vec![Data::Float(1.0), s("ADAM MOHAMED"), s("Laki-laki")],
            vec![Data::Float(2.0), s("ANAK AGUNG AYU RIANA"), s("Perempuan")],
            vec![e_(), s("Jumlah: 2"), e_()],
        ];
        let (m, src) = guess_mapping(&grid, "Sheet1", "KELAS XB").unwrap();
        assert_eq!((m.name, m.gender, m.nis, m.nisn), (Some(1), Some(2), None, None));
        assert_eq!((m.default_class.as_str(), src.as_str()), ("X-B", "nama file"));
        let rows = parse_mapped(&grid, &m).unwrap();
        assert_eq!(rows.len(), 2, "baris rekap tidak ikut");
        assert_eq!((rows[0].gender.as_str(), rows[1].gender.as_str()), ("L", "P"));
        assert!(rows.iter().all(|r| r.class_name == "X-B"));
    }

    fn e_() -> Data {
        Data::Empty
    }

    #[test]
    fn parent_name_columns_are_not_student_names() {
        let grid: Grid = vec![
            vec![s("No"), s("Nama Ayah"), s("Nama Peserta Didik"), s("Kelas")],
            vec![Data::Float(1.0), s("Bapak A"), s("Siswa Satu"), s("7a")],
        ];
        let (m, src) = guess_mapping(&grid, "Data", "siswa").unwrap();
        assert_eq!((m.name, m.class), (Some(2), Some(3)));
        assert_eq!(src, "kolom kelas");
        assert_eq!(parse_mapped(&grid, &m).unwrap()[0].class_name, "7-A");
    }

    #[test]
    fn same_class_written_differently() {
        for raw in ["IX-1", "9-1", "9.1", "Kelas 9.1", "ix.1", "IX 1", "kelas IX-1"] {
            assert_eq!(class_key(raw), Some((9, "1".to_string())), "{raw}");
        }
        assert_eq!(class_key("9A"), Some((9, "A".to_string())));
        let existing = vec!["IX-1".to_string(), "IX-2".to_string(), "VIII-1".to_string()];
        assert_eq!(resolve_class(&existing, "9.1"), "IX-1");
        assert_eq!(resolve_class(&existing, "Kelas 8.1"), "VIII-1");
        assert_eq!(resolve_class(&existing, "IX-3"), "IX-3", "kelas baru tetap kelas baru");
        assert_eq!(tidy_class("Kelas 9.1"), "9-1");
        assert_eq!(tidy_class("ix.2"), "IX-2");
        assert_eq!(tidy_class("9A"), "9-A");
    }

    #[test]
    fn english_headers_and_class_separator_rows() {
        let s = |v: &str| Data::String(v.into());
        let n = |v: f64| Data::Float(v);
        let grid: Grid = vec![
            vec![s("Student ID"), s("Gender"), s("Full Name"), s("NISN"), s("Class")],
            vec![n(4101.0), s("F"), s("Made Dewi"), s("0091234561"), s("IX-1")],
            vec![n(4102.0), s("M"), s("Gede Arya"), s("0091234562"), s("IX-1")],
        ];
        let (m, _) = guess_mapping(&grid, "Students", "").unwrap();
        assert_eq!((m.name, m.nis, m.nisn, m.gender, m.class), (Some(2), Some(0), Some(3), Some(1), Some(4)));
        assert_eq!(parse_mapped(&grid, &m).unwrap()[0].gender, "P");

        // baris "KELAS IX-1" tepat di bawah judul bukan judul kedua
        let grid: Grid = vec![
            vec![s("NO"), s("NAMA"), s("NIS")],
            vec![s("KELAS IX-1")],
            vec![n(1.0), s("Made Dewi"), n(4101.0)],
            vec![s("KELAS IX-2")],
            vec![n(1.0), s("Gede Arya"), n(4102.0)],
        ];
        assert!(!has_sub_header(&grid, 0, 1));
        let (m, _) = guess_mapping(&grid, "Gabungan", "").unwrap();
        let rows = parse_mapped(&grid, &m).unwrap();
        assert_eq!(rows.iter().map(|r| r.class_name.as_str()).collect::<Vec<_>>(), ["IX-1", "IX-2"]);
        // judul dua baris sungguhan tetap dikenali
        let grid: Grid = vec![vec![s("No"), s("Nama"), s("Nomor Induk")], vec![Data::Empty, Data::Empty, s("NIS"), s("NISN")]];
        assert!(has_sub_header(&grid, 0, 1));
    }
}
