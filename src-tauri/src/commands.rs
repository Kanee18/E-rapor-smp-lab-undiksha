//! Perintah yang dipanggil dari tampilan (JavaScript) lewat `invoke(...)`.
//!
//! Setiap perubahan langsung disimpan ke `data.json`. Pembaruan dibuat per
//! bagian (identitas, nilai, isian rapor) supaya layar yang satu tidak
//! menimpa data yang diubah dari layar lain.

use crate::grading::{validate_grading, validate_score};
use crate::importer::{
    apply_attendance, apply_scores, apply_students, ensure_class_configs, new_student_id, normalize_class,
    read_attendance, read_scores, read_students, scan_workbook, AttendanceImportReport, ScoreImportReport, SheetScan,
    StudentImportReport,
};
use crate::model::{
    canonical_key, class_sort_key, format_attendance_sia, AppData, ProjectEntry, RatingEntry, ScoreEntry, Settings,
    Student,
};
use crate::report;
use crate::security::{Security, SecurityStatus};
use crate::store::Store;
use crate::view::{student_view, StudentView};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

pub struct AppState {
    store: Mutex<Store>,
    warning: Mutex<Option<String>>,
    security: Mutex<Security>,
    /// Folder data utama (berisi data SMP, folder SD\ & SMA\, keamanan.json, jenjang.json).
    root: PathBuf,
    /// Jenjang yang sedang dibuka ("SD", "SMP", "SMA").
    level: Mutex<String>,
}

impl AppState {
    pub fn new(store: Store, warning: Option<String>, security: Security, root: PathBuf, level: String) -> Self {
        Self {
            store: Mutex::new(store),
            warning: Mutex::new(warning),
            security: Mutex::new(security),
            root,
            level: Mutex::new(level),
        }
    }
}

fn current_level(state: &State<'_, AppState>) -> String {
    state.level.lock().map(|l| l.clone()).unwrap_or_else(|_| crate::levels::DEFAULT_LEVEL.to_string())
}

fn security<'a>(state: &'a State<'_, AppState>) -> Result<MutexGuard<'a, Security>, String> {
    state.security.lock().map_err(|_| "Keamanan aplikasi bermasalah, tutup lalu buka lagi aplikasinya".to_string())
}

/// Semua perintah data lewat sini: selama aplikasi terkunci, permintaan ditolak di sisi Rust.
fn ensure_unlocked(state: &State<'_, AppState>) -> Result<(), String> {
    if security(state)?.is_locked() {
        return Err("Aplikasi terkunci. Masukkan kata sandi dulu.".into());
    }
    Ok(())
}

fn lock<'a>(state: &'a State<'_, AppState>) -> Result<MutexGuard<'a, Store>, String> {
    ensure_unlocked(state)?;
    state
        .store
        .lock()
        .map_err(|_| "Data sedang bermasalah, tutup lalu buka lagi aplikasinya".to_string())
}

fn sorted_students(data: &AppData) -> Vec<&Student> {
    let mut v: Vec<&Student> = data.students.iter().collect();
    v.sort_by(|a, b| crate::model::compare_students(a, b));
    v
}

fn find_student<'a>(data: &'a mut AppData, id: &str) -> Result<&'a mut Student, String> {
    data.students
        .iter_mut()
        .find(|s| s.id == id)
        .ok_or_else(|| "Siswa tidak ditemukan (mungkin sudah dihapus)".to_string())
}

// ---------------------------------------------------------------- data & pengaturan

#[derive(Serialize)]
pub struct DataResponse {
    data: AppData,
    data_dir: String,
    level: String,
    warning: Option<String>,
    custom_template: bool,
}

#[tauri::command(rename_all = "snake_case")]
pub fn get_data(state: State<'_, AppState>) -> Result<DataResponse, String> {
    let warning = state.warning.lock().ok().and_then(|mut w| w.take());
    let store = lock(&state)?;
    Ok(DataResponse {
        data: store.data.clone(),
        data_dir: store.dir.display().to_string(),
        level: current_level(&state),
        warning,
        custom_template: store.custom_template_path().exists(),
    })
}

fn slug(s: &str) -> String {
    let base: String = s
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .take(16)
        .collect();
    if base.is_empty() {
        "item".into()
    } else {
        base
    }
}

/// Isi id yang kosong dan pastikan tidak ada id kembar.
fn fix_ids<T>(items: &mut [T], get: impl Fn(&mut T) -> (&mut String, String)) {
    let mut seen: HashSet<String> = HashSet::new();
    for it in items.iter_mut() {
        let (id, name) = get(it);
        let trimmed = id.trim().to_string();
        let mut candidate = if trimmed.is_empty() { slug(&name) } else { trimmed };
        let base = candidate.clone();
        let mut n = 2;
        while seen.contains(&candidate) {
            candidate = format!("{base}-{n}");
            n += 1;
        }
        seen.insert(candidate.clone());
        *id = candidate;
    }
}

fn require_names<'a>(names: impl Iterator<Item = &'a String>, what: &str) -> Result<(), String> {
    for (i, n) in names.enumerate() {
        if n.trim().is_empty() {
            return Err(format!("{what} baris ke-{} belum diberi nama", i + 1));
        }
    }
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub fn save_settings(state: State<'_, AppState>, settings: Settings) -> Result<Settings, String> {
    let mut s = settings;
    validate_grading(&s.grading)?;
    for b in &mut s.grading.bands {
        b.code = b.code.trim().to_uppercase();
        b.label = b.label.trim().to_string();
    }
    require_names(s.subjects.iter().map(|x| &x.name), "Mapel")?;
    require_names(s.atl.iter().map(|x| &x.name), "Atribut ATL")?;
    require_names(s.projects.iter().map(|x| &x.name), "Proyek")?;
    fix_ids(&mut s.subjects, |x| (&mut x.id, x.name.clone()));
    fix_ids(&mut s.atl, |x| (&mut x.id, x.name.clone()));
    fix_ids(&mut s.projects, |x| (&mut x.id, x.name.clone()));
    s.narrative_bank.retain(|x| !x.text.trim().is_empty());
    for x in &mut s.narrative_bank {
        x.text = x.text.trim().to_string();
        x.category = x.category.trim().to_string();
    }
    fix_ids(&mut s.narrative_bank, |x| (&mut x.id, format!("k{}", x.category)));
    s.info_fields = crate::model::normalize_info_fields(&s.info_fields);
    for c in &mut s.classes {
        c.name = normalize_class(&c.name);
        c.homeroom_teacher = c.homeroom_teacher.trim().to_string();
    }
    s.classes.retain(|c| !c.name.is_empty());
    s.classes.sort_by_key(|c| class_sort_key(&c.name));
    s.classes.dedup_by(|a, b| a.name == b.name);
    if s.logo.is_empty() {
        s.logo = crate::model::default_school_logo();
    }

    let mut store = lock(&state)?;
    store.data.settings = s;
    ensure_class_configs(&mut store.data);
    store.save()?;
    Ok(store.data.settings.clone())
}

// ---------------------------------------------------------------- siswa

#[derive(Deserialize)]
pub struct StudentIdentity {
    id: Option<String>,
    nis: String,
    nisn: String,
    name: String,
    gender: String,
    class_name: String,
}

#[tauri::command(rename_all = "snake_case")]
pub fn student_views(state: State<'_, AppState>, class_name: Option<String>) -> Result<Vec<StudentView>, String> {
    let store = lock(&state)?;
    let data = &store.data;
    Ok(sorted_students(data)
        .into_iter()
        .filter(|s| class_name.as_ref().is_none_or(|c| &s.class_name == c))
        .map(|s| student_view(s, &data.settings))
        .collect())
}

#[tauri::command(rename_all = "snake_case")]
pub fn save_student_identity(state: State<'_, AppState>, student: StudentIdentity) -> Result<Student, String> {
    let name = student.name.split_whitespace().collect::<Vec<_>>().join(" ");
    let class_name = normalize_class(&student.class_name);
    if name.is_empty() {
        return Err("Nama siswa wajib diisi".into());
    }
    if class_name.is_empty() {
        return Err("Kelas wajib diisi".into());
    }
    let mut store = lock(&state)?;
    let data = &mut store.data;
    let existing_id = student.id.filter(|id| data.students.iter().any(|s| &s.id == id));
    let id = match existing_id {
        Some(id) => id,
        None => {
            let id = new_student_id(data, student.nis.trim());
            data.students.push(Student { id: id.clone(), ..Default::default() });
            id
        }
    };
    let next = crate::model::next_order(&data.students, &class_name);
    let s = find_student(data, &id)?;
    if s.order.is_none() || s.class_name != class_name {
        s.order = Some(next);
    }
    s.name = name;
    s.class_name = class_name;
    s.nis = student.nis.trim().to_string();
    s.nisn = student.nisn.trim().to_string();
    s.gender = student.gender.trim().to_uppercase();
    let saved = s.clone();
    ensure_class_configs(data);
    store.save()?;
    Ok(saved)
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_student(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let mut store = lock(&state)?;
    let before = store.data.students.len();
    store.data.students.retain(|s| s.id != id);
    if store.data.students.len() == before {
        return Err("Siswa tidak ditemukan".into());
    }
    store.save()
}

/// Simpan daftar satu kelas yang dibuat/diubah langsung di aplikasi.
/// Kalau ada siswa lama yang dihapus, data dibackup dulu.
#[tauri::command(rename_all = "snake_case")]
pub fn save_class_roster(
    state: State<'_, AppState>,
    original_class: Option<String>,
    class_name: String,
    rows: Vec<crate::roster::RosterRow>,
    remove: Vec<String>,
) -> Result<crate::roster::RosterResult, String> {
    let mut store = lock(&state)?;
    let mut data = store.data.clone();
    let result = crate::roster::apply_roster(&mut data, original_class.as_deref(), &class_name, rows, &remove)?;
    if result.removed > 0 && store.data_path().exists() {
        store.copy_to_backup("sebelum-edit-daftar-kelas")?;
    }
    store.data = data;
    store.save()?;
    Ok(result)
}

/// Ekspor daftar siswa ke Excel (bisa diimpor balik). `classes` kosong = semua kelas.
#[tauri::command(rename_all = "snake_case")]
pub async fn save_student_list(
    app: AppHandle,
    state: State<'_, AppState>,
    classes: Vec<String>,
) -> Result<Option<FormSaved>, String> {
    let level = current_level(&state);
    let (data, dir) = {
        let store = lock(&state)?;
        (store.data.clone(), store.dir.join("data siswa"))
    };
    if !data.students.iter().any(|s| classes.is_empty() || classes.contains(&s.class_name)) {
        return Err("Tidak ada siswa untuk diekspor".into());
    }
    let _ = std::fs::create_dir_all(&dir);
    let picked = app
        .dialog()
        .file()
        .set_title("Simpan data siswa ke Excel")
        .set_directory(&dir)
        .set_file_name(crate::roster::suggested_file_name(&data, &classes, &level))
        .add_filter("File Excel", &["xlsx"])
        .blocking_save_file();
    let Some(fp) = picked else { return Ok(None) };
    let mut path = fp.into_path().map_err(|e| e.to_string())?;
    if path.extension().is_none() {
        path.set_extension("xlsx");
    }
    let students = crate::roster::write_students(&data, &classes, &level, &path)?;
    Ok(Some(FormSaved { path: path.display().to_string(), students }))
}

#[derive(Serialize)]
pub struct TemplateSaved {
    path: String,
    #[serde(flatten)]
    info: crate::templates::TemplateInfo,
}

/// Simpan template Excel yang disarankan (data siswa, nilai mapel, nilai proyek, absensi).
#[tauri::command(rename_all = "snake_case")]
pub async fn save_template(
    app: AppHandle,
    state: State<'_, AppState>,
    request: crate::templates::TemplateRequest,
) -> Result<Option<TemplateSaved>, String> {
    let (data, dir) = {
        let store = lock(&state)?;
        (store.data.clone(), store.dir.join("template"))
    };
    // periksa dulu supaya kesalahan (mis. belum ada mapel) muncul sebelum dialog simpan
    let check = std::env::temp_dir().join("eraport-template-cek.xlsx");
    crate::templates::write_template(&data, &request, &check)?;
    let _ = std::fs::remove_file(&check);
    let _ = std::fs::create_dir_all(&dir);
    let picked = app
        .dialog()
        .file()
        .set_title("Simpan template Excel")
        .set_directory(&dir)
        .set_file_name(crate::templates::suggested_file_name(&data, &request))
        .add_filter("File Excel", &["xlsx"])
        .blocking_save_file();
    let Some(fp) = picked else { return Ok(None) };
    let mut path = fp.into_path().map_err(|e| e.to_string())?;
    if path.extension().is_none() {
        path.set_extension("xlsx");
    }
    let info = crate::templates::write_template(&data, &request, &path)?;
    Ok(Some(TemplateSaved { path: path.display().to_string(), info }))
}

#[derive(Serialize)]
pub struct DeleteClassesResult {
    students_deleted: usize,
    classes_deleted: usize,
    /// Lokasi backup otomatis yang dibuat sebelum menghapus (kalau ada).
    backup: Option<String>,
}

/// Hapus satu atau beberapa kelas beserta SEMUA siswanya (nilai & isian rapor ikut
/// terhapus) dan pengaturan kelasnya (wali kelas, tanda tangan).
/// Sebelum menghapus, data dibackup otomatis ke folder backup lokal.
#[tauri::command(rename_all = "snake_case")]
pub fn delete_classes(state: State<'_, AppState>, classes: Vec<String>) -> Result<DeleteClassesResult, String> {
    let targets: HashSet<String> = classes.iter().map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect();
    if targets.is_empty() {
        return Err("Pilih minimal satu kelas".into());
    }
    let mut store = lock(&state)?;
    let students_before = store.data.students.len();
    let classes_before = store.data.settings.classes.len();
    let found = store.data.students.iter().any(|s| targets.contains(&s.class_name))
        || store.data.settings.classes.iter().any(|c| targets.contains(&c.name));
    if !found {
        return Err("Kelas tidak ditemukan (mungkin sudah dihapus)".into());
    }
    let backup = if store.data_path().exists() {
        Some(store.copy_to_backup("sebelum-hapus-kelas")?.display().to_string())
    } else {
        None
    };
    store.data.students.retain(|s| !targets.contains(&s.class_name));
    store.data.settings.classes.retain(|c| !targets.contains(&c.name));
    let result = DeleteClassesResult {
        students_deleted: students_before - store.data.students.len(),
        classes_deleted: classes_before - store.data.settings.classes.len(),
        backup,
    };
    store.save()?;
    Ok(result)
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_score(
    state: State<'_, AppState>,
    id: String,
    key: String,
    ph: Option<f64>,
    pts: Option<f64>,
    manual: Option<f64>,
) -> Result<StudentView, String> {
    validate_score(ph, "PH")?;
    validate_score(pts, "PTS")?;
    validate_score(manual, "Nilai akhir manual")?;
    let ck = canonical_key(&key);
    if ck.is_empty() {
        return Err("Sumber nilai mapel belum diatur di Pengaturan".into());
    }
    let mut store = lock(&state)?;
    let s = find_student(&mut store.data, &id)?;
    if ph.is_none() && pts.is_none() && manual.is_none() {
        s.scores.remove(&ck);
    } else {
        s.scores.insert(ck, ScoreEntry { ph, pts, manual });
    }
    let snapshot = s.clone();
    store.save()?;
    Ok(student_view(&snapshot, &store.data.settings))
}

#[derive(Deserialize)]
pub struct ReportFields {
    attendance: String,
    #[serde(default)]
    attendance_s: Option<u32>,
    #[serde(default)]
    attendance_i: Option<u32>,
    #[serde(default)]
    attendance_a: Option<u32>,
    atl: BTreeMap<String, RatingEntry>,
    projects: Vec<ProjectEntry>,
    subject_notes: BTreeMap<String, String>,
}

#[tauri::command(rename_all = "snake_case")]
pub fn save_report_fields(state: State<'_, AppState>, id: String, fields: ReportFields) -> Result<StudentView, String> {
    let mut store = lock(&state)?;
    let s = find_student(&mut store.data, &id)?;
    s.attendance = fields.attendance.trim().to_string();
    s.attendance_s = fields.attendance_s;
    s.attendance_i = fields.attendance_i;
    s.attendance_a = fields.attendance_a;
    s.atl = fields
        .atl
        .into_iter()
        .map(|(k, v)| {
            (k, RatingEntry { grade: v.grade.trim().to_string(), indicator: v.indicator.trim().to_string() })
        })
        .filter(|(_, v)| !v.grade.is_empty() || !v.indicator.is_empty())
        .collect();
    s.projects = fields
        .projects
        .into_iter()
        .filter(|p| !p.project_id.trim().is_empty())
        .map(|p| ProjectEntry {
            project_id: p.project_id.trim().to_string(),
            understanding: p.understanding.trim().to_string(),
            inquiry: p.inquiry.trim().to_string(),
            narrative: p.narrative.trim().to_string(),
        })
        .collect();
    s.subject_notes = fields
        .subject_notes
        .into_iter()
        .map(|(k, v)| (k, v.trim().to_string()))
        .filter(|(_, v)| !v.is_empty())
        .collect();
    let snapshot = s.clone();
    store.save()?;
    Ok(student_view(&snapshot, &store.data.settings))
}

/// Ubah total hari efektif semester dan hitung ulang otomatis "% (Present)"
/// untuk semua siswa yang sudah memiliki data Sakit/Izin/Alpa.
#[tauri::command(rename_all = "snake_case")]
pub fn set_effective_days(state: State<'_, AppState>, days: u32) -> Result<usize, String> {
    if !(1..=366).contains(&days) {
        return Err("Hari efektif harus antara 1 dan 366 hari".into());
    }
    let mut store = lock(&state)?;
    store.data.settings.effective_days = days;
    let mut updated = 0;
    for s in &mut store.data.students {
        if let Some(calc) = format_attendance_sia(s.attendance_s, s.attendance_i, s.attendance_a, days) {
            if s.attendance != calc {
                s.attendance = calc;
                updated += 1;
            }
        }
    }
    store.save()?;
    Ok(updated)
}

/// Tambahkan satu proyek ke semua siswa di kelas (yang belum punya proyek itu).
#[tauri::command(rename_all = "snake_case")]
pub fn assign_project(state: State<'_, AppState>, class_name: String, project_id: String) -> Result<usize, String> {
    let mut store = lock(&state)?;
    if !store.data.settings.projects.iter().any(|p| p.id == project_id) {
        return Err("Proyek tidak ditemukan".into());
    }
    let mut added = 0;
    for s in store.data.students.iter_mut().filter(|s| s.class_name == class_name) {
        if !s.projects.iter().any(|p| p.project_id == project_id) {
            s.projects.push(ProjectEntry { project_id: project_id.clone(), ..Default::default() });
            added += 1;
        }
    }
    store.save()?;
    Ok(added)
}

// ---------------------------------------------------------------- impor Excel

#[tauri::command(rename_all = "snake_case")]
pub async fn pick_excel_file(app: AppHandle, state: State<'_, AppState>) -> Result<Option<String>, String> {
    ensure_unlocked(&state)?;
    let picked = app
        .dialog()
        .file()
        .set_title("Pilih file Excel")
        .add_filter("File Excel", &["xls", "xlsx", "xlsm", "xlsb", "ods"])
        .blocking_pick_file();
    match picked {
        None => Ok(None),
        Some(fp) => fp.into_path().map(|p| Some(p.display().to_string())).map_err(|e| e.to_string()),
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn scan_excel(state: State<'_, AppState>, path: String) -> Result<Vec<SheetScan>, String> {
    ensure_unlocked(&state)?;
    scan_workbook(&PathBuf::from(path))
}

#[tauri::command(rename_all = "snake_case")]
pub async fn import_students(
    state: State<'_, AppState>,
    path: String,
    sheet: String,
    classes: Vec<String>,
) -> Result<StudentImportReport, String> {
    let rows = read_students(&PathBuf::from(path), &sheet)?;
    let mut store = lock(&state)?;
    let report = apply_students(&mut store.data, rows, &classes);
    store.save()?;
    Ok(report)
}

/// Impor data siswa berformat bebas: tebak kolom/kelas dan kembalikan pratinjau.
/// `mapping` kosong = tebak otomatis; diisi = pakai pilihan pengguna.
#[tauri::command(rename_all = "snake_case")]
pub async fn inspect_student_sheet(
    state: State<'_, AppState>,
    path: String,
    sheet: String,
    mapping: Option<crate::student_flex::StudentMapping>,
) -> Result<crate::student_flex::FlexPreview, String> {
    ensure_unlocked(&state)?;
    crate::student_flex::inspect(&PathBuf::from(path), &sheet, mapping)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn import_students_mapped(
    state: State<'_, AppState>,
    path: String,
    sheet: String,
    mapping: crate::student_flex::StudentMapping,
    classes: Vec<String>,
) -> Result<StudentImportReport, String> {
    ensure_unlocked(&state)?;
    let rows = crate::student_flex::read_mapped(&PathBuf::from(path), &sheet, &mapping)?;
    let mut store = lock(&state)?;
    let report = apply_students(&mut store.data, rows, &classes);
    store.save()?;
    Ok(report)
}

/// Pratinjau file nilai mapel berformat bebas: tebakan kolom + beberapa baris pertama.
#[tauri::command(rename_all = "snake_case")]
pub async fn inspect_score_sheet(
    state: State<'_, AppState>,
    path: String,
    sheet: String,
    mapping: Option<crate::score_import::ScoreMapping>,
) -> Result<crate::score_import::ScorePreview, String> {
    let data = lock(&state)?.data.clone();
    crate::score_import::inspect(&data, &PathBuf::from(path), &sheet, mapping)
}

/// Impor nilai mapel berformat bebas. `apply = false` → hanya laporan pratinjau, data tidak berubah.
#[tauri::command(rename_all = "snake_case")]
pub async fn import_scores_mapped(
    state: State<'_, AppState>,
    path: String,
    sheet: String,
    mapping: crate::score_import::ScoreMapping,
    classes: Vec<String>,
    apply: bool,
) -> Result<crate::score_import::FlexScoreReport, String> {
    let mut store = lock(&state)?;
    let mut data = store.data.clone();
    let report = crate::score_import::import(&mut data, &PathBuf::from(path), &sheet, &mapping, &classes)?;
    if apply && report.values_set > 0 {
        store.data = data;
        store.save()?;
    }
    Ok(report)
}

/// Pratinjau file nilai proyek: tebakan kolom + beberapa baris pertama.
#[tauri::command(rename_all = "snake_case")]
pub async fn inspect_project_sheet(
    state: State<'_, AppState>,
    path: String,
    sheet: String,
    mapping: Option<crate::project_import::ProjectMapping>,
) -> Result<crate::project_import::ProjectPreview, String> {
    let data = lock(&state)?.data.clone();
    crate::project_import::inspect(&data, &PathBuf::from(path), &sheet, mapping)
}

/// Impor nilai proyek. `apply = false` → hanya laporan pratinjau, data tidak berubah.
#[tauri::command(rename_all = "snake_case")]
pub async fn import_project_scores(
    state: State<'_, AppState>,
    path: String,
    sheet: String,
    mapping: crate::project_import::ProjectMapping,
    project_id: String,
    classes: Vec<String>,
    apply: bool,
) -> Result<crate::project_import::ProjectImportReport, String> {
    let mut store = lock(&state)?;
    let mut data = store.data.clone();
    let report = crate::project_import::import(&mut data, &PathBuf::from(path), &sheet, &mapping, &project_id, &classes)?;
    if apply && report.changed_cells + report.new_entries > 0 {
        store.data = data;
        store.save()?;
    }
    Ok(report)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn import_scores(
    state: State<'_, AppState>,
    path: String,
    sheet: String,
    classes: Vec<String>,
) -> Result<ScoreImportReport, String> {
    let sheet_data = read_scores(&PathBuf::from(path), &sheet)?;
    let mut store = lock(&state)?;
    let report = apply_scores(&mut store.data, sheet_data, &classes);
    store.save()?;
    Ok(report)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn import_attendance(
    state: State<'_, AppState>,
    path: String,
    sheet: String,
    classes: Vec<String>,
    effective_days: u32,
    empty_as_zero: bool,
) -> Result<AttendanceImportReport, String> {
    if !(1..=366).contains(&effective_days) {
        return Err("Hari efektif harus antara 1 dan 366 hari".into());
    }
    let sheet_data = read_attendance(&PathBuf::from(path), &sheet)?;
    let mut store = lock(&state)?;
    let report = apply_attendance(&mut store.data, sheet_data, &classes, effective_days, empty_as_zero);
    store.save()?;
    Ok(report)
}

// ---------------------------------------------------------------- rapor

#[tauri::command(rename_all = "snake_case")]
pub fn render_report(state: State<'_, AppState>, ids: Vec<String>) -> Result<String, String> {
    let store = lock(&state)?;
    let custom = store.custom_template()?;
    report::render(&store.data, &ids, custom.as_deref())
}

/// Salin desain bawaan ke folder data sebagai `template-rapor.html` supaya bisa diubah.
#[tauri::command(rename_all = "snake_case")]
pub fn export_template(state: State<'_, AppState>) -> Result<String, String> {
    let store = lock(&state)?;
    let path = store.custom_template_path();
    if path.exists() {
        return Err(format!("Template kustom sudah ada: {}", path.display()));
    }
    std::fs::write(&path, report::BUILTIN_TEMPLATE).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

/// Kembali ke desain bawaan (template kustom diganti nama, tidak dihapus).
#[tauri::command(rename_all = "snake_case")]
pub fn reset_template(state: State<'_, AppState>) -> Result<String, String> {
    let store = lock(&state)?;
    let path = store.custom_template_path();
    if !path.exists() {
        return Err("Tidak ada template kustom".into());
    }
    let stamp = jiff::Zoned::now().strftime("%Y%m%d-%H%M%S").to_string();
    let dest = store.dir.join(format!("template-rapor-lama-{stamp}.html"));
    std::fs::rename(&path, &dest).map_err(|e| e.to_string())?;
    Ok(dest.display().to_string())
}

// ---------------------------------------------------------------- folder & backup

#[tauri::command(rename_all = "snake_case")]
pub fn open_data_folder(state: State<'_, AppState>) -> Result<(), String> {
    let dir = lock(&state)?.dir.clone();
    #[cfg(windows)]
    let mut cmd = std::process::Command::new("explorer");
    #[cfg(target_os = "macos")]
    let mut cmd = std::process::Command::new("open");
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = std::process::Command::new("xdg-open");
    cmd.arg(dir).spawn().map(|_| ()).map_err(|e| format!("Folder tidak bisa dibuka: {e}"))
}

#[tauri::command(rename_all = "snake_case")]
pub fn backup_now(state: State<'_, AppState>) -> Result<String, String> {
    let store = lock(&state)?;
    store.backup_now().map(|p| p.display().to_string())
}

// ---------------------------------------------------------------- form Excel untuk fasil / wali kelas

#[derive(Serialize)]
pub struct FormSaved {
    path: String,
    students: usize,
}

/// Buat form Excel. Dialog "Simpan sebagai" dibuka di folder data\form.
#[tauri::command(rename_all = "snake_case")]
pub async fn save_form(
    app: AppHandle,
    state: State<'_, AppState>,
    request: crate::forms::FormRequest,
) -> Result<Option<FormSaved>, String> {
    let (data, dir) = {
        let mut store = lock(&state)?;
        if let Some(d) = request.effective_days.filter(|&d| (1..=366).contains(&d)) {
            if store.data.settings.effective_days != d {
                store.data.settings.effective_days = d;
                let _ = store.save();
            }
        }
        (store.data.clone(), store.dir.join("form"))
    };
    // Cek dulu supaya kesalahan muncul sebelum dialog dibuka.
    crate::forms::build_form(&data, &request)?;
    let _ = std::fs::create_dir_all(&dir);
    let picked = app
        .dialog()
        .file()
        .set_title("Simpan form Excel")
        .set_directory(&dir)
        .set_file_name(crate::forms::suggested_file_name(&data, &request))
        .add_filter("File Excel", &["xlsx"])
        .blocking_save_file();
    let Some(fp) = picked else { return Ok(None) };
    let mut path = fp.into_path().map_err(|e| e.to_string())?;
    if path.extension().is_none() {
        path.set_extension("xlsx");
    }
    let students = crate::forms::write_form(&data, &request, &path)?;
    Ok(Some(FormSaved { path: path.display().to_string(), students }))
}

/// Impor form yang sudah diisi. `apply = false` hanya pratinjau (data tidak berubah).
#[tauri::command(rename_all = "snake_case")]
pub async fn import_form(
    state: State<'_, AppState>,
    path: String,
    apply: bool,
) -> Result<crate::forms::FormImportReport, String> {
    let path = PathBuf::from(path);
    let mut store = lock(&state)?;
    if !apply {
        let mut preview = store.data.clone();
        return crate::forms::import_form(&mut preview, &path);
    }
    let mut report = crate::forms::import_form(&mut store.data, &path)?;
    store.save()?;
    report.applied = true;
    Ok(report)
}

/// Tampilkan file di Explorer (berkas terpilih).
#[tauri::command(rename_all = "snake_case")]
pub fn reveal_path(state: State<'_, AppState>, path: String) -> Result<(), String> {
    ensure_unlocked(&state)?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer")
            .raw_arg(format!("/select,\"{}\"", path.replace('"', "")))
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Explorer tidak bisa dibuka: {e}"))
    }
    #[cfg(target_os = "macos")]
    {
        // Finder dengan berkasnya terpilih
        std::process::Command::new("open").arg("-R").arg(&path).spawn().map(|_| ()).map_err(|e| e.to_string())
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let dir = PathBuf::from(&path).parent().map(|p| p.to_path_buf()).unwrap_or_default();
        std::process::Command::new("xdg-open").arg(dir).spawn().map(|_| ()).map_err(|e| e.to_string())
    }
}

/// Cetak di Mac: dialog cetak dari dalam aplikasi tidak bisa diandalkan di WebKit macOS,
/// jadi rapor dijadikan satu PDF lalu dibuka di Preview (Cmd+P untuk mencetak).
/// Tanpa Chrome/Edge: HTML dibuka di browser bawaan (Safari) untuk dicetak dari sana.
/// Mengembalikan "pdf" atau "html".
#[tauri::command(rename_all = "snake_case")]
pub async fn print_via_system(state: State<'_, AppState>, ids: Vec<String>) -> Result<String, String> {
    let html = {
        let store = lock(&state)?;
        let custom = store.custom_template()?;
        report::render(&store.data, &ids, custom.as_deref())?
    };
    if ids.is_empty() {
        return Err("Pilih minimal satu siswa".into());
    }
    let dir = std::env::temp_dir().join("E-Raport cetak");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // berkas cetak lama dibersihkan (hanya milik aplikasi ini, di folder sementara)
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            let _ = std::fs::remove_file(e.path());
        }
    }
    let stamp = jiff::Zoned::now().strftime("%Y-%m-%d %H.%M.%S").to_string();
    let (path, kind) = match crate::pdf::find_edge() {
        Some(browser) => {
            let out = dir.join(format!("Rapor {stamp}.pdf"));
            let job = crate::pdf::PdfJob { html, out: out.clone(), label: "rapor".into() };
            let failures = crate::pdf::run_jobs(&browser, std::slice::from_ref(&job), 1, &|_, _, _| {})?;
            if let Some((_, e)) = failures.first() {
                return Err(format!("PDF gagal dibuat: {e}"));
            }
            (out, "pdf")
        }
        None => {
            let out = dir.join(format!("Rapor {stamp}.html"));
            std::fs::write(&out, html).map_err(|e| e.to_string())?;
            (out, "html")
        }
    };
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(windows)]
    let opener = "explorer";
    #[cfg(not(any(windows, target_os = "macos")))]
    let opener = "xdg-open";
    std::process::Command::new(opener).arg(&path).spawn().map_err(|e| format!("Berkas cetak tidak bisa dibuka: {e}"))?;
    Ok(kind.into())
}

// ---------------------------------------------------------------- backup luar, pemulihan, semester baru

/// Backup ke flashdisk / folder lain (dialog pilih folder).
#[tauri::command(rename_all = "snake_case")]
pub async fn backup_external(app: AppHandle, state: State<'_, AppState>) -> Result<Option<String>, String> {
    let last = lock(&state)?.data.meta.last_external_backup_dir.clone();
    let mut dialog = app.dialog().file().set_title("Pilih flashdisk atau folder untuk backup");
    if let Some(dir) = last.filter(|d| std::path::Path::new(d).exists()) {
        dialog = dialog.set_directory(dir);
    }
    let Some(picked) = dialog.blocking_pick_folder() else { return Ok(None) };
    let folder = picked.into_path().map_err(|e| e.to_string())?;
    let mut store = lock(&state)?;
    store.backup_to(&folder).map(|p| Some(p.display().to_string()))
}

/// Pilih file backup/arsip (.json) untuk dipulihkan.
#[tauri::command(rename_all = "snake_case")]
pub async fn pick_backup_file(app: AppHandle, state: State<'_, AppState>) -> Result<Option<String>, String> {
    let start = lock(&state)?.backup_dir();
    let picked = app
        .dialog()
        .file()
        .set_title("Pilih file backup E-Raport")
        .set_directory(start)
        .add_filter("Backup E-Raport", &["json"])
        .blocking_pick_file();
    match picked {
        None => Ok(None),
        Some(fp) => fp.into_path().map(|p| Some(p.display().to_string())).map_err(|e| e.to_string()),
    }
}

#[tauri::command(rename_all = "snake_case")]
pub fn inspect_backup(state: State<'_, AppState>, path: String) -> Result<crate::store::BackupInfo, String> {
    ensure_unlocked(&state)?;
    crate::store::backup_info(&PathBuf::from(path))
}

/// Pulihkan data dari file backup/arsip. Mengembalikan lokasi salinan data sebelum dipulihkan.
#[tauri::command(rename_all = "snake_case")]
pub fn restore_backup(state: State<'_, AppState>, path: String) -> Result<String, String> {
    let mut store = lock(&state)?;
    store.restore_from(&PathBuf::from(path)).map(|p| p.display().to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_archives(state: State<'_, AppState>) -> Result<Vec<crate::store::BackupInfo>, String> {
    Ok(lock(&state)?.list_archives())
}

#[tauri::command(rename_all = "snake_case")]
pub fn start_new_term(
    state: State<'_, AppState>,
    options: crate::store::NewTermOptions,
) -> Result<crate::store::BackupInfo, String> {
    lock(&state)?.start_new_term(&options)
}

// ---------------------------------------------------------------- leger kelas

/// Simpan leger kelas ke Excel (dialog "Simpan sebagai" di folder data\leger).
#[tauri::command(rename_all = "snake_case")]
pub async fn save_leger(
    app: AppHandle,
    state: State<'_, AppState>,
    classes: Vec<String>,
) -> Result<Option<FormSaved>, String> {
    let (data, dir) = {
        let store = lock(&state)?;
        (store.data.clone(), store.dir.join("leger"))
    };
    crate::leger::build_leger(&data, &classes)?;
    let _ = std::fs::create_dir_all(&dir);
    let picked = app
        .dialog()
        .file()
        .set_title("Simpan leger Excel")
        .set_directory(&dir)
        .set_file_name(crate::leger::suggested_file_name(&data, &classes))
        .add_filter("File Excel", &["xlsx"])
        .blocking_save_file();
    let Some(fp) = picked else { return Ok(None) };
    let mut path = fp.into_path().map_err(|e| e.to_string())?;
    if path.extension().is_none() {
        path.set_extension("xlsx");
    }
    let students = crate::leger::write_leger(&data, &classes, &path)?;
    Ok(Some(FormSaved { path: path.display().to_string(), students }))
}

// ---------------------------------------------------------------- PDF per siswa

/// Dialog pilih folder (umum).
#[tauri::command(rename_all = "snake_case")]
pub async fn pick_folder(app: AppHandle, state: State<'_, AppState>, title: String) -> Result<Option<String>, String> {
    ensure_unlocked(&state)?;
    match app.dialog().file().set_title(title).blocking_pick_folder() {
        None => Ok(None),
        Some(fp) => fp.into_path().map(|p| Some(p.display().to_string())).map_err(|e| e.to_string()),
    }
}

#[derive(Serialize)]
pub struct PdfExport {
    folder: String,
    first_file: Option<String>,
    files: usize,
    failed: Vec<String>,
}

#[derive(Clone, Serialize)]
struct PdfProgress {
    done: usize,
    total: usize,
    name: String,
}

/// Simpan rapor setiap siswa sebagai PDF terpisah:
/// `<folder>\Rapor <kelas> - <sub-judul> <tahun>\01 Nama Siswa.pdf`.
/// Kemajuan dikirim lewat event "pdf-progress".
#[tauri::command(rename_all = "snake_case")]
pub async fn export_pdfs(
    app: AppHandle,
    state: State<'_, AppState>,
    ids: Vec<String>,
    folder: String,
) -> Result<PdfExport, String> {
    use tauri::Emitter;
    let edge = crate::pdf::find_edge().ok_or_else(crate::pdf::no_browser_message)?;
    let (data, custom) = {
        let store = lock(&state)?;
        (store.data.clone(), store.custom_template()?)
    };
    let students: Vec<&Student> = ids.iter().filter_map(|id| data.students.iter().find(|s| &s.id == id)).collect();
    if students.is_empty() {
        return Err("Pilih minimal satu siswa".into());
    }
    let mut classes: Vec<&str> = students.iter().map(|s| s.class_name.as_str()).collect();
    classes.dedup();
    let class = if classes.len() == 1 { classes[0].to_string() } else { "beberapa kelas".into() };
    let st = &data.settings;
    let out_dir = PathBuf::from(&folder).join(crate::pdf::safe_name(&format!(
        "Rapor {class} - {} {}",
        st.report_subtitle.trim(),
        st.academic_year.trim().replace('/', "-")
    )));
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("Folder tidak bisa dibuat ({}): {e}", out_dir.display()))?;

    let mut jobs = Vec::new();
    for (i, s) in students.iter().enumerate() {
        jobs.push(crate::pdf::PdfJob {
            html: report::render(&data, std::slice::from_ref(&s.id), custom.as_deref())?,
            out: out_dir.join(format!("{:02} {}.pdf", i + 1, crate::pdf::safe_name(s.name.trim()))),
            label: s.name.clone(),
        });
    }
    let first_file = jobs.first().map(|j| j.out.display().to_string());
    let total = jobs.len();
    let emitter = app.clone();
    let failures = tauri::async_runtime::spawn_blocking(move || {
        crate::pdf::run_jobs(&edge, &jobs, 4, &|done, total, name| {
            let _ = emitter.emit("pdf-progress", PdfProgress { done, total, name: name.to_string() });
        })
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(PdfExport {
        folder: out_dir.display().to_string(),
        first_file,
        files: total - failures.len(),
        failed: failures.into_iter().map(|(n, e)| format!("{n}: {e}")).collect(),
    })
}

// ---------------------------------------------------------------- logo & tanda tangan

/// Dialog pilih gambar (PNG/JPG/WebP). `title` default: pilih logo.
#[tauri::command(rename_all = "snake_case")]
pub async fn pick_image_file(app: AppHandle, state: State<'_, AppState>, title: Option<String>) -> Result<Option<String>, String> {
    ensure_unlocked(&state)?;
    let picked = app
        .dialog()
        .file()
        .set_title(title.as_deref().unwrap_or("Pilih logo sekolah"))
        .add_filter("Gambar", &["png", "jpg", "jpeg", "webp"])
        .blocking_pick_file();
    match picked {
        None => Ok(None),
        Some(fp) => fp.into_path().map(|p| Some(p.display().to_string())).map_err(|e| e.to_string()),
    }
}

/// Baca gambar logo menjadi data URL untuk disimpan di pengaturan (maks. 10 MB; diperkecil di tampilan).
#[tauri::command(rename_all = "snake_case")]
pub fn load_logo(state: State<'_, AppState>, path: String) -> Result<String, String> {
    ensure_unlocked(&state)?;
    let bytes = std::fs::read(&path).map_err(|e| format!("Gambar tidak bisa dibaca: {e}"))?;
    if bytes.len() > 10 * 1024 * 1024 {
        return Err(format!(
            "Gambar terlalu besar ({} MB). Pakai gambar di bawah 10 MB.",
            bytes.len() / (1024 * 1024)
        ));
    }
    image_data_url(&bytes)
}

/// Baca foto/gambar sumber tanda tangan (boleh foto HP berukuran besar, maks. 25 MB).
/// Hasilnya diolah di aplikasi (latar dihapus, dipotong, diperkecil) sebelum disimpan.
#[tauri::command(rename_all = "snake_case")]
pub fn load_signature_source(state: State<'_, AppState>, path: String) -> Result<String, String> {
    ensure_unlocked(&state)?;
    let bytes = std::fs::read(&path).map_err(|e| format!("Gambar tidak bisa dibaca: {e}"))?;
    if bytes.len() > 25 * 1024 * 1024 {
        return Err(format!("Gambar terlalu besar ({} MB). Maksimal 25 MB.", bytes.len() / (1024 * 1024)));
    }
    image_data_url(&bytes)
}

/// Bungkus isi file gambar menjadi data URL; format dikenali dari isi file, bukan nama.
fn image_data_url(bytes: &[u8]) -> Result<String, String> {
    use base64::Engine;
    let mime = if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        "image/png"
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else {
        return Err("Format gambar tidak dikenali. Pakai PNG, JPG, atau WebP (foto HEIC dari iPhone: ubah ke JPG dulu).".into());
    };
    Ok(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

// ---------------------------------------------------------------- kunci aplikasi

#[tauri::command(rename_all = "snake_case")]
pub fn security_status(state: State<'_, AppState>) -> Result<SecurityStatus, String> {
    Ok(security(&state)?.status())
}

#[tauri::command(rename_all = "snake_case")]
pub fn security_unlock(state: State<'_, AppState>, password: String) -> Result<SecurityStatus, String> {
    let mut sec = security(&state)?;
    sec.unlock(&password)?;
    Ok(sec.status())
}

/// Lupa kata sandi: kode pemulihan + kata sandi baru. Mengembalikan kode pemulihan baru.
#[tauri::command(rename_all = "snake_case")]
pub fn security_reset_with_recovery(state: State<'_, AppState>, code: String, new_password: String) -> Result<String, String> {
    security(&state)?.reset_with_recovery(&code, &new_password)
}

/// Pasang/ganti kata sandi. Saat pertama dipasang, mengembalikan kode pemulihan.
#[tauri::command(rename_all = "snake_case")]
pub fn security_set_password(
    state: State<'_, AppState>,
    current: Option<String>,
    new_password: String,
) -> Result<Option<String>, String> {
    security(&state)?.set_password(current.as_deref(), &new_password)
}

/// Lupa kata sandi: kode 6 angka Google Authenticator + kata sandi baru.
#[tauri::command(rename_all = "snake_case")]
pub fn security_reset_with_authenticator(state: State<'_, AppState>, code: String, new_password: String) -> Result<(), String> {
    security(&state)?.reset_with_authenticator(&code, &new_password)
}

/// Hubungkan Google Authenticator (langkah 1): periksa kata sandi, kembalikan kode QR.
#[tauri::command(rename_all = "snake_case")]
pub fn security_authenticator_begin(state: State<'_, AppState>, current: String) -> Result<crate::security::AuthenticatorSetup, String> {
    // nama akun di HP: nama sekolah (tanpa data pribadi), mis. "E-Raport: SMP Lab Undiksha"
    let account = {
        let store = lock(&state)?;
        let school = store.data.settings.school_name.trim().to_string();
        if school.is_empty() { "Laptop sekolah".to_string() } else { school }
    };
    security(&state)?.authenticator_begin(&current, &account)
}

/// Langkah 2: kode dari HP → simpan.
#[tauri::command(rename_all = "snake_case")]
pub fn security_authenticator_confirm(state: State<'_, AppState>, code: String) -> Result<(), String> {
    security(&state)?.authenticator_confirm(&code)
}

#[tauri::command(rename_all = "snake_case")]
pub fn security_authenticator_remove(state: State<'_, AppState>, current: String) -> Result<(), String> {
    security(&state)?.authenticator_remove(&current)
}

#[tauri::command(rename_all = "snake_case")]
pub fn security_disable(state: State<'_, AppState>, current: String) -> Result<(), String> {
    security(&state)?.disable(&current)
}

#[tauri::command(rename_all = "snake_case")]
pub fn security_set_auto_lock(state: State<'_, AppState>, minutes: u32) -> Result<(), String> {
    security(&state)?.set_auto_lock(minutes)
}

#[tauri::command(rename_all = "snake_case")]
pub fn security_lock(state: State<'_, AppState>) -> Result<SecurityStatus, String> {
    let mut sec = security(&state)?;
    sec.lock();
    Ok(sec.status())
}

// ---------------------------------------------------------------- jenjang (SD / SMP / SMA)

#[derive(Serialize)]
pub struct LevelInfo {
    id: String,
    label: String,
    school: String,
    active: bool,
    /// Sudah pernah dibuka (punya data.json).
    exists: bool,
    students: usize,
    classes: Vec<String>,
}

fn class_list(data: &AppData) -> Vec<String> {
    let mut c: Vec<String> = data.students.iter().map(|s| s.class_name.clone()).collect();
    c.sort_by_key(|x| class_sort_key(x));
    c.dedup();
    c
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_levels(state: State<'_, AppState>) -> Result<Vec<LevelInfo>, String> {
    let store = lock(&state)?;
    let active = current_level(&state);
    Ok(crate::levels::LEVELS
        .iter()
        .map(|l| {
            let is_active = l.id == active;
            let path = crate::levels::level_dir(&state.root, l.id).join("data.json");
            let other = if is_active { None } else { crate::store::read_data_file(&path).ok() };
            let data = if is_active { Some(&store.data) } else { other.as_ref() };
            LevelInfo {
                id: l.id.into(),
                label: l.label.into(),
                school: data
                    .map(|d| d.settings.school_name.clone())
                    .filter(|n| !n.trim().is_empty())
                    .unwrap_or_else(|| l.school.into()),
                active: is_active,
                exists: is_active || path.exists(),
                students: data.map(|d| d.students.len()).unwrap_or(0),
                classes: data.map(class_list).unwrap_or_default(),
            }
        })
        .collect())
}

/// Buka data jenjang lain. Jenjang yang belum pernah dibuka dibuat baru; `copy_settings` = salin
/// pengaturan (mapel, ATL, proyek, predikat, bank kalimat, kop) dari jenjang yang sedang dibuka.
#[tauri::command(rename_all = "snake_case")]
pub fn switch_level(state: State<'_, AppState>, id: String, copy_settings: bool) -> Result<(), String> {
    let level = crate::levels::find(&id).ok_or("Jenjang tidak dikenal")?;
    let mut store = lock(&state)?;
    if current_level(&state) == level.id {
        return Ok(());
    }
    let dir = crate::levels::level_dir(&state.root, level.id);
    let is_new = !dir.join("data.json").exists();
    let (mut next, warning) = Store::open(dir)?;
    if is_new {
        next.data.settings.school_name = level.school.into();
        if copy_settings {
            crate::levels::copy_shared_settings(&store.data.settings, &mut next.data.settings);
        }
    }
    if is_new || next.data.meta.level.is_empty() {
        next.data.meta.level = level.id.into();
        next.save()?;
    }
    crate::levels::write_active(&state.root, level.id)?;
    *store = next;
    if let Ok(mut l) = state.level.lock() {
        *l = level.id.into();
    }
    if let (Some(w), Ok(mut slot)) = (warning, state.warning.lock()) {
        *slot = Some(w);
    }
    Ok(())
}

#[derive(Serialize)]
pub struct MoveClassesResult {
    report: crate::levels::MoveReport,
    target: String,
    backup: String,
}

/// Pindahkan kelas (siswa + nilai + isian + wali kelas) dari jenjang yang dibuka ke jenjang lain.
/// Kedua data dibackup dulu; data tujuan disimpan lebih dulu supaya tidak ada yang hilang kalau gagal.
#[tauri::command(rename_all = "snake_case")]
pub fn move_classes_to_level(
    state: State<'_, AppState>,
    classes: Vec<String>,
    target: String,
) -> Result<MoveClassesResult, String> {
    let level = crate::levels::find(&target).ok_or("Jenjang tujuan tidak dikenal")?;
    let mut store = lock(&state)?;
    if current_level(&state) == level.id {
        return Err("Jenjang tujuan sama dengan jenjang yang sedang dibuka".into());
    }
    if classes.is_empty() {
        return Err("Pilih minimal satu kelas".into());
    }
    let dir = crate::levels::level_dir(&state.root, level.id);
    let is_new = !dir.join("data.json").exists();
    let (mut dst, _) = Store::open(dir)?;
    if is_new {
        dst.data.settings.school_name = level.school.into();
        crate::levels::copy_shared_settings(&store.data.settings, &mut dst.data.settings);
    } else {
        dst.copy_to_backup("sebelum-terima-kelas")?;
    }
    dst.data.meta.level = level.id.into();
    let backup = store.copy_to_backup("sebelum-pindah-kelas")?.display().to_string();
    let report = crate::levels::move_classes(&mut store.data, &mut dst.data, &classes);
    dst.save()?;
    store.save()?;
    Ok(MoveClassesResult { report, target: level.label.into(), backup })
}
