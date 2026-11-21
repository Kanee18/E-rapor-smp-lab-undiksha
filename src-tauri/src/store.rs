//! Penyimpanan data ke satu file JSON, plus backup, pemulihan, dan arsip semester.
//!
//! - `data.json` ditulis secara atomik (tulis ke file sementara lalu rename),
//!   jadi file tidak setengah tertulis kalau laptop mati mendadak.
//! - Setiap hari aplikasi dibuka, salinan `data.json` disimpan di `backup/`
//!   (30 hari terakhir).
//! - Kalau `data.json` rusak, file itu TIDAK ditimpa: diganti nama menjadi
//!   `data-rusak-<waktu>.json` dan aplikasi mulai dengan data kosong.
//! - Backup ke luar (flashdisk / folder lain) dicatat waktunya supaya aplikasi
//!   bisa mengingatkan kalau ada perubahan yang belum di-backup.
//! - Sebelum data diganti (pulihkan / semester baru), data saat itu selalu
//!   disalin dulu ke `backup/`.

use crate::model::{AppData, DATA_VERSION};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const DATA_FILE: &str = "data.json";
const BACKUP_DIR: &str = "backup";
const ARCHIVE_DIR: &str = "arsip";
const EXTERNAL_DIR: &str = "E-Raport backup";
const KEEP_DAILY_BACKUPS: usize = 30;
pub const CUSTOM_TEMPLATE_FILE: &str = "template-rapor.html";

pub struct Store {
    pub dir: PathBuf,
    pub data: AppData,
}

fn timestamp(fmt: &str) -> String {
    jiff::Zoned::now().strftime(fmt).to_string()
}

/// Stempel waktu UTC ("2026-11-14T02:30:00Z"); bisa dibandingkan sebagai teks.
pub fn now_utc() -> String {
    jiff::Timestamp::now().strftime("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Nama file aman untuk Windows.
fn safe_file_name(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| if r#"\/:*?"<>|"#.contains(c) || c.is_control() { '-' } else { c })
        .collect();
    let t = cleaned.trim().trim_matches('.').to_string();
    if t.is_empty() {
        "arsip".into()
    } else {
        t.chars().take(80).collect()
    }
}

/// Ringkasan isi sebuah file backup/arsip, untuk ditampilkan sebelum dipulihkan.
#[derive(Debug, Clone, Serialize)]
pub struct BackupInfo {
    pub path: String,
    pub file: String,
    pub students: usize,
    pub classes: Vec<String>,
    pub academic_year: String,
    pub subtitle: String,
    pub last_change_at: Option<String>,
    pub archive_label: Option<String>,
    pub archived_at: Option<String>,
}

/// Pilihan saat memulai semester / tahun ajaran baru.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct NewTermOptions {
    /// Nama arsip, mis. "2026-2027 Term 1".
    pub archive_label: String,
    pub academic_year: String,
    pub report_subtitle: String,
    /// true = daftar siswa tetap (ganti term); false = siswa dikosongkan (tahun ajaran baru).
    pub keep_students: bool,
}

pub fn read_data_file(path: &Path) -> Result<AppData, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("File tidak bisa dibaca: {e}"))?;
    serde_json::from_str::<AppData>(&text).map_err(|_| "File ini bukan backup E-Raport (isinya tidak dikenali)".to_string())
}

pub fn backup_info(path: &Path) -> Result<BackupInfo, String> {
    let data = read_data_file(path)?;
    let mut classes: Vec<String> = data.students.iter().map(|s| s.class_name.clone()).collect();
    classes.sort_by_key(|c| crate::model::class_sort_key(c));
    classes.dedup();
    Ok(BackupInfo {
        path: path.display().to_string(),
        file: path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
        students: data.students.len(),
        classes,
        academic_year: data.settings.academic_year.clone(),
        subtitle: data.settings.report_subtitle.clone(),
        last_change_at: data.meta.last_change_at.clone(),
        archive_label: data.meta.archive_label.clone(),
        archived_at: data.meta.archived_at.clone(),
    })
}

impl Store {
    /// Buka (atau buat) penyimpanan di `dir`. Mengembalikan peringatan kalau ada masalah.
    pub fn open(dir: PathBuf) -> Result<(Store, Option<String>), String> {
        fs::create_dir_all(&dir).map_err(|e| format!("Folder data tidak bisa dibuat ({}): {e}", dir.display()))?;
        let path = dir.join(DATA_FILE);
        if !path.exists() {
            let mut store = Store { dir, data: AppData::default() };
            store.save()?;
            return Ok((store, None));
        }
        let text = fs::read_to_string(&path).map_err(|e| format!("data.json tidak bisa dibaca: {e}"))?;
        match serde_json::from_str::<AppData>(&text) {
            Ok(mut data) => {
                data.version = DATA_VERSION;
                if data.settings.logo.is_empty() {
                    data.settings.logo = crate::model::default_school_logo();
                }
                let store = Store { dir, data };
                let warning = store.daily_backup().err().map(|e| format!("Backup harian gagal: {e}"));
                Ok((store, warning))
            }
            Err(err) => {
                let broken = dir.join(format!("data-rusak-{}.json", timestamp("%Y%m%d-%H%M%S")));
                fs::rename(&path, &broken).map_err(|e| format!("data.json rusak dan tidak bisa dipindahkan: {e}"))?;
                let mut store = Store { dir, data: AppData::default() };
                store.save()?;
                Ok((
                    store,
                    Some(format!(
                        "File data.json tidak bisa dibaca ({err}). File lama disimpan sebagai {} — cek folder backup untuk memulihkan.",
                        broken.display()
                    )),
                ))
            }
        }
    }

    pub fn data_path(&self) -> PathBuf {
        self.dir.join(DATA_FILE)
    }

    pub fn backup_dir(&self) -> PathBuf {
        self.dir.join(BACKUP_DIR)
    }

    pub fn archive_dir(&self) -> PathBuf {
        self.dir.join(ARCHIVE_DIR)
    }

    pub fn custom_template_path(&self) -> PathBuf {
        self.dir.join(CUSTOM_TEMPLATE_FILE)
    }

    pub fn custom_template(&self) -> Result<Option<String>, String> {
        let p = self.custom_template_path();
        if !p.exists() {
            return Ok(None);
        }
        fs::read_to_string(&p)
            .map(Some)
            .map_err(|e| format!("{CUSTOM_TEMPLATE_FILE} tidak bisa dibaca: {e}"))
    }

    /// Simpan perubahan data (mencatat waktu perubahan).
    pub fn save(&mut self) -> Result<(), String> {
        self.data.meta.last_change_at = Some(now_utc());
        self.write()
    }

    /// Simpan tanpa menandai ada perubahan isi (dipakai untuk catatan backup).
    fn write(&self) -> Result<(), String> {
        let json = serde_json::to_string_pretty(&self.data).map_err(|e| e.to_string())?;
        write_atomic(&self.data_path(), json.as_bytes())
    }

    /// Salinan dengan jam di folder backup lokal.
    pub fn backup_now(&self) -> Result<PathBuf, String> {
        self.copy_to_backup("data")
    }

    pub(crate) fn copy_to_backup(&self, prefix: &str) -> Result<PathBuf, String> {
        let dir = self.backup_dir();
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let dest = dir.join(format!("{prefix}-{}.json", timestamp("%Y-%m-%d_%H%M%S")));
        fs::copy(self.data_path(), &dest).map_err(|e| format!("Backup gagal: {e}"))?;
        Ok(dest)
    }

    /// Backup ke folder lain (mis. flashdisk): `<folder>\E-Raport backup\data-<waktu>.json`.
    pub fn backup_to(&mut self, folder: &Path) -> Result<PathBuf, String> {
        let dir = if folder.file_name().is_some_and(|n| n == EXTERNAL_DIR) {
            folder.to_path_buf()
        } else {
            folder.join(EXTERNAL_DIR)
        };
        fs::create_dir_all(&dir).map_err(|e| format!("Folder tujuan tidak bisa dibuat ({}): {e}", dir.display()))?;
        let stamp = timestamp("%Y-%m-%d_%H%M%S");
        let dest = dir.join(format!("data-{stamp}.json"));
        fs::copy(self.data_path(), &dest).map_err(|e| format!("Backup gagal (flashdisk penuh atau terkunci?): {e}"))?;
        // pastikan salinannya utuh
        read_data_file(&dest).map_err(|_| "Backup tersalin tapi tidak bisa dibaca ulang; coba folder lain".to_string())?;
        let template = self.custom_template_path();
        if template.exists() {
            let _ = fs::copy(&template, dir.join(format!("template-rapor-{stamp}.html")));
        }
        self.data.meta.last_external_backup_at = Some(now_utc());
        self.data.meta.last_external_backup_dir = Some(folder.display().to_string());
        self.write()?;
        Ok(dest)
    }

    /// Ganti data dengan isi file backup/arsip. Data saat ini disalin dulu ke backup.
    pub fn restore_from(&mut self, path: &Path) -> Result<PathBuf, String> {
        let mut incoming = read_data_file(path)?;
        let safety = self.copy_to_backup("sebelum-pulih")?;
        // catatan backup luar tetap mengikuti laptop ini
        incoming.meta.last_external_backup_at = self.data.meta.last_external_backup_at.clone();
        incoming.meta.last_external_backup_dir = self.data.meta.last_external_backup_dir.clone();
        incoming.meta.archive_label = None;
        incoming.meta.archived_at = None;
        incoming.version = DATA_VERSION;
        self.data = incoming;
        self.save()?;
        Ok(safety)
    }

    pub fn list_archives(&self) -> Vec<BackupInfo> {
        let Ok(entries) = fs::read_dir(self.archive_dir()) else {
            return Vec::new();
        };
        let mut list: Vec<BackupInfo> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .filter_map(|p| backup_info(&p).ok())
            .collect();
        list.sort_by(|a, b| b.archived_at.cmp(&a.archived_at));
        list
    }

    /// Arsipkan data sekarang, lalu mulai semester/tahun ajaran baru:
    /// pengaturan dibawa; nilai, ATL, proyek, narasi, dan kehadiran dikosongkan.
    pub fn start_new_term(&mut self, opts: &NewTermOptions) -> Result<BackupInfo, String> {
        let label = opts.archive_label.trim();
        if label.is_empty() {
            return Err("Nama arsip wajib diisi".into());
        }
        let dir = self.archive_dir();
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let mut archived = self.data.clone();
        archived.meta.archive_label = Some(label.to_string());
        archived.meta.archived_at = Some(now_utc());
        let mut dest = dir.join(format!("{}.json", safe_file_name(label)));
        if dest.exists() {
            dest = dir.join(format!("{} {}.json", safe_file_name(label), timestamp("%Y%m%d-%H%M%S")));
        }
        let json = serde_json::to_string_pretty(&archived).map_err(|e| e.to_string())?;
        write_atomic(&dest, json.as_bytes())?;
        let info = backup_info(&dest)?;
        self.copy_to_backup("sebelum-semester-baru")?;

        let d = &mut self.data;
        if !opts.academic_year.trim().is_empty() {
            d.settings.academic_year = opts.academic_year.trim().to_string();
        }
        if !opts.report_subtitle.trim().is_empty() {
            d.settings.report_subtitle = opts.report_subtitle.trim().to_string();
        }
        if opts.keep_students {
            for s in &mut d.students {
                s.scores.clear();
                s.atl.clear();
                s.projects.clear();
                s.subject_notes.clear();
                s.attendance.clear();
                s.attendance_s = None;
                s.attendance_i = None;
                s.attendance_a = None;
            }
        } else {
            d.students.clear();
            d.settings.classes.clear();
        }
        d.score_columns.clear();
        self.save()?;
        Ok(info)
    }

    fn daily_backup(&self) -> Result<(), String> {
        let dir = self.backup_dir();
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let dest = dir.join(format!("harian-{}.json", timestamp("%Y-%m-%d")));
        if !dest.exists() {
            fs::copy(self.data_path(), &dest).map_err(|e| e.to_string())?;
        }
        let mut daily: Vec<PathBuf> = fs::read_dir(&dir)
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("harian-")))
            .collect();
        daily.sort();
        while daily.len() > KEEP_DAILY_BACKUPS {
            let old = daily.remove(0);
            let _ = fs::remove_file(old);
        }
        Ok(())
    }
}

pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, bytes).map_err(|e| format!("Gagal menyimpan data: {e}"))?;
    // Di Windows, rename menimpa file tujuan (MoveFileEx + REPLACE_EXISTING).
    fs::rename(&tmp, path).map_err(|e| format!("Gagal menyimpan data: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{RatingEntry, ScoreEntry, Student};

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("eraport-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    fn with_student(store: &mut Store) {
        let mut s = Student { id: "1".into(), name: "Made Dewi".into(), class_name: "IX-1".into(), attendance: "98%".into(), ..Default::default() };
        s.scores.insert("IPA".into(), ScoreEntry { ph: Some(90.0), pts: Some(92.0), manual: None });
        s.atl.insert("caring".into(), RatingEntry { grade: "EE".into(), indicator: String::new() });
        store.data.students.push(s);
        store.save().unwrap();
    }

    #[test]
    fn save_and_reload() {
        let dir = temp_dir("reload");
        let (mut store, warn) = Store::open(dir.clone()).unwrap();
        assert!(warn.is_none());
        store.data.settings.academic_year = "2030/2031".into();
        store.save().unwrap();
        assert!(store.data.meta.last_change_at.is_some());
        let (store2, _) = Store::open(dir.clone()).unwrap();
        assert_eq!(store2.data.settings.academic_year, "2030/2031");
        assert!(dir.join("backup").read_dir().unwrap().count() >= 1, "backup harian dibuat");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn broken_file_is_kept_not_overwritten() {
        let dir = temp_dir("broken");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(DATA_FILE), "{ ini bukan json").unwrap();
        let (_store, warn) = Store::open(dir.clone()).unwrap();
        assert!(warn.unwrap().contains("tidak bisa dibaca"));
        let kept = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().starts_with("data-rusak-"));
        assert!(kept);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn external_backup_then_restore() {
        let dir = temp_dir("ext");
        let usb = temp_dir("usb");
        fs::create_dir_all(&usb).unwrap();
        let (mut store, _) = Store::open(dir.clone()).unwrap();
        with_student(&mut store);
        let changed_before = store.data.meta.last_change_at.clone();

        let file = store.backup_to(&usb).unwrap();
        assert!(file.starts_with(usb.join(EXTERNAL_DIR)));
        assert!(store.data.meta.last_external_backup_at.is_some());
        assert_eq!(store.data.meta.last_change_at, changed_before, "backup tidak dihitung sebagai perubahan isi");
        let info = backup_info(&file).unwrap();
        assert_eq!((info.students, info.classes.clone()), (1, vec!["IX-1".to_string()]));

        // data rusak/terhapus, lalu dipulihkan dari flashdisk
        store.data.students.clear();
        store.save().unwrap();
        let safety = store.restore_from(&file).unwrap();
        assert_eq!(store.data.students.len(), 1);
        assert!(safety.exists(), "data sebelum dipulihkan diamankan dulu");
        assert!(store.data.meta.last_external_backup_at.is_some());
        assert!(backup_info(&dir.join("data.json")).is_ok());

        let not_backup = usb.join("bukan.json");
        fs::write(&not_backup, "[1,2,3]").unwrap();
        assert!(store.restore_from(&not_backup).is_err());
        assert_eq!(store.data.students.len(), 1, "file salah tidak mengubah data");
        let _ = fs::remove_dir_all(dir);
        let _ = fs::remove_dir_all(usb);
    }

    #[test]
    fn new_term_archives_and_clears() {
        let dir = temp_dir("term");
        let (mut store, _) = Store::open(dir.clone()).unwrap();
        with_student(&mut store);
        store.data.settings.principal_name = "Kepala Sekolah".into();

        let opts = NewTermOptions {
            archive_label: "2026/2027 Term 1".into(),
            academic_year: String::new(),
            report_subtitle: "STUDENT PROGRESS REPORT — TERM 2".into(),
            keep_students: true,
        };
        let info = store.start_new_term(&opts).unwrap();
        assert_eq!(info.archive_label.as_deref(), Some("2026/2027 Term 1"));
        assert_eq!(info.students, 1);
        let s = &store.data.students[0];
        assert!(s.scores.is_empty() && s.atl.is_empty() && s.attendance.is_empty());
        assert_eq!(s.name, "Made Dewi", "daftar siswa tetap");
        assert_eq!(store.data.settings.principal_name, "Kepala Sekolah", "pengaturan dibawa");
        assert_eq!(store.data.settings.report_subtitle, "STUDENT PROGRESS REPORT — TERM 2");

        let archives = store.list_archives();
        assert_eq!(archives.len(), 1);
        // arsip masih berisi nilai lama dan bisa dipulihkan
        store.restore_from(Path::new(&archives[0].path)).unwrap();
        assert_eq!(store.data.students[0].atl["caring"].grade, "EE");
        assert!(store.data.meta.archive_label.is_none());

        // tahun ajaran baru: siswa dikosongkan; nama arsip kembar tidak menimpa
        let info2 = store
            .start_new_term(&NewTermOptions { archive_label: "2026/2027 Term 1".into(), keep_students: false, ..Default::default() })
            .unwrap();
        assert_ne!(info2.path, info.path);
        assert!(store.data.students.is_empty() && store.data.settings.classes.is_empty());
        assert_eq!(store.list_archives().len(), 2);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn file_names_are_safe() {
        assert_eq!(safe_file_name("2026/2027 Term 1"), "2026-2027 Term 1");
        assert_eq!(safe_file_name("  ..  "), "arsip");
    }
}
