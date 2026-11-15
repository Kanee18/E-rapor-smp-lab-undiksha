//! Data siswa yang sudah dihitung (nilai akhir, predikat, indikator, hal
//! yang perlu dicek). Dipakai bersama oleh layar aplikasi dan rapor, jadi
//! apa yang terlihat di aplikasi = apa yang tercetak.

use crate::grading::{fmt_num, resolve_input, resolve_subject, Resolved, Status};
use crate::model::{canonical_key, Settings, Student};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct SubjectView {
    pub subject_id: String,
    pub name: String,
    pub source_key: String,
    pub ph: Option<f64>,
    pub pts: Option<f64>,
    pub manual: Option<f64>,
    pub result: Resolved,
    pub indicator: String,
    pub indicator_custom: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RatingView {
    pub atl_id: String,
    pub name: String,
    pub input: String,
    pub result: Resolved,
    pub indicator: String,
    pub indicator_custom: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectView {
    pub project_id: String,
    pub name: String,
    pub theme: String,
    pub central_idea: String,
    pub lines: Vec<String>,
    pub understanding: Resolved,
    pub inquiry: Resolved,
    pub narrative: String,
    pub missing_config: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Progress {
    pub subjects_done: usize,
    pub subjects_total: usize,
    pub atl_done: usize,
    pub atl_total: usize,
    pub projects: usize,
    pub projects_done: usize,
    pub attendance: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct StudentView {
    pub student: Student,
    pub subjects: Vec<SubjectView>,
    pub atl: Vec<RatingView>,
    pub projects: Vec<ProjectView>,
    pub issues: Vec<String>,
    pub progress: Progress,
}

/// Baris "Lines of Inquiry": buang baris kosong dan nomor di depan ("1. ", "2) ").
pub fn split_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| {
            let t = l.trim();
            let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
            let rest = &t[digits..];
            if digits > 0 && (rest.starts_with('.') || rest.starts_with(')')) {
                rest[1..].trim().to_string()
            } else {
                t.to_string()
            }
        })
        .filter(|l| !l.is_empty())
        .collect()
}

fn pick(custom: Option<&String>, default: &str) -> (String, bool) {
    match custom.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        Some(c) => (c.to_string(), true),
        None => (default.to_string(), false),
    }
}

pub fn student_view(s: &Student, settings: &Settings) -> StudentView {
    let g = &settings.grading;
    let mut issues = Vec::new();
    let mut progress = Progress::default();

    let subjects: Vec<SubjectView> = settings
        .subjects
        .iter()
        .map(|cfg| {
            let entry = s.scores.get(&canonical_key(&cfg.source_key));
            let result = resolve_subject(entry, g);
            let (indicator, indicator_custom) = pick(s.subject_notes.get(&cfg.id), &cfg.indicator);
            match result.status {
                Status::Ok => progress.subjects_done += 1,
                Status::BelowScale => {
                    progress.subjects_done += 1;
                    issues.push(format!("{}: {}", cfg.name, result.message));
                }
                _ => issues.push(format!("{}: {}", cfg.name, result.message)),
            }
            SubjectView {
                subject_id: cfg.id.clone(),
                name: cfg.name.clone(),
                source_key: cfg.source_key.clone(),
                ph: entry.and_then(|e| e.ph),
                pts: entry.and_then(|e| e.pts),
                manual: entry.and_then(|e| e.manual),
                result,
                indicator,
                indicator_custom,
            }
        })
        .collect();
    progress.subjects_total = subjects.len();

    let mut atl_empty = 0;
    let atl: Vec<RatingView> = settings
        .atl
        .iter()
        .map(|cfg| {
            let entry = s.atl.get(&cfg.id);
            let input = entry.map(|e| e.grade.clone()).unwrap_or_default();
            let result = resolve_input(&input, g);
            let (indicator, indicator_custom) = pick(entry.map(|e| &e.indicator), &cfg.indicator);
            match result.status {
                Status::Ok | Status::BelowScale => progress.atl_done += 1,
                Status::Empty => atl_empty += 1,
                _ => issues.push(format!("ATL {}: {}", cfg.name, result.message)),
            }
            RatingView {
                atl_id: cfg.id.clone(),
                name: cfg.name.clone(),
                input,
                result,
                indicator,
                indicator_custom,
            }
        })
        .collect();
    progress.atl_total = atl.len();
    if atl_empty > 0 {
        issues.push(format!("ATL: {atl_empty} dari {} belum diisi", atl.len()));
    }

    let projects: Vec<ProjectView> = s
        .projects
        .iter()
        .map(|e| {
            let cfg = settings.projects.iter().find(|p| p.id == e.project_id);
            let name = cfg.map(|p| p.name.clone()).unwrap_or_else(|| "(proyek dihapus)".into());
            let understanding = resolve_input(&e.understanding, g);
            let inquiry = resolve_input(&e.inquiry, g);
            let mut done = true;
            for (label, r) in [("Understanding Level", &understanding), ("Inquiry & Research Skills", &inquiry)] {
                match r.status {
                    Status::Ok | Status::BelowScale => {}
                    Status::Empty => {
                        done = false;
                        issues.push(format!("{name}: {label} belum diisi"));
                    }
                    _ => {
                        done = false;
                        issues.push(format!("{name}: {label} — {}", r.message));
                    }
                }
            }
            if e.narrative.trim().is_empty() {
                done = false;
                issues.push(format!("{name}: narasi guru belum diisi"));
            }
            if cfg.is_none() {
                issues.push(format!("{name}: proyek tidak ada lagi di Pengaturan"));
            }
            if done {
                progress.projects_done += 1;
            }
            ProjectView {
                project_id: e.project_id.clone(),
                name,
                theme: cfg.map(|p| p.theme.clone()).unwrap_or_default(),
                central_idea: cfg.map(|p| p.central_idea.clone()).unwrap_or_default(),
                lines: cfg.map(|p| split_lines(&p.lines_of_inquiry)).unwrap_or_default(),
                understanding,
                inquiry,
                narrative: e.narrative.trim().to_string(),
                missing_config: cfg.is_none(),
            }
        })
        .collect();
    progress.projects = projects.len();
    if projects.is_empty() {
        issues.push("Belum ada data proyek (UOI)".into());
    }

    progress.attendance = !s.attendance.trim().is_empty();
    if !progress.attendance {
        issues.push("Kehadiran belum diisi".into());
    }

    StudentView {
        student: s.clone(),
        subjects,
        atl,
        projects,
        issues,
        progress,
    }
}

/// Teks predikat untuk rapor: "ME", atau "ME (92)" kalau angka ditampilkan.
pub fn mark_text(r: &Resolved, show_number: bool) -> String {
    match (&r.code, r.score) {
        (Some(c), Some(v)) if show_number => format!("{c} ({})", fmt_num(v)),
        (Some(c), _) => c.clone(),
        _ => "—".into(),
    }
}

/// "Meeting Expectations (ME)" untuk bagian UOI.
pub fn long_mark_text(r: &Resolved) -> String {
    match (&r.label, &r.code) {
        (Some(l), Some(c)) => format!("{l} ({c})"),
        (None, Some(c)) => c.clone(),
        _ => "—".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ProjectEntry, RatingEntry, ScoreEntry};

    #[test]
    fn lines_of_inquiry_strip_numbers() {
        assert_eq!(
            split_lines("1. Satu\n\n2) Dua\nTiga"),
            vec!["Satu".to_string(), "Dua".to_string(), "Tiga".to_string()]
        );
    }

    #[test]
    fn full_student_has_no_issues() {
        let settings = Settings::default();
        let mut s = Student {
            name: "Contoh".into(),
            class_name: "IX-1".into(),
            attendance: "98%".into(),
            ..Default::default()
        };
        for subj in &settings.subjects {
            s.scores.insert(
                canonical_key(&subj.source_key),
                ScoreEntry { ph: Some(92.0), pts: Some(94.0), manual: None },
            );
        }
        for a in &settings.atl {
            s.atl.insert(a.id.clone(), RatingEntry { grade: "ME".into(), indicator: String::new() });
        }
        s.projects.push(ProjectEntry {
            project_id: "p1".into(),
            understanding: "ME".into(),
            inquiry: "97".into(),
            narrative: "Bagus.".into(),
        });
        let v = student_view(&s, &settings);
        assert!(v.issues.is_empty(), "{:?}", v.issues);
        assert_eq!(v.subjects[0].result.code.as_deref(), Some("ME")); // 93
        assert_eq!(v.projects[0].inquiry.code.as_deref(), Some("EE"));
        assert_eq!(v.progress.subjects_done, 3);
        assert_eq!(v.progress.projects_done, 1);
    }

    #[test]
    fn custom_indicator_wins() {
        let settings = Settings::default();
        let mut s = Student::default();
        s.subject_notes.insert("ipa".into(), "  Khusus siswa ini ".into());
        let v = student_view(&s, &settings);
        assert_eq!(v.subjects[0].indicator, "Khusus siswa ini");
        assert!(v.subjects[0].indicator_custom);
        assert!(!v.subjects[1].indicator_custom);
    }
}
