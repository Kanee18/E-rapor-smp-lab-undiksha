//! Leger kelas ke Excel: rekap nilai & predikat semua siswa per kelas,
//! plus sheet "Rekap" berisi sebaran predikat per mapel.
//!
//! Angka diambil dari `view::student_view`, yaitu perhitungan yang sama dengan
//! rapor, jadi leger dan rapor selalu cocok.

use crate::grading::{Resolved, Status};
use crate::importer::normalize_class;
use crate::model::{class_sort_key, AppData};
use crate::view::{student_view, StudentView};
use rust_xlsxwriter::{Color, Format, FormatAlign, FormatBorder, Workbook, Worksheet, XlsxError};
use std::collections::BTreeMap;
use std::path::Path;

const NAVY: u32 = 0x1A365D;
const RULE: u32 = 0xD9D4CA;
const RANK_COLORS: [u32; 4] = [0x1F6B47, 0x2A5DA8, 0x8F6408, 0xA23B2C];
const BELOW_FILL: u32 = 0xFBE3DF;

fn xerr(e: XlsxError) -> String {
    format!("Gagal membuat leger: {e}")
}

/// Nama sheet Excel maks. 31 karakter dan tanpa []:*?/\
fn sheet_name(s: &str) -> String {
    let t: String = s.chars().filter(|c| !"[]:*?/\\".contains(*c)).take(31).collect();
    if t.is_empty() {
        "Kelas".into()
    } else {
        t
    }
}

pub fn suggested_file_name(data: &AppData, classes: &[String]) -> String {
    let st = &data.settings;
    let name = format!("Leger {} {} - {}.xlsx", st.report_subtitle.trim(), st.academic_year.trim(), classes.join(", "));
    name.chars().map(|c| if r#"\/:*?"<>|"#.contains(c) { '-' } else { c }).collect()
}

struct Formats {
    title: Format,
    muted: Format,
    header: Format,
    cell: Format,
    num: Format,
    marks: Vec<Format>,
    marks_below: Vec<Format>,
    ok: Format,
    warn: Format,
}

fn formats() -> Formats {
    let border = |f: Format| f.set_border(FormatBorder::Thin).set_border_color(Color::RGB(RULE));
    let mark = |color: u32| border(Format::new().set_bold().set_align(FormatAlign::Center).set_font_color(Color::RGB(color)));
    Formats {
        title: Format::new().set_bold().set_font_size(14).set_font_color(Color::RGB(NAVY)),
        muted: Format::new().set_font_color(Color::RGB(0x5E6672)),
        header: border(
            Format::new()
                .set_bold()
                .set_text_wrap()
                .set_align(FormatAlign::Center)
                .set_align(FormatAlign::VerticalCenter)
                .set_font_color(Color::White)
                .set_background_color(Color::RGB(NAVY)),
        ),
        cell: border(Format::new()),
        num: border(Format::new().set_num_format("0.##").set_align(FormatAlign::Right)),
        marks: RANK_COLORS.iter().map(|c| mark(*c)).collect(),
        marks_below: RANK_COLORS.iter().map(|c| mark(*c).set_background_color(Color::RGB(BELOW_FILL))).collect(),
        ok: border(Format::new().set_font_color(Color::RGB(RANK_COLORS[0]))),
        warn: border(Format::new().set_font_color(Color::RGB(RANK_COLORS[2]))),
    }
}

/// Peringkat predikat (0 = tertinggi), dari rentang di Pengaturan.
fn ranks(data: &AppData) -> BTreeMap<String, usize> {
    let mut bands = data.settings.grading.bands.clone();
    bands.sort_by(|a, b| b.min.partial_cmp(&a.min).unwrap_or(std::cmp::Ordering::Equal));
    bands.into_iter().enumerate().map(|(i, b)| (b.code, i)).collect()
}

fn write_mark(
    ws: &mut Worksheet,
    r: u32,
    c: u16,
    res: &Resolved,
    f: &Formats,
    ranks: &BTreeMap<String, usize>,
) -> Result<(), XlsxError> {
    match &res.code {
        Some(code) => {
            let i = ranks.get(code).copied().unwrap_or(3).min(3);
            let fmt = if res.status == Status::BelowScale { &f.marks_below[i] } else { &f.marks[i] };
            ws.write_string_with_format(r, c, code, fmt)?;
        }
        None => {
            ws.write_blank(r, c, &f.cell)?;
        }
    }
    Ok(())
}

fn write_opt_num(ws: &mut Worksheet, r: u32, c: u16, v: Option<f64>, f: &Formats) -> Result<(), XlsxError> {
    match v {
        Some(x) => ws.write_number_with_format(r, c, x, &f.num).map(|_| ()),
        None => ws.write_blank(r, c, &f.cell).map(|_| ()),
    }
}

fn class_sheet(data: &AppData, class: &str, views: &[StudentView], f: &Formats) -> Result<Worksheet, XlsxError> {
    let st = &data.settings;
    let rk = ranks(data);
    let mut ws = Worksheet::new();
    ws.set_name(sheet_name(class))?;
    let homeroom = st.classes.iter().find(|c| c.name == class).map(|c| c.homeroom_teacher.clone()).unwrap_or_default();
    ws.write_string_with_format(0, 0, format!("Leger kelas {class}"), &f.title)?;
    ws.write_string_with_format(
        1,
        0,
        format!(
            "{} · {} · tahun ajaran {}{}",
            st.school_name,
            st.report_subtitle,
            st.academic_year,
            if homeroom.trim().is_empty() { String::new() } else { format!(" · wali kelas {homeroom}") }
        ),
        &f.muted,
    )?;

    // proyek yang dipakai siswa di kelas ini (urut sesuai Pengaturan)
    let projects: Vec<&crate::model::ProjectConfig> = st
        .projects
        .iter()
        .filter(|p| views.iter().any(|v| v.student.projects.iter().any(|e| e.project_id == p.id)))
        .collect();

    // dua baris judul: grup (baris 3) dan kolom (baris 4)
    let (gr, hr) = (3u32, 4u32);
    let mut col: u16 = 0;
    let fixed = [("No", 5.0), ("NIS", 9.0), ("NISN", 12.0), ("Nama siswa", 32.0), ("L/P", 5.0)];
    for (label, w) in fixed {
        ws.merge_range(gr, col, hr, col, label, &f.header)?;
        ws.set_column_width(col, w)?;
        col += 1;
    }
    for s in &st.subjects {
        ws.merge_range(gr, col, gr, col + 3, &s.name, &f.header)?;
        for (i, (label, w)) in [("PH", 7.0), ("PTS", 7.0), ("Akhir", 7.0), ("Pred.", 7.0)].into_iter().enumerate() {
            ws.write_string_with_format(hr, col + i as u16, label, &f.header)?;
            ws.set_column_width(col + i as u16, w)?;
        }
        col += 4;
    }
    if !st.atl.is_empty() {
        let n = st.atl.len() as u16;
        if n > 1 {
            ws.merge_range(gr, col, gr, col + n - 1, "ATL & Learner Profile", &f.header)?;
        } else {
            ws.write_string_with_format(gr, col, "ATL", &f.header)?;
        }
        for (i, a) in st.atl.iter().enumerate() {
            ws.write_string_with_format(hr, col + i as u16, &a.name, &f.header)?;
            ws.set_column_width(col + i as u16, 11.0)?;
        }
        col += n;
    }
    for p in &projects {
        ws.merge_range(gr, col, gr, col + 1, &p.name, &f.header)?;
        ws.write_string_with_format(hr, col, "Underst.", &f.header)?;
        ws.write_string_with_format(hr, col + 1, "Inquiry", &f.header)?;
        ws.set_column_width(col, 9.0)?;
        ws.set_column_width(col + 1, 9.0)?;
        col += 2;
    }
    for (label, w) in [("Kehadiran", 14.0), ("Status rapor", 34.0)] {
        ws.merge_range(gr, col, hr, col, label, &f.header)?;
        ws.set_column_width(col, w)?;
        col += 1;
    }
    ws.set_row_height(hr, 30)?;

    for (i, v) in views.iter().enumerate() {
        let r = hr + 1 + i as u32;
        let s = &v.student;
        let mut c: u16 = 0;
        ws.write_number_with_format(r, c, (i + 1) as f64, &f.cell)?;
        ws.write_string_with_format(r, c + 1, &s.nis, &f.cell)?;
        ws.write_string_with_format(r, c + 2, &s.nisn, &f.cell)?;
        ws.write_string_with_format(r, c + 3, &s.name, &f.cell)?;
        ws.write_string_with_format(r, c + 4, &s.gender, &f.cell)?;
        c += 5;
        for sv in &v.subjects {
            write_opt_num(&mut ws, r, c, sv.ph, f)?;
            write_opt_num(&mut ws, r, c + 1, sv.pts, f)?;
            write_opt_num(&mut ws, r, c + 2, sv.result.score, f)?;
            write_mark(&mut ws, r, c + 3, &sv.result, f, &rk)?;
            c += 4;
        }
        for a in &v.atl {
            write_mark(&mut ws, r, c, &a.result, f, &rk)?;
            c += 1;
        }
        for p in &projects {
            match v.projects.iter().find(|e| e.project_id == p.id) {
                Some(e) => {
                    write_mark(&mut ws, r, c, &e.understanding, f, &rk)?;
                    write_mark(&mut ws, r, c + 1, &e.inquiry, f, &rk)?;
                }
                None => {
                    ws.write_blank(r, c, &f.cell)?;
                    ws.write_blank(r, c + 1, &f.cell)?;
                }
            }
            c += 2;
        }
        ws.write_string_with_format(r, c, &s.attendance, &f.cell)?;
        if v.issues.is_empty() {
            ws.write_string_with_format(r, c + 1, "Lengkap", &f.ok)?;
        } else {
            ws.write_string_with_format(r, c + 1, format!("{} catatan: {}", v.issues.len(), v.issues.join("; ")), &f.warn)?;
        }
    }
    ws.set_freeze_panes(hr + 1, 4)?;
    ws.set_landscape();
    Ok(ws)
}

fn summary_sheet(data: &AppData, by_class: &[(String, Vec<StudentView>)], f: &Formats) -> Result<Worksheet, XlsxError> {
    let st = &data.settings;
    let mut bands = st.grading.bands.clone();
    bands.sort_by(|a, b| b.min.partial_cmp(&a.min).unwrap_or(std::cmp::Ordering::Equal));
    let lowest = bands.last().map(|b| crate::grading::fmt_num(b.min)).unwrap_or_default();

    let mut ws = Worksheet::new();
    ws.set_name("Rekap")?;
    ws.write_string_with_format(0, 0, "Rekap predikat mapel", &f.title)?;
    ws.write_string_with_format(
        1,
        0,
        format!("{} · {} · tahun ajaran {}", st.school_name, st.report_subtitle, st.academic_year),
        &f.muted,
    )?;
    let hr = 3u32;
    let mut headers: Vec<String> = vec!["Mapel".into(), "Kelas".into(), "Siswa".into()];
    headers.extend(bands.iter().map(|b| b.code.clone()));
    headers.push(format!("Di bawah {lowest}"));
    headers.push("Belum lengkap".into());
    headers.push("Rata-rata nilai akhir".into());
    for (c, h) in headers.iter().enumerate() {
        ws.write_string_with_format(hr, c as u16, h, &f.header)?;
        ws.set_column_width(c as u16, if c == 0 { 30.0 } else { 11.0 })?;
    }
    ws.set_row_height(hr, 30)?;

    let mut r = hr + 1;
    for (si, subj) in st.subjects.iter().enumerate() {
        let mut rows: Vec<(String, Vec<&StudentView>)> =
            by_class.iter().map(|(c, v)| (c.clone(), v.iter().collect())).collect();
        if by_class.len() > 1 {
            rows.push(("Semua".into(), by_class.iter().flat_map(|(_, v)| v.iter()).collect()));
        }
        for (class, views) in rows {
            let results: Vec<&Resolved> = views.iter().filter_map(|v| v.subjects.get(si)).map(|s| &s.result).collect();
            ws.write_string_with_format(r, 0, &subj.name, &f.cell)?;
            ws.write_string_with_format(r, 1, &class, &f.cell)?;
            ws.write_number_with_format(r, 2, results.len() as f64, &f.cell)?;
            for (bi, b) in bands.iter().enumerate() {
                let n = results.iter().filter(|x| x.code.as_deref() == Some(&b.code) && x.status == Status::Ok).count();
                ws.write_number_with_format(r, 3 + bi as u16, n as f64, &f.num)?;
            }
            let base = 3 + bands.len() as u16;
            let below = results.iter().filter(|x| x.status == Status::BelowScale).count();
            let incomplete = results.iter().filter(|x| matches!(x.status, Status::Empty | Status::Incomplete)).count();
            ws.write_number_with_format(r, base, below as f64, if below > 0 { &f.marks_below[3] } else { &f.num })?;
            ws.write_number_with_format(r, base + 1, incomplete as f64, &f.num)?;
            let scores: Vec<f64> = results.iter().filter_map(|x| x.score).collect();
            if scores.is_empty() {
                ws.write_blank(r, base + 2, &f.cell)?;
            } else {
                let avg = scores.iter().sum::<f64>() / scores.len() as f64;
                ws.write_number_with_format(r, base + 2, (avg * 100.0).round() / 100.0, &f.num)?;
            }
            r += 1;
        }
    }
    ws.write_string_with_format(
        r + 1,
        0,
        format!("\"Di bawah {lowest}\" tetap dicetak sebagai predikat terendah di rapor sampai ada keputusan aturannya."),
        &f.muted,
    )?;
    ws.set_freeze_panes(hr + 1, 2)?;
    Ok(ws)
}

/// Buat workbook leger untuk kelas-kelas yang dipilih.
pub fn build_leger(data: &AppData, classes: &[String]) -> Result<(Workbook, usize), String> {
    let mut wanted: Vec<String> = classes.iter().map(|c| normalize_class(c)).filter(|c| !c.is_empty()).collect();
    wanted.sort_by_key(|c| class_sort_key(c));
    wanted.dedup();
    let mut by_class: Vec<(String, Vec<StudentView>)> = Vec::new();
    for class in &wanted {
        let mut students: Vec<&crate::model::Student> = data.students.iter().filter(|s| &s.class_name == class).collect();
        students.sort_by(|a, b| crate::model::compare_students(a, b));
        if students.is_empty() {
            continue;
        }
        by_class.push((class.clone(), students.into_iter().map(|s| student_view(s, &data.settings)).collect()));
    }
    if by_class.is_empty() {
        return Err("Tidak ada siswa di kelas yang dipilih".into());
    }
    let f = formats();
    let mut wb = Workbook::new();
    wb.push_worksheet(summary_sheet(data, &by_class, &f).map_err(xerr)?);
    let mut total = 0;
    for (class, views) in &by_class {
        total += views.len();
        wb.push_worksheet(class_sheet(data, class, views, &f).map_err(xerr)?);
    }
    Ok((wb, total))
}

pub fn write_leger(data: &AppData, classes: &[String], path: &Path) -> Result<usize, String> {
    let (mut wb, n) = build_leger(data, classes)?;
    wb.save(path)
        .map_err(|e| format!("File tidak bisa disimpan (mungkin sedang dibuka di Excel?): {e}"))?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ProjectEntry, RatingEntry, ScoreEntry, Student};
    use calamine::{open_workbook_auto, Data, Reader};

    fn sample() -> AppData {
        let mut d = AppData::default();
        let mut a = Student { id: "1".into(), nis: "11".into(), name: "Made Dewi".into(), class_name: "IX-1".into(), gender: "P".into(), attendance: "98%".into(), ..Default::default() };
        a.scores.insert("IPA".into(), ScoreEntry { ph: Some(95.0), pts: Some(97.0), manual: None }); // 96 EE
        a.scores.insert("BINDO".into(), ScoreEntry { ph: Some(70.0), pts: Some(72.0), manual: None }); // 71 < 80
        a.atl.insert("caring".into(), RatingEntry { grade: "ME".into(), indicator: String::new() });
        a.projects.push(ProjectEntry { project_id: "p1".into(), understanding: "AE".into(), inquiry: "92".into(), narrative: "Ok.".into() });
        let mut b = Student { id: "2".into(), name: "Putu Arya".into(), class_name: "IX-2".into(), ..Default::default() };
        b.scores.insert("IPA".into(), ScoreEntry { ph: Some(90.0), pts: Some(92.0), manual: None }); // 91 ME
        d.students = vec![a, b];
        d
    }

    fn cell(path: &Path, sheet: &str, r: u32, c: u32) -> String {
        let mut wb = open_workbook_auto(path).unwrap();
        let range = wb.worksheet_range(sheet).unwrap();
        match range.get_value((r, c)) {
            Some(Data::String(s)) => s.clone(),
            Some(Data::Float(f)) => crate::grading::fmt_num(*f),
            Some(Data::Int(i)) => i.to_string(),
            _ => String::new(),
        }
    }

    #[test]
    fn leger_has_summary_and_class_sheets() {
        let d = sample();
        let path = std::env::temp_dir().join(format!("eraport-leger-{}.xlsx", std::process::id()));
        let n = write_leger(&d, &["IX-2".into(), "IX-1".into()], &path).unwrap();
        assert_eq!(n, 2);
        let wb = open_workbook_auto(&path).unwrap();
        assert_eq!(wb.sheet_names(), vec!["Rekap", "IX-1", "IX-2"]);

        // IX-1: baris data pertama = baris 5; kolom 5..8 = IPA PH/PTS/Akhir/Pred.
        assert_eq!(cell(&path, "IX-1", 5, 3), "Made Dewi");
        assert_eq!(cell(&path, "IX-1", 5, 7), "96");
        assert_eq!(cell(&path, "IX-1", 5, 8), "EE");
        assert_eq!(cell(&path, "IX-1", 5, 12), "BE", "71 di bawah rentang tetap BE");
        // kolom proyek: setelah 3 mapel (12 kolom) + 7 ATL
        let pcol = 5 + 12 + 7;
        assert_eq!(cell(&path, "IX-1", 5, pcol), "AE");
        assert_eq!(cell(&path, "IX-1", 5, pcol + 1), "ME", "92 → ME");
        assert_eq!(cell(&path, "IX-1", 3, pcol + 2), "Kehadiran", "judul gabungan tersimpan di baris atas");
        assert_eq!(cell(&path, "IX-1", 5, pcol + 2), "98%");

        // Rekap: IPA IX-1 (baris 4), IX-2 (5), Semua (6); kolom 3 = EE, 4 = ME
        assert_eq!(cell(&path, "Rekap", 4, 1), "IX-1");
        assert_eq!(cell(&path, "Rekap", 4, 3), "1");
        assert_eq!(cell(&path, "Rekap", 5, 4), "1");
        assert_eq!(cell(&path, "Rekap", 6, 2), "2");
        // B. Indo IX-1: 1 siswa di bawah 80 (kolom 3 + 4 band = 7)
        assert_eq!(cell(&path, "Rekap", 7, 1), "IX-1");
        assert_eq!(cell(&path, "Rekap", 7, 7), "1");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn empty_selection_is_rejected() {
        assert!(build_leger(&sample(), &["VII-1".into()]).is_err());
        assert_eq!(sheet_name("IX/1:*"), "IX1");
    }
}
