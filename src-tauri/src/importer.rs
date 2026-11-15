//! Impor dari Excel (.xls / .xlsx / .ods).
//!
//! Dua jenis sheet dikenali otomatis:
//! - **Data siswa** (ekspor Dapodik): ada kolom "Nama" + "NIPD"/"NIS"/"NISN".
//! - **Rekap nilai**: ada kolom "NAMA SISWA", baris judul mapel, dan di
//!   bawahnya baris "PH" / "PTS" (seperti sheet "Data Nilai Kls 7 8,9").
//!
//! Baris pemisah kelas (mis. sel "IX-1" di kolom pertama) dipakai untuk
//! mengetahui kelas setiap siswa.

use crate::model::{canonical_key, class_sort_key, AppData, ClassConfig, Student};
use crate::names::{match_names, normalize_name, MatchKind};
use calamine::{open_workbook_auto, Data, Reader};
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

type Grid = Vec<Vec<Data>>;

static EMPTY: Data = Data::Empty;

fn cell(grid: &Grid, r: usize, c: usize) -> &Data {
    grid.get(r).and_then(|row| row.get(c)).unwrap_or(&EMPTY)
}

/// Isi sel sebagai teks. Angka bulat ditulis tanpa ".0" (4635.0 → "4635").
pub(crate) fn cell_text(d: &Data) -> String {
    match d {
        Data::String(s) => s.trim().to_string(),
        Data::Int(i) => i.to_string(),
        Data::Float(f) if (f - f.round()).abs() < 1e-9 => format!("{}", f.round() as i64),
        Data::Float(f) => f.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::DateTimeIso(s) | Data::DurationIso(s) => s.clone(),
        _ => String::new(),
    }
}

fn cell_number(d: &Data) -> Option<f64> {
    match d {
        Data::Float(f) if f.is_finite() => Some(*f),
        Data::Int(i) => Some(*i as f64),
        Data::String(s) => crate::grading::parse_number(s),
        _ => None,
    }
}

/// Teks header untuk dibandingkan: huruf besar, tanpa tanda baca.
fn header(d: &Data) -> String {
    normalize_name(&cell_text(d))
}

fn open(path: &Path) -> Result<calamine::Sheets<std::io::BufReader<std::fs::File>>, String> {
    open_workbook_auto(path).map_err(|e| format!("File tidak bisa dibuka: {e}"))
}

fn read_grid(path: &Path, sheet: &str) -> Result<Grid, String> {
    sheet_grid(&mut open(path)?, sheet)
}

fn sheet_grid<RS: std::io::Read + std::io::Seek>(
    wb: &mut calamine::Sheets<RS>,
    sheet: &str,
) -> Result<Grid, String> {
    let range = wb
        .worksheet_range(sheet)
        .map_err(|e| format!("Sheet \"{sheet}\" tidak bisa dibaca: {e}"))?;
    // Susun ulang dengan posisi absolut supaya kolom A selalu indeks 0.
    let (r0, c0) = range.start().unwrap_or((0, 0));
    let (r0, c0) = (r0 as usize, c0 as usize);
    let mut grid: Grid = vec![Vec::new(); r0];
    for row in range.rows() {
        let mut v = vec![Data::Empty; c0];
        v.extend(row.iter().cloned());
        grid.push(v);
    }
    Ok(grid)
}

/// "Kelas IX-1" / "ix - 1" → "IX-1".
pub fn normalize_class(s: &str) -> String {
    let mut t = s.trim().to_uppercase();
    if let Some(rest) = t.strip_prefix("KELAS") {
        t = rest.trim().to_string();
    }
    t.split_whitespace().collect::<Vec<_>>().join("").replace("--", "-")
}

/// Apakah teks ini nama kelas seperti "IX-1", "VIII-3", "9-2"?
pub fn looks_like_class(s: &str) -> bool {
    let n = normalize_class(s);
    let Some((a, b)) = n.split_once('-') else {
        return false;
    };
    let grade_ok = !a.is_empty()
        && (a.chars().all(|c| matches!(c, 'I' | 'V' | 'X')) || a.chars().all(|c| c.is_ascii_digit()));
    let part_ok = !b.is_empty() && b.len() <= 3 && b.chars().all(|c| c.is_ascii_alphanumeric());
    grade_ok && part_ok && n.len() <= 10
}

// ---------------------------------------------------------------- scan

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SheetKind {
    Students,
    Scores,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClassCount {
    pub name: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SheetScan {
    pub name: String,
    pub kind: SheetKind,
    pub classes: Vec<ClassCount>,
    /// Untuk sheet nilai: judul mapel yang ditemukan.
    pub subjects: Vec<String>,
}

fn count_classes(classes: impl Iterator<Item = String>) -> Vec<ClassCount> {
    let mut map: BTreeMap<(u32, u32, String), usize> = BTreeMap::new();
    for c in classes {
        *map.entry(class_sort_key(&c)).or_default() += 1;
    }
    map.into_iter()
        .map(|((_, _, name), count)| ClassCount { name, count })
        .collect()
}

/// Baca semua sheet dan tebak jenisnya.
pub fn scan_workbook(path: &Path) -> Result<Vec<SheetScan>, String> {
    let mut wb = open(path)?;
    let mut out = Vec::new();
    for name in wb.sheet_names() {
        let grid = sheet_grid(&mut wb, &name)?;
        if let Ok(rows) = parse_students(&grid) {
            out.push(SheetScan {
                name,
                kind: SheetKind::Students,
                classes: count_classes(rows.into_iter().map(|r| r.class_name)),
                subjects: vec![],
            });
        } else if let Ok(sheet) = parse_scores(&grid) {
            out.push(SheetScan {
                name,
                kind: SheetKind::Scores,
                classes: count_classes(sheet.rows.into_iter().map(|r| r.class_name)),
                subjects: sheet.subjects,
            });
        } else {
            out.push(SheetScan {
                name,
                kind: SheetKind::Unknown,
                classes: vec![],
                subjects: vec![],
            });
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------- data siswa

#[derive(Debug, Clone, PartialEq)]
pub struct StudentRow {
    pub name: String,
    pub nis: String,
    pub nisn: String,
    pub gender: String,
    pub class_name: String,
    /// Isi kolom Rombel kalau BERBEDA dengan judul kelas di file (judul kelas yang dipakai).
    pub rombel_conflict: Option<String>,
}

struct StudentCols {
    name: usize,
    nis: Option<usize>,
    nisn: Option<usize>,
    gender: Option<usize>,
    class: Option<usize>,
}

fn find_student_header(grid: &Grid) -> Option<(usize, StudentCols)> {
    for (r, row) in grid.iter().enumerate().take(25) {
        let find = |names: &[&str]| (0..row.len()).find(|&c| names.contains(&header(&row[c]).as_str()));
        let Some(name) = find(&["NAMA", "NAMA SISWA", "NAMA PESERTA DIDIK", "NAMA LENGKAP"]) else {
            continue;
        };
        let nis = find(&["NIPD", "NIS", "NOMOR INDUK", "NOMOR INDUK SISWA"]);
        let nisn = find(&["NISN"]);
        if nis.is_none() && nisn.is_none() {
            continue;
        }
        let cols = StudentCols {
            name,
            nis,
            nisn,
            gender: find(&["JK", "L P", "JENIS KELAMIN"]),
            class: find(&["ROMBEL SAAT INI", "ROMBEL", "KELAS"]),
        };
        return Some((r, cols));
    }
    None
}

/// NISN selalu 10 digit; kalau tersimpan sebagai angka, nol di depan hilang.
fn format_nisn(d: &Data) -> String {
    match d {
        Data::Float(_) | Data::Int(_) => {
            let t = cell_text(d);
            if t.chars().all(|c| c.is_ascii_digit()) && t.len() < 10 {
                format!("{t:0>10}")
            } else {
                t
            }
        }
        _ => cell_text(d),
    }
}

pub fn parse_students(grid: &Grid) -> Result<Vec<StudentRow>, String> {
    let (hr, cols) = find_student_header(grid).ok_or("Kolom Nama dan NIS/NISN tidak ditemukan")?;
    let mut section: Option<String> = None;
    let mut out = Vec::new();
    for r in hr + 1..grid.len() {
        let first = cell_text(cell(grid, r, 0));
        let name = cell_text(cell(grid, r, cols.name));
        if name.is_empty() {
            if looks_like_class(&first) {
                section = Some(normalize_class(&first));
            }
            continue;
        }
        let nis = cols.nis.map(|c| cell_text(cell(grid, r, c))).unwrap_or_default();
        let nisn = cols.nisn.map(|c| format_nisn(cell(grid, r, c))).unwrap_or_default();
        if nis.is_empty() && nisn.is_empty() && cell_number(cell(grid, r, 0)).is_none() {
            continue; // bukan baris siswa (mis. sub-judul)
        }
        // Judul kelas di file (baris "IX-1") didahulukan karena itu yang dipakai
        // sekolah di rekap nilai; kolom Rombel Dapodik kadang belum diperbarui.
        let class_from_col = cols
            .class
            .map(|c| normalize_class(&cell_text(cell(grid, r, c))))
            .filter(|c| !c.is_empty());
        let class_name = section.clone().or_else(|| class_from_col.clone()).unwrap_or_default();
        let rombel_conflict = class_from_col.filter(|c| *c != class_name);
        let gender = cols
            .gender
            .map(|c| cell_text(cell(grid, r, c)).to_uppercase())
            .unwrap_or_default();
        out.push(StudentRow {
            name,
            nis,
            nisn,
            gender,
            class_name,
            rombel_conflict,
        });
    }
    if out.is_empty() {
        return Err("Tidak ada baris siswa".into());
    }
    Ok(out)
}

pub fn read_students(path: &Path, sheet: &str) -> Result<Vec<StudentRow>, String> {
    parse_students(&read_grid(path, sheet)?)
}

// ---------------------------------------------------------------- nilai

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Component {
    Ph,
    Pts,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SubjectScore {
    pub label: String,
    pub ph: Option<f64>,
    pub pts: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScoreRow {
    pub class_name: String,
    pub name: String,
    pub scores: Vec<SubjectScore>,
}

#[derive(Debug, Clone)]
pub struct ScoreSheet {
    pub subjects: Vec<String>,
    pub rows: Vec<ScoreRow>,
}

struct ScoreCol {
    col: usize,
    subject: String,
    comp: Component,
}

fn is_summary_column(h: &str) -> bool {
    matches!(
        normalize_name(h).as_str(),
        "RATA RATA" | "RATA2" | "JUMLAH" | "TOTAL" | "RANKING" | "PERINGKAT" | "KET" | "KETERANGAN"
    )
}

fn find_score_header(grid: &Grid) -> Option<(usize, usize, Vec<ScoreCol>)> {
    for r in 0..grid.len().min(25).saturating_sub(1) {
        let row = &grid[r];
        let Some(name_col) = (0..row.len())
            .find(|&c| matches!(header(&row[c]).as_str(), "NAMA SISWA" | "NAMA" | "NAMA PESERTA DIDIK"))
        else {
            continue;
        };
        let width = row.len().max(grid[r + 1].len());
        let mut current: Option<String> = None;
        let mut cols = Vec::new();
        for c in name_col + 1..width {
            let h = cell_text(cell(grid, r, c));
            if !h.is_empty() {
                current = Some(h.to_uppercase());
            }
            let comp = match header(cell(grid, r + 1, c)).as_str() {
                "PH" | "NH" | "HARIAN" | "NILAI HARIAN" => Some(Component::Ph),
                "PTS" | "STS" | "PSTS" | "SUMATIF TENGAH SEMESTER" => Some(Component::Pts),
                _ => None,
            };
            if let (Some(subject), Some(comp)) = (&current, comp) {
                if !is_summary_column(subject) {
                    cols.push(ScoreCol {
                        col: c,
                        subject: subject.clone(),
                        comp,
                    });
                }
            }
        }
        if !cols.is_empty() {
            return Some((r, name_col, cols));
        }
    }
    None
}

pub fn parse_scores(grid: &Grid) -> Result<ScoreSheet, String> {
    let (hr, name_col, cols) = find_score_header(grid)
        .ok_or("Format nilai tidak dikenali (perlu kolom NAMA SISWA dan baris PH/PTS)")?;
    let mut subjects: Vec<String> = Vec::new();
    for sc in &cols {
        if !subjects.contains(&sc.subject) {
            subjects.push(sc.subject.clone());
        }
    }
    let mut section: Option<String> = None;
    let mut rows = Vec::new();
    for r in hr + 2..grid.len() {
        let first = cell_text(cell(grid, r, 0));
        let name = cell_text(cell(grid, r, name_col));
        if name.is_empty() {
            if looks_like_class(&first) {
                section = Some(normalize_class(&first));
            }
            continue;
        }
        if cell_number(cell(grid, r, 0)).is_none() && name_col > 0 {
            continue; // baris tanpa nomor urut: bukan siswa
        }
        let mut scores: Vec<SubjectScore> = subjects
            .iter()
            .map(|s| SubjectScore {
                label: s.clone(),
                ph: None,
                pts: None,
            })
            .collect();
        for sc in &cols {
            let v = cell_number(cell(grid, r, sc.col));
            let slot = scores.iter_mut().find(|s| s.label == sc.subject).expect("subject ada");
            match sc.comp {
                Component::Ph => slot.ph = v,
                Component::Pts => slot.pts = v,
            }
        }
        rows.push(ScoreRow {
            class_name: section.clone().unwrap_or_default(),
            name,
            scores,
        });
    }
    if rows.is_empty() {
        return Err("Tidak ada baris nilai siswa".into());
    }
    Ok(ScoreSheet { subjects, rows })
}

pub fn read_scores(path: &Path, sheet: &str) -> Result<ScoreSheet, String> {
    parse_scores(&read_grid(path, sheet)?)
}

// ---------------------------------------------------------------- gabung ke data

#[derive(Debug, Clone, Serialize, Default)]
pub struct StudentImportReport {
    pub added: usize,
    pub updated: usize,
    pub classes: Vec<String>,
    /// Siswa yang kolom Rombel-nya berbeda dengan judul kelas di file.
    pub rombel_conflicts: Vec<String>,
}

fn wanted_set(classes: &[String]) -> HashSet<String> {
    classes.iter().map(|c| normalize_class(c)).collect()
}

pub fn ensure_class_configs(data: &mut AppData) {
    let mut names: Vec<String> = data.students.iter().map(|s| s.class_name.clone()).collect();
    names.sort_by_key(|c| class_sort_key(c));
    names.dedup();
    for n in names {
        if !n.is_empty() && !data.settings.classes.iter().any(|c| c.name == n) {
            data.settings.classes.push(ClassConfig {
                name: n,
                homeroom_teacher: String::new(),
                ..Default::default()
            });
        }
    }
    data.settings.classes.sort_by_key(|c| class_sort_key(&c.name));
}

pub fn new_student_id(data: &AppData, preferred: &str) -> String {
    let taken = |id: &str| data.students.iter().any(|s| s.id == id);
    if !preferred.is_empty() && !taken(preferred) {
        return preferred.to_string();
    }
    (1..)
        .map(|i| format!("siswa-{i}"))
        .find(|id| !taken(id))
        .expect("selalu ada id kosong")
}

/// Tambah/perbarui siswa. Nilai & isian rapor siswa yang sudah ada TIDAK diubah.
pub fn apply_students(data: &mut AppData, rows: Vec<StudentRow>, classes: &[String]) -> StudentImportReport {
    let wanted = wanted_set(classes);
    let mut report = StudentImportReport::default();
    let mut seen_classes: Vec<String> = Vec::new();
    for row in rows.into_iter().filter(|r| wanted.contains(&r.class_name)) {
        let norm = normalize_name(&row.name);
        let existing = data.students.iter().position(|s| {
            (!row.nis.is_empty() && s.nis == row.nis)
                || (!row.nisn.is_empty() && s.nisn == row.nisn)
                || (s.class_name == row.class_name && normalize_name(&s.name) == norm)
        });
        if !seen_classes.contains(&row.class_name) {
            seen_classes.push(row.class_name.clone());
        }
        if let Some(rombel) = &row.rombel_conflict {
            report
                .rombel_conflicts
                .push(format!("{} — dipakai {}, di kolom Rombel tertulis {rombel}", row.name, row.class_name));
        }
        match existing {
            Some(i) => {
                let s = &mut data.students[i];
                s.name = row.name;
                if !row.nis.is_empty() {
                    s.nis = row.nis;
                }
                if !row.nisn.is_empty() {
                    s.nisn = row.nisn;
                }
                if !row.gender.is_empty() {
                    s.gender = row.gender;
                }
                s.class_name = row.class_name;
                report.updated += 1;
            }
            None => {
                let id = new_student_id(data, &row.nis);
                data.students.push(Student {
                    id,
                    nis: row.nis,
                    nisn: row.nisn,
                    name: row.name,
                    gender: row.gender,
                    class_name: row.class_name,
                    ..Default::default()
                });
                report.added += 1;
            }
        }
    }
    ensure_class_configs(data);
    seen_classes.sort_by_key(|c| class_sort_key(c));
    report.classes = seen_classes;
    report
}

#[derive(Debug, Clone, Serialize)]
pub struct ApproxMatch {
    pub class_name: String,
    pub from_name: String,
    pub to_name: String,
    pub kind: MatchKind,
}

#[derive(Debug, Clone, Serialize)]
pub struct Unmatched {
    pub class_name: String,
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ScoreImportReport {
    pub matched: usize,
    pub approximate: Vec<ApproxMatch>,
    pub unmatched: Vec<Unmatched>,
    pub values_set: usize,
    pub invalid_values: usize,
    pub subjects: Vec<String>,
}

/// Masukkan nilai ke siswa yang cocok. Sel kosong di file TIDAK menghapus nilai lama.
pub fn apply_scores(data: &mut AppData, sheet: ScoreSheet, classes: &[String]) -> ScoreImportReport {
    let wanted = wanted_set(classes);
    let mut report = ScoreImportReport {
        subjects: sheet.subjects.clone(),
        ..Default::default()
    };
    let mut by_class: BTreeMap<String, Vec<ScoreRow>> = BTreeMap::new();
    for row in sheet.rows.into_iter().filter(|r| wanted.contains(&r.class_name)) {
        by_class.entry(row.class_name.clone()).or_default().push(row);
    }
    for (class_name, rows) in by_class {
        let idx: Vec<usize> = (0..data.students.len())
            .filter(|&i| data.students[i].class_name == class_name)
            .collect();
        let cand: Vec<String> = idx.iter().map(|&i| data.students[i].name.clone()).collect();
        let queries: Vec<String> = rows.iter().map(|r| r.name.clone()).collect();
        let matches = match_names(&queries, &cand);
        for (row, m) in rows.into_iter().zip(matches) {
            let Some((ci, kind)) = m else {
                report.unmatched.push(Unmatched {
                    class_name: class_name.clone(),
                    name: row.name,
                    reason: if idx.is_empty() {
                        "Kelas ini belum ada di data siswa".into()
                    } else {
                        "Nama tidak ditemukan di data siswa".into()
                    },
                });
                continue;
            };
            let student = &mut data.students[idx[ci]];
            report.matched += 1;
            if kind != MatchKind::Exact {
                report.approximate.push(ApproxMatch {
                    class_name: class_name.clone(),
                    from_name: row.name.clone(),
                    to_name: student.name.clone(),
                    kind,
                });
            }
            for s in row.scores {
                let entry = student.scores.entry(canonical_key(&s.label)).or_default();
                for (val, slot) in [(s.ph, &mut entry.ph), (s.pts, &mut entry.pts)] {
                    match val {
                        Some(v) if (0.0..=100.0).contains(&v) => {
                            *slot = Some(v);
                            report.values_set += 1;
                        }
                        Some(_) => report.invalid_values += 1,
                        None => {}
                    }
                }
            }
        }
    }
    for s in &report.subjects {
        if !data.score_columns.iter().any(|c| canonical_key(c) == canonical_key(s)) {
            data.score_columns.push(s.clone());
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample() -> Option<PathBuf> {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../berkas_atau_template/Contoh rekap nilai siswa.xls");
        p.exists().then_some(p)
    }

    #[test]
    fn class_names() {
        assert_eq!(normalize_class("Kelas IX-1"), "IX-1");
        assert_eq!(normalize_class(" ix - 2 "), "IX-2");
        assert!(looks_like_class("IX-1"));
        assert!(looks_like_class("VIII-3"));
        assert!(!looks_like_class("Daftar Peserta Didik"));
        assert!(!looks_like_class("NAMA SISWA"));
    }

    #[test]
    fn grid_parsing_with_sections() {
        let s = |v: &str| Data::String(v.into());
        let f = Data::Float;
        let e = Data::Empty;
        let grid: Grid = vec![
            vec![s("NO."), s("NAMA SISWA"), s("IPA"), e.clone(), s("RATA-RATA"), e.clone()],
            vec![e.clone(), e.clone(), s("PH"), s("PTS"), s("PH"), s("PTS")],
            vec![s("IX-1"), e.clone(), e.clone(), e.clone(), e.clone(), e.clone()],
            vec![f(1.0), s("MADE DEWI"), f(88.0), f(92.0), f(90.0), f(90.0)],
            vec![f(2.0), s("PUTU ARYA"), f(91.0), e.clone(), e.clone(), e.clone()],
        ];
        let sheet = parse_scores(&grid).unwrap();
        assert_eq!(sheet.subjects, vec!["IPA"]); // RATA-RATA diabaikan
        assert_eq!(sheet.rows.len(), 2);
        assert_eq!(sheet.rows[0].class_name, "IX-1");
        assert_eq!(sheet.rows[0].scores[0].ph, Some(88.0));
        assert_eq!(sheet.rows[1].scores[0].pts, None);
    }

    #[test]
    fn section_header_wins_over_stale_rombel() {
        let s = |v: &str| Data::String(v.into());
        let e = Data::Empty;
        let grid: Grid = vec![
            vec![s("No"), s("Nama"), s("NIPD"), s("NISN"), s("Rombel Saat Ini")],
            vec![s("VII-4"), e.clone(), e.clone(), e.clone(), e.clone()],
            vec![Data::Float(1.0), s("Made Dewi"), s("5001"), s("0123456789"), s("Kelas VII-2")],
            vec![Data::Float(2.0), s("Putu Arya"), s("5002"), s("0123456788"), s("Kelas VII-4")],
        ];
        let rows = parse_students(&grid).unwrap();
        assert_eq!(rows[0].class_name, "VII-4");
        assert_eq!(rows[0].rombel_conflict.as_deref(), Some("VII-2"));
        assert_eq!(rows[1].rombel_conflict, None);
    }

    #[test]
    fn rombel_column_used_when_no_section_headers() {
        let s = |v: &str| Data::String(v.into());
        let grid: Grid = vec![
            vec![s("No"), s("Nama"), s("NISN"), s("Kelas")],
            vec![Data::Float(1.0), s("Made Dewi"), s("0123456789"), s("IX-2")],
        ];
        let rows = parse_students(&grid).unwrap();
        assert_eq!(rows[0].class_name, "IX-2");
        assert_eq!(rows[0].rombel_conflict, None);
    }

    #[test]
    fn import_sample_file_kelas_9() {
        let Some(path) = sample() else {
            eprintln!("file contoh tidak ada, test dilewati");
            return;
        };
        let scans = scan_workbook(&path).unwrap();
        let students_sheet = scans.iter().find(|s| s.kind == SheetKind::Students).expect("sheet siswa");
        let scores_sheet = scans.iter().find(|s| s.kind == SheetKind::Scores).expect("sheet nilai");
        let nine: Vec<String> = students_sheet
            .classes
            .iter()
            .filter(|c| c.name.starts_with("IX-"))
            .map(|c| c.name.clone())
            .collect();
        assert_eq!(nine, vec!["IX-1", "IX-2", "IX-3"]);
        // Jumlah per kelas di data siswa harus sama dengan di rekap nilai
        // (termasuk VII-4 yang kolom Rombel-nya di Dapodik belum diperbarui).
        let counts = |s: &SheetScan| s.classes.iter().map(|c| (c.name.clone(), c.count)).collect::<Vec<_>>();
        assert_eq!(counts(students_sheet), counts(scores_sheet));

        let mut data = AppData::default();
        let rows = read_students(&path, &students_sheet.name).unwrap();
        let rep = apply_students(&mut data, rows, &nine);
        assert_eq!(rep.added, 79);
        assert_eq!(data.settings.classes.len(), 3);
        assert!(data.students.iter().all(|s| s.nisn.len() == 10), "NISN 10 digit");

        let sheet = read_scores(&path, &scores_sheet.name).unwrap();
        assert!(sheet.subjects.iter().any(|s| s == "IPA"));
        assert!(sheet.subjects.iter().any(|s| s == "B. INDO"));
        let rep = apply_scores(&mut data, sheet, &nine);
        assert_eq!(rep.matched, 79, "semua siswa kelas 9 cocok: {:?}", rep.unmatched);
        assert!(rep.unmatched.is_empty());
        assert_eq!(rep.approximate.len(), 6, "nama yang disingkat/salah ketik");

        // Siswa pertama IX-1 di file: IPA PH 80, PTS 90.
        let first = data.students.iter().find(|s| s.class_name == "IX-1").unwrap();
        let ipa = &first.scores["IPA"];
        assert_eq!((ipa.ph, ipa.pts), (Some(80.0), Some(90.0)));

        // Impor ulang tidak menggandakan siswa.
        let rows = read_students(&path, &students_sheet.name).unwrap();
        let rep2 = apply_students(&mut data, rows, &nine);
        assert_eq!((rep2.added, rep2.updated), (0, 79));
    }
}
