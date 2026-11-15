//! Buat contoh rapor HTML langsung dari file Excel, tanpa membuka aplikasi.
//! Berguna untuk mengecek desain rapor dengan cepat.
//!
//!   cargo run --example contoh_rapor -- "<file.xls>" "<hasil.html>" [kelas] [jumlah]
//!
//! ATL, proyek, dan kehadiran diisi CONTOH supaya semua bagian rapor terlihat.

use e_raport_lib::importer::{apply_scores, apply_students, read_scores, read_students, scan_workbook, SheetKind};
use e_raport_lib::model::{AppData, ProjectEntry, RatingEntry};
use e_raport_lib::report;
use std::path::Path;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let (Some(xls), Some(out)) = (args.get(1), args.get(2)) else {
        return Err("pakai: contoh_rapor <file.xls> <hasil.html> [kelas] [jumlah]".into());
    };
    let class = args.get(3).cloned().unwrap_or_else(|| "IX-1".into());
    let count: usize = args.get(4).and_then(|n| n.parse().ok()).unwrap_or(2);
    let path = Path::new(xls);

    let scans = scan_workbook(path)?;
    let students = scans.iter().find(|s| s.kind == SheetKind::Students).ok_or("sheet data siswa tidak ada")?;
    let scores = scans.iter().find(|s| s.kind == SheetKind::Scores).ok_or("sheet nilai tidak ada")?;
    let classes = vec![class.clone()];

    let mut data = AppData::default();
    apply_students(&mut data, read_students(path, &students.name)?, &classes);
    let rep = apply_scores(&mut data, read_scores(path, &scores.name)?, &classes);
    eprintln!("cocok {} siswa, perkiraan {}, tidak cocok {}", rep.matched, rep.approximate.len(), rep.unmatched.len());

    let ratings = ["ME", "EE", "AE", "ME", "ME", "EE", "AE"];
    let atl_ids: Vec<String> = data.settings.atl.iter().map(|a| a.id.clone()).collect();
    for s in data.students.iter_mut() {
        s.attendance = "98% (Present)".into();
        for (i, id) in atl_ids.iter().enumerate() {
            s.atl.insert(id.clone(), RatingEntry { grade: ratings[i % ratings.len()].into(), indicator: String::new() });
        }
        s.projects.push(ProjectEntry {
            project_id: "p1".into(),
            understanding: "92".into(),
            inquiry: "EE".into(),
            narrative: "(Contoh narasi) Actively engaged in the unit and explained findings clearly to peers. \
                        To progress further, the student is encouraged to apply more critical thinking."
                .into(),
        });
    }
    data.settings.classes.iter_mut().for_each(|c| c.homeroom_teacher = "(Contoh Wali Kelas), S.Pd.".into());

    let ids: Vec<String> = data.students.iter().take(count).map(|s| s.id.clone()).collect();
    let html = report::render(&data, &ids, None)?;
    std::fs::write(out, html).map_err(|e| e.to_string())?;
    eprintln!("rapor ditulis ke {out}");
    Ok(())
}
