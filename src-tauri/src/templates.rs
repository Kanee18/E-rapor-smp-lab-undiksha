//! Template Excel baku untuk guru: data siswa, nilai mapel, nilai proyek, dan absensi.
//!
//! Susunannya SAMA untuk SD, SMP, dan SMA, supaya semua file dari guru seragam:
//! lima kolom identitas yang sama di setiap template (No, Nama Siswa, NIS, NISN, Kelas),
//! lalu kolom isian sesuai jenisnya. Nilai mapel memakai satu file per mata pelajaran
//! (nama mapel ditulis di bagian atas), karena satu guru biasanya memegang satu mapel.
//! Semua template langsung terbaca impor aplikasi tanpa "Atur kolom sendiri".
//!
//! Dari aplikasi, daftar siswa boleh ikut dicantumkan (`with_students`) supaya guru tidak
//! perlu mengetik nama; susunan kolomnya tetap sama dengan template kosong.

use crate::model::{AppData, Student};
use rust_xlsxwriter::{
    Color, DataValidation, DataValidationErrorStyle, DataValidationRule, Format, FormatAlign, FormatBorder, Workbook,
    Worksheet, XlsxError,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

const BLUE: u32 = 0x3653D3;
const RULE: u32 = 0xD9D4CA;
const GREY: u32 = 0xF1F0EC;
const FIELD_BG: u32 = 0xFFF4CC;
/// Baris judul kolom (0-based): baris 5 di Excel (baris 4 sengaja kosong sebagai pemisah).
pub const HEADER_ROW: u32 = 4;
/// Baris kosong bernomor pada template tanpa daftar siswa.
const BLANK_ROWS: u32 = 100;
/// Lima kolom identitas yang sama di semua template.
const ID_COLS: [(&str, f64); 5] = [("No", 6.0), ("Nama Siswa", 36.0), ("NIS", 12.0), ("NISN", 14.0), ("Kelas", 9.0)];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TemplateKind {
    Students,
    Scores,
    Projects,
    Attendance,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TemplateRequest {
    pub kind: TemplateKind,
    /// Cantumkan daftar siswa dari aplikasi (selain template data siswa).
    #[serde(default)]
    pub with_students: bool,
    /// Kelas yang dicantumkan; kosong = semua kelas.
    #[serde(default)]
    pub classes: Vec<String>,
    /// Template nilai proyek: proyek yang langsung ditulis di bagian atas (boleh kosong).
    #[serde(default)]
    pub project_id: Option<String>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct TemplateInfo {
    /// Jumlah siswa yang sudah tercantum (0 = template kosong).
    pub students: usize,
}

fn xerr(e: XlsxError) -> String {
    format!("Gagal membuat file Excel: {e}")
}

struct Styles {
    title: Format,
    muted: Format,
    head: Format,
    label: Format,
    field: Format,
    fixed: Format,
    fixed_c: Format,
    fixed_text: Format,
    input: Format,
    input_c: Format,
    input_text: Format,
    guide_h: Format,
    guide: Format,
}

fn styles() -> Styles {
    let border = |f: Format| f.set_border(FormatBorder::Thin).set_border_color(Color::RGB(RULE));
    let grey = || border(Format::new().set_background_color(Color::RGB(GREY)));
    Styles {
        title: Format::new().set_bold().set_font_size(14).set_font_color(Color::RGB(BLUE)),
        muted: Format::new().set_font_color(Color::RGB(0x5E6672)),
        head: Format::new()
            .set_bold()
            .set_text_wrap()
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter)
            .set_font_color(Color::White)
            .set_background_color(Color::RGB(BLUE))
            .set_border(FormatBorder::Thin)
            .set_border_color(Color::RGB(0x2C44B8)),
        label: Format::new().set_bold().set_align(FormatAlign::Right).set_align(FormatAlign::VerticalCenter),
        field: border(Format::new().set_bold().set_background_color(Color::RGB(FIELD_BG)).set_align(FormatAlign::VerticalCenter)),
        fixed: grey(),
        fixed_c: grey().set_align(FormatAlign::Center),
        fixed_text: grey().set_num_format("@"),
        input: border(Format::new().set_align(FormatAlign::Top).set_text_wrap()),
        input_c: border(Format::new().set_align(FormatAlign::Center)),
        input_text: border(Format::new().set_num_format("@")),
        guide_h: Format::new().set_bold().set_font_size(13).set_font_color(Color::RGB(BLUE)),
        guide: Format::new().set_text_wrap().set_align(FormatAlign::Top),
    }
}

/// Kelas yang dicantumkan: pilihan pengguna, atau semua kelas yang punya siswa (urut seperti di aplikasi).
fn class_list(data: &AppData, wanted: &[String]) -> Vec<String> {
    if !wanted.is_empty() {
        return wanted.to_vec();
    }
    let mut students: Vec<&Student> = data.students.iter().collect();
    students.sort_by(|a, b| crate::model::compare_students(a, b));
    let mut list: Vec<String> = Vec::new();
    for s in students {
        if !s.class_name.is_empty() && !list.contains(&s.class_name) {
            list.push(s.class_name.clone());
        }
    }
    list
}

fn students_for<'a>(data: &'a AppData, req: &TemplateRequest) -> Vec<&'a Student> {
    if !req.with_students || req.kind == TemplateKind::Students {
        return Vec::new();
    }
    let mut out = Vec::new();
    for class in class_list(data, &req.classes) {
        let mut v: Vec<&Student> = data.students.iter().filter(|s| s.class_name == class).collect();
        v.sort_by(|a, b| crate::model::compare_students(a, b));
        out.extend(v);
    }
    out
}

fn project_label(data: &AppData, id: Option<&str>) -> Option<String> {
    let p = data.settings.projects.iter().find(|p| Some(p.id.as_str()) == id)?;
    Some(if p.theme.trim().is_empty() { p.name.clone() } else { format!("{} — {}", p.name, p.theme.trim()) })
}

fn safe_file_name(s: &str) -> String {
    s.chars().map(|c| if r#"\/:*?"<>|"#.contains(c) { '-' } else { c }).collect()
}

/// Nama file baku (sama untuk semua jenjang).
pub fn standard_file_name(kind: TemplateKind) -> &'static str {
    match kind {
        TemplateKind::Students => "Template 1 - Data Siswa.xlsx",
        TemplateKind::Scores => "Template 2 - Nilai Mapel.xlsx",
        TemplateKind::Projects => "Template 3 - Nilai Proyek.xlsx",
        TemplateKind::Attendance => "Template 4 - Absensi.xlsx",
    }
}

pub fn suggested_file_name(data: &AppData, req: &TemplateRequest) -> String {
    let base = standard_file_name(req.kind).trim_end_matches(".xlsx").to_string();
    if !req.with_students || req.kind == TemplateKind::Students {
        return base + ".xlsx";
    }
    let classes = class_list(data, &req.classes);
    safe_file_name(&format!("{base} - {}.xlsx", classes.join(", ")))
}

struct Layout {
    sheet: &'static str,
    title: &'static str,
    /// Isian di baris 2 (mis. "Mata pelajaran"): label + nilai awal.
    field: Option<(&'static str, String)>,
    note: &'static str,
    /// Kolom isian setelah kolom identitas: (judul, lebar).
    cols: Vec<(&'static str, f64)>,
    guide_title: &'static str,
    guide: Vec<&'static str>,
}

fn layout(data: &AppData, req: &TemplateRequest) -> Layout {
    let common_end = [
        "",
        "ATURAN UNTUK SEMUA TEMPLATE",
        "• Lima kolom pertama selalu sama: No, Nama Siswa, NIS, NISN, Kelas. Isi NISN (10 angka) kalau ada: siswa dicocokkan lewat NISN, lalu lewat nama dan kelas.",
        "• Kelas ditulis dengan pola yang sama, mis. I-A, IV-B, VII-1, IX-2, X-B, XII-IPA-1.",
        "• Isi mulai baris 6. Satu baris = satu siswa. Jangan menghapus atau mengubah baris 1–5 (judul).",
        "• Jangan menggabungkan sel dan jangan menyisipkan kolom baru di antara kolom yang ada.",
        "• Sel yang dibiarkan kosong tidak menghapus isian yang sudah ada di aplikasi.",
        "• Template ini sama untuk SD, SMP, dan SMA.",
    ];
    let with = |mut v: Vec<&'static str>| {
        v.extend(common_end);
        v
    };
    match req.kind {
        TemplateKind::Students => Layout {
            sheet: "Data siswa",
            title: "TEMPLATE DATA SISWA · E-RAPORT",
            field: None,
            note: "Isi satu baris untuk satu siswa. Urutan baris = nomor absen. Petunjuk lengkap ada di sheet \"Petunjuk\".",
            cols: vec![("L/P", 7.0)],
            guide_title: "Cara mengisi template data siswa",
            guide: with(vec![
                "1. Nama Siswa: nama lengkap sesuai Dapodik. Wajib diisi.",
                "2. NIS dan NISN: isi minimal salah satu. Kolomnya berformat teks, jadi angka nol di depan NISN tidak hilang.",
                "3. Kelas: nama kelas siswa. Satu file boleh berisi beberapa kelas.",
                "4. L/P: L untuk laki-laki, P untuk perempuan.",
                "5. Masukkan ke aplikasi: Data siswa → Impor dari Excel → pilih file ini → periksa → Impor.",
                "",
                "Contoh satu baris:   1  |  Adi Putra Wijaya  |  7001  |  0098765401  |  IX-1  |  L",
            ]),
        },
        TemplateKind::Scores => Layout {
            sheet: "Nilai mapel",
            title: "TEMPLATE NILAI MAPEL · E-RAPORT",
            field: Some(("Mata pelajaran:", String::new())),
            note: "Satu file untuk satu mata pelajaran. Tulis nama mapel di kotak kuning, lalu isi PH dan PTS (angka 0–100).",
            cols: vec![("PH", 10.0), ("PTS", 10.0)],
            guide_title: "Cara mengisi template nilai mapel",
            guide: with(vec![
                "1. Tulis nama mata pelajaran di kotak kuning (baris 2), mis. IPA, Matematika, Bahasa Indonesia. Satu file = satu mapel; boleh berisi beberapa kelas.",
                "2. PH: rata-rata penilaian harian. PTS: nilai penilaian tengah semester. Isi angka 0–100; boleh desimal, mis. 87,5.",
                "3. Nilai akhir dan predikat dihitung aplikasi (bobot PH/PTS di Pengaturan). Tidak perlu rumus di Excel.",
                "4. Masukkan ke aplikasi: Nilai mapel → Impor nilai dari Excel → pilih file ini → periksa mapel & kelas → Periksa → Terapkan.",
            ]),
        },
        TemplateKind::Projects => Layout {
            sheet: "Nilai proyek",
            title: "TEMPLATE NILAI PROYEK · E-RAPORT",
            field: Some(("Proyek:", project_label(data, req.project_id.as_deref()).unwrap_or_default())),
            note: "Satu file untuk satu proyek. Tulis nama proyek di kotak kuning. Nilai: EE / ME / AE / BE (atau E, M, A, B, atau angka 0–100).",
            cols: vec![("Understanding Level", 15.0), ("Inquiry & Research Skills", 15.0), ("Teacher Narrative Feedback (English)", 64.0)],
            guide_title: "Cara mengisi template nilai proyek",
            guide: with(vec![
                "1. Tulis nama proyek di kotak kuning (baris 2), sama dengan nama atau tema proyek di aplikasi.",
                "2. Understanding Level dan Inquiry & Research Skills: isi EE, ME, AE, atau BE. Boleh huruf awalnya saja (E, M, A, B) atau angka 0–100.",
                "3. Teacher Narrative Feedback: bahasa Inggris, 2–4 kalimat. Boleh dikosongkan lalu dibuat di aplikasi dengan Draf otomatis.",
                "4. Siswa yang tidak ikut proyek ini tidak perlu dicantumkan (atau biarkan nilainya kosong).",
                "5. Masukkan ke aplikasi: Isi rapor → tab Nilai proyek → Impor nilai proyek dari Excel → pilih file ini → Periksa → Terapkan.",
            ]),
        },
        TemplateKind::Attendance => Layout {
            sheet: "Absensi",
            title: "TEMPLATE ABSENSI · E-RAPORT",
            field: None,
            note: "Isi JUMLAH HARI sakit, izin, dan alpa selama satu semester. Persentase kehadiran dihitung aplikasi.",
            cols: vec![("Sakit", 9.0), ("Izin", 9.0), ("Alpa", 9.0)],
            guide_title: "Cara mengisi template absensi",
            guide: with(vec![
                "1. Sakit, Izin, Alpa: jumlah HARI selama semester (angka bulat). Kosong dianggap 0 hari.",
                "2. Persentase kehadiran di rapor dihitung otomatis dari hari efektif, mis. 98% (Present).",
                "3. Masukkan ke aplikasi: Isi rapor → Impor absensi Excel → pilih file ini → periksa hari efektif → Impor.",
            ]),
        },
    }
}

/// Buat template sesuai permintaan dan simpan ke `path`.
pub fn write_template(data: &AppData, req: &TemplateRequest, path: &Path) -> Result<TemplateInfo, String> {
    let mut wb = Workbook::new();
    let info = build(&mut wb, data, req)?;
    wb.save(path)
        .map_err(|e| format!("File tidak bisa disimpan (mungkin sedang dibuka di Excel?): {e}"))?;
    Ok(info)
}

fn build(wb: &mut Workbook, data: &AppData, req: &TemplateRequest) -> Result<TemplateInfo, String> {
    let st = styles();
    let lay = layout(data, req);
    let students = students_for(data, req);
    if req.with_students && req.kind != TemplateKind::Students && students.is_empty() {
        return Err("Belum ada siswa di kelas yang dipilih. Simpan template kosong, atau masukkan data siswa dulu.".into());
    }

    let ws = wb.add_worksheet();
    ws.set_name(lay.sheet).map_err(xerr)?;
    let ncols = (ID_COLS.len() + lay.cols.len()) as u16;
    ws.write_with_format(0, 0, lay.title, &st.title).map_err(xerr)?;
    if let Some((label, value)) = &lay.field {
        ws.set_row_height(1, 24).map_err(xerr)?;
        ws.write_with_format(1, 1, *label, &st.label).map_err(xerr)?;
        ws.merge_range(1, 2, 1, 4, value, &st.field).map_err(xerr)?;
    }
    ws.write_with_format(2, 0, lay.note, &st.muted).map_err(xerr)?;
    for (c, (t, w)) in ID_COLS.iter().chain(lay.cols.iter()).enumerate() {
        ws.write_with_format(HEADER_ROW, c as u16, *t, &st.head).map_err(xerr)?;
        ws.set_column_width(c as u16, *w).map_err(xerr)?;
    }
    ws.set_row_height(HEADER_ROW, 32).map_err(xerr)?;

    // baris data
    let tall = req.kind == TemplateKind::Projects;
    let n = if students.is_empty() { BLANK_ROWS as usize } else { students.len() };
    let mut no_in_class = 0u32;
    let mut prev_class = "";
    for i in 0..n {
        let r = HEADER_ROW + 1 + i as u32;
        match students.get(i) {
            Some(s) => {
                if s.class_name != prev_class {
                    prev_class = &s.class_name;
                    no_in_class = 0;
                }
                no_in_class += 1;
                ws.write_with_format(r, 0, no_in_class, &st.fixed_c).map_err(xerr)?;
                ws.write_with_format(r, 1, &s.name, &st.fixed).map_err(xerr)?;
                ws.write_with_format(r, 2, &s.nis, &st.fixed_text).map_err(xerr)?;
                ws.write_with_format(r, 3, &s.nisn, &st.fixed_text).map_err(xerr)?;
                ws.write_with_format(r, 4, &s.class_name, &st.fixed_c).map_err(xerr)?;
            }
            None => {
                ws.write_with_format(r, 0, i as u32 + 1, &st.fixed_c).map_err(xerr)?;
                ws.write_blank(r, 1, &st.input).map_err(xerr)?;
                ws.write_blank(r, 2, &st.input_text).map_err(xerr)?;
                ws.write_blank(r, 3, &st.input_text).map_err(xerr)?;
                ws.write_blank(r, 4, &st.input_c).map_err(xerr)?;
            }
        }
        for (k, _) in lay.cols.iter().enumerate() {
            let c = ID_COLS.len() as u16 + k as u16;
            let wide = tall && k == 2;
            ws.write_blank(r, c, if wide { &st.input } else { &st.input_c }).map_err(xerr)?;
        }
        if tall {
            ws.set_row_height(r, 42).map_err(xerr)?;
        }
    }
    let first = HEADER_ROW + 1;
    let last = HEADER_ROW + n as u32;
    let c0 = ID_COLS.len() as u16;

    // pilihan / pemeriksaan isian (peringatan saja, bukan penolakan)
    match req.kind {
        TemplateKind::Students => {
            let lp = DataValidation::new()
                .allow_list_strings(&["L", "P"])
                .map_err(xerr)?
                .set_error_style(DataValidationErrorStyle::Warning)
                .set_error_message("Isi L (laki-laki) atau P (perempuan).")
                .map_err(xerr)?;
            ws.add_data_validation(first, c0, last, c0, &lp).map_err(xerr)?;
        }
        TemplateKind::Scores => {
            let v = DataValidation::new()
                .allow_decimal_number(DataValidationRule::Between(0.0, 100.0))
                .set_error_style(DataValidationErrorStyle::Warning)
                .set_error_message("Nilai harus angka 0–100.")
                .map_err(xerr)?;
            ws.add_data_validation(first, c0, last, c0 + 1, &v).map_err(xerr)?;
            let hint = DataValidation::new()
                .allow_any_value()
                .set_input_title("Mata pelajaran")
                .map_err(xerr)?
                .set_input_message("Tulis nama mapel, mis. IPA, Matematika, Bahasa Indonesia.")
                .map_err(xerr)?;
            ws.add_data_validation(1, 2, 1, 4, &hint).map_err(xerr)?;
        }
        TemplateKind::Projects => {
            let mut bands = data.settings.grading.bands.clone();
            bands.sort_by(|x, y| y.min.partial_cmp(&x.min).unwrap_or(std::cmp::Ordering::Equal));
            let codes: Vec<String> = bands.into_iter().map(|b| b.code).collect();
            let v = DataValidation::new()
                .allow_list_strings(&codes)
                .map_err(xerr)?
                .set_error_style(DataValidationErrorStyle::Warning)
                .set_error_message(format!("Isi {} (boleh huruf awalnya saja) atau angka 0–100. Klik Yes kalau isiannya angka.", codes.join(" / ")))
                .map_err(xerr)?;
            ws.add_data_validation(first, c0, last, c0 + 1, &v).map_err(xerr)?;
        }
        TemplateKind::Attendance => {
            let v = DataValidation::new()
                .allow_whole_number(DataValidationRule::Between(0, 366))
                .set_error_style(DataValidationErrorStyle::Warning)
                .set_error_message("Isi jumlah hari (angka bulat 0–366).")
                .map_err(xerr)?;
            ws.add_data_validation(first, c0, last, c0 + 2, &v).map_err(xerr)?;
        }
    }
    ws.set_freeze_panes(HEADER_ROW + 1, 2).map_err(xerr)?;
    ws.autofilter(HEADER_ROW, 0, last, ncols - 1).map_err(xerr)?;

    // petunjuk
    let mut g = Worksheet::new();
    g.set_name("Petunjuk").map_err(xerr)?;
    g.set_column_width(0, 3).map_err(xerr)?;
    g.set_column_width(1, 110).map_err(xerr)?;
    g.write_with_format(1, 1, lay.guide_title, &st.guide_h).map_err(xerr)?;
    for (i, l) in lay.guide.iter().enumerate() {
        g.write_with_format(3 + i as u32, 1, *l, &st.guide).map_err(xerr)?;
    }
    wb.push_worksheet(g);
    Ok(TemplateInfo { students: students.len() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::importer::{read_grid, Grid};
    use crate::model::{ClassConfig, ProjectConfig};
    use calamine::Data;

    fn sample() -> AppData {
        let mut d = AppData::default();
        let mk = |id: &str, name: &str, class: &str, order: u32, nisn: &str| Student {
            id: id.into(),
            order: Some(order),
            name: name.into(),
            nisn: nisn.into(),
            class_name: class.into(),
            ..Default::default()
        };
        d.students = vec![
            mk("a", "Ayu Lestari", "IX-1", 1, "0011111111"),
            mk("b", "Bagus Pratama", "IX-1", 2, "0022222222"),
            mk("c", "Citra Dewi", "IX-2", 1, ""),
        ];
        d.settings.classes = vec![
            ClassConfig { name: "IX-1".into(), ..Default::default() },
            ClassConfig { name: "IX-2".into(), ..Default::default() },
        ];
        d.settings.projects = vec![ProjectConfig { id: "p1".into(), name: "Project 1".into(), theme: "Who We Are".into(), ..Default::default() }];
        d
    }

    fn req(kind: TemplateKind, with_students: bool) -> TemplateRequest {
        TemplateRequest { kind, with_students, classes: vec![], project_id: None }
    }

    fn tmp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("eraport-tpl-{}-{name}.xlsx", std::process::id()))
    }

    fn set(g: &mut Grid, r: usize, c: usize, v: Data) {
        while g.len() <= r {
            g.push(Vec::new());
        }
        while g[r].len() <= c {
            g[r].push(Data::Empty);
        }
        g[r][c] = v;
    }

    fn s(v: &str) -> Data {
        Data::String(v.into())
    }

    /// Tulis grid kembali ke .xlsx (seperti guru menyimpan dari Excel).
    fn save_grid(g: &Grid, sheet: &str, path: &Path) {
        let mut wb = Workbook::new();
        let ws = wb.add_worksheet();
        ws.set_name(sheet).unwrap();
        for (r, row) in g.iter().enumerate() {
            for (c, d) in row.iter().enumerate() {
                match d {
                    Data::String(t) => { ws.write_string(r as u32, c as u16, t).unwrap(); }
                    Data::Float(f) => { ws.write_number(r as u32, c as u16, *f).unwrap(); }
                    Data::Int(i) => { ws.write_number(r as u32, c as u16, *i as f64).unwrap(); }
                    _ => {}
                }
            }
        }
        wb.save(path).unwrap();
    }

    #[test]
    fn all_templates_share_the_same_identity_columns() {
        let d = sample();
        for kind in [TemplateKind::Students, TemplateKind::Scores, TemplateKind::Projects, TemplateKind::Attendance] {
            let p = tmp(&format!("{kind:?}"));
            write_template(&d, &req(kind, false), &p).unwrap();
            let sheet = layout(&d, &req(kind, false)).sheet;
            let g = read_grid(&p, sheet).unwrap();
            let heads: Vec<String> = g[HEADER_ROW as usize].iter().take(5).map(crate::importer::cell_text).collect();
            assert_eq!(heads, ["No", "Nama Siswa", "NIS", "NISN", "Kelas"], "{kind:?}");
            let scans = crate::importer::scan_workbook(&p).unwrap();
            assert_eq!(scans[0].name, sheet, "sheet isian harus yang pertama");
            assert!(scans.iter().any(|s| s.name == "Petunjuk" && s.kind == crate::importer::SheetKind::Unknown));
            let _ = std::fs::remove_file(&p);
        }
    }

    #[test]
    fn student_template_filled_by_a_teacher_imports() {
        let d = sample();
        let p = tmp("siswa");
        write_template(&d, &req(TemplateKind::Students, false), &p).unwrap();
        let mut g = read_grid(&p, "Data siswa").unwrap();
        let r = HEADER_ROW as usize + 1;
        set(&mut g, r, 1, s("Dewi Anggraini"));
        set(&mut g, r, 3, s("0098765401"));
        set(&mut g, r, 4, s("VII-2"));
        set(&mut g, r, 5, s("P"));
        set(&mut g, r + 1, 1, s("Eko Saputra"));
        set(&mut g, r + 1, 2, s("7002"));
        set(&mut g, r + 1, 4, s("VII-2"));
        let rows = crate::importer::parse_students(&g).unwrap();
        let got: Vec<_> = rows.iter().map(|x| (x.name.as_str(), x.nis.as_str(), x.nisn.as_str(), x.gender.as_str(), x.class_name.as_str())).collect();
        assert_eq!(got, [("Dewi Anggraini", "", "0098765401", "P", "VII-2"), ("Eko Saputra", "7002", "", "", "VII-2")]);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn blank_score_template_one_subject_per_file_imports() {
        let mut d = sample();
        let p = tmp("nilai");
        write_template(&d, &req(TemplateKind::Scores, false), &p).unwrap();
        // guru menulis mapel, nama (beda kapital), NISN, kelas, dan nilai
        let mut g = read_grid(&p, "Nilai mapel").unwrap();
        set(&mut g, 1, 2, s("Science (IPA)"));
        let r = HEADER_ROW as usize + 1;
        set(&mut g, r, 1, s("AYU LESTARI"));
        set(&mut g, r, 3, s("0011111111"));
        set(&mut g, r, 4, s("IX-1"));
        set(&mut g, r, 5, Data::Float(88.0));
        set(&mut g, r, 6, Data::Float(91.5));
        set(&mut g, r + 1, 1, s("Citra Dewi"));
        set(&mut g, r + 1, 4, s("IX-2"));
        set(&mut g, r + 1, 5, Data::Float(95.0));
        save_grid(&g, "Nilai mapel", &p);
        let pv = crate::score_import::inspect(&d, &p, "Nilai mapel", None).unwrap();
        let m = &pv.mapping;
        assert_eq!((m.name, m.nis, m.nisn, m.class), (Some(1), Some(2), Some(3), Some(4)));
        let ipa = d.settings.subjects.iter().find(|x| x.name.contains("IPA")).unwrap().source_key.clone();
        let parts: Vec<_> = m.scores.iter().map(|c| (c.col, c.subject.as_str(), c.part.as_str())).collect();
        assert_eq!(parts, [(5, ipa.as_str(), "ph"), (6, ipa.as_str(), "pts")]);
        let rep = crate::score_import::import(&mut d, &p, "Nilai mapel", m, &["IX-1".into(), "IX-2".into()]).unwrap();
        assert_eq!((rep.matched, rep.values_set), (2, 3));
        let a = d.students.iter().find(|s| s.id == "a").unwrap();
        let sc = a.scores.values().next().unwrap();
        assert_eq!((sc.ph, sc.pts), (Some(88.0), Some(91.5)));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn unknown_subject_name_is_kept_as_written() {
        let d = sample();
        let p = tmp("nilai-baru");
        write_template(&d, &req(TemplateKind::Scores, false), &p).unwrap();
        let mut g = read_grid(&p, "Nilai mapel").unwrap();
        set(&mut g, 1, 2, s("Seni Tari"));
        let r = HEADER_ROW as usize + 1;
        set(&mut g, r, 1, s("Ayu Lestari"));
        set(&mut g, r, 4, s("IX-1"));
        set(&mut g, r, 5, Data::Float(90.0));
        save_grid(&g, "Nilai mapel", &p);
        let pv = crate::score_import::inspect(&d, &p, "Nilai mapel", None).unwrap();
        assert!(pv.mapping.scores.iter().all(|c| c.subject == "Seni Tari"), "{:?}", pv.mapping.scores);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn indonesian_subject_names_match_the_settings() {
        let d = sample();
        let p = tmp("nilai-alias");
        write_template(&d, &req(TemplateKind::Scores, false), &p).unwrap();
        let mut g = read_grid(&p, "Nilai mapel").unwrap();
        set(&mut g, 1, 2, s("Bahasa Inggris"));
        let r = HEADER_ROW as usize + 1;
        set(&mut g, r, 1, s("Ayu Lestari"));
        set(&mut g, r, 4, s("IX-1"));
        set(&mut g, r, 5, Data::Float(90.0));
        save_grid(&g, "Nilai mapel", &p);
        let pv = crate::score_import::inspect(&d, &p, "Nilai mapel", None).unwrap();
        let english = d.settings.subjects.iter().find(|x| x.name.contains("English")).unwrap().source_key.clone();
        assert!(pv.mapping.scores.iter().all(|c| c.subject == english), "{:?}", pv.mapping.scores);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn score_template_from_the_app_lists_students_with_nisn() {
        let d = sample();
        let p = tmp("nilai-isi");
        let info = write_template(&d, &req(TemplateKind::Scores, true), &p).unwrap();
        assert_eq!(info.students, 3);
        let g = read_grid(&p, "Nilai mapel").unwrap();
        let row = |r: usize| (1..5).map(|c| crate::importer::cell_text(&g[r][c])).collect::<Vec<_>>();
        let r = HEADER_ROW as usize + 1;
        assert_eq!(row(r), ["Ayu Lestari", "", "0011111111", "IX-1"]);
        assert_eq!(row(r + 2), ["Citra Dewi", "", "", "IX-2"]);
        assert_eq!(crate::importer::cell_text(&g[r + 2][0]), "1", "nomor mulai lagi di kelas berikutnya");
        assert!(suggested_file_name(&d, &req(TemplateKind::Scores, true)).contains("IX-1, IX-2"));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn project_template_is_guessed_as_the_right_project() {
        let d = sample();
        let p = tmp("proyek");
        let mut r = req(TemplateKind::Projects, true);
        r.project_id = Some("p1".into());
        r.classes = vec!["IX-1".into()];
        assert_eq!(write_template(&d, &r, &p).unwrap().students, 2);
        let pv = crate::project_import::inspect(&d, &p, "Nilai proyek", None).unwrap();
        assert_eq!(pv.guessed_project.as_deref(), Some("p1"));
        let m = &pv.mapping;
        assert_eq!((m.name, m.class, m.understanding, m.inquiry, m.narrative), (Some(1), Some(4), Some(5), Some(6), Some(7)));
        // template kosong: guru menulis sendiri nama proyeknya
        write_template(&d, &req(TemplateKind::Projects, false), &p).unwrap();
        let mut g = read_grid(&p, "Nilai proyek").unwrap();
        set(&mut g, 1, 2, s("Project 1"));
        let r = HEADER_ROW as usize + 1;
        set(&mut g, r, 1, s("Ayu Lestari"));
        set(&mut g, r, 4, s("IX-1"));
        set(&mut g, r, 5, s("E"));
        save_grid(&g, "Nilai proyek", &p);
        let pv = crate::project_import::inspect(&d, &p, "Nilai proyek", None).unwrap();
        assert_eq!(pv.guessed_project.as_deref(), Some("p1"));
        assert_eq!(pv.rows[0].understanding, "EE");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn attendance_template_reads_days_per_student() {
        let mut d = sample();
        let p = tmp("absensi");
        write_template(&d, &req(TemplateKind::Attendance, true), &p).unwrap();
        let scans = crate::importer::scan_workbook(&p).unwrap();
        assert_eq!(scans[0].kind, crate::importer::SheetKind::Attendance);
        let mut g = read_grid(&p, "Absensi").unwrap();
        let r = HEADER_ROW as usize + 1;
        set(&mut g, r, 5, Data::Float(2.0));
        set(&mut g, r, 6, Data::Float(1.0));
        let sheet = crate::importer::parse_attendance(&g, "Absensi").unwrap();
        let rep = crate::importer::apply_attendance(&mut d, sheet, &["IX-1".into(), "IX-2".into()], 100, true);
        assert_eq!(rep.matched, 3);
        assert_eq!(d.students.iter().find(|s| s.id == "a").unwrap().attendance, "97% (Present)");
        let _ = std::fs::remove_file(&p);
    }

    /// Membuat file template baku (kosong) untuk dibagikan:
    /// `ERAPORT_TEMPLATE_DIR=<folder> cargo test --lib generate_standard_templates -- --ignored`
    #[test]
    #[ignore]
    fn generate_standard_templates() {
        let dir = std::env::var("ERAPORT_TEMPLATE_DIR").expect("ERAPORT_TEMPLATE_DIR");
        std::fs::create_dir_all(&dir).unwrap();
        let d = AppData::default();
        for kind in [TemplateKind::Students, TemplateKind::Scores, TemplateKind::Projects, TemplateKind::Attendance] {
            write_template(&d, &req(kind, false), &Path::new(&dir).join(standard_file_name(kind))).unwrap();
        }
    }
}
