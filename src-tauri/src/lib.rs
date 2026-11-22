//! E-Raport National Plus Class.
//!
//! Alur: impor data siswa & nilai dari Excel → isi ATL/proyek/narasi →
//! nilai angka otomatis jadi predikat → cetak rapor (PDF/printer).

mod commands;
pub mod forms;
pub mod grading;
pub mod levels;
pub mod importer;
pub mod leger;
pub mod model;
pub mod names;
pub mod pdf;
pub mod report;
pub mod roster;
pub mod security;
pub mod student_flex;
pub mod store;
pub mod view;

use std::path::PathBuf;
use tauri::Manager;

/// Folder data: `Dokumen\E-Raport` (bisa diganti lewat variabel lingkungan
/// `ERAPORT_DATA_DIR`, berguna untuk uji coba tanpa mengganggu data asli).
fn resolve_data_dir(app: &tauri::App) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Ok(dir) = std::env::var("ERAPORT_DATA_DIR") {
        if !dir.trim().is_empty() {
            return Ok(PathBuf::from(dir));
        }
    }
    let base = app
        .path()
        .document_dir()
        .or_else(|_| app.path().app_data_dir())?;
    Ok(base.join("E-Raport"))
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let root = resolve_data_dir(app)?;
            // buka jenjang terakhir (SD / SMP / SMA); SMP = folder utama (data lama)
            let level = levels::read_active(&root);
            let (mut store, warning) = store::Store::open(levels::level_dir(&root, &level))?;
            if store.data.meta.level.is_empty() {
                store.data.meta.level = level.clone(); // tersimpan bersama perubahan berikutnya
            }
            let security = security::Security::open(&root).unwrap_or_else(|e| security::Security::broken(&root, e));
            app.manage(commands::AppState::new(store, warning, security, root, level));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_data,
            commands::save_settings,
            commands::student_views,
            commands::save_student_identity,
            commands::delete_student,
            commands::delete_classes,
            commands::set_score,
            commands::save_report_fields,
            commands::set_effective_days,
            commands::assign_project,
            commands::pick_excel_file,
            commands::scan_excel,
            commands::import_students,
            commands::inspect_student_sheet,
            commands::import_students_mapped,
            commands::import_scores,
            commands::import_attendance,
            commands::render_report,
            commands::export_template,
            commands::reset_template,
            commands::open_data_folder,
            commands::backup_now,
            commands::save_form,
            commands::import_form,
            commands::reveal_path,
            commands::backup_external,
            commands::pick_backup_file,
            commands::inspect_backup,
            commands::restore_backup,
            commands::list_archives,
            commands::start_new_term,
            commands::save_leger,
            commands::save_class_roster,
            commands::save_student_list,
            commands::pick_folder,
            commands::export_pdfs,
            commands::pick_image_file,
            commands::load_logo,
            commands::load_signature_source,
            commands::list_levels,
            commands::switch_level,
            commands::move_classes_to_level,
            commands::security_status,
            commands::security_unlock,
            commands::security_reset_with_recovery,
            commands::security_set_password,
            commands::security_disable,
            commands::security_set_auto_lock,
            commands::security_lock,
        ])
        .run(tauri::generate_context!())
        .expect("gagal menjalankan aplikasi E-Raport");
}
