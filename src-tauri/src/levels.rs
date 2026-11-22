//! Jenjang sekolah (SD / SMP / SMA). Setiap jenjang punya folder data sendiri: siswa, pengaturan
//! (kop, kepala sekolah, mapel, indikator, proyek, wali kelas, tanda tangan), backup, dan arsip.
//!
//! Tata letak folder (di bawah folder data utama, mis. `Dokumen\E-Raport`):
//! - SMP: folder utama itu sendiri — data yang sudah ada sejak awal tetap di tempatnya;
//! - SD:  `SD\`;  SMA: `SMA\`.
//!
//! Kata sandi aplikasi (`keamanan.json`) dan pilihan jenjang terakhir (`jenjang.json`)
//! ada di folder utama dan berlaku untuk semua jenjang.

use crate::importer::new_student_id;
use crate::model::{class_sort_key, AppData, Settings};
use crate::names::normalize_name;
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub struct Level {
    pub id: &'static str,
    pub label: &'static str,
    pub school: &'static str,
    /// Rentang tingkat kelas (1–6, 7–9, 10–12).
    pub grades: (u32, u32),
}

pub const LEVELS: [Level; 3] = [
    Level { id: "SD", label: "SD", school: "SD Laboratorium Undiksha Singaraja", grades: (1, 6) },
    Level { id: "SMP", label: "SMP", school: "SMP Laboratorium Undiksha Singaraja", grades: (7, 9) },
    Level { id: "SMA", label: "SMA", school: "SMA Laboratorium Undiksha Singaraja", grades: (10, 12) },
];
pub const DEFAULT_LEVEL: &str = "SMP";
const ACTIVE_FILE: &str = "jenjang.json";

pub fn find(id: &str) -> Option<&'static Level> {
    LEVELS.iter().find(|l| l.id.eq_ignore_ascii_case(id.trim()))
}

/// Folder data jenjang. SMP memakai folder utama supaya data lama tidak perlu dipindah.
pub fn level_dir(root: &Path, id: &str) -> PathBuf {
    if id == DEFAULT_LEVEL { root.to_path_buf() } else { root.join(id) }
}

/// Jenjang terakhir yang dibuka (default SMP).
pub fn read_active(root: &Path) -> String {
    std::fs::read_to_string(root.join(ACTIVE_FILE))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v.get("active").and_then(|a| a.as_str()).map(str::to_string))
        .and_then(|id| find(&id).map(|l| l.id.to_string()))
        .unwrap_or_else(|| DEFAULT_LEVEL.to_string())
}

pub fn write_active(root: &Path, id: &str) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    crate::store::write_atomic(&root.join(ACTIVE_FILE), format!("{{\"active\":\"{id}\"}}").as_bytes())
}

/// Jenjang dari nama kelas: "I-C" → SD, "IX-1" → SMP, "X-B" → SMA. `None` kalau tidak jelas.
pub fn level_of_class(class: &str) -> Option<&'static str> {
    let (grade, _, _) = class_sort_key(class);
    LEVELS.iter().find(|l| (l.grades.0..=l.grades.1).contains(&grade)).map(|l| l.id)
}

/// Pengaturan yang disalin saat membuka jenjang baru: predikat, mapel, ATL, proyek, bank kalimat,
/// kop/judul, logo, dll. Identitas yang pasti berbeda per jenjang TIDAK disalin: nama sekolah,
/// kepala sekolah beserta tanda tangannya, dan daftar kelas (wali kelas + tanda tangan).
pub fn copy_shared_settings(from: &Settings, to: &mut Settings) {
    let school = std::mem::take(&mut to.school_name);
    *to = from.clone();
    to.school_name = school;
    to.principal_name.clear();
    to.principal_signature.clear();
    to.classes.clear();
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct MoveReport {
    pub moved: usize,
    /// Siswa yang sudah ada di jenjang tujuan (NISN sama, atau nama sama di kelas yang sama).
    pub skipped: Vec<String>,
}

/// Pindahkan kelas (siswa beserta nilai & isian rapornya, juga wali kelas & tanda tangannya)
/// dari `src` ke `dst`. ID yang bentrok diganti; siswa yang sudah ada di tujuan tidak digandakan.
pub fn move_classes(src: &mut AppData, dst: &mut AppData, classes: &[String]) -> MoveReport {
    let targets: HashSet<&str> = classes.iter().map(|c| c.as_str()).collect();
    let mut report = MoveReport::default();
    let (mut moving, staying): (Vec<_>, Vec<_>) =
        std::mem::take(&mut src.students).into_iter().partition(|s| targets.contains(s.class_name.as_str()));
    moving.sort_by(crate::model::compare_students);
    src.students = staying;
    for mut s in moving {
        let norm = normalize_name(&s.name);
        let exists = dst.students.iter().any(|d| {
            (!s.nisn.is_empty() && d.nisn == s.nisn) || (d.class_name == s.class_name && normalize_name(&d.name) == norm)
        });
        if exists {
            report.skipped.push(format!("{} ({})", s.name, s.class_name));
            continue;
        }
        s.id = new_student_id(dst, &s.id);
        if dst.students.iter().any(|d| d.class_name == s.class_name) {
            s.order = Some(crate::model::next_order(&dst.students, &s.class_name));
        }
        dst.students.push(s);
        report.moved += 1;
    }
    for c in src.settings.classes.iter().filter(|c| targets.contains(c.name.as_str())) {
        match dst.settings.classes.iter_mut().find(|d| d.name == c.name) {
            Some(d) => {
                if d.homeroom_teacher.trim().is_empty() {
                    d.homeroom_teacher = c.homeroom_teacher.clone();
                }
                if d.homeroom_signature.is_empty() {
                    d.homeroom_signature = c.homeroom_signature.clone();
                }
            }
            None => dst.settings.classes.push(c.clone()),
        }
    }
    src.settings.classes.retain(|c| !targets.contains(c.name.as_str()));
    for col in &src.score_columns {
        if !dst.score_columns.contains(col) {
            dst.score_columns.push(col.clone());
        }
    }
    crate::importer::ensure_class_configs(dst);
    report
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use crate::model::{ClassConfig, Student};

    #[test]
    fn folders_and_class_levels() {
        let root = Path::new("C:/E-Raport");
        assert_eq!(level_dir(root, "SMP"), root.to_path_buf());
        assert_eq!(level_dir(root, "SD"), root.join("SD"));
        assert_eq!(level_of_class("I-C"), Some("SD"));
        assert_eq!(level_of_class("VI-2"), Some("SD"));
        assert_eq!(level_of_class("IX-1"), Some("SMP"));
        assert_eq!(level_of_class("X-B"), Some("SMA"));
        assert_eq!(level_of_class("XII-IPA1"), Some("SMA"));
        assert_eq!(level_of_class("Kelas Bintang"), None);
    }

    #[test]
    fn active_level_is_remembered() {
        let root = std::env::temp_dir().join(format!("eraport-levels-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(read_active(&root), "SMP");
        write_active(&root, "SD").unwrap();
        assert_eq!(read_active(&root), "SD");
        std::fs::write(root.join(ACTIVE_FILE), "{\"active\":\"TK\"}").unwrap();
        assert_eq!(read_active(&root), "SMP", "jenjang tak dikenal → bawaan");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn copy_settings_keeps_identity_out() {
        let mut smp = Settings::default();
        smp.principal_name = "Kepsek SMP".into();
        smp.principal_signature = "data:image/png;base64,X".into();
        smp.classes = vec![ClassConfig { name: "IX-1".into(), ..Default::default() }];
        smp.grading.weight_ph = 40.0;
        let mut sd = Settings { school_name: "SD Laboratorium Undiksha Singaraja".into(), ..Default::default() };
        copy_shared_settings(&smp, &mut sd);
        assert_eq!(sd.school_name, "SD Laboratorium Undiksha Singaraja");
        assert_eq!(sd.grading.weight_ph, 40.0);
        assert!(sd.principal_name.is_empty() && sd.principal_signature.is_empty() && sd.classes.is_empty());
    }

    #[test]
    fn move_classes_between_levels() {
        let st = |id: &str, name: &str, class: &str, nisn: &str| Student {
            id: id.into(),
            name: name.into(),
            class_name: class.into(),
            nisn: nisn.into(),
            attendance: "99% (Present)".into(),
            ..Default::default()
        };
        let mut smp = AppData::default();
        smp.students = vec![st("1", "Siswa SMP", "IX-1", ""), st("2", "Anak SD", "I-C", "3195900273"), st("3", "Anak SD Dua", "I-C", "")];
        smp.settings.classes = vec![
            ClassConfig { name: "IX-1".into(), ..Default::default() },
            ClassConfig { name: "I-C".into(), homeroom_teacher: "Bu Wali SD".into(), ..Default::default() },
        ];
        let mut sd = AppData::default();
        sd.students = vec![st("2", "Anak SD Dua", "I-C", "")]; // sudah ada di SD; id "2" juga terpakai
        let rep = move_classes(&mut smp, &mut sd, &["I-C".into()]);
        assert_eq!(rep.moved, 1);
        assert_eq!(rep.skipped, vec!["Anak SD Dua (I-C)".to_string()]);
        assert_eq!(smp.students.len(), 1);
        assert_eq!(smp.settings.classes.len(), 1);
        assert_eq!(sd.students.len(), 2);
        let moved = sd.students.iter().find(|s| s.name == "Anak SD").unwrap();
        assert_ne!(moved.id, "2", "id bentrok diganti");
        assert_eq!(moved.attendance, "99% (Present)", "isian rapor ikut pindah");
        assert_eq!(sd.settings.classes.iter().find(|c| c.name == "I-C").unwrap().homeroom_teacher, "Bu Wali SD");
    }
}
