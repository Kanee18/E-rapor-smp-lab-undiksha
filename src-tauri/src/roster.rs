//! Daftar siswa yang dibuat/diubah langsung di aplikasi (tanpa file Excel),
//! dan ekspor daftar siswa ke Excel yang bisa diimpor balik (di laptop lain / jenjang lain).
//!
//! Satu "daftar kelas" = semua siswa satu kelas, berurutan. Urutan baris di
//! layar menjadi nomor absen (`Student::order`). Nilai & isian rapor siswa yang
//! sudah ada tidak disentuh; yang berubah hanya identitas dan urutannya.

use crate::importer::{ensure_class_configs, new_student_id, normalize_class};
use crate::model::{AppData, Student};
use crate::student_flex::normalize_gender;
use rust_xlsxwriter::{Color, Format, FormatBorder, Workbook, XlsxError};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RosterRow {
    /// ID siswa yang sudah ada; kosong = siswa baru.
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub nis: String,
    #[serde(default)]
    pub nisn: String,
    #[serde(default)]
    pub gender: String,
    /// Jumlah hari Sakit/Izin/Alpa. `None` = tidak diubah.
    #[serde(default)]
    pub attendance: Option<Sia>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq)]
pub struct Sia {
    #[serde(default)]
    pub s: Option<u32>,
    #[serde(default)]
    pub i: Option<u32>,
    #[serde(default)]
    pub a: Option<u32>,
}

#[derive(Debug, Default, Serialize, PartialEq)]
pub struct RosterResult {
    pub class_name: String,
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    /// ID siswa sesuai urutan baris yang disimpan (siswa baru mendapat ID di sini).
    pub ids: Vec<String>,
}

fn clean(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Simpan daftar satu kelas.
/// - `original_class`: kelas yang sedang diedit (None = kelas baru).
/// - `rows`: semua siswa kelas itu, sesuai urutan di layar.
/// - `remove`: ID siswa lama yang dihapus dari daftar (nilai & isiannya ikut terhapus).
pub fn apply_roster(
    data: &mut AppData,
    original_class: Option<&str>,
    class_name: &str,
    rows: Vec<RosterRow>,
    remove: &[String],
) -> Result<RosterResult, String> {
    let canon = |c: &str| crate::student_flex::canonical_class(c).unwrap_or_else(|| normalize_class(c));
    let class_name = canon(class_name);
    if class_name.is_empty() {
        return Err("Nama kelas wajib diisi, mis. VII-1".into());
    }
    let original = original_class.map(|c| c.trim().to_string()).filter(|c| !c.is_empty());

    // ---- periksa isi baris
    let mut rows: Vec<RosterRow> = rows
        .into_iter()
        .map(|r| RosterRow {
            id: r.id.filter(|i| !i.trim().is_empty()),
            name: clean(&r.name),
            nis: r.nis.trim().to_string(),
            nisn: r.nisn.trim().to_string(),
            gender: normalize_gender(&r.gender),
            attendance: r.attendance,
        })
        .collect();
    // baris yang benar-benar kosong (siswa baru tanpa isi) diabaikan
    rows.retain(|r| r.id.is_some() || !(r.name.is_empty() && r.nis.is_empty() && r.nisn.is_empty()));
    if rows.is_empty() {
        return Err("Daftar masih kosong. Isi minimal satu nama siswa.".into());
    }
    let mut seen_nis: HashMap<&str, usize> = HashMap::new();
    let mut seen_nisn: HashMap<&str, usize> = HashMap::new();
    for (i, r) in rows.iter().enumerate() {
        let no = i + 1;
        if r.name.is_empty() {
            return Err(format!("Baris {no}: nama siswa belum diisi"));
        }
        if let Some(sia) = r.attendance {
            if [sia.s, sia.i, sia.a].into_iter().flatten().any(|d| d > 366) {
                return Err(format!("Baris {no} ({}): jumlah hari S/I/A maksimal 366", r.name));
            }
        }
        if !r.nisn.is_empty() && !(r.nisn.len() == 10 && r.nisn.chars().all(|c| c.is_ascii_digit())) {
            return Err(format!("Baris {no} ({}): NISN harus 10 angka, atau kosongkan saja", r.name));
        }
        if !r.nis.is_empty() {
            if let Some(prev) = seen_nis.insert(r.nis.as_str(), no) {
                return Err(format!("NIS {} dipakai dua kali (baris {prev} dan {no})", r.nis));
            }
        }
        if !r.nisn.is_empty() {
            if let Some(prev) = seen_nisn.insert(r.nisn.as_str(), no) {
                return Err(format!("NISN {} dipakai dua kali (baris {prev} dan {no})", r.nisn));
            }
        }
    }

    // ---- ID harus ada, tidak dobel, dan tidak sekaligus dihapus
    let mut row_ids: HashSet<&str> = HashSet::new();
    for r in &rows {
        if let Some(id) = r.id.as_deref() {
            if !data.students.iter().any(|s| s.id == id) {
                return Err("Ada siswa yang sudah dihapus dari layar lain. Tutup lalu buka lagi daftar ini.".into());
            }
            if !row_ids.insert(id) {
                return Err("Ada siswa yang muncul dua kali di daftar".into());
            }
        }
    }
    let remove: HashSet<&str> = remove.iter().map(|s| s.as_str()).filter(|id| !row_ids.contains(id)).collect();

    // ---- bentrok dengan siswa lain (di luar daftar ini)
    let others = || data.students.iter().filter(|s| !row_ids.contains(s.id.as_str()) && !remove.contains(s.id.as_str()));
    if original.as_deref() != Some(class_name.as_str()) {
        if let Some(s) = others().find(|s| s.class_name == class_name) {
            return Err(format!(
                "Kelas {class_name} sudah ada (mis. {}). Buka kelas itu lewat \"Edit daftar\" untuk menambah siswanya.",
                s.name
            ));
        }
    }
    for r in rows.iter().filter(|r| !r.nisn.is_empty()) {
        if let Some(s) = others().find(|s| s.nisn == r.nisn) {
            return Err(format!("NISN {} sudah dipakai {} (kelas {})", r.nisn, s.name, s.class_name));
        }
    }
    if let Some(o) = original.as_deref() {
        if let Some(s) = others().find(|s| s.class_name == o) {
            // daftar dari layar tidak lengkap (siswa ditambah dari layar lain sementara ini dibuka)
            return Err(format!("{} (kelas {o}) belum ada di daftar ini. Tutup lalu buka lagi daftarnya.", s.name));
        }
    }

    // ---- terapkan
    let mut result = RosterResult { class_name: class_name.clone(), ..Default::default() };
    if !remove.is_empty() {
        let before = data.students.len();
        data.students.retain(|s| !remove.contains(s.id.as_str()));
        result.removed = before - data.students.len();
    }
    let effective_days = data.settings.effective_days;
    let set_sia = |s: &mut Student, sia: Option<Sia>| {
        let Some(sia) = sia else { return };
        s.attendance_s = sia.s;
        s.attendance_i = sia.i;
        s.attendance_a = sia.a;
        if let Some(text) = crate::model::format_attendance_sia(sia.s, sia.i, sia.a, effective_days) {
            s.attendance = text;
        }
    };
    for (i, r) in rows.into_iter().enumerate() {
        let order = Some(i as u32 + 1);
        match r.id {
            Some(id) => {
                let s = data.students.iter_mut().find(|s| s.id == id).expect("sudah diperiksa");
                set_sia(s, r.attendance);
                result.ids.push(id);
                s.name = r.name;
                s.nis = r.nis;
                s.nisn = r.nisn;
                s.gender = r.gender;
                s.class_name = class_name.clone();
                s.order = order;
                result.updated += 1;
            }
            None => {
                let preferred = if r.nis.is_empty() { r.nisn.clone() } else { r.nis.clone() };
                let id = new_student_id(data, &preferred);
                result.ids.push(id.clone());
                let mut s = Student {
                    id,
                    order,
                    name: r.name,
                    nis: r.nis,
                    nisn: r.nisn,
                    gender: r.gender,
                    class_name: class_name.clone(),
                    ..Default::default()
                };
                set_sia(&mut s, r.attendance);
                data.students.push(s);
                result.added += 1;
            }
        }
    }

    // ---- kelas diganti namanya: wali kelas & tanda tangan ikut pindah
    if let Some(o) = original.filter(|o| *o != class_name) {
        if !data.students.iter().any(|s| s.class_name == o) {
            let target_exists = data.settings.classes.iter().any(|c| c.name == class_name);
            if target_exists {
                data.settings.classes.retain(|c| c.name != o);
            } else if let Some(cfg) = data.settings.classes.iter_mut().find(|c| c.name == o) {
                cfg.name = class_name.clone();
            }
        }
    }
    ensure_class_configs(data);
    Ok(result)
}

// ---------------------------------------------------------------- ekspor Excel

fn xerr(e: XlsxError) -> String {
    format!("Gagal membuat file Excel: {e}")
}

pub fn suggested_file_name(data: &AppData, classes: &[String], level: &str) -> String {
    let what = if classes.is_empty() { format!("semua kelas {level}") } else { classes.join(", ") };
    let name = format!("Data siswa {} - {what}.xlsx", data.settings.academic_year.trim());
    name.chars().map(|c| if r#"\/:*?"<>|"#.contains(c) { '-' } else { c }).collect()
}

/// Tulis daftar siswa (kolom No, Nama, NIS, NISN, L/P, Kelas) ke Excel.
/// Susunannya sama dengan format yang dikenali "Impor dari Excel", jadi file ini
/// bisa diimpor balik. `classes` kosong = semua kelas.
pub fn write_students(data: &AppData, classes: &[String], level: &str, path: &Path) -> Result<usize, String> {
    let mut list: Vec<&Student> = data
        .students
        .iter()
        .filter(|s| classes.is_empty() || classes.contains(&s.class_name))
        .collect();
    if list.is_empty() {
        return Err("Tidak ada siswa untuk diekspor".into());
    }
    list.sort_by(|a, b| crate::model::compare_students(a, b));

    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    ws.set_name("Data siswa").map_err(xerr)?;
    let title = Format::new().set_bold().set_font_size(14);
    let muted = Format::new().set_font_color(Color::RGB(0x6B7280));
    let head = Format::new()
        .set_bold()
        .set_font_color(Color::White)
        .set_background_color(Color::RGB(0x3653D3))
        .set_border(FormatBorder::Thin)
        .set_border_color(Color::RGB(0xD9D4CA));
    let body = Format::new().set_border(FormatBorder::Thin).set_border_color(Color::RGB(0xD9D4CA));
    // NIS/NISN disimpan sebagai TEKS supaya nol di depan tidak hilang
    let text = body.clone().set_num_format("@");

    let st = &data.settings;
    let school = if st.school_name.trim().is_empty() { level.to_string() } else { st.school_name.trim().to_string() };
    ws.write_with_format(0, 0, format!("Data siswa {school}"), &title).map_err(xerr)?;
    ws.write_with_format(
        1,
        0,
        format!("Tahun ajaran {} · dibuat dari aplikasi E-Raport, bisa diimpor lagi lewat Data siswa → Impor dari Excel", st.academic_year.trim()),
        &muted,
    )
    .map_err(xerr)?;

    let hr = 3u32;
    for (c, (t, w)) in [("No", 6.0), ("Nama", 38.0), ("NIS", 14.0), ("NISN", 15.0), ("L/P", 6.0), ("Kelas", 10.0)]
        .into_iter()
        .enumerate()
    {
        ws.write_with_format(hr, c as u16, t, &head).map_err(xerr)?;
        ws.set_column_width(c as u16, w).map_err(xerr)?;
    }
    let mut no_in_class = 0u32;
    let mut prev_class = "";
    for (i, s) in list.iter().enumerate() {
        if s.class_name != prev_class {
            prev_class = &s.class_name;
            no_in_class = 0;
        }
        no_in_class += 1;
        let r = hr + 1 + i as u32;
        ws.write_with_format(r, 0, no_in_class, &body).map_err(xerr)?;
        ws.write_with_format(r, 1, &s.name, &body).map_err(xerr)?;
        ws.write_with_format(r, 2, &s.nis, &text).map_err(xerr)?;
        ws.write_with_format(r, 3, &s.nisn, &text).map_err(xerr)?;
        ws.write_with_format(r, 4, &s.gender, &body).map_err(xerr)?;
        ws.write_with_format(r, 5, &s.class_name, &body).map_err(xerr)?;
    }
    ws.set_freeze_panes(hr + 1, 0).map_err(xerr)?;
    ws.autofilter(hr, 0, hr + list.len() as u32, 5).map_err(xerr)?;
    wb.save(path)
        .map_err(|e| format!("File tidak bisa disimpan (mungkin sedang dibuka di Excel?): {e}"))?;
    Ok(list.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ClassConfig, ScoreEntry};

    fn row(id: Option<&str>, name: &str, nis: &str, nisn: &str, g: &str) -> RosterRow {
        RosterRow { id: id.map(Into::into), name: name.into(), nis: nis.into(), nisn: nisn.into(), gender: g.into(), attendance: None }
    }

    fn sample() -> AppData {
        let mut d = AppData::default();
        let mut a = Student { id: "a".into(), order: Some(1), name: "Ayu".into(), nis: "101".into(), class_name: "VII-1".into(), ..Default::default() };
        a.scores.insert("IPA".into(), ScoreEntry { ph: Some(90.0), pts: Some(92.0), manual: None });
        let b = Student { id: "b".into(), order: Some(2), name: "Bagus".into(), nis: "102".into(), nisn: "0012345678".into(), class_name: "VII-1".into(), ..Default::default() };
        let c = Student { id: "c".into(), order: Some(1), name: "Citra".into(), nis: "201".into(), class_name: "VII-2".into(), ..Default::default() };
        d.students = vec![a, b, c];
        d.settings.classes = vec![
            ClassConfig { name: "VII-1".into(), homeroom_teacher: "Bu Sari".into(), ..Default::default() },
            ClassConfig { name: "VII-2".into(), ..Default::default() },
        ];
        d
    }

    #[test]
    fn new_class_made_in_the_app_keeps_typed_order() {
        let mut d = AppData::default();
        let rows = vec![
            row(None, "  Zaki   Pratama ", "", "", "laki-laki"),
            row(None, "", "", "", ""), // baris kosong diabaikan
            row(None, "Ani", "7", "", "P"),
        ];
        let r = apply_roster(&mut d, None, "kelas 1c", rows, &[]).unwrap();
        assert_eq!((r.class_name.as_str(), r.added, r.updated, r.removed), ("1-C", 2, 0, 0));
        assert_eq!(r.ids, d.students.iter().map(|s| s.id.clone()).collect::<Vec<_>>());
        let names: Vec<_> = d.students.iter().map(|s| (s.name.as_str(), s.order, s.gender.as_str())).collect();
        assert_eq!(names, [("Zaki Pratama", Some(1), "L"), ("Ani", Some(2), "P")]);
        assert!(d.settings.classes.iter().any(|c| c.name == "1-C"));
    }

    #[test]
    fn editing_reorders_keeps_scores_and_removes_only_chosen() {
        let mut d = sample();
        let rows = vec![row(Some("b"), "Bagus", "102", "0012345678", "L"), row(Some("a"), "Ayu Lestari", "101", "", "P"), row(None, "Dewi", "103", "", "P")];
        let r = apply_roster(&mut d, Some("VII-1"), "VII-1", rows, &[]).unwrap();
        assert_eq!((r.added, r.updated, r.removed), (1, 2, 0));
        let a = d.students.iter().find(|s| s.id == "a").unwrap();
        assert_eq!((a.name.as_str(), a.order), ("Ayu Lestari", Some(2)));
        assert!(a.scores.contains_key("IPA"), "nilai tidak boleh hilang");

        let rows = vec![row(Some("a"), "Ayu Lestari", "101", "", "P")];
        let r = apply_roster(&mut d, Some("VII-1"), "VII-1", rows, &["b".into(), "103".into()]).unwrap();
        assert_eq!(r.removed, 2);
        assert!(d.students.iter().any(|s| s.id == "c"), "kelas lain tidak tersentuh");
    }

    #[test]
    fn attendance_typed_in_the_sheet_is_saved_and_converted() {
        let mut d = sample();
        d.settings.effective_days = 100;
        let mut ayu = row(Some("a"), "Ayu", "101", "", "P");
        ayu.attendance = Some(Sia { s: Some(2), i: Some(1), a: None });
        let mut baru = row(None, "Dewi", "", "", "P");
        baru.attendance = Some(Sia { s: None, i: None, a: Some(4) });
        let bagus = row(Some("b"), "Bagus", "102", "0012345678", "L"); // S/I/A tidak dikirim → tidak diubah
        let r = apply_roster(&mut d, Some("VII-1"), "VII-1", vec![ayu, bagus, baru], &[]).unwrap();
        assert_eq!(r.ids[..2], ["a".to_string(), "b".to_string()]);
        let get = |id: &str| d.students.iter().find(|s| s.id == id).unwrap();
        assert_eq!((get("a").attendance_s, get("a").attendance_i, get("a").attendance.as_str()), (Some(2), Some(1), "97% (Present)"));
        assert_eq!(get(&r.ids[2]).attendance, "96% (Present)");
        assert_eq!(get("b").attendance_s, None);

        let mut bad = row(Some("a"), "Ayu", "101", "", "P");
        bad.attendance = Some(Sia { s: Some(400), i: None, a: None });
        let b = row(Some("b"), "Bagus", "102", "0012345678", "L");
        assert!(apply_roster(&mut d, Some("VII-1"), "VII-1", vec![bad, b], &[]).unwrap_err().contains("366"));
    }

    #[test]
    fn renaming_a_class_moves_homeroom_settings() {
        let mut d = sample();
        let rows = vec![row(Some("a"), "Ayu", "101", "", ""), row(Some("b"), "Bagus", "102", "0012345678", "")];
        apply_roster(&mut d, Some("VII-1"), "VII-3", rows, &[]).unwrap();
        let cfg = d.settings.classes.iter().find(|c| c.name == "VII-3").unwrap();
        assert_eq!(cfg.homeroom_teacher, "Bu Sari");
        assert!(!d.settings.classes.iter().any(|c| c.name == "VII-1"));
    }

    #[test]
    fn rejects_mistakes_before_changing_anything() {
        let mut d = sample();
        let before = serde_json::to_string(&d.students).unwrap();
        let cases: Vec<(Option<&str>, &str, Vec<RosterRow>, &str)> = vec![
            (None, "VII-2", vec![row(None, "X", "", "", "")], "sudah ada"),
            (None, "VII-4", vec![row(None, "X", "", "123", "")], "10 angka"),
            (None, "VII-4", vec![row(None, "X", "9", "", ""), row(None, "Y", "9", "", "")], "dua kali"),
            (None, "VII-4", vec![row(None, "X", "", "0012345678", "")], "Bagus"),
            (None, "VII-4", vec![row(None, "", "55", "", "")], "nama"),
            (Some("VII-1"), "VII-1", vec![row(Some("a"), "Ayu", "", "", "")], "Bagus"), // Bagus tidak dikirim & tidak dihapus
            (None, "", vec![row(None, "X", "", "", "")], "kelas"),
        ];
        for (orig, class, rows, needle) in cases {
            let err = apply_roster(&mut d, orig, class, rows, &[]).unwrap_err();
            assert!(err.contains(needle), "{err} harus menyebut {needle}");
            assert_eq!(serde_json::to_string(&d.students).unwrap(), before);
        }
    }

    #[test]
    fn exported_file_imports_back_in_the_same_order() {
        let mut d = sample();
        d.students[1].order = Some(0); // Bagus di depan Ayu
        let path = std::env::temp_dir().join(format!("eraport-roster-{}.xlsx", std::process::id()));
        assert_eq!(write_students(&d, &[], "SMP", &path).unwrap(), 3);
        let scans = crate::importer::scan_workbook(&path).unwrap();
        assert_eq!(scans[0].kind, crate::importer::SheetKind::Students);
        let rows = crate::importer::read_students(&path, "Data siswa").unwrap();
        let got: Vec<_> = rows.iter().map(|r| (r.name.as_str(), r.nis.as_str(), r.nisn.as_str(), r.class_name.as_str())).collect();
        assert_eq!(got, [("Bagus", "102", "0012345678", "VII-1"), ("Ayu", "101", "", "VII-1"), ("Citra", "201", "", "VII-2")]);

        // impor ke laptop lain (data kosong): urutan & identitas sama
        let mut other = AppData::default();
        crate::importer::apply_students(&mut other, rows, &["VII-1".into(), "VII-2".into()]);
        let mut list: Vec<_> = other.students.iter().collect();
        list.sort_by(|a, b| crate::model::compare_students(a, b));
        assert_eq!(list.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Bagus", "Ayu", "Citra"]);
        let _ = std::fs::remove_file(&path);
    }
}
