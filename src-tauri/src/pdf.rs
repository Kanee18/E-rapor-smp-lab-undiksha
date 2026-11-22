//! PDF per siswa lewat Microsoft Edge mode headless.
//!
//! Edge selalu terpasang di Windows 10/11 dan memakai mesin yang sama dengan
//! tampilan aplikasi (WebView2), jadi hasil PDF sama dengan pratinjau/cetak.
//! Beberapa proses Edge dijalankan bersamaan supaya satu kelas cepat selesai.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Satu rapor biasanya selesai ±1 detik; batas ini hanya untuk proses Edge yang macet.
const TIMEOUT: Duration = Duration::from_secs(60);

/// Lokasi msedge.exe, atau `None` kalau tidak ditemukan.
pub fn find_edge() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    for var in ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"] {
        if let Ok(base) = std::env::var(var) {
            candidates.push(PathBuf::from(base).join(r"Microsoft\Edge\Application\msedge.exe"));
        }
    }
    candidates.push(PathBuf::from(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"));
    candidates.push(PathBuf::from(r"C:\Program Files\Microsoft\Edge\Application\msedge.exe"));
    candidates.into_iter().find(|p| p.is_file())
}

/// Nama file aman untuk Windows (tanpa \/:*?"<>|), maks. 100 karakter.
pub fn safe_name(s: &str) -> String {
    let t: String = s
        .chars()
        .map(|c| if r#"\/:*?"<>|"#.contains(c) || c.is_control() { '-' } else { c })
        .collect();
    let t = t.trim().trim_end_matches('.').trim().to_string();
    if t.is_empty() {
        "rapor".into()
    } else {
        t.chars().take(100).collect()
    }
}

/// Path lokal → URL file:/// (karakter khusus di-encode).
pub fn file_url(p: &Path) -> String {
    let s = p.display().to_string().replace('\\', "/");
    let mut out = String::from("file:///");
    for b in s.trim_start_matches('/').bytes() {
        let keep = b.is_ascii_alphanumeric() || b"/:._-~".contains(&b);
        if keep {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub struct PdfJob {
    /// HTML lengkap satu rapor.
    pub html: String,
    pub out: PathBuf,
    /// Nama siswa (untuk laporan kemajuan/galat).
    pub label: String,
}

fn print_one(edge: &Path, html_path: &Path, out: &Path, profile: &Path) -> Result<(), String> {
    let _ = std::fs::remove_file(out);
    let mut cmd = Command::new(edge);
    cmd.arg("--headless=new")
        .arg("--disable-gpu")
        .arg("--no-pdf-header-footer")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--disable-extensions")
        .arg(format!("--user-data-dir={}", profile.display()))
        .arg(format!("--print-to-pdf={}", out.display()))
        .arg(file_url(html_path));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: tanpa jendela konsol
    }
    let mut child = cmd.spawn().map_err(|e| format!("Edge tidak bisa dijalankan: {e}"))?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() > TIMEOUT => {
                kill_tree(&mut child);
                return Err("waktu habis".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(e.to_string()),
        }
    }
    match std::fs::metadata(out) {
        Ok(m) if m.len() > 0 => Ok(()),
        _ => Err("PDF tidak terbentuk".into()),
    }
}

/// Hentikan Edge beserta proses anaknya. `child.kill()` saja meninggalkan proses anak
/// yang tetap mengunci folder profil, sehingga percobaan berikutnya ikut macet.
fn kill_tree(child: &mut std::process::Child) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new("taskkill")
            .args(["/T", "/F", "/PID", &child.id().to_string()])
            .creation_flags(0x0800_0000)
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Jalankan semua pekerjaan dengan `workers` proses Edge paralel.
/// `progress(selesai, total, nama)` dipanggil setiap satu PDF selesai.
/// Mengembalikan daftar galat (nama siswa, alasan).
pub fn run_jobs(
    edge: &Path,
    jobs: &[PdfJob],
    workers: usize,
    progress: &(dyn Fn(usize, usize, &str) + Sync),
) -> Result<Vec<(String, String)>, String> {
    let work = std::env::temp_dir().join(format!(
        "eraport-pdf-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
    ));
    std::fs::create_dir_all(&work).map_err(|e| format!("Folder sementara tidak bisa dibuat: {e}"))?;
    for (i, job) in jobs.iter().enumerate() {
        std::fs::write(work.join(format!("{i}.html")), &job.html).map_err(|e| e.to_string())?;
    }
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let failures: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());
    let total = jobs.len();
    std::thread::scope(|scope| {
        for w in 0..workers.max(1).min(total.max(1)) {
            let (next, done, failures, work) = (&next, &done, &failures, &work);
            scope.spawn(move || {
                let profile = work.join(format!("profil-{w}"));
                loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some(job) = jobs.get(i) else { break };
                    let html = work.join(format!("{i}.html"));
                    // Edge kadang macet sekali-sekali; coba ulang satu kali dengan profil baru.
                    let result = print_one(edge, &html, &job.out, &profile)
                        .or_else(|_| print_one(edge, &html, &job.out, &work.join(format!("profil-{w}-ulang-{i}"))));
                    if let Err(e) = result {
                        if let Ok(mut f) = failures.lock() {
                            f.push((job.label.clone(), e));
                        }
                    }
                    let n = done.fetch_add(1, Ordering::SeqCst) + 1;
                    progress(n, total, &job.label);
                }
            });
        }
    });
    let _ = std::fs::remove_dir_all(&work);
    Ok(failures.into_inner().unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_urls() {
        assert_eq!(safe_name("IX-1 / Made: Dewi?"), "IX-1 - Made- Dewi-");
        assert_eq!(safe_name("  ..  "), "rapor");
        assert_eq!(file_url(Path::new(r"C:\Data Saya\rapor #1.html")), "file:///C:/Data%20Saya/rapor%20%231.html");
    }

    #[test]
    fn edge_prints_pdfs_with_css_page_size() {
        let Some(edge) = find_edge() else {
            eprintln!("Edge tidak ada, test dilewati");
            return;
        };
        let dir = std::env::temp_dir().join(format!("eraport-pdf-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let page = |size: &str, text: &str| {
            format!("<!doctype html><html><head><meta charset=utf-8><style>@page {{ size: {size}; margin: 0 }}</style></head><body><h1>{text}</h1></body></html>")
        };
        let jobs = vec![
            PdfJob { html: page("A4", "Satu"), out: dir.join("a4.pdf"), label: "A".into() },
            PdfJob { html: page("215mm 330mm", "Dua"), out: dir.join("f4.pdf"), label: "B".into() },
        ];
        let calls = AtomicUsize::new(0);
        let failures = run_jobs(&edge, &jobs, 2, &|_, _, _| {
            calls.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();
        assert!(failures.is_empty(), "{failures:?}");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        for job in &jobs {
            let bytes = std::fs::read(&job.out).unwrap();
            assert!(bytes.starts_with(b"%PDF"));
        }
        // ukuran halaman mengikuti CSS: F4 lebih tinggi daripada A4
        let mediabox = |p: &Path| {
            let s = String::from_utf8_lossy(&std::fs::read(p).unwrap()).to_string();
            let i = s.find("/MediaBox").expect("MediaBox");
            s[i..i + 60].to_string()
        };
        let (a4, f4) = (mediabox(&jobs[0].out), mediabox(&jobs[1].out));
        assert!(a4.contains("841") || a4.contains("842"), "A4: {a4}");
        assert!(f4.contains("935"), "F4: {f4}");
        let _ = std::fs::remove_dir_all(dir);
    }
}
