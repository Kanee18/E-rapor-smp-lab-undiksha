//! Form Excel untuk fasilitator / wali kelas.
//!
//! Alur: operator membuat form dari aplikasi → fasil mengisi di Excel →
//! operator mengimpor kembali. Siswa dicocokkan lewat kolom ID tersembunyi,
//! jadi urutan baris boleh diubah dan nama tidak perlu ditebak.
//!
//! Isi form:
//! - sheet "Isian": baris judul + tabel siswa (kolom Kelas & Nama dikunci)
//! - sheet "Petunjuk": cara mengisi + arti predikat
//! - sheet tersembunyi "_eraport": penanda form, versi, id proyek, dan kunci tiap kolom

use crate::grading::{resolve_input, Status};
use crate::importer::{cell_text, ensure_class_configs, new_student_id, normalize_class};
use crate::model::{
    canonical_key, class_sort_key, format_attendance_sia, AppData, ClassConfig, GradingConfig, ProjectConfig,
    ProjectEntry, Student,
};
use crate::names::normalize_name;
use calamine::{open_workbook_auto, Data, Reader};
use rust_xlsxwriter::{
    Color, DataValidation, DataValidationErrorStyle, Format, FormatAlign, FormatBorder, Formula, ProtectionOptions,
    Workbook, Worksheet,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

const MARKER: &str = "eraport-form";
const VERSION: u32 = 1;
const META_SHEET: &str = "_eraport";
const DATA_SHEET: &str = "Isian";
/// Baris judul kolom di sheet Isian (0-based); data mulai di baris berikutnya.
const HEADER_ROW: u32 = 3;
/// Baris di sheet _eraport yang berisi kunci tiap kolom.
const KEYS_ROW: u32 = 4;

const NAVY: u32 = 0x1A365D;
const LOCKED_BG: u32 = 0xF1EEE8;
const RULE: u32 = 0xCDC7BB;

/// Permintaan dari tampilan: siswa mana dan kolom apa saja.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct FormRequest {
    pub classes: Vec<String>,
    pub include_atl: bool,
    /// Kalau diisi, kolom proyek (Understanding, Inquiry, narasi) ikut.
    pub project_id: Option<String>,
    /// Hanya siswa yang sudah diberi proyek ini.
    pub only_project_members: bool,
    pub include_attendance: bool,
    /// Total hari efektif sekolah untuk rumus kehadiran (S/I/A → % Present).
    pub effective_days: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
enum Key {
    Id,
    Class,
    Name,
    Atl(String),
    Understanding,
    Inquiry,
    Narrative,
    Sick,
    Permit,
    Alpha,
    Attendance,
}

impl Key {
    fn as_str(&self) -> String {
        match self {
            Key::Id => "id".into(),
            Key::Class => "class".into(),
            Key::Name => "name".into(),
            Key::Atl(id) => format!("atl:{id}"),
            Key::Understanding => "uoi:understanding".into(),
            Key::Inquiry => "uoi:inquiry".into(),
            Key::Narrative => "uoi:narrative".into(),
            Key::Sick => "attendance:s".into(),
            Key::Permit => "attendance:i".into(),
            Key::Alpha => "attendance:a".into(),
            Key::Attendance => "attendance".into(),
        }
    }

    fn parse(s: &str) -> Option<Key> {
        Some(match s.trim() {
            "id" => Key::Id,
            "class" => Key::Class,
            "name" => Key::Name,
            "uoi:understanding" => Key::Understanding,
            "uoi:inquiry" => Key::Inquiry,
            "uoi:narrative" => Key::Narrative,
            "attendance:s" => Key::Sick,
            "attendance:i" => Key::Permit,
            "attendance:a" => Key::Alpha,
            "attendance" => Key::Attendance,
            other => Key::Atl(other.strip_prefix("atl:")?.to_string()),
        })
    }
}

fn col_letter(mut c: usize) -> String {
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

fn project_label(data: &AppData, project_id: &str) -> Option<String> {
    data.settings.projects.iter().find(|p| p.id == project_id).map(|p| {
        if p.theme.trim().is_empty() {
            p.name.clone()
        } else {
            format!("{} — {}", p.name, p.theme.trim())
        }
    })
}

/// Siswa yang masuk form, urut kelas lalu nama.
pub fn selected_students<'a>(data: &'a AppData, req: &FormRequest) -> Vec<&'a Student> {
    let classes: Vec<String> = req.classes.iter().map(|c| normalize_class(c)).collect();
    let mut list: Vec<&Student> = data
        .students
        .iter()
        .filter(|s| classes.contains(&s.class_name))
        .filter(|s| match (&req.project_id, req.only_project_members) {
            (Some(pid), true) => s.projects.iter().any(|p| &p.project_id == pid),
            _ => true,
        })
        .collect();
    list.sort_by(|a, b| {
        class_sort_key(&a.class_name)
            .cmp(&class_sort_key(&b.class_name))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    list
}

/// Nama file usulan, mis. "Form Project 1 - IX-1, IX-2.xlsx".
pub fn suggested_file_name(data: &AppData, req: &FormRequest) -> String {
    let what = match &req.project_id {
        Some(pid) => data
            .settings
            .projects
            .iter()
            .find(|p| &p.id == pid)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "Proyek".into()),
        None if req.include_atl => "ATL".into(),
        None => "Kehadiran".into(),
    };
    let mut classes: Vec<String> = req.classes.iter().map(|c| normalize_class(c)).collect();
    classes.sort_by_key(|c| class_sort_key(c));
    let name = format!("Form {what} - {}.xlsx", classes.join(", "));
    name.chars().map(|c| if r#"\/:*?"<>|"#.contains(c) { '-' } else { c }).collect()
}

fn xerr(e: rust_xlsxwriter::XlsxError) -> String {
    format!("Gagal membuat file Excel: {e}")
}

/// Buat workbook form. Isian yang sudah ada ikut ditulis supaya bisa dilanjutkan.
pub fn build_form(data: &AppData, req: &FormRequest) -> Result<(Workbook, usize), String> {
    let st = &data.settings;
    let project_id = req.project_id.clone().filter(|p| !p.trim().is_empty());
    if let Some(pid) = &project_id {
        if project_label(data, pid).is_none() {
            return Err("Proyek yang dipilih tidak ada di Pengaturan".into());
        }
    }
    if !req.include_atl && project_id.is_none() && !req.include_attendance {
        return Err("Pilih minimal satu isian: ATL, proyek, atau kehadiran".into());
    }
    let students = selected_students(data, req);
    if students.is_empty() {
        return Err("Tidak ada siswa untuk kelas/proyek yang dipilih".into());
    }

    // kolom
    let mut cols: Vec<(Key, String, f64)> = vec![
        (Key::Id, "ID".into(), 8.0),
        (Key::Class, "Kelas".into(), 8.0),
        (Key::Name, "Nama siswa".into(), 34.0),
    ];
    if req.include_atl {
        for a in &st.atl {
            cols.push((Key::Atl(a.id.clone()), a.name.clone(), 13.0));
        }
    }
    if project_id.is_some() {
        cols.push((Key::Understanding, "Understanding Level".into(), 14.0));
        cols.push((Key::Inquiry, "Inquiry & Research Skills".into(), 14.0));
        cols.push((Key::Narrative, "Teacher Narrative Feedback (English)".into(), 64.0));
    }
    if req.include_attendance {
        cols.push((Key::Sick, "Sakit (S)".into(), 10.0));
        cols.push((Key::Permit, "Izin (I)".into(), 10.0));
        cols.push((Key::Alpha, "Alpa (A)".into(), 10.0));
        cols.push((Key::Attendance, "Kehadiran".into(), 18.0));
    }
    let eff_days = req.effective_days.filter(|&d| d > 0).unwrap_or_else(|| st.effective_days());
    let s_col_idx = cols.iter().position(|(k, _, _)| *k == Key::Sick);
    let a_col_idx = cols.iter().position(|(k, _, _)| *k == Key::Alpha);

    let codes: Vec<String> = {
        let mut b = st.grading.bands.clone();
        b.sort_by(|x, y| y.min.partial_cmp(&x.min).unwrap_or(std::cmp::Ordering::Equal));
        b.into_iter().map(|b| b.code).collect()
    };

    // format
    let title = Format::new().set_bold().set_font_size(14).set_font_color(Color::RGB(NAVY));
    let muted = Format::new().set_font_color(Color::RGB(0x5E6672));
    let header = Format::new()
        .set_bold()
        .set_text_wrap()
        .set_font_color(Color::White)
        .set_background_color(Color::RGB(NAVY))
        .set_align(FormatAlign::VerticalCenter)
        .set_border(FormatBorder::Thin)
        .set_border_color(Color::RGB(NAVY));
    let locked = Format::new()
        .set_background_color(Color::RGB(LOCKED_BG))
        .set_border(FormatBorder::Thin)
        .set_border_color(Color::RGB(RULE))
        .set_align(FormatAlign::Top);
    let input = Format::new()
        .set_unlocked()
        .set_border(FormatBorder::Thin)
        .set_border_color(Color::RGB(RULE))
        .set_align(FormatAlign::Top);
    let input_center = input.clone().set_align(FormatAlign::Center);
    let input_wrap = input.clone().set_text_wrap();

    let mut sheet = Worksheet::new();
    sheet.set_name(DATA_SHEET).map_err(xerr)?;
    let heading = match &project_id {
        Some(pid) => format!("Form isian rapor — {}", project_label(data, pid).unwrap_or_default()),
        None => "Form isian rapor".to_string(),
    };
    let mut classes: Vec<String> = students.iter().map(|s| s.class_name.clone()).collect();
    classes.dedup();
    sheet.write_string_with_format(0, 1, &heading, &title).map_err(xerr)?;
    sheet
        .write_string_with_format(
            1,
            1,
            format!("{} · Kelas {} · {} siswa · tahun ajaran {}", st.school_name, classes.join(", "), students.len(), st.academic_year),
            &muted,
        )
        .map_err(xerr)?;
    sheet
        .write_string_with_format(
            2,
            1,
            format!(
                "Isi kolom putih. Penilaian: {} atau angka 0–100. Kolom abu-abu jangan diubah. Simpan, lalu kirim kembali file ini.",
                codes.join(" / ")
            ),
            &muted,
        )
        .map_err(xerr)?;

    for (c, (_, label, width)) in cols.iter().enumerate() {
        let c = c as u16;
        sheet.set_column_width(c, *width).map_err(xerr)?;
        sheet.write_string_with_format(HEADER_ROW, c, label, &header).map_err(xerr)?;
    }
    sheet.set_row_height(HEADER_ROW, 32).map_err(xerr)?;
    sheet.set_column_hidden(0).map_err(xerr)?;

    let project_entry = |s: &Student| -> Option<ProjectEntry> {
        project_id.as_ref().and_then(|pid| s.projects.iter().find(|p| &p.project_id == pid).cloned())
    };
    let has_narrative = cols.iter().any(|(k, _, _)| *k == Key::Narrative);
    for (i, s) in students.iter().enumerate() {
        let r = HEADER_ROW + 1 + i as u32;
        let pe = project_entry(s).unwrap_or_default();
        for (c, (key, _, _)) in cols.iter().enumerate() {
            let c_u16 = c as u16;
            match key {
                Key::Sick | Key::Permit | Key::Alpha => {
                    let days = match key {
                        Key::Sick => s.attendance_s,
                        Key::Permit => s.attendance_i,
                        _ => s.attendance_a,
                    };
                    match days {
                        Some(d) => sheet.write_number_with_format(r, c_u16, d as f64, &input_center).map(|_| ()),
                        None => sheet.write_string_with_format(r, c_u16, "", &input_center).map(|_| ()),
                    }
                    .map_err(xerr)?;
                    continue;
                }
                Key::Attendance => {
                    let has_sia = s.attendance_s.is_some() || s.attendance_i.is_some() || s.attendance_a.is_some();
                    if !has_sia && !s.attendance.trim().is_empty() {
                        sheet.write_string_with_format(r, c_u16, &s.attendance, &input).map_err(xerr)?;
                    } else if let (Some(sc), Some(ac)) = (s_col_idx, a_col_idx) {
                        let r_excel = r + 1;
                        let range = format!("{}{r_excel}:{}{r_excel}", col_letter(sc), col_letter(ac));
                        let f = format!(
                            "=IF(COUNT({range})=0,\"\",TEXT(ROUND(MAX(0,{eff_days}-SUM({range}))/{eff_days},2),\"0%\")&\" (Present)\")"
                        );
                        let cached = format_attendance_sia(s.attendance_s, s.attendance_i, s.attendance_a, eff_days)
                            .unwrap_or_else(|| s.attendance.clone());
                        sheet
                            .write_formula_with_format(r, c_u16, Formula::new(f).set_result(cached), &input)
                            .map_err(xerr)?;
                    } else {
                        sheet.write_string_with_format(r, c_u16, &s.attendance, &input).map_err(xerr)?;
                    }
                    continue;
                }
                _ => {}
            }
            let (value, fmt, is_grade) = match key {
                Key::Id => (s.id.clone(), &locked, false),
                Key::Class => (s.class_name.clone(), &locked, false),
                Key::Name => (s.name.clone(), &locked, false),
                Key::Atl(aid) => (s.atl.get(aid).map(|e| e.grade.clone()).unwrap_or_default(), &input_center, true),
                Key::Understanding => (pe.understanding.clone(), &input_center, true),
                Key::Inquiry => (pe.inquiry.clone(), &input_center, true),
                Key::Narrative => (pe.narrative.clone(), &input_wrap, false),
                Key::Sick | Key::Permit | Key::Alpha | Key::Attendance => unreachable!(),
            };
            // Penilaian: kode ditulis huruf besar, angka ditulis sebagai angka
            // (bukan teks, supaya Excel tidak menandainya "number stored as text").
            match is_grade.then(|| crate::grading::parse_number(&value)).flatten() {
                Some(n) => sheet.write_number_with_format(r, c_u16, n, fmt).map(|_| ()),
                None => {
                    let v = if is_grade { normalize_grade(&value, &st.grading) } else { value };
                    sheet.write_string_with_format(r, c_u16, &v, fmt).map(|_| ())
                }
            }
            .map_err(xerr)?;
        }
        if has_narrative {
            sheet.set_row_height(r, 48).map_err(xerr)?;
        }
    }

    // pilihan predikat; angka tetap boleh (peringatan saja, bukan penolakan)
    let last_row = HEADER_ROW + students.len() as u32;
    let validation = DataValidation::new()
        .allow_list_strings(&codes)
        .map_err(xerr)?
        .set_error_style(DataValidationErrorStyle::Warning)
        .set_error_title("Isian penilaian")
        .map_err(xerr)?
        .set_error_message(format!("Isi {} atau angka 0–100. Klik Yes kalau isiannya angka.", codes.join(" / ")))
        .map_err(xerr)?;
    for (c, (key, _, _)) in cols.iter().enumerate() {
        if matches!(key, Key::Atl(_) | Key::Understanding | Key::Inquiry) {
            sheet
                .add_data_validation(HEADER_ROW + 1, c as u16, last_row, c as u16, &validation)
                .map_err(xerr)?;
        }
    }
    sheet.set_freeze_panes(HEADER_ROW + 1, 3).map_err(xerr)?;
    let mut options = ProtectionOptions::new();
    options.format_columns = true;
    options.format_rows = true;
    sheet.protect_with_options(&options);

    // petunjuk
    let mut guide = Worksheet::new();
    guide.set_name("Petunjuk").map_err(xerr)?;
    guide.set_column_width(0, 4).map_err(xerr)?;
    guide.set_column_width(1, 100).map_err(xerr)?;
    let wrap = Format::new().set_text_wrap().set_align(FormatAlign::Top);
    let mut lines: Vec<(String, Option<&Format>)> = vec![
        ("Cara mengisi form".into(), Some(&title)),
        ("1. Buka sheet \"Isian\". Isi hanya kolom putih; kolom abu-abu (kelas & nama) terkunci.".into(), None),
        (format!("2. Penilaian diisi dengan kode {} (pilih dari daftar) atau angka 0–100. Angka otomatis dikonversi ke predikat.", codes.join(" / ")), None),
        ("3. Narasi ditulis dalam bahasa Inggris, 2–4 kalimat: apa yang sudah baik dan apa langkah berikutnya.".into(), None),
        ("4. Kolom yang dibiarkan kosong tidak menghapus isian yang sudah ada di aplikasi.".into(), None),
        ("5. Jangan menghapus sheet atau mengganti nama file berlebihan; urutan baris boleh diubah.".into(), None),
        ("6. Simpan sebagai .xlsx lalu kirim kembali ke operator rapor.".into(), None),
        (String::new(), None),
        ("Arti predikat".into(), Some(&title)),
    ];
    let mut bands = st.grading.bands.clone();
    bands.sort_by(|x, y| y.min.partial_cmp(&x.min).unwrap_or(std::cmp::Ordering::Equal));
    for b in &bands {
        lines.push((
            format!("{} = {} ({}–{})", b.code, b.label, crate::grading::fmt_num(b.min), crate::grading::fmt_num(b.max)),
            None,
        ));
    }
    if req.include_atl {
        lines.push((String::new(), None));
        lines.push(("Indikator ATL".into(), Some(&title)));
        for a in &st.atl {
            lines.push((format!("{}: {}", a.name, a.indicator), None));
        }
    }
    for (i, (text, fmt)) in lines.iter().enumerate() {
        guide
            .write_string_with_format(i as u32, 1, text, fmt.unwrap_or(&wrap))
            .map_err(xerr)?;
    }

    // penanda tersembunyi
    let mut meta = Worksheet::new();
    meta.set_name(META_SHEET).map_err(xerr)?;
    meta.write_string(0, 0, MARKER).map_err(xerr)?;
    meta.write_number(0, 1, VERSION as f64).map_err(xerr)?;
    meta.write_string(1, 0, "project_id").map_err(xerr)?;
    meta.write_string(1, 1, project_id.clone().unwrap_or_default()).map_err(xerr)?;
    meta.write_string(2, 0, "header_row").map_err(xerr)?;
    meta.write_number(2, 1, HEADER_ROW as f64).map_err(xerr)?;
    meta.write_string(3, 0, "effective_days").map_err(xerr)?;
    meta.write_number(3, 1, eff_days as f64).map_err(xerr)?;
    for (c, (key, _, _)) in cols.iter().enumerate() {
        meta.write_string(KEYS_ROW, c as u16, key.as_str()).map_err(xerr)?;
    }
    // Simpan juga konfigurasi proyek, kelas, dan snapshot data lengkap tiap siswa
    // supaya saat file form ini dibuka ("Impor form") di aplikasi E-Raport guru lain,
    // daftar siswa beserta nilai dan isian yang sudah dibuat guru pertama ikut terbawa.
    if let Ok(proj_json) = serde_json::to_string(&st.projects) {
        meta.write_string(5, 0, "projects_config").map_err(xerr)?;
        meta.write_string(5, 1, &proj_json).map_err(xerr)?;
    }
    if let Ok(sc_json) = serde_json::to_string(&data.score_columns) {
        meta.write_string(6, 0, "score_columns").map_err(xerr)?;
        meta.write_string(6, 1, &sc_json).map_err(xerr)?;
    }
    let classes_slim: Vec<ClassConfig> = st
        .classes
        .iter()
        .map(|c| ClassConfig {
            name: c.name.clone(),
            homeroom_teacher: c.homeroom_teacher.clone(),
            homeroom_signature: String::new(),
        })
        .collect();
    if let Ok(cls_json) = serde_json::to_string(&classes_slim) {
        meta.write_string(7, 0, "classes_config").map_err(xerr)?;
        meta.write_string(7, 1, &cls_json).map_err(xerr)?;
    }
    for (i, s) in students.iter().enumerate() {
        if let Ok(s_json) = serde_json::to_string(s) {
            let row = 8 + i as u32;
            meta.write_string(row, 0, "student").map_err(xerr)?;
            meta.write_string(row, 1, &s.id).map_err(xerr)?;
            meta.write_string(row, 2, &s_json).map_err(xerr)?;
        }
    }
    meta.set_hidden(true);

    let mut wb = Workbook::new();
    wb.push_worksheet(sheet);
    wb.push_worksheet(guide);
    wb.push_worksheet(meta);
    Ok((wb, students.len()))
}

pub fn write_form(data: &AppData, req: &FormRequest, path: &Path) -> Result<usize, String> {
    let (mut wb, n) = build_form(data, req)?;
    wb.save(path)
        .map_err(|e| format!("File tidak bisa disimpan (mungkin sedang dibuka di Excel?): {e}"))?;
    Ok(n)
}

// ---------------------------------------------------------------- impor

#[derive(Debug, Clone, Serialize, Default)]
pub struct FormImportReport {
    pub applied: bool,
    pub project: Option<String>,
    /// Baris yang cocok dengan siswa (termasuk siswa baru yang ditambahkan).
    pub students: usize,
    /// Siswa baru yang belum ada di aplikasi ini dan otomatis ditambahkan dari form.
    pub students_added: usize,
    /// Isian yang dimasukkan (termasuk yang sama dengan sebelumnya).
    pub values_set: usize,
    /// Isian yang berbeda dengan yang sudah ada di aplikasi.
    pub changed: usize,
    /// Isian lama yang tertimpa nilai berbeda (untuk dicek operator).
    pub overwritten: Vec<String>,
    pub invalid: Vec<String>,
    pub unknown_rows: Vec<String>,
    pub notes: Vec<String>,
    pub projects_added: usize,
}

fn grid(range: calamine::Range<Data>) -> Vec<Vec<Data>> {
    let (r0, c0) = range.start().unwrap_or((0, 0));
    let mut g: Vec<Vec<Data>> = vec![Vec::new(); r0 as usize];
    for row in range.rows() {
        let mut v = vec![Data::Empty; c0 as usize];
        v.extend(row.iter().cloned());
        g.push(v);
    }
    g
}

fn at(g: &[Vec<Data>], r: usize, c: usize) -> String {
    g.get(r).and_then(|row| row.get(c)).map(cell_text).unwrap_or_default()
}

/// Kode predikat ditulis huruf besar; angka dibiarkan (koma → titik).
fn normalize_grade(raw: &str, grading: &GradingConfig) -> String {
    let t = raw.trim();
    match grading.bands.iter().find(|b| b.code.eq_ignore_ascii_case(t)) {
        Some(b) => b.code.clone(),
        None => t.replace(',', "."),
    }
}

/// Baca form yang sudah diisi dan masukkan ke `data`.
/// Untuk pratinjau, panggil dengan salinan data lalu buang hasilnya.
pub fn import_form(data: &mut AppData, path: &Path) -> Result<FormImportReport, String> {
    let mut wb = open_workbook_auto(path).map_err(|e| format!("File tidak bisa dibuka: {e}"))?;
    let meta = wb
        .worksheet_range(META_SHEET)
        .map(grid)
        .map_err(|_| "Ini bukan form dari E-Raport (penanda form tidak ditemukan). Buat form lewat tombol \"Buat form Excel\".".to_string())?;
    if at(&meta, 0, 0) != MARKER {
        return Err("Ini bukan form dari E-Raport".into());
    }
    let version: u32 = at(&meta, 0, 1).parse().unwrap_or(0);
    if version == 0 || version > VERSION {
        return Err("Versi form tidak dikenali; buat ulang form dari aplikasi versi ini".into());
    }
    let project_id = Some(at(&meta, 1, 1)).filter(|p| !p.is_empty());
    let header_row: usize = at(&meta, 2, 1).parse().unwrap_or(HEADER_ROW as usize);
    let eff_days: u32 = if at(&meta, 3, 0) == "effective_days" {
        at(&meta, 3, 1).parse::<u32>().ok().filter(|&d| d > 0).unwrap_or_else(|| data.settings.effective_days())
    } else {
        data.settings.effective_days()
    };
    let keys: Vec<Option<Key>> = meta
        .get(KEYS_ROW as usize)
        .map(|row| row.iter().map(|d| Key::parse(&cell_text(d))).collect())
        .unwrap_or_default();
    let col_of = |k: &Key| keys.iter().position(|x| x.as_ref() == Some(k));
    let id_col = col_of(&Key::Id).ok_or("Kolom ID tidak ditemukan di form")?;
    let class_col = col_of(&Key::Class);
    let name_col = col_of(&Key::Name);

    let sheet = wb
        .worksheet_range(DATA_SHEET)
        .map(grid)
        .map_err(|_| format!("Sheet \"{DATA_SHEET}\" tidak ditemukan di form"))?;

    // Baca snapshot metadata & data siswa dari sheet tersembunyi _eraport (kalau ada).
    let mut snapshots_by_id: BTreeMap<String, Student> = BTreeMap::new();
    let mut snapshots_by_name: BTreeMap<(String, String), Student> = BTreeMap::new();
    for r in 5..meta.len() {
        match at(&meta, r, 0).as_str() {
            "projects_config" => {
                if let Ok(projs) = serde_json::from_str::<Vec<ProjectConfig>>(&at(&meta, r, 1)) {
                    for p in projs {
                        if let Some(existing) = data.settings.projects.iter_mut().find(|x| x.id == p.id) {
                            if existing.theme.trim().is_empty() && !p.theme.trim().is_empty() {
                                existing.theme = p.theme;
                            }
                            if existing.central_idea.trim().is_empty() && !p.central_idea.trim().is_empty() {
                                existing.central_idea = p.central_idea;
                            }
                            if existing.lines_of_inquiry.trim().is_empty() && !p.lines_of_inquiry.trim().is_empty() {
                                existing.lines_of_inquiry = p.lines_of_inquiry;
                            }
                            if existing.facilitators.trim().is_empty() && !p.facilitators.trim().is_empty() {
                                existing.facilitators = p.facilitators;
                            }
                        } else if !p.id.trim().is_empty() && !p.name.trim().is_empty() {
                            data.settings.projects.push(p);
                        }
                    }
                }
            }
            "score_columns" => {
                if let Ok(cols) = serde_json::from_str::<Vec<String>>(&at(&meta, r, 1)) {
                    for c in cols {
                        if !c.trim().is_empty()
                            && !data.score_columns.iter().any(|x| canonical_key(x) == canonical_key(&c))
                        {
                            data.score_columns.push(c);
                        }
                    }
                }
            }
            "classes_config" => {
                if let Ok(cls_list) = serde_json::from_str::<Vec<ClassConfig>>(&at(&meta, r, 1)) {
                    for c in cls_list {
                        if let Some(existing) = data.settings.classes.iter_mut().find(|x| x.name == c.name) {
                            if existing.homeroom_teacher.trim().is_empty() && !c.homeroom_teacher.trim().is_empty() {
                                existing.homeroom_teacher = c.homeroom_teacher;
                            }
                        } else if !c.name.trim().is_empty() {
                            data.settings.classes.push(ClassConfig {
                                name: c.name,
                                homeroom_teacher: c.homeroom_teacher,
                                homeroom_signature: String::new(),
                            });
                        }
                    }
                }
            }
            "student" => {
                if let Ok(st_snap) = serde_json::from_str::<Student>(&at(&meta, r, 2)) {
                    if !st_snap.id.is_empty() {
                        snapshots_by_id.insert(st_snap.id.clone(), st_snap.clone());
                    }
                    let nk = (normalize_class(&st_snap.class_name), normalize_name(&st_snap.name));
                    if !nk.1.is_empty() {
                        snapshots_by_name.insert(nk, st_snap);
                    }
                }
            }
            _ => {}
        }
    }

    let form_has_atl = keys.iter().any(|k| matches!(k, Some(Key::Atl(_))));
    let form_has_att = keys
        .iter()
        .any(|k| matches!(k, Some(Key::Attendance | Key::Sick | Key::Permit | Key::Alpha)));

    let mut rep = FormImportReport::default();
    let project_ok = match &project_id {
        Some(pid) => {
            rep.project = project_label(data, pid);
            if rep.project.is_none() {
                rep.notes.push("Proyek di form ini sudah tidak ada di Pengaturan; kolom proyek dilewati.".into());
            }
            rep.project.is_some()
        }
        None => false,
    };
    for (c, k) in keys.iter().enumerate() {
        if let Some(Key::Atl(aid)) = k {
            if !data.settings.atl.iter().any(|a| &a.id == aid) {
                let label = at(&sheet, header_row, c);
                rep.notes.push(format!("Atribut ATL \"{label}\" sudah tidak ada di Pengaturan; kolomnya dilewati."));
            }
        }
    }

    let grading = data.settings.grading.clone();
    let atl_names: Vec<(String, String)> = data.settings.atl.iter().map(|a| (a.id.clone(), a.name.clone())).collect();

    for r in header_row + 1..sheet.len() {
        let id = at(&sheet, r, id_col);
        let name = name_col.map(|c| at(&sheet, r, c)).unwrap_or_default();
        if id.is_empty() && name.is_empty() {
            continue;
        }
        let class = class_col.map(|c| normalize_class(&at(&sheet, r, c))).unwrap_or_default();
        let snap = (!id.is_empty())
            .then(|| snapshots_by_id.get(&id).cloned())
            .flatten()
            .or_else(|| snapshots_by_name.get(&(class.clone(), normalize_name(&name))).cloned());

        let existing_idx = data.students.iter().position(|s| !id.is_empty() && s.id == id).or_else(|| {
            let n = normalize_name(&name);
            data.students
                .iter()
                .position(|s| !n.is_empty() && normalize_name(&s.name) == n && (class.is_empty() || s.class_name == class))
        });

        let idx = match existing_idx {
            Some(i) => i,
            None => {
                let resolved_name = if !name.trim().is_empty() {
                    name.trim().to_string()
                } else if let Some(sn) = &snap {
                    sn.name.trim().to_string()
                } else {
                    String::new()
                };
                if resolved_name.is_empty() {
                    rep.unknown_rows.push(format!("ID {id}"));
                    continue;
                }
                let resolved_class = if !class.is_empty() {
                    class.clone()
                } else {
                    snap.as_ref().map(|sn| normalize_class(&sn.class_name)).unwrap_or_default()
                };
                let preferred_id = if !id.trim().is_empty() {
                    id.trim().to_string()
                } else {
                    snap.as_ref().map(|sn| sn.nis.trim().to_string()).unwrap_or_default()
                };
                let new_id = new_student_id(data, &preferred_id);
                data.students.push(Student {
                    id: new_id,
                    nis: snap.as_ref().map(|sn| sn.nis.clone()).unwrap_or_default(),
                    nisn: snap.as_ref().map(|sn| sn.nisn.clone()).unwrap_or_default(),
                    name: resolved_name,
                    gender: snap.as_ref().map(|sn| sn.gender.clone()).unwrap_or_default(),
                    class_name: resolved_class,
                    ..Default::default()
                });
                rep.students_added += 1;
                rep.changed += 1;
                data.students.len() - 1
            }
        };
        rep.students += 1;
        let student = &mut data.students[idx];
        let sname = student.name.clone();

        // `grade`: kolom penilaian dibandingkan tanpa membedakan huruf besar/kecil
        // ("me" = "ME"), jadi perbedaan huruf saja tidak dilaporkan sebagai perubahan.
        let set_text = |rep: &mut FormImportReport, label: &str, slot: &mut String, value: String, grade: bool| {
            rep.values_set += 1;
            let same = if grade { slot.trim().eq_ignore_ascii_case(value.trim()) } else { slot.trim() == value.trim() };
            if same {
                *slot = value;
            } else {
                rep.changed += 1;
                if !slot.trim().is_empty() {
                    let short = |s: &str| {
                        let t: String = s.chars().take(40).collect();
                        if s.chars().count() > 40 { format!("{t}…") } else { t }
                    };
                    rep.overwritten.push(format!("{sname} — {label}: {} → {}", short(slot.trim()), short(value.trim())));
                }
                *slot = value;
            }
        };

        // Gabungkan data tambahan dari snapshot guru pembuat form (identitas, nilai mapel,
        // catatan indikator khusus, proyek lain, atau ATL/kehadiran yang tidak ditampilkan sebagai kolom form).
        if let Some(sn) = snap {
            if student.nis.is_empty() && !sn.nis.is_empty() {
                student.nis = sn.nis;
            }
            if student.nisn.is_empty() && !sn.nisn.is_empty() {
                student.nisn = sn.nisn;
            }
            if student.gender.is_empty() && !sn.gender.is_empty() {
                student.gender = sn.gender;
            }
            if student.class_name.is_empty() && !sn.class_name.is_empty() {
                student.class_name = normalize_class(&sn.class_name);
            }
            for (k, sc) in sn.scores {
                let cur = student.scores.entry(k).or_default();
                for (src_val, dst_slot) in [(sc.ph, &mut cur.ph), (sc.pts, &mut cur.pts), (sc.manual, &mut cur.manual)] {
                    if let Some(v) = src_val {
                        rep.values_set += 1;
                        if *dst_slot != Some(v) {
                            *dst_slot = Some(v);
                            rep.changed += 1;
                        }
                    }
                }
            }
            for (k, note) in sn.subject_notes {
                if !note.trim().is_empty() {
                    let slot = student.subject_notes.entry(k).or_default();
                    set_text(&mut rep, "Indikator mapel", slot, note.trim().to_string(), false);
                }
            }
            for (aid, r_entry) in sn.atl {
                let Some((_, label)) = atl_names.iter().find(|(id, _)| id == &aid) else { continue };
                if !r_entry.indicator.trim().is_empty() {
                    let entry = student.atl.entry(aid.clone()).or_default();
                    set_text(&mut rep, &format!("Indikator {label}"), &mut entry.indicator, r_entry.indicator.trim().to_string(), false);
                }
                if !form_has_atl && !r_entry.grade.trim().is_empty() {
                    let entry = student.atl.entry(aid).or_default();
                    set_text(&mut rep, label, &mut entry.grade, normalize_grade(&r_entry.grade, &grading), true);
                }
            }
            for pe in sn.projects {
                if project_id.as_deref() == Some(pe.project_id.as_str()) {
                    continue; // proyek utama form ini dibaca langsung dari sel sheet Isian di bawah
                }
                if pe.understanding.trim().is_empty() && pe.inquiry.trim().is_empty() && pe.narrative.trim().is_empty() {
                    continue;
                }
                let pos = match student.projects.iter().position(|p| p.project_id == pe.project_id) {
                    Some(p) => p,
                    None => {
                        student.projects.push(ProjectEntry { project_id: pe.project_id.clone(), ..Default::default() });
                        rep.projects_added += 1;
                        student.projects.len() - 1
                    }
                };
                let dst = &mut student.projects[pos];
                if !pe.understanding.trim().is_empty() {
                    set_text(&mut rep, "Understanding Level", &mut dst.understanding, normalize_grade(&pe.understanding, &grading), true);
                }
                if !pe.inquiry.trim().is_empty() {
                    set_text(&mut rep, "Inquiry & Research Skills", &mut dst.inquiry, normalize_grade(&pe.inquiry, &grading), true);
                }
                if !pe.narrative.trim().is_empty() {
                    set_text(&mut rep, "Narasi", &mut dst.narrative, pe.narrative.trim().to_string(), false);
                }
            }
            if !form_has_att {
                if sn.attendance_s.is_some() || sn.attendance_i.is_some() || sn.attendance_a.is_some() {
                    student.attendance_s = sn.attendance_s;
                    student.attendance_i = sn.attendance_i;
                    student.attendance_a = sn.attendance_a;
                }
                if !sn.attendance.trim().is_empty() {
                    set_text(&mut rep, "Kehadiran", &mut student.attendance, sn.attendance.trim().to_string(), false);
                }
            }
        }

        // Cek kolom Sakit (S), Izin (I), Alpa (A) lebih dulu untuk menghitung Kehadiran otomatis.
        let s_col = col_of(&Key::Sick);
        let i_col = col_of(&Key::Permit);
        let a_col = col_of(&Key::Alpha);
        let mut sia_handled = false;
        if s_col.is_some() || i_col.is_some() || a_col.is_some() {
            let parse_days = |col: Option<usize>, label: &str, rep: &mut FormImportReport| -> Option<Option<u32>> {
                let Some(c) = col else { return Some(None) };
                let raw = at(&sheet, r, c);
                let t = raw.trim();
                if t.is_empty() {
                    return Some(None);
                }
                if t == "-" {
                    return Some(Some(0));
                }
                match crate::grading::parse_number(t) {
                    Some(v) if (0.0..=366.0).contains(&v) && (v - v.round()).abs() < 1e-6 => Some(Some(v.round() as u32)),
                    _ => {
                        rep.invalid.push(format!("{sname} — {label}: \"{t}\" harus angka hari (0 atau lebih)"));
                        None
                    }
                }
            };
            let s_val = parse_days(s_col, "Sakit (S)", &mut rep);
            let i_val = parse_days(i_col, "Izin (I)", &mut rep);
            let a_val = parse_days(a_col, "Alpa (A)", &mut rep);
            if let (Some(so), Some(io), Some(ao)) = (s_val, i_val, a_val) {
                if so.is_some() || io.is_some() || ao.is_some() {
                    let sv = so.unwrap_or(0);
                    let iv = io.unwrap_or(0);
                    let av = ao.unwrap_or(0);
                    student.attendance_s = Some(sv);
                    student.attendance_i = Some(iv);
                    student.attendance_a = Some(av);
                    if let Some(calc) = format_attendance_sia(Some(sv), Some(iv), Some(av), eff_days) {
                        set_text(&mut rep, "Kehadiran", &mut student.attendance, calc, false);
                    }
                    sia_handled = true;
                }
            } else {
                sia_handled = true; // ada angka tidak valid di S/I/A, jangan timpa dengan rumus lama
            }
        }

        for (c, key) in keys.iter().enumerate() {
            let Some(key) = key else { continue };
            let raw = at(&sheet, r, c);
            if raw.trim().is_empty() {
                continue;
            }
            match key {
                Key::Id | Key::Class | Key::Name | Key::Sick | Key::Permit | Key::Alpha => {}
                Key::Atl(aid) => {
                    let Some((_, label)) = atl_names.iter().find(|(id, _)| id == aid) else { continue };
                    if resolve_input(&raw, &grading).status == Status::Invalid {
                        rep.invalid.push(format!("{sname} — {label}: \"{}\" tidak terbaca", raw.trim()));
                        continue;
                    }
                    let value = normalize_grade(&raw, &grading);
                    let entry = student.atl.entry(aid.clone()).or_default();
                    set_text(&mut rep, label, &mut entry.grade, value, true);
                }
                Key::Understanding | Key::Inquiry | Key::Narrative if project_ok => {
                    let pid = project_id.clone().unwrap_or_default();
                    let label = match key {
                        Key::Understanding => "Understanding Level",
                        Key::Inquiry => "Inquiry & Research Skills",
                        _ => "Narasi",
                    };
                    if !matches!(key, Key::Narrative) && resolve_input(&raw, &grading).status == Status::Invalid {
                        rep.invalid.push(format!("{sname} — {label}: \"{}\" tidak terbaca", raw.trim()));
                        continue;
                    }
                    let pos = match student.projects.iter().position(|p| p.project_id == pid) {
                        Some(p) => p,
                        None => {
                            student.projects.push(ProjectEntry { project_id: pid.clone(), ..Default::default() });
                            rep.projects_added += 1;
                            student.projects.len() - 1
                        }
                    };
                    let entry = &mut student.projects[pos];
                    match key {
                        Key::Understanding => set_text(&mut rep, label, &mut entry.understanding, normalize_grade(&raw, &grading), true),
                        Key::Inquiry => set_text(&mut rep, label, &mut entry.inquiry, normalize_grade(&raw, &grading), true),
                        _ => set_text(&mut rep, label, &mut entry.narrative, raw.trim().to_string(), false),
                    }
                }
                Key::Attendance if !sia_handled && !raw.trim().starts_with('=') => {
                    set_text(&mut rep, "Kehadiran", &mut student.attendance, raw.trim().to_string(), false);
                }
                _ => {}
            }
        }
    }
    ensure_class_configs(data);
    Ok(rep)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::RatingEntry;
    use std::path::PathBuf;

    fn sample() -> AppData {
        let mut d = AppData::default();
        for (id, name, class) in [("1", "Made Dewi", "IX-1"), ("2", "Putu Arya", "IX-1"), ("3", "Kadek Sari", "IX-2")] {
            d.students.push(Student {
                id: id.into(),
                nis: id.into(),
                name: name.into(),
                class_name: class.into(),
                ..Default::default()
            });
        }
        d
    }

    fn tmp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("eraport-form-{name}-{}.xlsx", std::process::id()))
    }

    fn grade(g: &str) -> RatingEntry {
        RatingEntry { grade: g.into(), indicator: String::new() }
    }

    #[test]
    fn round_trip_restores_values() {
        let mut filled = sample();
        filled.students[0].atl.insert("caring".into(), grade("EE"));
        filled.students[0].attendance = "98%".into();
        filled.students[1].projects.push(ProjectEntry {
            project_id: "p1".into(),
            understanding: "92".into(),
            inquiry: "me".into(),
            narrative: "Great work.\nKeep going.".into(),
        });
        let req = FormRequest {
            classes: vec!["IX-1".into()],
            include_atl: true,
            project_id: Some("p1".into()),
            only_project_members: false,
            include_attendance: true,
            ..Default::default()
        };
        let path = tmp("roundtrip");
        assert_eq!(write_form(&filled, &req, &path).unwrap(), 2);

        let mut fresh = sample();
        let rep = import_form(&mut fresh, &path).unwrap();
        assert_eq!(rep.students, 2);
        assert_eq!(fresh.students[0].atl["caring"].grade, "EE");
        assert_eq!(fresh.students[0].attendance, "98%");
        let p = &fresh.students[1].projects[0];
        assert_eq!(
            (p.understanding.as_str(), p.inquiry.as_str(), p.narrative.as_str()),
            ("92", "ME", "Great work.\nKeep going.")
        );
        assert!(rep.overwritten.is_empty() && rep.invalid.is_empty() && rep.unknown_rows.is_empty());
        assert_eq!(rep.projects_added, 1);
        assert!(fresh.students[0].projects.is_empty(), "baris tanpa isian proyek tidak diberi proyek kosong");
        assert!(fresh.students[2].atl.is_empty(), "siswa kelas lain tidak tersentuh");

        let again = import_form(&mut fresh, &path).unwrap();
        assert_eq!(again.changed, 0, "impor ulang file yang sama tidak mengubah apa-apa");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn overwrite_and_invalid_are_reported() {
        let mut filled = sample();
        filled.students[0].atl.insert("caring".into(), grade("bagus"));
        filled.students[1].atl.insert("caring".into(), grade("EE"));
        let req = FormRequest { classes: vec!["IX-1".into()], include_atl: true, ..Default::default() };
        let path = tmp("overwrite");
        write_form(&filled, &req, &path).unwrap();

        let mut target = sample();
        target.students[1].atl.insert("caring".into(), grade("AE"));
        let rep = import_form(&mut target, &path).unwrap();
        assert_eq!(rep.invalid.len(), 1, "{:?}", rep.invalid);
        assert_eq!(rep.overwritten.len(), 1, "{:?}", rep.overwritten);
        assert!(rep.overwritten[0].contains("AE → EE"));
        assert_eq!(target.students[1].atl["caring"].grade, "EE");
        assert!(!target.students[0].atl.contains_key("caring"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn case_only_difference_is_not_a_change() {
        let mut filled = sample();
        filled.students[0].atl.insert("caring".into(), grade("ME"));
        let req = FormRequest { classes: vec!["IX-1".into()], include_atl: true, ..Default::default() };
        let path = tmp("case");
        write_form(&filled, &req, &path).unwrap();
        let mut target = sample();
        target.students[0].atl.insert("caring".into(), grade("me"));
        let rep = import_form(&mut target, &path).unwrap();
        assert_eq!((rep.changed, rep.overwritten.len()), (0, 0));
        assert_eq!(target.students[0].atl["caring"].grade, "ME", "tetap dirapikan jadi huruf besar");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn empty_cells_keep_existing_and_missing_students_added() {
        let req = FormRequest { classes: vec!["IX-1".into()], include_atl: true, ..Default::default() };
        let path = tmp("empty");
        write_form(&sample(), &req, &path).unwrap();

        let mut target = sample();
        target.students[0].atl.insert("caring".into(), grade("ME"));
        target.students.remove(1);
        let rep = import_form(&mut target, &path).unwrap();
        assert_eq!(target.students[0].atl["caring"].grade, "ME");
        assert_eq!(rep.students_added, 1);
        assert!(target.students.iter().any(|s| s.name == "Putu Arya"));
        assert_eq!(rep.values_set, 0);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn guru_shares_form_to_another_guru_with_empty_app() {
        use crate::model::ScoreEntry;
        let mut guru1 = sample();
        guru1.students[0].nis = "5001".into();
        guru1.students[0].nisn = "0012345678".into();
        guru1.students[0].gender = "P".into();
        guru1.students[0].atl.insert("caring".into(), RatingEntry { grade: "EE".into(), indicator: "Khusus Dewi".into() });
        guru1.students[0].scores.insert("IPA".into(), ScoreEntry { ph: Some(90.0), pts: Some(94.0), manual: None });
        guru1.students[0].projects.push(ProjectEntry {
            project_id: "p1".into(),
            understanding: "92".into(),
            inquiry: "ME".into(),
            narrative: "Very active in inquiry.".into(),
        });
        guru1.students[0].attendance_s = Some(1);
        guru1.students[0].attendance_i = Some(1);
        guru1.students[0].attendance_a = Some(0);
        guru1.students[0].attendance = "98% (Present)".into();

        let req = FormRequest {
            classes: vec!["IX-1".into()],
            include_atl: true,
            project_id: Some("p1".into()),
            include_attendance: true,
            effective_days: Some(100),
            ..Default::default()
        };
        let path = tmp("share-guru");
        write_form(&guru1, &req, &path).unwrap();

        // Guru 2 membuka aplikasi E-Raport yang masih kosong (belum ada siswa) lalu mengimpor form dari Guru 1.
        let mut guru2 = AppData::default();
        let rep = import_form(&mut guru2, &path).unwrap();
        assert_eq!(rep.students_added, 2);
        assert_eq!(guru2.students.len(), 2);
        let s0 = &guru2.students[0];
        assert_eq!(s0.name, "Made Dewi");
        assert_eq!(s0.nisn, "0012345678");
        assert_eq!(s0.gender, "P");
        assert_eq!(s0.atl["caring"].grade, "EE");
        assert_eq!(s0.atl["caring"].indicator, "Khusus Dewi");
        assert_eq!(s0.scores["IPA"].ph, Some(90.0));
        assert_eq!(s0.projects[0].understanding, "92");
        assert_eq!(s0.projects[0].narrative, "Very active in inquiry.");
        assert_eq!(s0.attendance, "98% (Present)");
        assert!(guru2.settings.classes.iter().any(|c| c.name == "IX-1"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn selection_validation_and_file_name() {
        let mut d = sample();
        d.students[2].projects.push(ProjectEntry { project_id: "p2".into(), ..Default::default() });
        let req = FormRequest {
            classes: vec!["IX-2".into(), "IX-1".into()],
            project_id: Some("p2".into()),
            only_project_members: true,
            ..Default::default()
        };
        assert_eq!(selected_students(&d, &req).len(), 1);
        assert_eq!(suggested_file_name(&d, &req), "Form Project 2 - IX-1, IX-2.xlsx");
        let nothing = FormRequest { classes: vec!["IX-1".into()], ..Default::default() };
        assert!(build_form(&d, &nothing).is_err());
        let wrong = FormRequest { classes: vec!["IX-1".into()], project_id: Some("zzz".into()), ..Default::default() };
        assert!(build_form(&d, &wrong).is_err());
    }

    #[test]
    fn non_form_excel_is_rejected() {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../berkas_atau_template/Contoh rekap nilai siswa.xls");
        if !p.exists() {
            return;
        }
        let err = import_form(&mut sample(), &p).unwrap_err();
        assert!(err.contains("bukan form"), "{err}");
    }

    #[test]
    fn sia_attendance_calculates_percentage() {
        let mut filled = sample();
        filled.students[0].attendance_s = Some(2);
        filled.students[0].attendance_i = Some(1);
        filled.students[0].attendance_a = Some(0);
        filled.students[1].attendance_s = Some(0);
        let req = FormRequest {
            classes: vec!["IX-1".into()],
            include_attendance: true,
            effective_days: Some(100),
            ..Default::default()
        };
        let path = tmp("sia");
        write_form(&filled, &req, &path).unwrap();

        let mut fresh = sample();
        let rep = import_form(&mut fresh, &path).unwrap();
        assert_eq!(rep.changed, 2);
        assert_eq!(fresh.students[0].attendance, "97% (Present)");
        assert_eq!(fresh.students[0].attendance_s, Some(2));
        assert_eq!(fresh.students[0].attendance_i, Some(1));
        assert_eq!(fresh.students[0].attendance_a, Some(0));
        assert_eq!(fresh.students[1].attendance, "100% (Present)");
        let _ = std::fs::remove_file(path);
    }
}
