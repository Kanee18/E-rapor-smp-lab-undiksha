// Pemanggil perintah Rust (lihat src-tauri/src/commands.rs).
// Nama argumen memakai snake_case karena perintah ditandai rename_all = "snake_case".

const invoke = (cmd, args) => window.__TAURI__.core.invoke(cmd, args);

export const api = {
  getData: () => invoke('get_data'),
  listLevels: () => invoke('list_levels'),
  switchLevel: (id, copySettings) => invoke('switch_level', { id, copy_settings: copySettings }),
  moveClassesToLevel: (classes, target) => invoke('move_classes_to_level', { classes, target }),
  saveSettings: (settings) => invoke('save_settings', { settings }),

  studentViews: (className = null) => invoke('student_views', { class_name: className }),
  saveStudentIdentity: (student) => invoke('save_student_identity', { student }),
  deleteStudent: (id) => invoke('delete_student', { id }),
  deleteClasses: (classes) => invoke('delete_classes', { classes }),
  saveClassRoster: (originalClass, className, rows, remove) =>
    invoke('save_class_roster', { original_class: originalClass, class_name: className, rows, remove }),
  saveStudentList: (classes) => invoke('save_student_list', { classes }),
  setScore: (id, key, ph, pts, manual) => invoke('set_score', { id, key, ph, pts, manual }),
  saveReportFields: (id, fields) => invoke('save_report_fields', { id, fields }),
  setEffectiveDays: (days) => invoke('set_effective_days', { days }),
  assignProject: (className, projectId) =>
    invoke('assign_project', { class_name: className, project_id: projectId }),

  pickExcelFile: () => invoke('pick_excel_file'),
  scanExcel: (path) => invoke('scan_excel', { path }),
  importStudents: (path, sheet, classes) => invoke('import_students', { path, sheet, classes }),
  inspectStudentSheet: (path, sheet, mapping = null) => invoke('inspect_student_sheet', { path, sheet, mapping }),
  importStudentsMapped: (path, sheet, mapping, classes) => invoke('import_students_mapped', { path, sheet, mapping, classes }),
  importScores: (path, sheet, classes) => invoke('import_scores', { path, sheet, classes }),
  importAttendance: (path, sheet, classes, effectiveDays, emptyAsZero) =>
    invoke('import_attendance', { path, sheet, classes, effective_days: effectiveDays, empty_as_zero: emptyAsZero }),

  renderReport: (ids) => invoke('render_report', { ids }),
  exportTemplate: () => invoke('export_template'),
  resetTemplate: () => invoke('reset_template'),
  openDataFolder: () => invoke('open_data_folder'),
  backupNow: () => invoke('backup_now'),

  backupExternal: () => invoke('backup_external'),
  pickBackupFile: () => invoke('pick_backup_file'),
  inspectBackup: (path) => invoke('inspect_backup', { path }),
  restoreBackup: (path) => invoke('restore_backup', { path }),
  listArchives: () => invoke('list_archives'),
  startNewTerm: (options) => invoke('start_new_term', { options }),

  saveForm: (request) => invoke('save_form', { request }),
  saveLeger: (classes) => invoke('save_leger', { classes }),
  pickFolder: (title) => invoke('pick_folder', { title }),
  exportPdfs: (ids, folder) => invoke('export_pdfs', { ids, folder }),
  pickImageFile: (title = null) => invoke('pick_image_file', { title }),
  loadLogo: (path) => invoke('load_logo', { path }),
  loadSignatureSource: (path) => invoke('load_signature_source', { path }),
  importForm: (path, apply) => invoke('import_form', { path, apply }),
  revealPath: (path) => invoke('reveal_path', { path }),

  securityStatus: () => invoke('security_status'),
  securityUnlock: (password) => invoke('security_unlock', { password }),
  securityResetWithRecovery: (code, newPassword) => invoke('security_reset_with_recovery', { code, new_password: newPassword }),
  securitySetPassword: (current, newPassword) => invoke('security_set_password', { current, new_password: newPassword }),
  securityDisable: (current) => invoke('security_disable', { current }),
  securitySetAutoLock: (minutes) => invoke('security_set_auto_lock', { minutes }),
  securityLock: () => invoke('security_lock'),
};
