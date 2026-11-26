//! Impor nilai mapel dari file Excel buatan guru yang susunannya bebas.
//!
//! Format rekap sekolah (baris mapel + baris PH/PTS) sudah dibaca `importer::parse_scores`.
//! Modul ini untuk file lain: judul satu baris ("IPA PH"), satu sheet per mapel
//! ("NH 1, NH 2, Rata-rata NH, PTS"), angka berkoma, kelas di judul/nama sheet, dsb.
//! Kolom ditebak dari judul DAN isinya, lalu bisa dikoreksi di layar. Siswa dicocokkan
//! lewat NISN/NIS kalau ada, lalu nama (persis → singkatan → dipendekkan → mirip).
//! Sel kosong TIDAK menghapus nilai lama.

use crate::grading::{fmt_num, parse_number};
use crate::importer::{cell, cell_text, count_classes, format_nisn, header, read_grid, ApproxMatch, ClassCount, Grid, Unmatched};
use crate::model::{canonical_key, AppData, ScoreEntry};
use crate::names::{match_names, normalize_name, MatchKind};
use crate::student_flex::{
    canonical_class, class_from_text, col_letter, has_sub_header, is_footer_row, is_name_header, known_classes, resolve_class,
    tidy_class, ColumnInfo,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

/// Satu kolom nilai: mapel + bagian ("ph", "pts", atau "final" = nilai akhir langsung).
/// Beberapa kolom dengan mapel & bagian yang sama dirata-rata (mis. NH 1, NH 2, NH 3).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScoreColumn {
    pub col: usize,
    pub subject: String,
    pub part: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScoreMapping {
    /// Baris judul kolom (0-based); data dibaca mulai baris sesudahnya.
    pub header_row: usize,
    pub name: Option<usize>,
    pub nis: Option<usize>,
    pub nisn: Option<usize>,
    pub class: Option<usize>,
    /// Kelas untuk baris tanpa kolom/judul kelas. Kosong = dicocokkan ke kelas yang dipilih.
    pub default_class: String,
    pub scores: Vec<ScoreColumn>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubjectChoice {
    /// Kunci penyimpanan (judul kolom sumber di Pengaturan → Mapel, atau judul dari file).
    pub key: String,
    pub name: String,
    /// Ada di Pengaturan → Mapel (tercetak di rapor).
    pub on_report: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PreviewRow {
    pub row: usize,
    pub name: String,
    pub class_name: String,
    /// Per mapel+bagian (urut `ScorePreview::parts`), sudah dirata-rata; "" = kosong.
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScorePreview {
    pub columns: Vec<ColumnInfo>,
    pub mapping: ScoreMapping,
    pub subjects: Vec<SubjectChoice>,
    /// Judul kolom pratinjau, mis. "IPA · PH".
    pub parts: Vec<String>,
    /// Asal tebakan kelas: "kolom kelas", "judul kelas di tabel", "judul tabel", "nama sheet", "nama file", "".
    pub class_source: String,
    pub total: usize,
    pub rows: Vec<PreviewRow>,
    pub classes: Vec<ClassCount>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct FlexScoreReport {
    pub rows_read: usize,
    pub matched: usize,
    /// Nilai yang berubah/baru terisi.
    pub values_set: usize,
    /// Nilai yang sama dengan yang sudah tersimpan.
    pub unchanged: usize,
    /// Nilai lama yang tertimpa, mis. "Ayu: IPA PH 80 → 85".
    pub overwritten: Vec<String>,
    pub approximate: Vec<ApproxMatch>,
    pub unmatched: Vec<Unmatched>,
    /// Isian yang bukan angka 0–100, mis. "Baris 7 (Ayu), IPA PH: \"A\"".
    pub invalid: Vec<String>,
    pub subjects: Vec<String>,
}

// ---------------------------------------------------------------- kata kunci judul

const PART_WORDS: [(&str, &str); 24] = [
    ("SUMATIF TENGAH SEMESTER", "pts"),
    ("PENILAIAN TENGAH SEMESTER", "pts"),
    ("TENGAH SEMESTER", "pts"),
    ("NILAI HARIAN", "ph"),
    ("PENILAIAN HARIAN", "ph"),
    ("ULANGAN HARIAN", "ph"),
    ("NILAI AKHIR", "final"),
    ("NILAI RAPOR", "final"),
    ("NILAI RAPORT", "final"),
    ("MIDTERM", "pts"),
    ("HARIAN", "ph"),
    ("FORMATIF", "ph"),
    ("PSTS", "pts"),
    ("PTS", "pts"),
    ("STS", "pts"),
    ("UTS", "pts"),
    ("MID", "pts"),
    ("PH", "ph"),
    ("NH", "ph"),
    ("UH", "ph"),
    ("AKHIR", "final"),
    ("FINAL", "final"),
    ("NA", "final"),
    ("NR", "final"),
];
const NOISE: [&str; 9] = ["NILAI", "RATA", "RERATA", "RATA2", "SKOR", "SCORE", "MAPEL", "MATA", "PELAJARAN"];

#[derive(Debug, PartialEq)]
struct HeaderPart {
    part: &'static str,
    /// Kolom per ulangan ("NH 1", "PH2"), bukan rata-rata.
    single: bool,
    /// Kolom rata-rata ("Rata-rata NH").
    average: bool,
    /// Sisa judul sesudah kata bagian dibuang = nama mapel (bisa kosong).
    subject: String,
}

/// "IPA PH" → (ph, "IPA"); "NH 1" → (ph, single); "Rata-rata NH" → (ph, average); "B INDO PTS" → (pts, "B INDO").
fn read_header(h: &str) -> Option<HeaderPart> {
    let words: Vec<String> = h.split_whitespace().map(str::to_string).collect();
    // "PH1" / "NH2" → "PH 1"
    let words: Vec<String> = words
        .into_iter()
        .flat_map(|w| {
            let split = w.find(|c: char| c.is_ascii_digit()).filter(|&i| i > 0 && w[..i].chars().all(|c| c.is_ascii_alphabetic()));
            match split {
                Some(i) => vec![w[..i].to_string(), w[i..].to_string()],
                None => vec![w],
            }
        })
        .collect();
    let text = format!(" {} ", words.join(" "));
    let (phrase, part) = PART_WORDS.iter().find(|(p, _)| text.contains(&format!(" {p} ")))?;
    let rest = text.replacen(&format!(" {phrase} "), " ", 1);
    let rest_words: Vec<&str> = rest.split_whitespace().collect();
    let single = rest_words.iter().any(|w| w.chars().all(|c| c.is_ascii_digit()) && w.len() <= 2);
    let average = words.iter().any(|w| w.starts_with("RATA") || w == "RERATA" || w == "AVERAGE" || w == "AVG");
    let subject = rest_words
        .iter()
        .filter(|w| !NOISE.contains(w) && !(w.chars().all(|c| c.is_ascii_digit()) && w.len() <= 2))
        .copied()
        .collect::<Vec<_>>()
        .join(" ");
    Some(HeaderPart { part, single: single && !average, average, subject })
}

fn is_class_header(h: &str) -> bool {
    matches!(h, "KELAS" | "KLS" | "ROMBEL" | "ROMBEL SAAT INI" | "CLASS" | "ROMBONGAN BELAJAR")
}

fn is_summary(h: &str) -> bool {
    let first = h.split_whitespace().next().unwrap_or("");
    matches!(first, "JUMLAH" | "TOTAL" | "RANKING" | "RANK" | "PERINGKAT" | "KET" | "KETERANGAN" | "DESKRIPSI" | "PREDIKAT" | "CATATAN")
        || matches!(h, "RATA RATA" | "RATA2" | "RERATA" | "AVERAGE")
}

/// Mapel di Pengaturan yang disebut di teks (nama sheet, judul tabel, nama file).
fn subject_in_text(data: &AppData, texts: &[String]) -> Option<String> {
    let hay: Vec<String> = texts.iter().map(|t| format!(" {} ", normalize_name(t))).collect();
    let hay_compact: Vec<String> = texts.iter().map(|t| canonical_key(t)).collect();
    data.settings.subjects.iter().find_map(|s| {
        let key = s.source_key.trim();
        let hit = |needle: &str| {
            let n = normalize_name(needle);
            !n.is_empty() && (hay.iter().any(|h| h.contains(&format!(" {n} "))) || hay_compact.iter().any(|h| *h == canonical_key(needle)))
        };
        (hit(key) || hit(&s.name)).then(|| key.to_string())
    })
}

/// Nama mapel yang setara (Indonesia / singkatan / Inggris), untuk mencocokkan nama mapel
/// yang ditulis guru di template dengan nama/sumber nilai di Pengaturan.
const SUBJECT_ALIASES: &[&[&str]] = &[
    &["MATEMATIKA", "MAT", "MTK", "MATHEMATICS", "MATH", "MATHS"],
    &["IPA", "SCIENCE", "ILMU PENGETAHUAN ALAM", "SAINS"],
    &["IPS", "SOCIAL STUDIES", "ILMU PENGETAHUAN SOSIAL"],
    &["BAHASA INDONESIA", "B INDO", "B INDONESIA", "BINDO", "INDONESIAN", "BAHASA INDO"],
    &["BAHASA INGGRIS", "B ING", "B INGGRIS", "BING", "ENGLISH"],
    &["BAHASA BALI", "B BALI", "BBALI"],
    &["PJOK", "PENJAS", "PENJASKES", "PENJASORKES", "PHYSICAL EDUCATION"],
    &["PENDIDIKAN PANCASILA", "PANCASILA", "PKN", "PPKN"],
    &["AGAMA", "PENDIDIKAN AGAMA"],
    &["SENI BUDAYA", "SENI", "SBDP", "ART", "ARTS"],
    &["INFORMATIKA", "TIK", "ICT", "COMPUTER SCIENCE"],
];

/// Nama mapel yang diketik guru (mis. "Matematika") → kunci mapel di Pengaturan (mis. "MAT").
fn subject_by_alias(data: &AppData, typed: &str) -> Option<String> {
    let t = format!(" {} ", normalize_name(typed));
    let group = SUBJECT_ALIASES.iter().find(|g| g.iter().any(|a| t.contains(&format!(" {a} "))))?;
    data.settings.subjects.iter().find_map(|s| {
        let names = [format!(" {} ", normalize_name(&s.source_key)), format!(" {} ", normalize_name(&s.name))];
        group.iter().any(|a| names.iter().any(|n| n.contains(&format!(" {a} ")))).then(|| s.source_key.clone())
    })
}

/// Judul dari file → kunci mapel di Pengaturan kalau setara ("B.Indo" = "B. INDO"), selain itu apa adanya.
fn subject_key(data: &AppData, label: &str) -> String {
    let k = canonical_key(label);
    data.settings
        .subjects
        .iter()
        .find(|s| canonical_key(&s.source_key) == k || canonical_key(&s.name) == k)
        .map(|s| s.source_key.clone())
        .or_else(|| data.score_columns.iter().find(|c| canonical_key(c) == k).cloned())
        .unwrap_or_else(|| label.trim().to_string())
}

fn is_score_value(d: &calamine::Data) -> bool {
    match d {
        calamine::Data::Float(f) => (0.0..=100.0).contains(f),
        calamine::Data::Int(i) => (0..=100).contains(i),
        calamine::Data::String(s) => parse_number(s).is_some_and(|v| (0.0..=100.0).contains(&v)),
        _ => false,
    }
}

// ---------------------------------------------------------------- penebakan

pub fn guess_mapping(data: &AppData, grid: &Grid, sheet: &str, file_stem: &str) -> Option<(ScoreMapping, String)> {
    let (hr, name_col) = (0..grid.len().min(30)).find_map(|r| {
        (0..grid[r].len()).find(|&c| is_name_header(&header(cell(grid, r, c)))).map(|c| (r, c))
    })?;
    let sub = has_sub_header(grid, hr, name_col);
    let first_data = if sub { hr + 2 } else { hr + 1 };
    let width = grid.iter().skip(hr).take(80).map(|r| r.len()).max().unwrap_or(0);
    // judul atas yang digabung (mis. "IPA" di atas PH & PTS) berlaku ke kanan sampai judul berikutnya
    let mut top_carry = String::new();
    let headers: Vec<String> = (0..width)
        .map(|c| {
            let top = header(cell(grid, hr, c));
            if !top.is_empty() {
                top_carry = top.clone();
            }
            if !sub {
                return top;
            }
            let bottom = header(cell(grid, hr + 1, c));
            let top = if top.is_empty() && c > name_col && !bottom.is_empty() { top_carry.clone() } else { top };
            [top, bottom].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ")
        })
        .collect();
    let data_rows: Vec<usize> = (first_data..grid.len())
        .filter(|&r| {
            let n = cell_text(cell(grid, r, name_col));
            !n.is_empty() && !is_footer_row(&n)
        })
        .take(200)
        .collect();

    // teks di luar tabel untuk menebak mapel & kelas
    let mut around: Vec<String> = vec![sheet.to_string(), file_stem.to_string()];
    around.extend((0..hr).flat_map(|r| grid[r].iter().map(cell_text)).filter(|t| !t.is_empty()));
    // isian "Mata pelajaran: …" di atas tabel (template baku): dipakai apa adanya kalau tidak ada di Pengaturan
    let typed_subject = (0..hr).find_map(|r| {
        let row = &grid[r];
        let at = (0..row.len()).find(|&c| matches!(header(&row[c]).as_str(), "MATA PELAJARAN" | "MAPEL" | "MATA PELAJARAN MAPEL" | "SUBJECT"))?;
        (at + 1..row.len()).map(|c| cell_text(&row[c])).find(|t| !t.is_empty())
    });
    let sheet_subject = subject_in_text(data, &around)
        .or_else(|| typed_subject.as_deref().and_then(|t| subject_by_alias(data, t)))
        .or(typed_subject)
        .or_else(|| {
        let generic = normalize_name(sheet);
        let looks_generic = generic.starts_with("SHEET") || generic.starts_with("LEMBAR") || generic.contains("NILAI") || generic.contains("REKAP");
        (!looks_generic && canonical_class(sheet).is_none() && sheet.chars().count() <= 24).then(|| sheet.trim().to_string())
    });

    let mut m = ScoreMapping { header_row: first_data - 1, name: Some(name_col), ..Default::default() };
    let mut found: Vec<(ScoreColumn, HeaderPart)> = Vec::new();
    for c in (0..width).filter(|&c| c != name_col) {
        let h = &headers[c];
        if h.is_empty() {
            continue;
        }
        if h.contains("NISN") {
            m.nisn.get_or_insert(c);
            continue;
        }
        if matches!(h.as_str(), "NIS" | "NIPD" | "NO INDUK" | "NOMOR INDUK" | "NIS LOKAL" | "STUDENT ID") || h.contains("INDUK") {
            m.nis.get_or_insert(c);
            continue;
        }
        if is_class_header(h) {
            m.class.get_or_insert(c);
            continue;
        }
        if is_summary(h) {
            continue;
        }
        let Some(hp) = read_header(h) else { continue };
        // isinya harus angka 0–100 (atau kosong)
        let vals: Vec<&calamine::Data> = data_rows.iter().map(|&r| cell(grid, r, c)).filter(|d| !cell_text(d).is_empty()).collect();
        let numeric = vals.iter().filter(|d| is_score_value(d) || cell_text(d) == "-").count();
        if !vals.is_empty() && numeric * 10 < vals.len() * 7 {
            continue;
        }
        let label = if hp.subject.is_empty() { sheet_subject.clone().unwrap_or_default() } else { hp.subject.clone() };
        let subject = if label.is_empty() { String::new() } else { subject_key(data, &label) };
        found.push((ScoreColumn { col: c, subject, part: hp.part.to_string() }, hp));
    }
    // per mapel: kolom rata-rata menggantikan kolom per ulangan; nilai akhir hanya dipakai
    // kalau tidak ada PH/PTS (supaya rumus bobot di Pengaturan tetap berlaku)
    let keep: Vec<ScoreColumn> = found
        .iter()
        .filter(|(sc, hp)| {
            let same = |p: &str| found.iter().filter(|(o, _)| o.subject == sc.subject && o.part == p).collect::<Vec<_>>();
            if hp.single && same(&sc.part).iter().any(|(_, o)| !o.single) {
                return false;
            }
            if sc.part == "final" && (!same("ph").is_empty() || !same("pts").is_empty()) {
                return false;
            }
            true
        })
        .map(|(sc, _)| sc.clone())
        .collect();
    m.scores = keep;

    // kelas: kolom → baris pemisah kelas di tabel → judul di atas tabel → nama sheet → nama file
    let mut source = String::new();
    let has_sections = data_rows.is_empty()
        || (first_data..grid.len()).any(|r| {
            cell_text(cell(grid, r, name_col)).is_empty() && class_from_text(&cell_text(cell(grid, r, 0))).or_else(|| canonical_class(&cell_text(cell(grid, r, 0))).filter(|c| c.contains('-'))).is_some()
        });
    if m.class.is_some() {
        source = "kolom kelas".into();
    } else if has_sections && !data_rows.is_empty() {
        source = "judul kelas di tabel".into();
    } else if let Some(c) = (0..hr).flat_map(|r| grid[r].iter()).find_map(|d| class_from_text(&cell_text(d))) {
        m.default_class = c;
        source = "judul tabel".into();
    } else if let Some(c) = canonical_class(sheet).filter(|_| sheet.trim().chars().count() <= 8).or_else(|| class_from_text(sheet)) {
        m.default_class = c;
        source = "nama sheet".into();
    } else if let Some(c) = class_from_text(file_stem).or_else(|| canonical_class(file_stem)) {
        m.default_class = c;
        source = "nama file".into();
    }
    if !m.default_class.is_empty() {
        m.default_class = resolve_class(&known_classes(data), &m.default_class);
    }
    Some((m, source))
}

// ---------------------------------------------------------------- pembacaan

struct ParsedRow {
    row: usize,
    name: String,
    nis: String,
    nisn: String,
    class_name: String,
    /// (mapel, bagian) → nilai (sudah dirata-rata kalau beberapa kolom).
    values: Vec<((String, String), Option<f64>)>,
}

/// Mapel+bagian unik dari pilihan kolom, urut kemunculan.
fn groups(m: &ScoreMapping) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for sc in m.scores.iter().filter(|s| !s.subject.trim().is_empty()) {
        let k = (sc.subject.trim().to_string(), sc.part.clone());
        if !out.contains(&k) {
            out.push(k);
        }
    }
    out
}

fn part_label(p: &str) -> &'static str {
    match p {
        "ph" => "PH",
        "pts" => "PTS",
        _ => "Nilai akhir",
    }
}

fn parse_rows(grid: &Grid, m: &ScoreMapping) -> Result<(Vec<ParsedRow>, Vec<String>), String> {
    let name_col = m.name.ok_or("Pilih kolom Nama dulu")?;
    if m.scores.iter().any(|s| s.subject.trim().is_empty()) {
        return Err("Ada kolom nilai yang belum dipilih mapelnya".into());
    }
    let keys = groups(m);
    if keys.is_empty() {
        return Err("Pilih minimal satu kolom nilai (mapel · PH / PTS / Nilai akhir)".into());
    }
    let mut section: Option<String> = None;
    let mut rows = Vec::new();
    let mut invalid = Vec::new();
    for r in m.header_row + 1..grid.len() {
        let name = cell_text(cell(grid, r, name_col)).split_whitespace().collect::<Vec<_>>().join(" ");
        if name.is_empty() {
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
        let mut values = Vec::new();
        for k in &keys {
            let mut got: Vec<f64> = Vec::new();
            for sc in m.scores.iter().filter(|s| s.subject.trim() == k.0 && s.part == k.1) {
                let d = cell(grid, r, sc.col);
                let t = cell_text(d);
                if t.is_empty() || t == "-" || t == "–" {
                    continue;
                }
                let v = match d {
                    calamine::Data::Float(f) => Some(*f),
                    calamine::Data::Int(i) => Some(*i as f64),
                    _ => parse_number(&t),
                };
                match v {
                    Some(v) if (0.0..=100.0).contains(&v) => got.push(v),
                    _ => invalid.push(format!("Baris {} ({name}), {} {}: \"{t}\"", r + 1, k.0, part_label(&k.1))),
                }
            }
            let avg = (!got.is_empty()).then(|| {
                let a = got.iter().sum::<f64>() / got.len() as f64;
                (a * 100.0).round() / 100.0
            });
            values.push((k.clone(), avg));
        }
        let class_name = m
            .class
            .map(|c| cell_text(cell(grid, r, c)))
            .filter(|t| !t.is_empty())
            .map(|t| tidy_class(&t))
            .or_else(|| section.clone())
            .unwrap_or_else(|| m.default_class.trim().to_string());
        rows.push(ParsedRow {
            row: r + 1,
            name,
            nis: m.nis.map(|c| cell_text(cell(grid, r, c))).unwrap_or_default(),
            nisn: m.nisn.map(|c| format_nisn(cell(grid, r, c))).unwrap_or_default(),
            class_name,
            values,
        });
    }
    if rows.is_empty() {
        return Err("Tidak ada baris siswa di bawah judul kolom".into());
    }
    Ok((rows, invalid))
}

fn subject_choices(data: &AppData, m: &ScoreMapping) -> Vec<SubjectChoice> {
    let mut out: Vec<SubjectChoice> = data
        .settings
        .subjects
        .iter()
        .filter(|s| !s.source_key.trim().is_empty())
        .map(|s| SubjectChoice { key: s.source_key.clone(), name: s.name.clone(), on_report: true })
        .collect();
    let extra = data.score_columns.iter().cloned().chain(m.scores.iter().map(|s| s.subject.clone()));
    for k in extra {
        if !k.trim().is_empty() && !out.iter().any(|c| canonical_key(&c.key) == canonical_key(&k)) {
            out.push(SubjectChoice { name: k.clone(), key: k, on_report: false });
        }
    }
    out
}

pub fn inspect(data: &AppData, path: &Path, sheet: &str, mapping: Option<ScoreMapping>) -> Result<ScorePreview, String> {
    let grid = read_grid(path, sheet)?;
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let (m, class_source) = match mapping {
        Some(m) => (m, String::new()),
        None => guess_mapping(data, &grid, sheet, &stem)
            .ok_or("Kolom Nama tidak ditemukan di sheet ini. Pastikan ada judul kolom \"Nama\" / \"Nama Siswa\".")?,
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
    let subjects = subject_choices(data, &m);
    let keys = groups(&m);
    let name_of = |k: &str| subjects.iter().find(|s| s.key == k).map(|s| s.name.clone()).unwrap_or_else(|| k.to_string());
    let parts = keys.iter().map(|(s, p)| format!("{} · {}", name_of(s), part_label(p))).collect();
    let mut warnings = Vec::new();
    let (rows, invalid) = match parse_rows(&grid, &m) {
        Ok(x) => x,
        Err(e) => {
            return Ok(ScorePreview { columns, mapping: m, subjects, parts, class_source, total: 0, rows: vec![], classes: vec![], warnings: vec![e] })
        }
    };
    if !invalid.is_empty() {
        warnings.push(format!("{} isian bukan angka 0–100 dan akan dilewati, mis. {}", invalid.len(), invalid[0]));
    }
    let off_report: Vec<String> =
        keys.iter().map(|(s, _)| s.clone()).filter(|s| !subjects.iter().any(|c| c.key == *s && c.on_report)).collect::<HashSet<_>>().into_iter().collect();
    if !off_report.is_empty() {
        warnings.push(format!(
            "Mapel {} belum ada di Pengaturan → Mapel, jadi nilainya disimpan tapi belum tercetak di rapor.",
            off_report.join(", ")
        ));
    }
    let known = known_classes(data);
    Ok(ScorePreview {
        columns,
        subjects,
        parts,
        class_source,
        total: rows.len(),
        classes: count_classes(rows.iter().map(|r| resolve_class(&known, &r.class_name))),
        rows: rows
            .iter()
            .take(8)
            .map(|r| PreviewRow {
                row: r.row,
                name: r.name.clone(),
                class_name: resolve_class(&known, &r.class_name),
                values: r.values.iter().map(|(_, v)| v.map(fmt_num).unwrap_or_default()).collect(),
            })
            .collect(),
        mapping: m,
        warnings,
    })
}

// ---------------------------------------------------------------- penerapan

fn apply_rows(data: &mut AppData, rows: Vec<ParsedRow>, invalid: Vec<String>, classes: &[String]) -> Result<FlexScoreReport, String> {
    if classes.is_empty() {
        return Err("Pilih minimal satu kelas".into());
    }
    let known = known_classes(data);
    let chosen: Vec<String> = classes.iter().map(|c| resolve_class(&known, c)).collect();
    let mut report = FlexScoreReport { rows_read: rows.len(), invalid, ..Default::default() };

    let mut by_class: BTreeMap<String, Vec<ParsedRow>> = BTreeMap::new();
    for mut row in rows {
        row.class_name = resolve_class(&known, &row.class_name);
        if !row.class_name.is_empty() && !chosen.contains(&row.class_name) {
            report.unmatched.push(Unmatched {
                class_name: row.class_name.clone(),
                name: row.name,
                reason: "Kelasnya tidak dipilih / tidak ada di data siswa".into(),
            });
            continue;
        }
        by_class.entry(row.class_name.clone()).or_default().push(row);
    }

    for (class_key, rows) in by_class {
        let idx: Vec<usize> = (0..data.students.len())
            .filter(|&i| {
                let c = &data.students[i].class_name;
                if class_key.is_empty() { chosen.contains(c) } else { *c == class_key }
            })
            .collect();
        // NISN / NIS dulu, sisanya lewat nama
        let mut resolved: Vec<Option<(usize, MatchKind)>> = rows
            .iter()
            .map(|r| {
                idx.iter()
                    .position(|&i| {
                        let s = &data.students[i];
                        (!r.nisn.is_empty() && s.nisn == r.nisn) || (!r.nis.is_empty() && s.nis == r.nis)
                    })
                    .map(|p| (p, MatchKind::Exact))
            })
            .collect();
        let taken: HashSet<usize> = resolved.iter().flatten().map(|(p, _)| *p).collect();
        let free: Vec<usize> = (0..idx.len()).filter(|p| !taken.contains(p)).collect();
        let pending: Vec<usize> = (0..rows.len()).filter(|&i| resolved[i].is_none()).collect();
        let queries: Vec<String> = pending.iter().map(|&i| rows[i].name.clone()).collect();
        let cand: Vec<String> = free.iter().map(|&p| data.students[idx[p]].name.clone()).collect();
        for (qi, m) in pending.iter().zip(match_names(&queries, &cand)) {
            resolved[*qi] = m.map(|(ci, kind)| (free[ci], kind));
        }

        for (row, m) in rows.into_iter().zip(resolved) {
            let Some((p, kind)) = m else {
                // namanya persis ada di kelas lain? sebutkan, supaya mudah dibetulkan
                let norm = normalize_name(&row.name);
                let elsewhere: Vec<String> = data
                    .students
                    .iter()
                    .filter(|s| normalize_name(&s.name) == norm && !idx.iter().any(|&i| data.students[i].id == s.id))
                    .map(|s| s.class_name.clone())
                    .collect();
                report.unmatched.push(Unmatched {
                    class_name: if class_key.is_empty() { chosen.join(", ") } else { class_key.clone() },
                    name: row.name,
                    reason: if !elsewhere.is_empty() {
                        format!("Di data siswa ada di kelas {} — centang kelas itu, atau periksa kelas di file", elsewhere.join(", "))
                    } else if idx.is_empty() {
                        "Kelas ini belum ada di data siswa".into()
                    } else {
                        "Nama tidak ditemukan di data siswa".into()
                    },
                });
                continue;
            };
            let s = &mut data.students[idx[p]];
            report.matched += 1;
            if kind != MatchKind::Exact {
                report.approximate.push(ApproxMatch {
                    class_name: s.class_name.clone(),
                    from_name: row.name.clone(),
                    to_name: s.name.clone(),
                    kind,
                });
            }
            let name = s.name.clone();
            for ((subject, part), v) in row.values {
                let Some(v) = v else { continue };
                let entry: &mut ScoreEntry = s.scores.entry(canonical_key(&subject)).or_default();
                let slot = match part.as_str() {
                    "ph" => &mut entry.ph,
                    "pts" => &mut entry.pts,
                    _ => &mut entry.manual,
                };
                match *slot {
                    Some(old) if (old - v).abs() < 1e-9 => report.unchanged += 1,
                    old => {
                        if let Some(old) = old {
                            report.overwritten.push(format!("{name}: {subject} {} {} → {}", part_label(&part), fmt_num(old), fmt_num(v)));
                        }
                        *slot = Some(v);
                        report.values_set += 1;
                    }
                }
                if !report.subjects.contains(&subject) {
                    report.subjects.push(subject.clone());
                }
            }
        }
    }
    for sub in &report.subjects {
        if !data.score_columns.iter().any(|c| canonical_key(c) == canonical_key(sub)) {
            data.score_columns.push(sub.clone());
        }
    }
    Ok(report)
}

pub fn import(data: &mut AppData, path: &Path, sheet: &str, mapping: &ScoreMapping, classes: &[String]) -> Result<FlexScoreReport, String> {
    let grid = read_grid(path, sheet)?;
    let (rows, invalid) = parse_rows(&grid, mapping)?;
    apply_rows(data, rows, invalid, classes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Student;
    use calamine::Data;

    fn s(v: &str) -> Data {
        Data::String(v.into())
    }
    fn n(v: f64) -> Data {
        Data::Float(v)
    }

    fn data() -> AppData {
        let mut d = AppData::default();
        let mk = |id: &str, name: &str, class: &str, nisn: &str| Student {
            id: id.into(),
            name: name.into(),
            class_name: class.into(),
            nisn: nisn.into(),
            ..Default::default()
        };
        d.students = vec![
            mk("1", "Made Dewi Lestari", "IX-1", "0091234561"),
            mk("2", "I Gede Arya Pratama", "IX-1", ""),
            mk("3", "Kadek Citra", "IX-2", ""),
        ];
        d
    }

    #[test]
    fn header_words() {
        let hp = read_header("IPA PH").unwrap();
        assert_eq!((hp.part, hp.subject.as_str(), hp.single), ("ph", "IPA", false));
        let hp = read_header("B INDO PTS").unwrap();
        assert_eq!((hp.part, hp.subject.as_str()), ("pts", "B INDO"));
        let hp = read_header("NH1").unwrap();
        assert_eq!((hp.part, hp.single, hp.subject.as_str()), ("ph", true, ""));
        let hp = read_header("RATA RATA NH").unwrap();
        assert_eq!((hp.part, hp.average, hp.subject.as_str()), ("ph", true, ""));
        assert_eq!(read_header("NILAI AKHIR").unwrap().part, "final");
        assert_eq!(read_header("MATEMATIKA SUMATIF TENGAH SEMESTER").unwrap().subject, "MATEMATIKA");
        assert!(read_header("NAMA AYAH").is_none());
    }

    #[test]
    fn one_header_row_with_subject_and_part() {
        let d = data();
        let g: Grid = vec![
            vec![s("No"), s("Nama"), s("IPA PH"), s("IPA PTS"), s("B.Indo PH"), s("B.Indo PTS"), s("Jumlah")],
            vec![n(1.0), s("Made Dewi Lestari"), n(88.0), n(90.0), s("85,5"), n(86.0), n(349.5)],
            vec![n(2.0), s("I Gede Arya Pratama"), n(92.0), s("-"), n(91.0), n(90.0), n(273.0)],
        ];
        let (m, src) = guess_mapping(&d, &g, "IX-1", "nilai").unwrap();
        assert_eq!(m.scores.len(), 4, "{m:?}");
        assert_eq!(m.scores[2], ScoreColumn { col: 4, subject: "B. INDO".into(), part: "ph".into() }, "judul setara dengan mapel Pengaturan");
        assert_eq!((m.default_class.as_str(), src.as_str()), ("IX-1", "nama sheet"));
        let (rows, invalid) = parse_rows(&g, &m).unwrap();
        assert!(invalid.is_empty());
        assert_eq!(rows[0].values[2].1, Some(85.5));
        assert_eq!(rows[1].values[1].1, None, "\"-\" = kosong");
    }

    #[test]
    fn sheet_per_subject_with_several_daily_scores() {
        let d = data();
        let g: Grid = vec![
            vec![s("DAFTAR NILAI KELAS IX-1")],
            vec![s("No"), s("Nama Siswa"), s("NH 1"), s("NH 2"), s("NH 3"), s("PTS"), s("Nilai Akhir")],
            vec![n(1.0), s("Made Dewi Lestari"), n(80.0), n(90.0), Data::Empty, n(88.0), n(86.0)],
        ];
        let (m, src) = guess_mapping(&d, &g, "IPA", "").unwrap();
        // NH 1..3 dirata-rata, nilai akhir tidak dipakai karena ada PH/PTS
        assert_eq!(m.scores.iter().filter(|c| c.part == "ph").count(), 3);
        assert!(m.scores.iter().all(|c| c.subject == "IPA" && c.part != "final"), "{m:?}");
        assert_eq!((m.default_class.as_str(), src.as_str()), ("IX-1", "judul tabel"));
        let (rows, _) = parse_rows(&g, &m).unwrap();
        assert_eq!(rows[0].values, vec![(("IPA".into(), "ph".into()), Some(85.0)), (("IPA".into(), "pts".into()), Some(88.0))]);

        // ada kolom rata-rata → kolom per ulangan diabaikan
        let g: Grid = vec![
            vec![s("No"), s("Nama"), s("NH1"), s("NH2"), s("Rata-rata NH"), s("STS")],
            vec![n(1.0), s("Made Dewi Lestari"), n(80.0), n(90.0), n(85.0), n(88.0)],
        ];
        let (m, _) = guess_mapping(&d, &g, "Matematika", "").unwrap();
        assert_eq!(m.scores.iter().map(|c| c.col).collect::<Vec<_>>(), vec![4, 5]);
        assert_eq!(m.scores[0].subject, "Matematika", "mapel di luar Pengaturan diambil dari nama sheet");
    }

    #[test]
    fn school_recap_layout_and_class_sections() {
        let d = data();
        let g: Grid = vec![
            vec![s("NO"), s("NAMA SISWA"), s("IPA"), Data::Empty],
            vec![Data::Empty, Data::Empty, s("PH"), s("PTS")],
            vec![s("IX-1")],
            vec![n(1.0), s("Made Dewi Lestari"), n(88.0), n(90.0)],
            vec![s("IX-2")],
            vec![n(1.0), s("Kadek Citra"), n(70.0), n(75.0)],
        ];
        let (m, src) = guess_mapping(&d, &g, "Rekap", "").unwrap();
        assert_eq!(m.scores, vec![
            ScoreColumn { col: 2, subject: "IPA".into(), part: "ph".into() },
            ScoreColumn { col: 3, subject: "IPA".into(), part: "pts".into() },
        ]);
        assert_eq!(src, "judul kelas di tabel");
        let (rows, _) = parse_rows(&g, &m).unwrap();
        assert_eq!(rows.iter().map(|r| r.class_name.as_str()).collect::<Vec<_>>(), ["IX-1", "IX-2"]);
    }

    #[test]
    fn apply_matches_by_nisn_and_name_reports_overwrites_and_keeps_empty() {
        let mut d = data();
        d.students[1].scores.insert("IPA".into(), ScoreEntry { ph: Some(70.0), pts: Some(75.0), manual: None });
        let m = ScoreMapping {
            header_row: 0,
            name: Some(0),
            nisn: Some(1),
            class: Some(2),
            scores: vec![ScoreColumn { col: 3, subject: "IPA".into(), part: "ph".into() }, ScoreColumn { col: 4, subject: "IPA".into(), part: "pts".into() }],
            ..Default::default()
        };
        let g: Grid = vec![
            vec![s("Nama"), s("NISN"), s("Kelas"), s("PH"), s("PTS")],
            vec![s("Dewi"), n(91234561.0), s("9.1"), n(88.0), n(90.0)], // NISN tanpa nol depan, kelas "9.1"
            vec![s("I GEDE ARYA P."), Data::Empty, s("IX-1"), n(80.0), Data::Empty],
            vec![s("Kadek Citra"), Data::Empty, s("IX-2"), n(99.0), Data::Empty],
        ];
        let (rows, invalid) = parse_rows(&g, &m).unwrap();
        let r = apply_rows(&mut d, rows, invalid, &["IX-1".into()]).unwrap();
        assert_eq!(r.matched, 2);
        assert_eq!(r.values_set, 3);
        assert_eq!(r.overwritten, ["I Gede Arya Pratama: IPA PH 70 → 80"]);
        assert_eq!(r.unmatched.len(), 1, "IX-2 tidak dipilih");
        assert_eq!(d.students[0].scores["IPA"].ph, Some(88.0));
        assert_eq!(d.students[1].scores["IPA"].pts, Some(75.0), "sel kosong tidak menghapus");
        assert_eq!(d.students[2].scores.get("IPA"), None);
    }

    #[test]
    fn rows_without_class_are_matched_in_chosen_classes() {
        let mut d = data();
        let m = ScoreMapping {
            header_row: 0,
            name: Some(0),
            scores: vec![ScoreColumn { col: 1, subject: "IPA".into(), part: "final".into() }],
            ..Default::default()
        };
        let g: Grid = vec![vec![s("Nama"), s("NA")], vec![s("Kadek Citra"), n(91.0)]];
        let (rows, invalid) = parse_rows(&g, &m).unwrap();
        let r = apply_rows(&mut d, rows, invalid, &["IX-1".into(), "IX-2".into()]).unwrap();
        assert_eq!(r.matched, 1);
        assert_eq!(d.students[2].scores["IPA"].manual, Some(91.0));
    }
}
