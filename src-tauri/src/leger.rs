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
const AVG_FILL: u32 = 0xF1F3F8;

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
    center: Format,
    int: Format,
    dec: Format,
    avg_label: Format,
    avg_int: Format,
    avg_dec: Format,
    marks: Vec<Format>,
    marks_below: Vec<Format>,
    ok: Format,
    warn: Format,
    sign_name: Format,
}

fn formats() -> Formats {
    let border = |f: Format| f.set_border(FormatBorder::Thin).set_border_color(Color::RGB(RULE));
    let mark = |color: u32| border(Format::new().set_bold().set_align(FormatAlign::Center).set_font_color(Color::RGB(color)));
    let avg = |f: Format| f.set_bold().set_background_color(Color::RGB(AVG_FILL));
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
        center: border(Format::new().set_align(FormatAlign::Center)),
        // angka bulat tanpa titik di belakang ("80", bukan "80.")
        int: border(Format::new().set_num_format("0").set_align(FormatAlign::Right)),
        dec: border(Format::new().set_num_format("0.0#").set_align(FormatAlign::Right)),
        avg_label: avg(border(Format::new())),
        avg_int: avg(border(Format::new().set_num_format("0").set_align(FormatAlign::Right))),
        avg_dec: avg(border(Format::new().set_num_format("0.0#").set_align(FormatAlign::Right))),
        marks: RANK_COLORS.iter().map(|c| mark(*c)).collect(),
        marks_below: RANK_COLORS.iter().map(|c| mark(*c).set_background_color(Color::RGB(BELOW_FILL))).collect(),
        ok: border(Format::new().set_font_color(Color::RGB(RANK_COLORS[0]))),
        warn: border(Format::new().set_font_color(Color::RGB(RANK_COLORS[2]))),
        sign_name: Format::new().set_bold().set_underline(rust_xlsxwriter::FormatUnderline::Single),
    }
}

/// Peringkat predikat (0 = tertinggi), dari rentang di Pengaturan.
fn ranks(data: &AppData) -> BTreeMap<String, usize> {
    let mut bands = data.settings.grading.bands.clone();
    bands.sort_by(|a, b| b.min.partial_cmp(&a.min).unwrap_or(std::cmp::Ordering::Equal));
    bands.into_iter().enumerate().map(|(i, b)| (b.code, i)).collect()
}

fn lowest_min(data: &AppData) -> String {
    data.settings
        .grading
        .bands
        .iter()
        .map(|b| b.min)
        .fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.min(x))))
        .map(crate::grading::fmt_num)
        .unwrap_or_default()
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

/// Angka dibulatkan 2 desimal; bilangan bulat tanpa desimal.
fn write_num(ws: &mut Worksheet, r: u32, c: u16, v: Option<f64>, int: &Format, dec: &Format, blank: &Format) -> Result<(), XlsxError> {
    match v {
        Some(x) => {
            let x = (x * 100.0).round() / 100.0;
            let fmt = if (x - x.round()).abs() < 1e-9 { int } else { dec };
            ws.write_number_with_format(r, c, x, fmt).map(|_| ())
        }
        None => ws.write_blank(r, c, blank).map(|_| ()),
    }
}

/// Lebar kolom (satuan karakter Excel) → poin.
fn col_points(width: f64) -> f64 {
    (width * 7.0 + 5.0).trunc() * 0.75
}

/// Kertas mendatar: kode ukuran kertas Excel dan lebar yang bisa dicetak (poin).
fn paper(data: &AppData) -> (u8, f64) {
    const MARGINS: f64 = 2.0 * 0.3 * 72.0;
    match data.settings.paper_size.as_str() {
        "F4" => (14, 13.0 * 72.0 - MARGINS),     // Folio 8,5 × 13 in
        "Letter" => (1, 11.0 * 72.0 - MARGINS), // 8,5 × 11 in
        _ => (9, 297.0 / 25.4 * 72.0 - MARGINS), // A4
    }
}

fn page_setup(ws: &mut Worksheet, data: &AppData) {
    ws.set_landscape();
    ws.set_paper_size(paper(data).0);
    ws.set_margins(0.3, 0.3, 0.5, 0.5, 0.25, 0.25);
    ws.set_footer("&L&8E-Raport · &A&R&8Halaman &P dari &N");
}

/// Ringkasan isian yang belum lengkap, singkat untuk satu sel (rinciannya di sheet Catatan).
fn short_status(v: &StudentView, lowest: &str) -> Option<String> {
    let p = &v.progress;
    let mut parts = Vec::new();
    if p.subjects_done < p.subjects_total {
        parts.push(format!("nilai mapel {}/{}", p.subjects_done, p.subjects_total));
    }
    let below = v.subjects.iter().filter(|s| s.result.status == Status::BelowScale).count();
    if below > 0 {
        parts.push(format!("{below} mapel < {lowest}"));
    }
    if p.atl_done < p.atl_total {
        parts.push(format!("ATL {}/{}", p.atl_done, p.atl_total));
    }
    if p.projects == 0 || p.projects_done < p.projects {
        parts.push("proyek".into());
    }
    if !p.attendance {
        parts.push("kehadiran".into());
    }
    if parts.is_empty() && !v.issues.is_empty() {
        parts.push(format!("{} catatan", v.issues.len()));
    }
    (!parts.is_empty()).then(|| parts.join(", "))
}

fn class_sheet(data: &AppData, class: &str, views: &[StudentView], f: &Formats) -> Result<Worksheet, XlsxError> {
    let st = &data.settings;
    let rk = ranks(data);
    let lowest = lowest_min(data);
    let eff = if st.effective_days == 0 { 100 } else { st.effective_days };
    let mut ws = Worksheet::new();
    ws.set_name(sheet_name(class))?;
    let homeroom = st.classes.iter().find(|c| c.name == class).map(|c| c.homeroom_teacher.clone()).unwrap_or_default();
    let subtitle = format!(
        "{} · {} · tahun ajaran {}{}",
        st.school_name,
        st.report_subtitle,
        st.academic_year,
        if homeroom.trim().is_empty() { String::new() } else { format!(" · wali kelas {homeroom}") }
    );

    // proyek yang dipakai siswa di kelas ini (urut sesuai Pengaturan)
    let projects: Vec<&crate::model::ProjectConfig> = st
        .projects
        .iter()
        .filter(|p| views.iter().any(|v| v.student.projects.iter().any(|e| e.project_id == p.id)))
        .collect();

    // Dicetak mendatar dalam dua bagian, masing-masing lengkap dengan No + Nama:
    //   halaman 1: identitas + nilai mapel
    //   halaman 2: ATL, proyek, kehadiran, kelengkapan rapor
    let (gr, hr) = (5u32, 6u32);
    let mut widths: Vec<f64> = Vec::new();
    let mut set_w = |ws: &mut Worksheet, col: u16, w: f64| -> Result<(), XlsxError> {
        ws.set_column_width(col, w)?;
        if widths.len() <= col as usize {
            widths.resize(col as usize + 1, 8.43);
        }
        widths[col as usize] = w;
        Ok(())
    };
    let longest = views.iter().map(|v| v.student.name.chars().count()).max().unwrap_or(10) as f64;
    let name_w = (longest * 1.05 + 2.0).clamp(22.0, 44.0);
    let ident = |ws: &mut Worksheet, col: u16, set_w: &mut dyn FnMut(&mut Worksheet, u16, f64) -> Result<(), XlsxError>| -> Result<(), XlsxError> {
        for (i, (label, w)) in [("No", 4.5), ("Nama siswa", name_w)].into_iter().enumerate() {
            ws.merge_range(gr, col + i as u16, hr, col + i as u16, label, &f.header)?;
            set_w(ws, col + i as u16, w)?;
        }
        Ok(())
    };

    // ---- bagian 1
    ident(&mut ws, 0, &mut set_w)?;
    for (i, (label, w)) in [("L/P", 4.5), ("NIS", 8.0), ("NISN", 12.0)].into_iter().enumerate() {
        ws.merge_range(gr, 2 + i as u16, hr, 2 + i as u16, label, &f.header)?;
        set_w(&mut ws, 2 + i as u16, w)?;
    }
    let subj_start: u16 = 5;
    let mut col = subj_start;
    for s in &st.subjects {
        ws.merge_range(gr, col, gr, col + 3, &s.name, &f.header)?;
        for (i, label) in ["PH", "PTS", "Akhir", "Pred."].into_iter().enumerate() {
            ws.write_string_with_format(hr, col + i as u16, label, &f.header)?;
            set_w(&mut ws, col + i as u16, 6.5)?;
        }
        col += 4;
    }
    let p1_end = col - 1;

    // ---- bagian 2
    let p2 = col;
    ident(&mut ws, p2, &mut set_w)?;
    col = p2 + 2;
    let atl_start = col;
    if !st.atl.is_empty() {
        let n = st.atl.len() as u16;
        if n > 1 {
            ws.merge_range(gr, col, gr, col + n - 1, "ATL & Learner Profile", &f.header)?;
        } else {
            ws.write_string_with_format(gr, col, "ATL", &f.header)?;
        }
        for (i, a) in st.atl.iter().enumerate() {
            ws.write_string_with_format(hr, col + i as u16, &a.name, &f.header)?;
            let w = (a.name.chars().count() as f64 * 0.9 + 2.0).clamp(9.0, 15.0);
            set_w(&mut ws, col + i as u16, w)?;
        }
        col += n;
    }
    let proj_start = col;
    for p in &projects {
        let title = if p.theme.trim().is_empty() { p.name.clone() } else { format!("{} — {}", p.name, p.theme) };
        ws.merge_range(gr, col, gr, col + 1, &title, &f.header)?;
        ws.write_string_with_format(hr, col, "Underst.", &f.header)?;
        ws.write_string_with_format(hr, col + 1, "Inquiry", &f.header)?;
        set_w(&mut ws, col, 10.0)?;
        set_w(&mut ws, col + 1, 10.0)?;
        col += 2;
    }
    let att_col = col;
    ws.merge_range(gr, col, gr, col + 3, "Kehadiran", &f.header)?;
    for (i, (label, w)) in [("S", 4.5), ("I", 4.5), ("A", 4.5), ("Hadir", 7.5)].into_iter().enumerate() {
        ws.write_string_with_format(hr, col + i as u16, label, &f.header)?;
        set_w(&mut ws, col + i as u16, w)?;
    }
    col += 4;
    let status_col = col;
    ws.merge_range(gr, col, hr, col, "Belum lengkap", &f.header)?;
    set_w(&mut ws, col, 40.0)?;
    ws.set_row_height(gr, 30)?;
    ws.set_row_height(hr, 32)?;

    // ---- judul tiap bagian (teksnya tidak melewati batas halaman)
    ws.write_string_with_format(0, 0, format!("Leger Kelas {class} — nilai mapel"), &f.title)?;
    ws.write_string_with_format(1, 0, &subtitle, &f.muted)?;
    ws.write_string_with_format(
        2,
        0,
        format!(
            "Akhir = {}% PH + {}% PTS. Predikat berlatar merah = nilai akhir di bawah {lowest} (di rapor tetap tercetak predikat terendah).",
            crate::grading::fmt_num(st.grading.weight_ph),
            crate::grading::fmt_num(st.grading.weight_pts)
        ),
        &f.muted,
    )?;
    ws.write_string_with_format(3, 0, "Sel kosong = belum diisi.", &f.muted)?;
    ws.write_string_with_format(0, p2, format!("Leger Kelas {class} — ATL, proyek, kehadiran"), &f.title)?;
    ws.write_string_with_format(1, p2, &subtitle, &f.muted)?;
    ws.write_string_with_format(2, p2, format!("S/I/A = jumlah hari sakit/izin/alpa · Hadir dihitung dari {eff} hari efektif."), &f.muted)?;
    ws.write_string_with_format(3, p2, "Belum lengkap = ringkasan; rinciannya ada di sheet Catatan. Sel kosong = belum diisi.", &f.muted)?;

    // ---- baris siswa
    let last_row = hr + views.len() as u32;
    for (i, v) in views.iter().enumerate() {
        let r = hr + 1 + i as u32;
        let s = &v.student;
        for base in [0, p2] {
            ws.write_number_with_format(r, base, (i + 1) as f64, &f.center)?;
            ws.write_string_with_format(r, base + 1, &s.name, &f.cell)?;
        }
        ws.write_string_with_format(r, 2, &s.gender, &f.center)?;
        ws.write_string_with_format(r, 3, &s.nis, &f.cell)?;
        ws.write_string_with_format(r, 4, &s.nisn, &f.cell)?;
        let mut c = subj_start;
        for sv in &v.subjects {
            write_num(&mut ws, r, c, sv.ph, &f.int, &f.dec, &f.cell)?;
            write_num(&mut ws, r, c + 1, sv.pts, &f.int, &f.dec, &f.cell)?;
            write_num(&mut ws, r, c + 2, sv.result.score, &f.int, &f.dec, &f.cell)?;
            write_mark(&mut ws, r, c + 3, &sv.result, f, &rk)?;
            c += 4;
        }
        for (k, a) in v.atl.iter().enumerate() {
            write_mark(&mut ws, r, atl_start + k as u16, &a.result, f, &rk)?;
        }
        for (k, p) in projects.iter().enumerate() {
            let c = proj_start + 2 * k as u16;
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
        }
        for (k, d) in [s.attendance_s, s.attendance_i, s.attendance_a].into_iter().enumerate() {
            write_num(&mut ws, r, att_col + k as u16, d.map(f64::from), &f.int, &f.dec, &f.cell)?;
        }
        // "98% (Present)" → "98%"
        let hadir = s.attendance.split(" (").next().unwrap_or("").trim();
        ws.write_string_with_format(r, att_col + 3, hadir, &f.center)?;
        match short_status(v, &lowest) {
            None => ws.write_string_with_format(r, status_col, "Lengkap", &f.ok)?,
            Some(t) => ws.write_string_with_format(r, status_col, t, &f.warn)?,
        };
    }

    // ---- baris rata-rata kelas (PH, PTS, Akhir per mapel)
    let avg_row = last_row + 1;
    for c in (0..=p1_end).chain(p2..=status_col) {
        ws.write_blank(avg_row, c, &f.avg_label)?;
    }
    ws.write_string_with_format(avg_row, 1, "Rata-rata kelas", &f.avg_label)?;
    for (si, _) in st.subjects.iter().enumerate() {
        let c = subj_start + 4 * si as u16;
        let pick: [fn(&crate::view::SubjectView) -> Option<f64>; 3] = [|s| s.ph, |s| s.pts, |s| s.result.score];
        for (k, get) in pick.iter().enumerate() {
            let vals: Vec<f64> = views.iter().filter_map(|v| v.subjects.get(si)).filter_map(get).collect();
            let avg = (!vals.is_empty()).then(|| vals.iter().sum::<f64>() / vals.len() as f64);
            write_num(&mut ws, avg_row, c + k as u16, avg, &f.avg_int, &f.avg_dec, &f.avg_label)?;
        }
    }

    // ---- tanda tangan di tiap bagian: kepala sekolah kiri, wali kelas kanan
    let width = |c: u16| widths.get(c as usize).copied().unwrap_or(8.43);
    let sr = avg_row + 3;
    let principal = if st.principal_name.trim().is_empty() { "......................................".to_string() } else { st.principal_name.clone() };
    let teacher = if homeroom.trim().is_empty() { "......................................".to_string() } else { homeroom.clone() };
    for (start, end) in [(0u16, p1_end), (p2, status_col)] {
        ws.write_string(sr, start + 1, "Mengetahui,")?;
        ws.write_string(sr + 1, start + 1, &st.principal_title)?;
        ws.write_string_with_format(sr + 5, start + 1, &principal, &f.sign_name)?;
        // kolom awal blok kanan: cukup ruang ±32 karakter sampai tepi halaman
        let mut c = end;
        let mut room = width(end);
        while c > start + 2 && room < 32.0 {
            c -= 1;
            room += width(c);
        }
        ws.write_string(sr, c, &st.place_date)?;
        ws.write_string(sr + 1, c, format!("{} {class}", st.homeroom_title))?;
        ws.write_string_with_format(sr + 5, c, &teacher, &f.sign_name)?;
    }

    // ---- cetak
    page_setup(&mut ws, data);
    let usable = paper(data).1;
    let pts = |range: std::ops::RangeInclusive<u16>| range.map(|c| col_points(width(c))).sum::<f64>();
    let widest = pts(0..=p1_end).max(pts(p2..=status_col));
    // 0,85: cadangan karena lebar kolom saat dicetak lebih besar dari perkiraan (metrik huruf printer)
    let scale = (usable * 0.85 / widest * 100.0).clamp(40.0, 100.0);
    ws.set_print_scale(scale.floor() as u16);
    ws.set_vertical_page_breaks(&[p2 as u32])?;
    ws.set_repeat_rows(gr, hr)?;
    ws.set_freeze_panes(hr + 1, 2)?;
    Ok(ws)
}

fn summary_sheet(data: &AppData, by_class: &[(String, Vec<StudentView>)], f: &Formats) -> Result<Worksheet, XlsxError> {
    let st = &data.settings;
    let mut bands = st.grading.bands.clone();
    bands.sort_by(|a, b| b.min.partial_cmp(&a.min).unwrap_or(std::cmp::Ordering::Equal));
    let lowest = lowest_min(data);

    let mut ws = Worksheet::new();
    ws.set_name("Rekap")?;
    ws.write_string_with_format(0, 0, "Rekap predikat mapel", &f.title)?;
    ws.write_string_with_format(
        1,
        0,
        format!("{} · {} · tahun ajaran {}", st.school_name, st.report_subtitle, st.academic_year),
        &f.muted,
    )?;
    ws.write_string_with_format(
        2,
        0,
        format!("Jumlah siswa per predikat nilai akhir. \"Di bawah {lowest}\" tetap tercetak sebagai predikat terendah di rapor."),
        &f.muted,
    )?;
    let hr = 4u32;
    let mut headers: Vec<String> = vec!["Mapel".into(), "Kelas".into(), "Siswa".into()];
    headers.extend(bands.iter().map(|b| b.code.clone()));
    headers.push(format!("Di bawah {lowest}"));
    headers.push("Belum ada nilai".into());
    headers.push("Rata-rata nilai akhir".into());
    for (c, h) in headers.iter().enumerate() {
        ws.write_string_with_format(hr, c as u16, h, &f.header)?;
        ws.set_column_width(c as u16, if c == 0 { 30.0 } else { 11.0 })?;
    }
    ws.set_row_height(hr, 32)?;

    let mut r = hr + 1;
    for (si, subj) in st.subjects.iter().enumerate() {
        let mut rows: Vec<(String, Vec<&StudentView>, bool)> =
            by_class.iter().map(|(c, v)| (c.clone(), v.iter().collect(), false)).collect();
        if by_class.len() > 1 {
            rows.push(("Semua kelas".into(), by_class.iter().flat_map(|(_, v)| v.iter()).collect(), true));
        }
        for (class, views, total) in rows {
            let (lab, int, dec) = if total { (&f.avg_label, &f.avg_int, &f.avg_dec) } else { (&f.cell, &f.int, &f.dec) };
            let results: Vec<&Resolved> = views.iter().filter_map(|v| v.subjects.get(si)).map(|s| &s.result).collect();
            ws.write_string_with_format(r, 0, &subj.name, lab)?;
            ws.write_string_with_format(r, 1, &class, lab)?;
            ws.write_number_with_format(r, 2, results.len() as f64, int)?;
            for (bi, b) in bands.iter().enumerate() {
                let n = results.iter().filter(|x| x.code.as_deref() == Some(&b.code) && x.status == Status::Ok).count();
                ws.write_number_with_format(r, 3 + bi as u16, n as f64, int)?;
            }
            let base = 3 + bands.len() as u16;
            let below = results.iter().filter(|x| x.status == Status::BelowScale).count();
            let incomplete = results.iter().filter(|x| matches!(x.status, Status::Empty | Status::Incomplete)).count();
            let below_fmt = if below > 0 { &f.marks_below[3] } else { int };
            ws.write_number_with_format(r, base, below as f64, below_fmt)?;
            ws.write_number_with_format(r, base + 1, incomplete as f64, int)?;
            let scores: Vec<f64> = results.iter().filter_map(|x| x.score).collect();
            let avg = (!scores.is_empty()).then(|| scores.iter().sum::<f64>() / scores.len() as f64);
            write_num(&mut ws, r, base + 2, avg, int, dec, lab)?;
            r += 1;
        }
    }
    page_setup(&mut ws, data);
    ws.set_print_fit_to_pages(1, 0);
    ws.set_repeat_rows(hr, hr)?;
    ws.set_freeze_panes(hr + 1, 2)?;
    Ok(ws)
}

/// Rincian isian yang belum lengkap per siswa (kolom "Belum lengkap" di sheet kelas hanya ringkasan).
fn notes_sheet(data: &AppData, by_class: &[(String, Vec<StudentView>)], f: &Formats) -> Result<Option<Worksheet>, XlsxError> {
    if by_class.iter().all(|(_, v)| v.iter().all(|x| x.issues.is_empty())) {
        return Ok(None);
    }
    let mut ws = Worksheet::new();
    ws.set_name("Catatan")?;
    ws.write_string_with_format(0, 0, "Rincian yang belum lengkap", &f.title)?;
    ws.write_string_with_format(1, 0, "Satu baris = satu catatan. Lengkapi di aplikasi (Isi rapor / Nilai mapel), lalu buat leger lagi.", &f.muted)?;
    let hr = 3u32;
    for (c, (h, w)) in [("Kelas", 9.0), ("No", 5.0), ("Nama siswa", 34.0), ("Catatan", 90.0)].into_iter().enumerate() {
        ws.write_string_with_format(hr, c as u16, h, &f.header)?;
        ws.set_column_width(c as u16, w)?;
    }
    let mut r = hr + 1;
    for (class, views) in by_class {
        for (i, v) in views.iter().enumerate() {
            for issue in &v.issues {
                ws.write_string_with_format(r, 0, class, &f.cell)?;
                ws.write_number_with_format(r, 1, (i + 1) as f64, &f.center)?;
                ws.write_string_with_format(r, 2, &v.student.name, &f.cell)?;
                ws.write_string_with_format(r, 3, issue, &f.cell)?;
                r += 1;
            }
        }
    }
    page_setup(&mut ws, data);
    ws.set_print_fit_to_pages(1, 0);
    ws.set_repeat_rows(hr, hr)?;
    ws.set_freeze_panes(hr + 1, 0)?;
    Ok(Some(ws))
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
    if let Some(ws) = notes_sheet(data, &by_class, &f).map_err(xerr)? {
        wb.push_worksheet(ws);
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
        assert_eq!(wb.sheet_names(), vec!["Rekap", "IX-1", "IX-2", "Catatan"]);

        // IX-1 bagian 1: judul di baris 5–6, data mulai baris 7; No, Nama, L/P, NIS, NISN, lalu mapel
        assert_eq!(cell(&path, "IX-1", 7, 1), "Made Dewi");
        assert_eq!(cell(&path, "IX-1", 7, 3), "11");
        assert_eq!(cell(&path, "IX-1", 7, 7), "96");
        assert_eq!(cell(&path, "IX-1", 7, 8), "EE");
        assert_eq!(cell(&path, "IX-1", 7, 12), "BE", "71 di bawah rentang tetap BE");
        assert_eq!(cell(&path, "IX-1", 8, 1), "Rata-rata kelas");
        assert_eq!(cell(&path, "IX-1", 8, 7), "96");
        // bagian 2 (sesudah 3 mapel × 4 kolom): No, Nama lagi, 7 ATL, proyek, kehadiran, status
        let p2 = 5 + 12;
        assert_eq!(cell(&path, "IX-1", 7, p2 + 1), "Made Dewi");
        let pcol = p2 + 2 + 7;
        assert_eq!(cell(&path, "IX-1", 5, pcol), "Project 1 — Who We Are");
        assert_eq!(cell(&path, "IX-1", 7, pcol), "AE");
        assert_eq!(cell(&path, "IX-1", 7, pcol + 1), "ME", "92 → ME");
        assert_eq!(cell(&path, "IX-1", 5, pcol + 2), "Kehadiran");
        assert_eq!(cell(&path, "IX-1", 7, pcol + 5), "98%");
        let status = cell(&path, "IX-1", 7, pcol + 6);
        assert!(status.contains("1 mapel < 80") && status.contains("ATL 1/7"), "{status}");

        // Rekap: IPA IX-1 (baris 5), IX-2 (6), Semua kelas (7); kolom 3 = EE, 4 = ME
        assert_eq!(cell(&path, "Rekap", 5, 1), "IX-1");
        assert_eq!(cell(&path, "Rekap", 5, 3), "1");
        assert_eq!(cell(&path, "Rekap", 6, 4), "1");
        assert_eq!(cell(&path, "Rekap", 7, 1), "Semua kelas");
        assert_eq!(cell(&path, "Rekap", 7, 2), "2");
        // B. Indo IX-1: 1 siswa di bawah 80 (kolom 3 + 4 band = 7)
        assert_eq!(cell(&path, "Rekap", 8, 1), "IX-1");
        assert_eq!(cell(&path, "Rekap", 8, 7), "1");

        // Catatan: rincian per siswa
        assert_eq!(cell(&path, "Catatan", 4, 2), "Made Dewi");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn empty_selection_is_rejected() {
        assert!(build_leger(&sample(), &["VII-1".into()]).is_err());
        assert_eq!(sheet_name("IX/1:*"), "IX1");
    }
}
