//! Membuat HTML rapor dari data. HTML ini ditampilkan di aplikasi dan
//! dicetak/disimpan ke PDF lewat dialog cetak.
//!
//! Desain bawaan ada di `templates/report.html`. Kalau di folder data ada
//! file `template-rapor.html`, file itu yang dipakai, jadi desain rapor bisa
//! diubah tanpa mengubah program.

use crate::grading::fmt_num;
use crate::model::{grade_class_label, AppData, Settings};
use crate::view::{long_mark_text, mark_text, student_view};
use minijinja::value::Serde;
use minijinja::{AutoEscape, Environment};
use serde::Serialize;

pub const BUILTIN_TEMPLATE: &str = include_str!("../templates/report.html");

#[derive(Serialize)]
struct Page {
    size_css: &'static str,
    width_mm: f64,
    height_mm: f64,
}

fn page_for(paper: &str) -> Page {
    match paper.trim().to_ascii_uppercase().as_str() {
        "F4" | "FOLIO" => Page { size_css: "215mm 330mm", width_mm: 215.0, height_mm: 330.0 },
        "LETTER" => Page { size_css: "letter", width_mm: 215.9, height_mm: 279.4 },
        _ => Page { size_css: "A4", width_mm: 210.0, height_mm: 297.0 },
    }
}

#[derive(Serialize)]
struct LegendItem {
    code: String,
    label: String,
    range: String,
}

#[derive(Serialize)]
struct Row {
    name: String,
    indicator: String,
    mark: String,
}

#[derive(Serialize)]
struct Project {
    name: String,
    theme: String,
    central_idea: String,
    lines: Vec<String>,
    understanding: String,
    inquiry: String,
    narrative: String,
}

#[derive(Serialize)]
struct ReportStudent {
    name: String,
    student_id: String,
    grade_class: String,
    homeroom: String,
    /// Gambar tanda tangan wali kelas (data URL); kosong = ruang kosong.
    homeroom_signature: String,
    attendance: String,
    atl: Vec<Row>,
    projects: Vec<Project>,
    subjects: Vec<Row>,
}

#[derive(Serialize)]
struct Context {
    page: Page,
    logo: String,
    school_name: String,
    title_lines: Vec<String>,
    subtitle: String,
    academic_year: String,
    place_date: String,
    principal_name: String,
    principal_title: String,
    principal_signature: String,
    homeroom_title: String,
    scale_descriptor: String,
    legend: Vec<LegendItem>,
    students: Vec<ReportStudent>,
}

fn legend(settings: &Settings) -> Vec<LegendItem> {
    let mut bands = settings.grading.bands.clone();
    bands.sort_by(|a, b| b.min.partial_cmp(&a.min).unwrap_or(std::cmp::Ordering::Equal));
    bands
        .into_iter()
        .map(|b| LegendItem {
            range: if (b.max - b.min).abs() < 1e-9 {
                fmt_num(b.min)
            } else {
                format!("{}–{}", fmt_num(b.min), fmt_num(b.max))
            },
            code: b.code,
            label: b.label,
        })
        .collect()
}

/// Gambar tanda tangan hanya dicetak dalam mode digital, dan hanya gambar yang disematkan
/// (data:image/…); alamat lain diabaikan supaya rapor tidak memuat apa pun dari luar.
fn signature(st: &Settings, image: &str) -> String {
    if st.digital_signature && image.starts_with("data:image/") { image.to_string() } else { String::new() }
}

fn build_context(data: &AppData, ids: &[String]) -> Context {
    let st = &data.settings;
    let show = st.show_numeric_score;
    let students = ids
        .iter()
        .filter_map(|id| data.students.iter().find(|s| &s.id == id))
        .map(|s| {
            let v = student_view(s, st);
            let class = st.classes.iter().find(|c| c.name == s.class_name);
            let homeroom = class.map(|c| c.homeroom_teacher.trim().to_string()).unwrap_or_default();
            let homeroom_signature = class.map(|c| signature(st, &c.homeroom_signature)).unwrap_or_default();
            let student_id = match (s.nis.trim(), s.nisn.trim()) {
                ("", "") => String::new(),
                (nis, "") => nis.to_string(),
                ("", nisn) => format!("NISN {nisn}"),
                (nis, nisn) => format!("{nis} / NISN {nisn}"),
            };
            ReportStudent {
                name: s.name.trim().to_string(),
                student_id,
                grade_class: grade_class_label(&s.class_name),
                homeroom,
                homeroom_signature,
                attendance: s.attendance.trim().to_string(),
                atl: v
                    .atl
                    .iter()
                    .map(|r| Row {
                        name: r.name.clone(),
                        indicator: r.indicator.clone(),
                        mark: mark_text(&r.result, false),
                    })
                    .collect(),
                projects: v
                    .projects
                    .iter()
                    .map(|p| Project {
                        name: p.name.clone(),
                        theme: p.theme.clone(),
                        central_idea: p.central_idea.clone(),
                        lines: p.lines.clone(),
                        understanding: long_mark_text(&p.understanding),
                        inquiry: long_mark_text(&p.inquiry),
                        narrative: p.narrative.clone(),
                    })
                    .collect(),
                subjects: v
                    .subjects
                    .iter()
                    .map(|r| Row {
                        name: r.name.clone(),
                        indicator: r.indicator.clone(),
                        mark: mark_text(&r.result, show),
                    })
                    .collect(),
            }
        })
        .collect();

    let legend = legend(st);
    let scale_descriptor = legend
        .iter()
        .map(|b| format!("{} = {}", b.code, b.label))
        .collect::<Vec<_>>()
        .join(" | ");
    Context {
        page: page_for(&st.paper_size),
        // hanya gambar yang disematkan; alamat lain diabaikan
        logo: if st.logo.starts_with("data:image/") { st.logo.clone() } else { String::new() },
        school_name: st.school_name.trim().to_string(),
        title_lines: st
            .report_title
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect(),
        subtitle: st.report_subtitle.trim().to_string(),
        academic_year: st.academic_year.trim().to_string(),
        place_date: st.place_date.trim().to_string(),
        principal_name: st.principal_name.trim().to_string(),
        principal_title: st.principal_title.trim().to_string(),
        principal_signature: signature(st, &st.principal_signature),
        homeroom_title: st.homeroom_title.trim().to_string(),
        scale_descriptor,
        legend,
        students,
    }
}

/// HTML lengkap untuk siswa-siswa `ids` (urutan dipertahankan, satu rapor per halaman baru).
pub fn render(data: &AppData, ids: &[String], custom_template: Option<&str>) -> Result<String, String> {
    let mut env = Environment::new();
    // Semua isian di-escape sebagai HTML, termasuk di template kustom.
    env.set_auto_escape_callback(|_| AutoEscape::Html);
    let source = custom_template.unwrap_or(BUILTIN_TEMPLATE);
    env.add_template_owned("report.html", source.to_string())
        .map_err(|e| format!("Template rapor bermasalah: {e:#}"))?;
    let tmpl = env.get_template("report.html").map_err(|e| e.to_string())?;
    tmpl.render(Serde(&build_context(data, ids)))
        .map_err(|e| format!("Gagal membuat rapor: {e:#}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ProjectEntry, ScoreEntry, Student};

    fn data_with(student: Student) -> AppData {
        let mut d = AppData::default();
        d.students.push(student);
        d
    }

    #[test]
    fn renders_and_escapes() {
        let mut s = Student {
            id: "1".into(),
            name: "Made <b>Dewi</b>".into(),
            class_name: "IX-1".into(),
            ..Default::default()
        };
        s.scores.insert("IPA".into(), ScoreEntry { ph: Some(95.0), pts: Some(97.0), manual: None });
        s.projects.push(ProjectEntry {
            project_id: "p1".into(),
            understanding: "ME".into(),
            inquiry: "EE".into(),
            narrative: "Aktif & teliti.".into(),
        });
        let html = render(&data_with(s), &["1".into()], None).unwrap();
        assert!(html.contains("Made &lt;b&gt;Dewi&lt;&#x2f;b&gt;") || html.contains("Made &lt;b&gt;Dewi&lt;/b&gt;"));
        assert!(!html.contains("<b>Dewi</b>"));
        assert!(html.contains("Grade 9 (IX-1)"));
        assert!(html.contains("Meeting Expectations (ME)"));
        assert!(html.contains("Who We Are"));
        assert!(html.contains("96–100"), "keterangan rentang nilai");
        assert!(!html.contains("AI responses"), "catatan AI dari template lama tidak ikut");
    }

    #[test]
    fn numeric_mark_optional() {
        let mut s = Student { id: "1".into(), class_name: "IX-1".into(), ..Default::default() };
        s.scores.insert("IPA".into(), ScoreEntry { manual: Some(92.0), ..Default::default() });
        let mut d = data_with(s);
        let html = render(&d, &["1".into()], None).unwrap();
        assert!(!html.contains("ME (92)"));
        d.settings.show_numeric_score = true;
        let html = render(&d, &["1".into()], None).unwrap();
        assert!(html.contains("ME (92)"));
    }

    #[test]
    fn signatures_only_in_digital_mode_and_per_class() {
        use crate::model::ClassConfig;
        let mut d = data_with(Student { id: "1".into(), class_name: "IX-1".into(), ..Default::default() });
        d.students.push(Student { id: "2".into(), class_name: "IX-2".into(), ..Default::default() });
        d.settings.classes = vec![
            ClassConfig { name: "IX-1".into(), homeroom_teacher: "Bu A".into(), homeroom_signature: "data:image/png;base64,QUFB".into() },
            ClassConfig { name: "IX-2".into(), homeroom_teacher: "Pak B".into(), homeroom_signature: String::new() },
        ];
        d.settings.principal_signature = "data:image/png;base64,UFBQ".into();
        let count = |html: &str| html.matches("class=\"sig-img\"").count();

        // mode tanda tangan basah: tidak ada gambar walaupun sudah diunggah
        assert_eq!(count(&render(&d, &["1".into()], None).unwrap()), 0);

        d.settings.digital_signature = true;
        let one = render(&d, &["1".into()], None).unwrap();
        assert_eq!(count(&one), 2, "wali kelas IX-1 + kepala sekolah");
        assert!(one.contains("QUFB") && one.contains("UFBQ"));
        // IX-2 belum punya tanda tangan wali kelas: hanya kepala sekolah
        let two = render(&d, &["2".into()], None).unwrap();
        assert_eq!(count(&two), 1);
        assert!(!two.contains("QUFB"));

        // alamat dari luar tidak pernah dimuat
        d.settings.principal_signature = "https://contoh.invalid/ttd.png".into();
        assert_eq!(count(&render(&d, &["2".into()], None).unwrap()), 0);
    }

    #[test]
    fn logo_only_when_embedded_image() {
        let mut d = data_with(Student { id: "1".into(), class_name: "IX-1".into(), ..Default::default() });
        assert!(!render(&d, &["1".into()], None).unwrap().contains("class=\"logo\""));
        d.settings.logo = "data:image/png;base64,iVBORw0KGgo=".into();
        let html = render(&d, &["1".into()], None).unwrap();
        assert!(html.contains("class=\"logo\""));
        assert!(html.contains("data:image"));
        d.settings.logo = "https://contoh.invalid/logo.png".into();
        assert!(!render(&d, &["1".into()], None).unwrap().contains("class=\"logo\""));
    }

    #[test]
    fn broken_custom_template_reports_error() {
        let d = AppData::default();
        let err = render(&d, &[], Some("{% for x in %}")).unwrap_err();
        assert!(err.contains("Template rapor bermasalah"));
    }
}
