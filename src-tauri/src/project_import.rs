//! Impor nilai proyek (UOI) dari file Excel buatan guru/fasilitator.
//!
//! Susunan file tiap guru berbeda, jadi kolomnya ditebak (Nama, Kelas, Understanding,
//! Inquiry, Narasi) dan bisa dikoreksi di layar. Nama dicocokkan ke data siswa seperti
//! impor nilai mapel (persis → singkatan → mirip). Sel kosong TIDAK menghapus isian lama.

use crate::grading::fmt_num;
use crate::importer::{cell, cell_text, header, read_grid, ApproxMatch, Grid, Unmatched};
use crate::model::{AppData, Band, ProjectEntry};
use crate::names::{match_names, normalize_name, MatchKind};
use crate::student_flex::{col_letter, has_sub_header, is_footer_row, is_name_header, known_classes, resolve_class, tidy_class, ColumnInfo};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectMapping {
    /// Baris judul kolom (0-based); data dibaca mulai baris sesudahnya.
    pub header_row: usize,
    pub name: Option<usize>,
    pub class: Option<usize>,
    pub understanding: Option<usize>,
    pub inquiry: Option<usize>,
    pub narrative: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectRow {
    /// Nomor baris di Excel (1-based), untuk pesan.
    pub row: usize,
    pub name: String,
    pub class_name: String,
    pub understanding: String,
    pub inquiry: String,
    pub narrative: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectPreview {
    pub columns: Vec<ColumnInfo>,
    pub mapping: ProjectMapping,
    pub total: usize,
    pub rows: Vec<ProjectRow>,
    /// Proyek yang namanya/temanya disebut di nama sheet atau judul tabel.
    pub guessed_project: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ProjectImportReport {
    pub rows_read: usize,
    pub matched: usize,
    /// Sel yang isinya berubah (Understanding / Inquiry / Narasi).
    pub changed_cells: usize,
    /// Siswa yang baru mendapat proyek ini.
    pub new_entries: usize,
    /// Isian lama yang tertimpa, mis. "Ayu: Understanding ME → EE".
    pub overwritten: Vec<String>,
    pub approximate: Vec<ApproxMatch>,
    pub unmatched: Vec<Unmatched>,
    /// Isian yang tidak terbaca sebagai predikat/angka, mis. "Baris 7 (Ayu): \"baik\"".
    pub invalid: Vec<String>,
    /// Siswa yang sudah punya proyek LAIN (setelah impor mereka punya dua proyek di rapor).
    pub other_project: Vec<String>,
}

// ---------------------------------------------------------------- predikat

/// Isian predikat dari Excel → kode ("EE") atau angka ("92"). `Err` kalau tidak terbaca.
/// Kosong → `Ok("")`.
pub fn normalize_grade(d: &calamine::Data, bands: &[Band]) -> Result<String, String> {
    use calamine::Data;
    let raw = cell_text(d);
    if raw.is_empty() {
        return Ok(String::new());
    }
    let number = match d {
        Data::Float(f) => Some(*f),
        Data::Int(i) => Some(*i as f64),
        _ => crate::grading::parse_number(&raw),
    };
    if let Some(n) = number {
        return if (0.0..=100.0).contains(&n) { Ok(fmt_num(n)) } else { Err(raw) };
    }
    let up = normalize_name(&raw);
    let compact: String = up.chars().filter(|c| !c.is_whitespace()).collect();
    if let Some(b) = bands.iter().find(|b| b.code.eq_ignore_ascii_case(&compact) || normalize_name(&b.label) == up) {
        return Ok(b.code.clone());
    }
    // satu huruf: E/M/A/B → kode yang diawali huruf itu (kalau hanya satu)
    if compact.chars().count() == 1 {
        let hits: Vec<&Band> = bands.iter().filter(|b| b.code.to_uppercase().starts_with(&compact)).collect();
        if hits.len() == 1 {
            return Ok(hits[0].code.clone());
        }
    }
    Err(raw)
}

// ---------------------------------------------------------------- penebakan kolom

fn is_understanding(h: &str) -> bool {
    ["UNDERSTANDING", "PEMAHAMAN", "KONSEP", "CONCEPT"].iter().any(|k| h.contains(k))
}
fn is_inquiry(h: &str) -> bool {
    ["INQUIRY", "RESEARCH", "INKUIRI", "PENELITIAN", "RISET", "PENYELIDIKAN"].iter().any(|k| h.contains(k))
}
fn is_narrative(h: &str) -> bool {
    ["NARASI", "NARRATIVE", "FEEDBACK", "CATATAN", "DESKRIPSI", "KOMENTAR", "COMMENT", "UMPAN BALIK"]
        .iter()
        .any(|k| h.contains(k))
}
fn is_class(h: &str) -> bool {
    matches!(h, "KELAS" | "KLS" | "ROMBEL" | "ROMBEL SAAT INI" | "CLASS" | "ROMBONGAN BELAJAR")
}
fn is_number_col(h: &str) -> bool {
    matches!(h, "NO" | "NO URUT" | "NOMOR" | "NOMOR URUT" | "ABSEN" | "NO ABSEN" | "NIS" | "NISN" | "NIPD")
        || h.contains("NISN")
        || h.contains("INDUK")
}

pub fn guess_mapping(grid: &Grid, bands: &[Band]) -> Option<ProjectMapping> {
    let (hr, name_col) = (0..grid.len().min(30)).find_map(|r| {
        (0..grid[r].len()).find(|&c| is_name_header(&header(cell(grid, r, c)))).map(|c| (r, c))
    })?;
    // judul dua baris (sel digabung): baris bawah kosong di kolom Nama tapi berisi teks di kolom lain
    let sub = has_sub_header(grid, hr, name_col);
    let width = grid.iter().skip(hr).take(80).map(|r| r.len()).max().unwrap_or(0);
    let col_header = |c: usize| {
        let a = header(cell(grid, hr, c));
        let b = if sub { header(cell(grid, hr + 1, c)) } else { String::new() };
        [a, b].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ")
    };
    let first_data = if sub { hr + 2 } else { hr + 1 };
    let data_rows: Vec<usize> = (first_data..grid.len())
        .filter(|&r| {
            let n = cell_text(cell(grid, r, name_col));
            !n.is_empty() && !is_footer_row(&n)
        })
        .take(200)
        .collect();

    let mut m = ProjectMapping { header_row: first_data - 1, name: Some(name_col), ..Default::default() };
    for c in (0..width).filter(|&c| c != name_col) {
        let h = col_header(c);
        if is_class(&h) {
            m.class.get_or_insert(c);
        } else if is_understanding(&h) {
            m.understanding.get_or_insert(c);
        } else if is_inquiry(&h) {
            m.inquiry.get_or_insert(c);
        } else if is_narrative(&h) {
            m.narrative.get_or_insert(c);
        }
    }

    // tanpa judul yang jelas: dua kolom pertama sesudah Nama yang isinya predikat/angka
    if m.understanding.is_none() || m.inquiry.is_none() {
        let used = |m: &ProjectMapping, c: usize| [m.name, m.class, m.understanding, m.inquiry, m.narrative].contains(&Some(c));
        let gradeish: Vec<usize> = (name_col + 1..width)
            .filter(|&c| !used(&m, c) && !is_number_col(&col_header(c)))
            .filter(|&c| {
                let vals: Vec<&calamine::Data> =
                    data_rows.iter().map(|&r| cell(grid, r, c)).filter(|d| !cell_text(d).is_empty()).collect();
                vals.len() >= 2 && vals.iter().filter(|d| normalize_grade(d, bands).is_ok()).count() * 10 >= vals.len() * 8
            })
            .collect();
        let mut it = gradeish.into_iter();
        if m.understanding.is_none() {
            m.understanding = it.next();
        }
        if m.inquiry.is_none() {
            m.inquiry = it.next();
        }
    }
    Some(m)
}

// ---------------------------------------------------------------- pembacaan

pub fn parse_rows(grid: &Grid, m: &ProjectMapping, bands: &[Band]) -> Result<(Vec<ProjectRow>, Vec<String>), String> {
    let name_col = m.name.ok_or("Pilih kolom Nama dulu")?;
    if m.understanding.is_none() && m.inquiry.is_none() && m.narrative.is_none() {
        return Err("Pilih minimal satu kolom: Understanding, Inquiry, atau Narasi".into());
    }
    let mut rows = Vec::new();
    let mut invalid = Vec::new();
    for r in m.header_row + 1..grid.len() {
        let name = cell_text(cell(grid, r, name_col)).split_whitespace().collect::<Vec<_>>().join(" ");
        if name.is_empty() || is_footer_row(&name) || is_name_header(&normalize_name(&name)) {
            continue;
        }
        let mut grade = |col: Option<usize>, what: &str| -> String {
            let Some(c) = col else { return String::new() };
            match normalize_grade(cell(grid, r, c), bands) {
                Ok(v) => v,
                Err(raw) => {
                    invalid.push(format!("Baris {} ({name}), {what}: \"{raw}\"", r + 1));
                    String::new()
                }
            }
        };
        let understanding = grade(m.understanding, "Understanding");
        let inquiry = grade(m.inquiry, "Inquiry");
        let narrative = m.narrative.map(|c| cell_text(cell(grid, r, c))).unwrap_or_default();
        let class_name = m
            .class
            .map(|c| cell_text(cell(grid, r, c)))
            .filter(|t| !t.is_empty())
            .map(|t| tidy_class(&t))
            .unwrap_or_default();
        rows.push(ProjectRow { row: r + 1, name, class_name, understanding, inquiry, narrative });
    }
    if rows.is_empty() {
        return Err("Tidak ada baris siswa di bawah judul kolom".into());
    }
    Ok((rows, invalid))
}

/// Proyek yang disebut di nama sheet / judul di atas tabel / nama file.
fn guess_project(data: &AppData, grid: &Grid, m: &ProjectMapping, sheet: &str, file_stem: &str) -> Option<String> {
    let mut texts: Vec<String> = vec![sheet.to_string(), file_stem.to_string()];
    texts.extend((0..m.header_row.min(grid.len())).flat_map(|r| grid[r].iter().map(cell_text)));
    let hay: Vec<String> = texts.iter().map(|t| normalize_name(t)).filter(|t| !t.is_empty()).collect();
    let mentions = |needle: &str| {
        let n = normalize_name(needle);
        !n.is_empty() && hay.iter().any(|t| format!(" {t} ").contains(&format!(" {n} ")))
    };
    // tema lebih khas daripada nama ("Who We Are" vs "Project 1")
    data.settings
        .projects
        .iter()
        .find(|p| mentions(&p.theme))
        .or_else(|| data.settings.projects.iter().find(|p| mentions(&p.name)))
        .map(|p| p.id.clone())
}

pub fn inspect(data: &AppData, path: &Path, sheet: &str, mapping: Option<ProjectMapping>) -> Result<ProjectPreview, String> {
    let grid = read_grid(path, sheet)?;
    let bands = &data.settings.grading.bands;
    let m = match mapping {
        Some(m) => m,
        None => guess_mapping(&grid, bands).ok_or(
            "Kolom Nama tidak ditemukan di sheet ini. Pastikan ada judul kolom \"Nama\" / \"Nama Siswa\".",
        )?,
    };
    let hr = m.header_row.min(grid.len().saturating_sub(1));
    let width = grid.iter().skip(hr.saturating_sub(1)).take(80).map(|r| r.len()).max().unwrap_or(0);
    let columns = (0..width)
        .map(|c| {
            let titles: Vec<String> =
                (hr.saturating_sub(1)..=hr).map(|r| cell_text(cell(&grid, r, c))).filter(|t| !t.is_empty()).collect();
            let samples = (hr + 1..grid.len())
                .map(|r| cell_text(cell(&grid, r, c)))
                .filter(|t| !t.is_empty())
                .take(3)
                .map(|t| t.chars().take(24).collect())
                .collect();
            ColumnInfo { index: c, letter: col_letter(c), header: titles.join(" / "), samples }
        })
        .collect();
    let (rows, invalid) = parse_rows(&grid, &m, bands)?;
    let mut warnings = Vec::new();
    if !invalid.is_empty() {
        warnings.push(format!(
            "{} isian tidak terbaca sebagai predikat ({}) atau angka 0–100 dan akan dilewati, mis. {}",
            invalid.len(),
            bands.iter().map(|b| b.code.as_str()).collect::<Vec<_>>().join("/"),
            invalid[0]
        ));
    }
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    Ok(ProjectPreview {
        columns,
        guessed_project: guess_project(data, &grid, &m, sheet, &stem),
        mapping: m,
        total: rows.len(),
        rows: rows.into_iter().take(8).collect(),
        warnings,
    })
}

// ---------------------------------------------------------------- penerapan

/// Masukkan nilai proyek ke siswa di `classes`. Dengan `apply = false` hanya menghitung
/// laporan (pratinjau) tanpa mengubah data.
pub fn apply_rows(
    data: &mut AppData,
    rows: Vec<ProjectRow>,
    invalid: Vec<String>,
    project_id: &str,
    classes: &[String],
) -> Result<ProjectImportReport, String> {
    if !data.settings.projects.iter().any(|p| p.id == project_id) {
        return Err("Proyek tidak ditemukan di Pengaturan".into());
    }
    if classes.is_empty() {
        return Err("Pilih minimal satu kelas".into());
    }
    let mut report = ProjectImportReport { rows_read: rows.len(), invalid, ..Default::default() };

    // kelompokkan: baris dengan kolom kelas dicocokkan di kelas itu saja, sisanya di semua kelas terpilih
    let mut groups: BTreeMap<String, Vec<ProjectRow>> = BTreeMap::new();
    let known = known_classes(data);
    for mut row in rows {
        row.class_name = resolve_class(&known, &row.class_name);
        let key = if row.class_name.is_empty() || !classes.contains(&row.class_name) {
            if !row.class_name.is_empty() {
                report.unmatched.push(Unmatched {
                    class_name: row.class_name.clone(),
                    name: row.name,
                    reason: "Kelasnya tidak dipilih / tidak ada di data siswa".into(),
                });
                continue;
            }
            String::new()
        } else {
            row.class_name.clone()
        };
        groups.entry(key).or_default().push(row);
    }

    let label = |k: &str| match k {
        "u" => "Understanding",
        "i" => "Inquiry",
        _ => "Narasi",
    };
    for (class_key, rows) in groups {
        let idx: Vec<usize> = (0..data.students.len())
            .filter(|&i| {
                let c = &data.students[i].class_name;
                if class_key.is_empty() { classes.contains(c) } else { *c == class_key }
            })
            .collect();
        let cand: Vec<String> = idx.iter().map(|&i| data.students[i].name.clone()).collect();
        let queries: Vec<String> = rows.iter().map(|r| r.name.clone()).collect();
        for (row, m) in rows.into_iter().zip(match_names(&queries, &cand)) {
            let Some((ci, kind)) = m else {
                report.unmatched.push(Unmatched {
                    class_name: if class_key.is_empty() { classes.join(", ") } else { class_key.clone() },
                    name: row.name,
                    reason: "Nama tidak ditemukan di data siswa".into(),
                });
                continue;
            };
            let s = &mut data.students[idx[ci]];
            report.matched += 1;
            if kind != MatchKind::Exact {
                report.approximate.push(ApproxMatch {
                    class_name: s.class_name.clone(),
                    from_name: row.name.clone(),
                    to_name: s.name.clone(),
                    kind,
                });
            }
            let has_values = !(row.understanding.is_empty() && row.inquiry.is_empty() && row.narrative.is_empty());
            if !has_values {
                continue;
            }
            let pos = match s.projects.iter().position(|p| p.project_id == project_id) {
                Some(p) => p,
                None => {
                    if s.projects.iter().any(|p| !p.project_id.is_empty()) {
                        report.other_project.push(s.name.clone());
                    }
                    s.projects.push(ProjectEntry { project_id: project_id.to_string(), ..Default::default() });
                    report.new_entries += 1;
                    s.projects.len() - 1
                }
            };
            let name = s.name.clone();
            let entry = &mut s.projects[pos];
            for (key, new, slot) in [
                ("u", row.understanding, &mut entry.understanding),
                ("i", row.inquiry, &mut entry.inquiry),
                ("n", row.narrative, &mut entry.narrative),
            ] {
                if new.is_empty() || *slot == new {
                    continue;
                }
                if !slot.trim().is_empty() {
                    let short = |t: &str| if t.chars().count() > 40 { format!("{}…", t.chars().take(40).collect::<String>()) } else { t.to_string() };
                    report.overwritten.push(format!("{name}: {} {} → {}", label(key), short(slot), short(&new)));
                }
                *slot = new;
                report.changed_cells += 1;
            }
        }
    }
    Ok(report)
}

pub fn import(
    data: &mut AppData,
    path: &Path,
    sheet: &str,
    mapping: &ProjectMapping,
    project_id: &str,
    classes: &[String],
) -> Result<ProjectImportReport, String> {
    let grid = read_grid(path, sheet)?;
    let (rows, invalid) = parse_rows(&grid, mapping, &data.settings.grading.bands)?;
    apply_rows(data, rows, invalid, project_id, classes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Student;
    use calamine::Data;

    fn s(v: &str) -> Data {
        Data::String(v.into())
    }

    fn data() -> AppData {
        let mut d = AppData::default();
        let mk = |id: &str, name: &str, class: &str| Student { id: id.into(), name: name.into(), class_name: class.into(), ..Default::default() };
        d.students = vec![mk("1", "I Made Ayu Lestari", "IX-1"), mk("2", "Putu Bagus Pratama", "IX-1"), mk("3", "Kadek Citra", "IX-2")];
        d.students[1].projects.push(ProjectEntry { project_id: "p1".into(), understanding: "ME".into(), ..Default::default() });
        d.students[2].projects.push(ProjectEntry { project_id: "p2".into(), ..Default::default() });
        d
    }

    #[test]
    fn grades_from_teacher_files() {
        let b = crate::model::default_bands();
        assert_eq!(normalize_grade(&s(" ee "), &b).unwrap(), "EE");
        assert_eq!(normalize_grade(&s("m"), &b).unwrap(), "ME");
        assert_eq!(normalize_grade(&s("Meeting Expectations"), &b).unwrap(), "ME");
        assert_eq!(normalize_grade(&Data::Float(92.0), &b).unwrap(), "92");
        assert_eq!(normalize_grade(&s("88,5"), &b).unwrap(), "88.5");
        assert_eq!(normalize_grade(&s(""), &b).unwrap(), "");
        assert!(normalize_grade(&s("baik"), &b).is_err());
        assert!(normalize_grade(&Data::Float(120.0), &b).is_err());
    }

    #[test]
    fn guesses_columns_from_headers_and_from_content() {
        let b = crate::model::default_bands();
        // judul jelas
        let g: Grid = vec![
            vec![s("NILAI PROJECT 1 - WHO WE ARE")],
            vec![s("No"), s("Nama Siswa"), s("Kelas"), s("Understanding Level"), s("Inquiry & Research"), s("Narasi Guru")],
            vec![Data::Float(1.0), s("Ayu"), s("IX-1"), s("EE"), s("ME"), s("Great work.")],
            vec![Data::Float(2.0), s("Bagus"), s("IX-1"), s("AE"), s("AE"), s("")],
        ];
        let m = guess_mapping(&g, &b).unwrap();
        assert_eq!((m.header_row, m.name, m.class, m.understanding, m.inquiry, m.narrative), (1, Some(1), Some(2), Some(3), Some(4), Some(5)));
        // judul tidak jelas ("Nilai 1", "Nilai 2"): ditebak dari isinya, kolom No/NIS dilewati
        let g: Grid = vec![
            vec![s("No"), s("NIS"), s("Nama"), s("Nilai 1"), s("Nilai 2")],
            vec![Data::Float(1.0), Data::Float(2301.0), s("Ayu"), Data::Float(95.0), s("ME")],
            vec![Data::Float(2.0), Data::Float(2302.0), s("Bagus"), Data::Float(88.0), s("AE")],
        ];
        let m = guess_mapping(&g, &b).unwrap();
        assert_eq!((m.understanding, m.inquiry), (Some(3), Some(4)));
    }

    #[test]
    fn import_matches_names_keeps_old_values_on_empty_cells_and_reports_overwrites() {
        let mut d = data();
        let row = |n: &str, c: &str, u: &str, i: &str, t: &str| ProjectRow {
            row: 2, name: n.into(), class_name: c.into(), understanding: u.into(), inquiry: i.into(), narrative: t.into(),
        };
        let rows = vec![
            row("I MADE AYU L.", "", "EE", "ME", "Great."), // singkatan → I Made Ayu Lestari
            row("Putu Bagus Pratama", "", "", "AE", ""), // Understanding kosong: ME lama tetap
            row("Kadek Citra", "", "ME", "", ""),       // kelas IX-2 tidak dipilih
            row("Siapa Ini", "", "EE", "", ""),
        ];
        let r = apply_rows(&mut d, rows, vec![], "p1", &["IX-1".into()]).unwrap();
        assert_eq!((r.matched, r.new_entries, r.changed_cells), (2, 1, 4));
        assert_eq!(r.unmatched.len(), 2);
        assert_eq!(r.approximate.len(), 1);
        let ayu = &d.students[0].projects[0];
        assert_eq!((ayu.understanding.as_str(), ayu.inquiry.as_str(), ayu.narrative.as_str()), ("EE", "ME", "Great."));
        let bagus = &d.students[1].projects[0];
        assert_eq!((bagus.understanding.as_str(), bagus.inquiry.as_str()), ("ME", "AE"));
        assert!(d.students[2].projects.iter().all(|p| p.project_id == "p2"));

        // impor ulang dengan nilai lain: tertimpa dan dilaporkan
        let r = apply_rows(&mut d, vec![row("I Made Ayu Lestari", "IX-1", "ME", "", "")], vec![], "p1", &["IX-1".into()]).unwrap();
        assert_eq!(r.overwritten, ["I Made Ayu Lestari: Understanding EE → ME"]);
        assert_eq!(r.new_entries, 0);
    }

    #[test]
    fn student_with_another_project_is_flagged() {
        let mut d = data();
        let rows = vec![ProjectRow { row: 2, name: "Kadek Citra".into(), class_name: "IX-2".into(), understanding: "ME".into(), inquiry: String::new(), narrative: String::new() }];
        let r = apply_rows(&mut d, rows, vec![], "p1", &["IX-1".into(), "IX-2".into()]).unwrap();
        assert_eq!(r.other_project, ["Kadek Citra"]);
        assert!(apply_rows(&mut d, vec![], vec![], "nope", &["IX-1".into()]).is_err());
    }
}
