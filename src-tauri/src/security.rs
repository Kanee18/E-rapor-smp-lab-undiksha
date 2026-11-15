//! Kunci aplikasi dengan kata sandi.
//!
//! - Kata sandi dan kode pemulihan disimpan sebagai hash Argon2id (tidak bisa dibalik).
//! - Disimpan di `keamanan.json`, terpisah dari `data.json`, jadi tidak ikut backup:
//!   memulihkan backup lama tidak mengubah atau menghapus kata sandi.
//! - Selama terkunci, semua perintah data ditolak di sisi Rust (bukan hanya disembunyikan).
//! - Setelah 5 kali salah, percobaan berikutnya harus menunggu (30 dtk, lalu berlipat, maks. 10 mnt).
//!
//! Batasan yang disengaja: file data tidak dienkripsi. Kunci ini melindungi pemakaian
//! aplikasi di laptop bersama, bukan file yang disalin keluar.

use argon2::password_hash::rand_core::{OsRng, RngCore};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const FILE_NAME: &str = "keamanan.json";
const MIN_LEN: usize = 6;
const MAX_LEN: usize = 128;
const FREE_ATTEMPTS: u32 = 5;
/// Huruf/angka yang tidak mudah tertukar (tanpa 0/O, 1/I/L).
const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct SecurityFile {
    password_hash: String,
    recovery_hash: String,
    /// Kunci otomatis setelah sekian menit tidak dipakai; 0 = tidak pernah.
    auto_lock_minutes: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SecurityStatus {
    pub enabled: bool,
    pub locked: bool,
    pub auto_lock_minutes: u32,
    /// Terisi kalau keamanan.json rusak.
    pub problem: Option<String>,
}

pub struct Security {
    file: PathBuf,
    cfg: SecurityFile,
    locked: bool,
    failures: u32,
    blocked_until: Option<Instant>,
    /// Pesan kalau keamanan.json rusak: aplikasi tetap terkunci dan tidak bisa dibuka.
    broken: Option<String>,
}

fn hash(secret: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(secret.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| format!("Gagal mengamankan kata sandi: {e}"))
}

fn verify(secret: &str, phc: &str) -> bool {
    PasswordHash::new(phc)
        .map(|parsed| Argon2::default().verify_password(secret.as_bytes(), &parsed).is_ok())
        .unwrap_or(false)
}

/// "abcd-efgh 2345" → "ABCDEFGH2345" (spasi/tanda hubung dan huruf kecil diabaikan).
fn normalize_code(code: &str) -> String {
    code.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect()
}

/// Kode pemulihan acak 16 karakter, ditampilkan sebagai XXXX-XXXX-XXXX-XXXX.
fn new_recovery_code() -> String {
    let mut out = String::new();
    for i in 0..16 {
        if i > 0 && i % 4 == 0 {
            out.push('-');
        }
        // tolak sampel yang berat sebelah supaya setiap karakter sama peluangnya
        let n = CODE_ALPHABET.len() as u32;
        let limit = u32::MAX - (u32::MAX % n);
        let v = loop {
            let r = OsRng.next_u32();
            if r < limit {
                break r % n;
            }
        };
        out.push(CODE_ALPHABET[v as usize] as char);
    }
    out
}

fn check_new_password(p: &str) -> Result<(), String> {
    let len = p.chars().count();
    if len < MIN_LEN {
        return Err(format!("Kata sandi minimal {MIN_LEN} karakter."));
    }
    if len > MAX_LEN {
        return Err(format!("Kata sandi maksimal {MAX_LEN} karakter."));
    }
    Ok(())
}

impl Security {
    /// Baca pengaturan keamanan dari folder data. Kalau kata sandi aktif, mulai dalam keadaan terkunci.
    /// File yang rusak/tidak terbaca dianggap "terkunci tanpa bisa dibuka" — bukan "tanpa kata sandi" —
    /// supaya merusak file tidak bisa dipakai untuk melewati kunci.
    pub fn open(dir: &Path) -> Result<Self, String> {
        let file = dir.join(FILE_NAME);
        let cfg = match std::fs::read(&file) {
            Ok(bytes) => serde_json::from_slice::<SecurityFile>(&bytes).map_err(|e| {
                format!("{FILE_NAME} rusak ({e}). Pulihkan file itu atau hubungi pembuat aplikasi.")
            })?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => SecurityFile { auto_lock_minutes: 10, ..Default::default() },
            Err(e) => return Err(format!("{FILE_NAME} tidak bisa dibaca: {e}")),
        };
        let locked = !cfg.password_hash.is_empty();
        Ok(Self { file, cfg, locked, failures: 0, blocked_until: None, broken: None })
    }

    /// Dipakai kalau `open` gagal: terkunci permanen dengan penjelasan, bukan tanpa kata sandi.
    pub fn broken(dir: &Path, message: String) -> Self {
        let cfg = SecurityFile { password_hash: "!".into(), ..Default::default() };
        Self { file: dir.join(FILE_NAME), cfg, locked: true, failures: 0, blocked_until: None, broken: Some(message) }
    }

    pub fn enabled(&self) -> bool {
        !self.cfg.password_hash.is_empty()
    }

    pub fn is_locked(&self) -> bool {
        self.locked
    }

    pub fn status(&self) -> SecurityStatus {
        SecurityStatus {
            enabled: self.enabled(),
            locked: self.locked,
            auto_lock_minutes: self.cfg.auto_lock_minutes,
            problem: self.broken.clone(),
        }
    }

    fn save(&self) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(&self.cfg).map_err(|e| e.to_string())?;
        crate::store::write_atomic(&self.file, &bytes)
    }

    /// Tolak sementara kalau sudah terlalu sering salah.
    fn throttle(&self) -> Result<(), String> {
        if let Some(until) = self.blocked_until {
            let now = Instant::now();
            if now < until {
                let secs = (until - now).as_secs() + 1;
                return Err(format!("Terlalu banyak percobaan salah. Coba lagi dalam {secs} detik."));
            }
        }
        Ok(())
    }

    fn record_failure(&mut self) -> String {
        self.failures += 1;
        if self.failures >= FREE_ATTEMPTS {
            let step = (self.failures - FREE_ATTEMPTS).min(5);
            let wait = Duration::from_secs((30u64 << step).min(600));
            self.blocked_until = Some(Instant::now() + wait);
            format!("Kata sandi salah. Tunggu {} detik sebelum mencoba lagi.", wait.as_secs())
        } else {
            let left = FREE_ATTEMPTS - self.failures;
            format!("Kata sandi salah. Sisa {left} percobaan sebelum harus menunggu.")
        }
    }

    fn record_success(&mut self) {
        self.failures = 0;
        self.blocked_until = None;
    }

    pub fn lock(&mut self) {
        if self.enabled() {
            self.locked = true;
        }
    }

    pub fn unlock(&mut self, password: &str) -> Result<(), String> {
        if let Some(msg) = &self.broken {
            return Err(msg.clone());
        }
        if !self.enabled() {
            self.locked = false;
            return Ok(());
        }
        self.throttle()?;
        if verify(password, &self.cfg.password_hash) {
            self.record_success();
            self.locked = false;
            Ok(())
        } else {
            Err(self.record_failure())
        }
    }

    /// Lupa kata sandi: buka dengan kode pemulihan, langsung pasang kata sandi baru.
    /// Mengembalikan kode pemulihan BARU (kode lama tidak berlaku lagi).
    pub fn reset_with_recovery(&mut self, code: &str, new_password: &str) -> Result<String, String> {
        if let Some(msg) = &self.broken {
            return Err(msg.clone());
        }
        if !self.enabled() {
            return Err("Kata sandi belum dipasang.".into());
        }
        self.throttle()?;
        if !verify(&normalize_code(code), &self.cfg.recovery_hash) {
            let msg = self.record_failure();
            return Err(msg.replace("Kata sandi salah", "Kode pemulihan salah"));
        }
        check_new_password(new_password)?;
        self.record_success();
        let recovery = new_recovery_code();
        self.cfg.password_hash = hash(new_password)?;
        self.cfg.recovery_hash = hash(&normalize_code(&recovery))?;
        self.save()?;
        self.locked = false;
        Ok(recovery)
    }

    /// Pasang kata sandi baru atau ganti kata sandi.
    /// Saat pertama dipasang, mengembalikan kode pemulihan baru; saat diganti, kode lama tetap berlaku.
    pub fn set_password(&mut self, current: Option<&str>, new_password: &str) -> Result<Option<String>, String> {
        if self.locked {
            return Err("Aplikasi terkunci.".into());
        }
        check_new_password(new_password)?;
        if self.enabled() {
            self.throttle()?;
            if !verify(current.unwrap_or(""), &self.cfg.password_hash) {
                return Err(self.record_failure().replace("Kata sandi salah", "Kata sandi lama salah"));
            }
            self.record_success();
            self.cfg.password_hash = hash(new_password)?;
            self.save()?;
            Ok(None)
        } else {
            let recovery = new_recovery_code();
            self.cfg.password_hash = hash(new_password)?;
            self.cfg.recovery_hash = hash(&normalize_code(&recovery))?;
            if self.cfg.auto_lock_minutes == 0 {
                self.cfg.auto_lock_minutes = 10;
            }
            self.save()?;
            Ok(Some(recovery))
        }
    }

    pub fn disable(&mut self, current: &str) -> Result<(), String> {
        if self.locked {
            return Err("Aplikasi terkunci.".into());
        }
        if !self.enabled() {
            return Ok(());
        }
        self.throttle()?;
        if !verify(current, &self.cfg.password_hash) {
            return Err(self.record_failure());
        }
        self.record_success();
        self.cfg.password_hash.clear();
        self.cfg.recovery_hash.clear();
        self.save()
    }

    pub fn set_auto_lock(&mut self, minutes: u32) -> Result<(), String> {
        if self.locked {
            return Err("Aplikasi terkunci.".into());
        }
        if minutes > 240 {
            return Err("Maksimal 240 menit.".into());
        }
        self.cfg.auto_lock_minutes = minutes;
        self.save()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("eraport-sec-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn off_by_default_and_never_locked() {
        let d = temp_dir("off");
        let s = Security::open(&d).unwrap();
        assert_eq!(s.status(), SecurityStatus { enabled: false, locked: false, auto_lock_minutes: 10, problem: None });
    }

    #[test]
    fn enable_lock_unlock_and_persist_as_hash() {
        let d = temp_dir("on");
        let mut s = Security::open(&d).unwrap();
        assert!(s.set_password(None, "123").is_err(), "terlalu pendek");
        let code = s.set_password(None, "rahasia-sekolah").unwrap().expect("kode pemulihan");
        assert_eq!(code.len(), 19);
        let raw = std::fs::read_to_string(d.join(FILE_NAME)).unwrap();
        assert!(!raw.contains("rahasia-sekolah") && !raw.contains(&code), "hanya hash yang tersimpan");
        assert!(raw.contains("$argon2id$"));

        // dibuka ulang → terkunci
        let mut s = Security::open(&d).unwrap();
        assert!(s.is_locked());
        assert!(s.unlock("salah").is_err());
        assert!(s.is_locked());
        s.unlock("rahasia-sekolah").unwrap();
        assert!(!s.is_locked());

        // ganti kata sandi butuh kata sandi lama; kode pemulihan lama tetap berlaku
        assert!(s.set_password(Some("salah"), "baru-123").is_err());
        assert_eq!(s.set_password(Some("rahasia-sekolah"), "baru-123").unwrap(), None);
        s.lock();
        assert!(s.unlock("rahasia-sekolah").is_err());
        s.unlock("baru-123").unwrap();
    }

    #[test]
    fn recovery_code_resets_password_and_rotates() {
        let d = temp_dir("rec");
        let mut s = Security::open(&d).unwrap();
        let code = s.set_password(None, "lupa-nanti").unwrap().unwrap();
        s.lock();
        assert!(s.reset_with_recovery("AAAA-AAAA-AAAA-AAAA", "baru-sekali").is_err());
        // huruf kecil & tanpa tanda hubung tetap diterima
        let loose = code.replace('-', " ").to_lowercase();
        let code2 = s.reset_with_recovery(&loose, "baru-sekali").unwrap();
        assert_ne!(code, code2);
        assert!(!s.is_locked());
        s.lock();
        s.unlock("baru-sekali").unwrap();
        // kode lama sudah tidak berlaku
        s.lock();
        assert!(s.reset_with_recovery(&code, "x-y-z-123").is_err());
    }

    #[test]
    fn too_many_failures_must_wait() {
        let d = temp_dir("thr");
        let mut s = Security::open(&d).unwrap();
        s.set_password(None, "benar-sekali").unwrap();
        s.lock();
        for _ in 0..FREE_ATTEMPTS {
            assert!(s.unlock("salah").is_err());
        }
        // sekarang kata sandi benar pun harus menunggu
        let err = s.unlock("benar-sekali").unwrap_err();
        assert!(err.contains("Coba lagi"), "{err}");
        assert!(s.is_locked());
    }

    #[test]
    fn disable_requires_password_and_corrupt_file_does_not_unlock() {
        let d = temp_dir("dis");
        let mut s = Security::open(&d).unwrap();
        s.set_password(None, "matikan-aku").unwrap();
        assert!(s.disable("salah").is_err());
        s.disable("matikan-aku").unwrap();
        assert!(!Security::open(&d).unwrap().enabled());

        std::fs::write(d.join(FILE_NAME), b"{rusak").unwrap();
        let err = Security::open(&d).err().expect("file rusak tidak boleh berarti tanpa kata sandi");
        let mut b = Security::broken(&d, err);
        assert!(b.status().enabled && b.is_locked());
        assert!(b.unlock("apa saja").is_err());
        assert!(b.reset_with_recovery("AAAA", "apa-saja-1").is_err());
    }

    #[test]
    fn recovery_codes_use_safe_alphabet() {
        for _ in 0..50 {
            let c = new_recovery_code();
            assert!(c.chars().filter(|&ch| ch != '-').all(|ch| CODE_ALPHABET.contains(&(ch as u8))));
        }
    }
}
