//! Struktur data aplikasi.
//!
//! Semua data disimpan dalam SATU file JSON (lihat `store.rs`). Setiap struct
//! memakai `#[serde(default)]` supaya file lama tetap bisa dibuka walaupun
//! nanti ada kolom baru yang ditambahkan.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const DATA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppData {
    pub version: u32,
    pub settings: Settings,
    pub students: Vec<Student>,
    /// Judul kolom nilai yang pernah diimpor (mis. "IPA", "B. INDO"),
    /// dipakai sebagai pilihan "sumber nilai" di Pengaturan.
    pub score_columns: Vec<String>,
    pub meta: DataMeta,
}

impl Default for AppData {
    fn default() -> Self {
        Self {
            version: DATA_VERSION,
            settings: Settings::default(),
            students: Vec::new(),
            score_columns: Vec::new(),
            meta: DataMeta::default(),
        }
    }
}

/// Catatan administratif. Waktu ditulis sebagai stempel UTC ISO-8601
/// ("2026-11-14T02:30:00Z"), jadi bisa dibandingkan sebagai teks.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct DataMeta {
    pub last_change_at: Option<String>,
    pub last_external_backup_at: Option<String>,
    pub last_external_backup_dir: Option<String>,
    /// Jenjang pemilik data ini ("SD", "SMP", "SMA"); dipakai untuk memperingatkan saat
    /// backup suatu jenjang hendak dipulihkan ke jenjang lain. Kosong = data lama.
    pub level: String,
    /// Hanya terisi di file arsip.
    pub archive_label: Option<String>,
    pub archived_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Logo sekolah sebagai data URL ("data:image/png;base64,..."). Kosong = tanpa logo.
    pub logo: String,
    /// Posisi logo sekolah di kepala rapor: "left" (default), "right", atau "top".
    pub logo_position: String,
    /// Logo universitas/kampus sebagai data URL ("data:image/png;base64,..."). Kosong = tanpa logo.
    pub university_logo: String,
    /// Posisi logo universitas/kampus di kepala rapor: "right" (default), "left", atau "top".
    pub university_logo_position: String,
    /// Baris kecil di atas judul rapor.
    pub school_name: String,
    /// Judul besar rapor; boleh lebih dari satu baris (pisahkan dengan Enter).
    pub report_title: String,
    pub report_subtitle: String,
    pub academic_year: String,
    /// Tempat & tanggal di atas tanda tangan, mis. "Singaraja, 30 November 2026".
    pub place_date: String,
    pub principal_name: String,
    pub principal_title: String,
    pub homeroom_title: String,
    /// true = tanda tangan digital (gambar) dicetak di rapor;
    /// false = ruang tanda tangan dikosongkan untuk tanda tangan basah.
    pub digital_signature: bool,
    /// Tanda tangan kepala sekolah: data URL PNG berlatar transparan. Kosong = belum ada.
    pub principal_signature: String,
    /// "A4", "F4", atau "Letter".
    pub paper_size: String,
    /// Tampilkan angka di samping predikat (mis. "ME (92)"). Default: tidak.
    pub show_numeric_score: bool,
    /// Total hari efektif sekolah dalam satu semester (untuk menghitung % kehadiran dari S/I/A).
    pub effective_days: u32,
    pub grading: GradingConfig,
    /// Baris-baris bagian 3 (Core Subject). Bisa ditambah/dihapus.
    pub subjects: Vec<SubjectConfig>,
    /// Baris-baris bagian 1 (ATL & Learner Profile).
    pub atl: Vec<AtlConfig>,
    /// Daftar proyek (bagian 2, UOI).
    pub projects: Vec<ProjectConfig>,
    pub classes: Vec<ClassConfig>,
    /// Bank kalimat untuk narasi guru (bahasa Inggris).
    pub narrative_bank: Vec<Phrase>,
}

/// Satu kalimat di bank narasi. `category`: kode predikat (EE/ME/AE/BE),
/// "next" (langkah berikutnya), atau "general".
/// Isian otomatis: {name}, {their} (his/her), {they} (he/she), {them} (him/her), {theme}.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct Phrase {
    pub id: String,
    pub category: String,
    pub text: String,
}

fn default_narrative_bank() -> Vec<Phrase> {
    let rows: &[(&str, &str)] = &[
        ("EE", "{name} consistently exceeded the expectations of this unit and showed an exceptional understanding of {theme}."),
        ("EE", "{name} asked insightful questions and independently extended {their} inquiry beyond the lessons."),
        ("EE", "{name} explained {their} findings with clarity and confidence, often helping peers deepen their understanding."),
        ("EE", "{name} showed outstanding research skills, using a wide range of sources and evaluating them critically."),
        ("ME", "{name} met the expectations of this unit and showed a solid understanding of {theme}."),
        ("ME", "{name} engaged actively in the inquiry and completed {their} tasks carefully."),
        ("ME", "{name} communicated {their} ideas clearly and worked well with {their} group."),
        ("ME", "{name} used appropriate sources to investigate the key questions of the unit."),
        ("AE", "{name} is approaching the expectations of this unit and is beginning to understand its key concepts."),
        ("AE", "{name} took part in the activities but sometimes needed guidance to complete {their} work."),
        ("AE", "{name} is developing {their} research skills and benefits from support when organising information."),
        ("AE", "With more consistent effort, {name} will be able to show {their} understanding more fully."),
        ("BE", "{name} has not yet met the expectations of this unit and needs further support with its key concepts."),
        ("BE", "{name} found several tasks in this unit challenging and needs regular guidance to stay on track."),
        ("BE", "{name} is encouraged to participate more actively and to ask for help when needed."),
        ("next", "To progress further, {name} is encouraged to apply more critical thinking when comparing different ideas."),
        ("next", "As a next step, {name} should focus on supporting {their} explanations with evidence."),
        ("next", "{name} is encouraged to share {their} ideas more often during group discussions."),
        ("next", "{name} would benefit from planning {their} time more carefully so that tasks are completed thoroughly."),
        ("next", "We look forward to seeing {name} take on more leadership in group work."),
        ("general", "{name} showed a positive attitude towards learning throughout the unit."),
        ("general", "{name} reflected honestly on {their} own progress and set clear goals for improvement."),
    ];
    rows.iter()
        .enumerate()
        .map(|(i, (cat, text))| Phrase { id: format!("k{}", i + 1), category: (*cat).into(), text: (*text).into() })
        .collect()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            logo: default_school_logo(),
            logo_position: "left".into(),
            university_logo: String::new(),
            university_logo_position: "right".into(),
            school_name: "SMP Laboratorium Undiksha Singaraja".into(),
            report_title: "NATIONAL PLUS CLASS\nIB ENRICHMENT PROGRAM".into(),
            report_subtitle: "STUDENT PROGRESS REPORT — TERM 1".into(),
            academic_year: "2026/2027".into(),
            place_date: "Singaraja, 30 November 2026".into(),
            principal_name: String::new(),
            principal_title: "Principal".into(),
            homeroom_title: "Homeroom Teacher".into(),
            digital_signature: false,
            principal_signature: String::new(),
            paper_size: "A4".into(),
            show_numeric_score: false,
            effective_days: 100,
            grading: GradingConfig::default(),
            subjects: default_subjects(),
            atl: default_atl(),
            projects: default_projects(),
            classes: Vec::new(),
            narrative_bank: default_narrative_bank(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GradingConfig {
    /// Bobot nilai harian (PH), dalam persen.
    pub weight_ph: f64,
    /// Bobot nilai sumatif tengah semester (PTS), dalam persen.
    pub weight_pts: f64,
    /// Bulatkan nilai akhir ke bilangan bulat sebelum dikonversi (85,5 → 86).
    pub round_final: bool,
    pub bands: Vec<Band>,
}

impl Default for GradingConfig {
    fn default() -> Self {
        Self {
            weight_ph: 50.0,
            weight_pts: 50.0,
            round_final: true,
            bands: default_bands(),
        }
    }
}

/// Satu predikat beserta rentang nilainya.
/// Konversi memakai batas bawah (`min`); `max` hanya untuk keterangan di rapor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Band {
    pub code: String,
    pub label: String,
    pub min: f64,
    pub max: f64,
}

impl Default for Band {
    fn default() -> Self {
        Self {
            code: String::new(),
            label: String::new(),
            min: 0.0,
            max: 100.0,
        }
    }
}

pub fn default_bands() -> Vec<Band> {
    // Sesuai catatan tangan kepala sekolah (versi yang sudah dikoreksi).
    vec![
        band("EE", "Exceeding Expectations", 96.0, 100.0),
        band("ME", "Meeting Expectations", 91.0, 95.0),
        band("AE", "Approaching Expectations", 86.0, 90.0),
        band("BE", "Below Expectations", 80.0, 85.0),
    ]
}

fn band(code: &str, label: &str, min: f64, max: f64) -> Band {
    Band {
        code: code.into(),
        label: label.into(),
        min,
        max,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SubjectConfig {
    pub id: String,
    /// Nama yang tercetak di rapor.
    pub name: String,
    /// Judul kolom di file nilai (mis. "IPA", "B. INDO").
    pub source_key: String,
    /// Indikator bawaan (statis) yang tercetak di rapor.
    pub indicator: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AtlConfig {
    pub id: String,
    pub name: String,
    pub indicator: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProjectConfig {
    pub id: String,
    /// Nama singkat, mis. "Project 1".
    pub name: String,
    /// Transdisciplinary Theme (judul/tema proyek).
    pub theme: String,
    pub central_idea: String,
    /// Satu baris = satu line of inquiry.
    pub lines_of_inquiry: String,
    /// Nama fasilitator (hanya catatan, tidak dicetak).
    pub facilitators: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ClassConfig {
    pub name: String,
    pub homeroom_teacher: String,
    /// Tanda tangan wali kelas: data URL PNG berlatar transparan. Kosong = belum ada.
    pub homeroom_signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Student {
    /// ID internal yang tidak berubah (biasanya NIS saat pertama diimpor).
    pub id: String,
    /// Urutan siswa di kelasnya, sama dengan urutan baris di file Excel saat diimpor
    /// (nomor absen). Kosong = data lama/ditambah sebelum ada urutan → di belakang, menurut abjad.
    pub order: Option<u32>,
    pub nis: String,
    pub nisn: String,
    pub name: String,
    /// "L" atau "P".
    pub gender: String,
    pub class_name: String,
    /// Bebas, mis. "98%" atau "98% (Present)".
    pub attendance: String,
    /// Jumlah hari Sakit (S), Izin (I), dan Alpa (A) (opsional, untuk hitung % otomatis).
    pub attendance_s: Option<u32>,
    pub attendance_i: Option<u32>,
    pub attendance_a: Option<u32>,
    /// Nilai per kolom sumber; kunci = `canonical_key(judul kolom)`.
    pub scores: BTreeMap<String, ScoreEntry>,
    /// Indikator khusus siswa ini per mapel (id mapel → teks). Kosong = pakai bawaan.
    pub subject_notes: BTreeMap<String, String>,
    /// Penilaian ATL per atribut (id atribut → isian).
    pub atl: BTreeMap<String, RatingEntry>,
    pub projects: Vec<ProjectEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct ScoreEntry {
    /// Nilai harian.
    pub ph: Option<f64>,
    /// Nilai sumatif tengah semester.
    pub pts: Option<f64>,
    /// Nilai akhir yang diisi manual; kalau ada, mengalahkan rumus PH/PTS.
    pub manual: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct RatingEntry {
    /// Boleh kode predikat ("ME") atau angka ("92").
    pub grade: String,
    /// Indikator khusus siswa ini. Kosong = pakai indikator bawaan.
    pub indicator: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct ProjectEntry {
    pub project_id: String,
    /// Understanding Level: kode predikat atau angka.
    pub understanding: String,
    /// Inquiry & Research Skills: kode predikat atau angka.
    pub inquiry: String,
    /// Teacher Narrative Feedback.
    pub narrative: String,
}

/// Kunci pembanding judul kolom nilai: huruf besar, hanya huruf & angka.
/// "B. Indo" dan "B. INDO" sama-sama menjadi "BINDO".
pub fn canonical_key(label: &str) -> String {
    label
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_uppercase())
        .collect()
}

fn default_subjects() -> Vec<SubjectConfig> {
    // Indikator di bawah ini hanya CONTOH, silakan diganti di Pengaturan.
    vec![
        SubjectConfig {
            id: "ipa".into(),
            name: "Science (IPA)".into(),
            source_key: "IPA".into(),
            indicator: "Applies scientific inquiry to investigate natural phenomena, interprets data accurately, and communicates conclusions clearly.".into(),
        },
        SubjectConfig {
            id: "bindo".into(),
            name: "Language: Bahasa Indonesia".into(),
            source_key: "B. INDO".into(),
            indicator: "Reads, understands, and produces a variety of texts in Bahasa Indonesia with accurate structure and language use.".into(),
        },
        SubjectConfig {
            id: "bing".into(),
            name: "Language: English".into(),
            source_key: "B. ING".into(),
            indicator: "Communicates ideas in English through reading, writing, listening, and speaking with growing accuracy and confidence.".into(),
        },
    ]
}

fn default_atl() -> Vec<AtlConfig> {
    // Diambil dari contoh template rapor.
    let rows = [
        ("inquirer", "Inquirer", "Demonstrates natural curiosity, asks open-ended questions, and actively engages in inquiry cycles."),
        ("caring", "Caring", "Highly empathetic; regularly helps peers who struggle with tasks and shows organic kindness during group work."),
        ("communicator", "Communicator", "Confidently shares ideas verbally and listens actively when peers are presenting."),
        ("reflective", "Reflective", "Thoughtfully assesses own work during self-reflection sessions and independently identifies areas to improve."),
        ("thinking", "Thinking Skills", "Effectively analyzes diverse information to form logical, independent arguments."),
        ("social", "Social Skills", "Cooperates well in diverse group settings and shows positive conflict-resolution skills."),
        ("selfmanagement", "Self-Management", "Stays on task and maintains organization of learning materials."),
    ];
    rows.iter()
        .map(|(id, name, ind)| AtlConfig {
            id: (*id).into(),
            name: (*name).into(),
            indicator: (*ind).into(),
        })
        .collect()
}

fn default_projects() -> Vec<ProjectConfig> {
    (1..=7)
        .map(|i| {
            let mut p = ProjectConfig {
                id: format!("p{i}"),
                name: format!("Project {i}"),
                ..Default::default()
            };
            if i == 1 {
                // Contoh dari template; proyek lain diisi sendiri.
                p.theme = "Who We Are".into();
                p.central_idea = "Human body systems are interconnected and impact our overall health and well-being.".into();
                p.lines_of_inquiry = "The functions of different body systems.\nHow personal choices impact physical and mental health.".into();
            }
            p
        })
        .collect()
}

/// Nilai angka romawi kelas (VII → 7, IX → 9). `None` kalau bukan angka romawi.
pub fn roman_to_int(s: &str) -> Option<u32> {
    let mut total = 0i64;
    let mut prev = 0i64;
    if s.is_empty() {
        return None;
    }
    for c in s.chars().rev() {
        let v = match c.to_ascii_uppercase() {
            'I' => 1,
            'V' => 5,
            'X' => 10,
            'L' => 50,
            _ => return None,
        };
        if v < prev {
            total -= v;
        } else {
            total += v;
            prev = v;
        }
    }
    u32::try_from(total).ok().filter(|v| *v > 0)
}

/// Kunci urut kelas yang "alami": VII-2 < VII-10 < VIII-1 < IX-1.
pub fn class_sort_key(name: &str) -> (u32, u32, String) {
    let (left, right) = name.split_once('-').unwrap_or((name, ""));
    let grade = roman_to_int(left.trim())
        .or_else(|| left.trim().parse().ok())
        .unwrap_or(u32::MAX);
    let num = right.trim().parse().unwrap_or(u32::MAX);
    (grade, num, name.to_string())
}

/// "IX-1" → "Grade 9 (IX-1)".
pub fn grade_class_label(class_name: &str) -> String {
    let left = class_name.split('-').next().unwrap_or("").trim();
    match roman_to_int(left).or_else(|| left.parse().ok()) {
        Some(g) => format!("Grade {g} ({class_name})"),
        None => class_name.to_string(),
    }
}

/// Urutan tampil siswa di seluruh aplikasi (daftar, nilai, isi rapor, cetak, PDF, form, leger):
/// kelas, lalu urutan di file Excel, lalu nama (untuk siswa yang belum punya urutan).
pub fn compare_students(a: &Student, b: &Student) -> std::cmp::Ordering {
    class_sort_key(&a.class_name)
        .cmp(&class_sort_key(&b.class_name))
        .then_with(|| a.order.unwrap_or(u32::MAX).cmp(&b.order.unwrap_or(u32::MAX)))
        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
}

/// Urutan untuk siswa yang ditambahkan ke akhir sebuah kelas.
pub fn next_order(students: &[Student], class_name: &str) -> u32 {
    students
        .iter()
        .filter(|s| s.class_name == class_name)
        .filter_map(|s| s.order)
        .max()
        .map_or(1, |m| m + 1)
}

/// Logo bawaan sekolah (Sekolah Lab Undiksha) sebagai data URL PNG.
pub fn default_school_logo() -> String {
    use base64::Engine;
    const BYTES: &[u8] = include_bytes!("../../ui/logo-sekolah.png");
    format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(BYTES))
}

impl Settings {
    /// Hari efektif yang sah (minimal 1, default 100 kalau file lama bernilai 0).
    pub fn effective_days(&self) -> u32 {
        if self.effective_days == 0 { 100 } else { self.effective_days }
    }
}

/// Hitung teks kehadiran "98% (Present)" dari jumlah hadir dan total hari efektif.
pub fn format_attendance_ratio(present: f64, total: f64) -> Option<String> {
    if !present.is_finite() || !total.is_finite() || total <= 0.0 {
        return None;
    }
    let pct = crate::grading::round_half_up((present.max(0.0) / total) * 100.0).clamp(0.0, 100.0) as u32;
    Some(format!("{pct}% (Present)"))
}

/// Hitung teks kehadiran "98% (Present)" dari jumlah hari Sakit (S), Izin (I), Alpa (A)
/// dan total hari efektif (`effective_days`). `None` kalau S/I/A semuanya belum diisi.
pub fn format_attendance_sia(s: Option<u32>, i: Option<u32>, a: Option<u32>, effective_days: u32) -> Option<String> {
    if s.is_none() && i.is_none() && a.is_none() {
        return None;
    }
    let total = if effective_days == 0 { 100 } else { effective_days };
    let absent = s.unwrap_or(0).saturating_add(i.unwrap_or(0)).saturating_add(a.unwrap_or(0));
    let present = total.saturating_sub(absent);
    format_attendance_ratio(present as f64, total as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_key_ignores_case_and_punctuation() {
        assert_eq!(canonical_key("B. INDO"), "BINDO");
        assert_eq!(canonical_key("b. indo"), "BINDO");
        assert_eq!(canonical_key(" IPA "), "IPA");
    }

    #[test]
    fn class_sorting_is_natural() {
        let mut v = vec!["IX-1", "VII-10", "VIII-1", "VII-2"];
        v.sort_by_key(|c| class_sort_key(c));
        assert_eq!(v, vec!["VII-2", "VII-10", "VIII-1", "IX-1"]);
    }

    #[test]
    fn grade_label_from_roman() {
        assert_eq!(grade_class_label("IX-1"), "Grade 9 (IX-1)");
        assert_eq!(grade_class_label("VIII-3"), "Grade 8 (VIII-3)");
        assert_eq!(grade_class_label("Khusus"), "Khusus");
    }

    #[test]
    fn old_files_without_new_fields_still_load() {
        let data: AppData = serde_json::from_str(r#"{"students":[{"name":"A"}]}"#).unwrap();
        assert_eq!(data.students[0].name, "A");
        assert_eq!(data.settings.grading.bands.len(), 4);
        // file lama tanpa bank kalimat tetap mendapat kalimat bawaan
        let old: AppData = serde_json::from_str(r#"{"settings":{"school_name":"X"}}"#).unwrap();
        assert_eq!(old.settings.school_name, "X");
        assert!(old.settings.narrative_bank.len() > 10);
    }
}
