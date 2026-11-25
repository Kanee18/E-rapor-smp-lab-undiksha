//! Kode 6 angka ala Google Authenticator (TOTP, RFC 6238: HMAC-SHA1, 30 detik).
//!
//! Dipakai sebagai cara kedua mengganti kata sandi yang terlupa. Berjalan tanpa internet:
//! HP dan laptop sama-sama menghitung kode dari rahasia bersama + jam. Karena itu jam laptop
//! harus kurang lebih benar; kode yang cocok dengan jam yang meleset dilaporkan supaya
//! pengguna tahu harus membetulkan jam, bukan mengira kodenya salah.

use hmac::{Hmac, Mac};
use sha1::Sha1;

pub const STEP: u64 = 30;
/// Kelonggaran jam: ±2 langkah (±60 detik).
const WINDOW: i64 = 2;
/// Untuk pesan "jam laptop meleset": periksa sampai ±13 jam (zona waktu keliru, jam BIOS mundur).
const DRIFT_STEPS: i64 = 13 * 3600 / STEP as i64;

const B32: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

pub fn base32(bytes: &[u8]) -> String {
    let mut out = String::new();
    let mut buf: u32 = 0;
    let mut bits = 0;
    for &b in bytes {
        buf = (buf << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            out.push(B32[((buf >> (bits - 5)) & 31) as usize] as char);
            bits -= 5;
        }
    }
    if bits > 0 {
        out.push(B32[((buf << (5 - bits)) & 31) as usize] as char);
    }
    out
}

pub fn base32_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut buf: u32 = 0;
    let mut bits = 0;
    for ch in s.chars().filter(|c| !c.is_whitespace() && *c != '=' && *c != '-') {
        let v = B32.iter().position(|&b| b as char == ch.to_ascii_uppercase())? as u32;
        buf = (buf << 5) | v;
        bits += 5;
        if bits >= 8 {
            out.push(((buf >> (bits - 8)) & 0xFF) as u8);
            bits -= 8;
        }
    }
    Some(out)
}

/// Kode untuk langkah waktu `counter` (= detik_unix / 30).
pub fn code_at(secret: &[u8], counter: u64) -> u32 {
    let mut mac = <Hmac<Sha1> as Mac>::new_from_slice(secret).expect("HMAC menerima kunci berapa pun panjangnya");
    mac.update(&counter.to_be_bytes());
    let h = mac.finalize().into_bytes();
    let off = (h[19] & 0x0f) as usize;
    let bin = u32::from_be_bytes([h[off] & 0x7f, h[off + 1], h[off + 2], h[off + 3]]);
    bin % 1_000_000
}

/// "123 456" → Some(123456). Harus tepat 6 angka.
pub fn parse_code(input: &str) -> Option<u32> {
    let digits: String = input.chars().filter(|c| !c.is_whitespace() && *c != '-').collect();
    (digits.len() == 6 && digits.chars().all(|c| c.is_ascii_digit())).then(|| digits.parse().ok()).flatten()
}

#[derive(Debug, PartialEq)]
pub enum Check {
    Ok,
    Wrong,
    /// Kode cocok, tapi untuk jam yang meleset sekian detik (positif = jam laptop terlambat).
    ClockOff(i64),
}

pub fn check(secret: &[u8], input: &str, now_unix: u64) -> Check {
    let Some(code) = parse_code(input) else { return Check::Wrong };
    let now = (now_unix / STEP) as i64;
    let at = |d: i64| u64::try_from(now + d).ok().map(|c| code_at(secret, c) == code).unwrap_or(false);
    if (-WINDOW..=WINDOW).any(at) {
        return Check::Ok;
    }
    // urut dari yang terdekat supaya perkiraan meleset paling masuk akal
    (WINDOW + 1..=DRIFT_STEPS)
        .flat_map(|d| [d, -d])
        .find(|&d| at(d))
        .map(|d| Check::ClockOff(d * STEP as i64))
        .unwrap_or(Check::Wrong)
}

pub fn now_unix() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Alamat untuk aplikasi authenticator (isi kode QR).
pub fn otpauth_uri(secret_b32: &str, account: &str) -> String {
    let enc = |s: &str| {
        s.bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
                _ => format!("%{b:02X}"),
            })
            .collect::<String>()
    };
    format!(
        "otpauth://totp/{}:{}?secret={secret_b32}&issuer={}&algorithm=SHA1&digits=6&period=30",
        enc("E-Raport"),
        enc(account),
        enc("E-Raport")
    )
}

/// Kode QR sebagai SVG (ditampilkan langsung di aplikasi, tanpa internet).
pub fn qr_svg(text: &str) -> Result<String, String> {
    use qrcode::render::svg;
    let code = qrcode::QrCode::with_error_correction_level(text, qrcode::EcLevel::M).map_err(|e| e.to_string())?;
    Ok(code.render::<svg::Color>().min_dimensions(220, 220).quiet_zone(true).build())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc6238_reference_codes() {
        // RFC 6238 lampiran B (SHA1, rahasia "12345678901234567890"), 6 digit terakhir
        let secret = b"12345678901234567890";
        assert_eq!(code_at(secret, 59 / 30), 287082);
        assert_eq!(code_at(secret, 1111111109 / 30), 81804);
        assert_eq!(code_at(secret, 1234567890 / 30), 5924);
        assert_eq!(code_at(secret, 2000000000 / 30), 279037);
    }

    #[test]
    fn base32_round_trip() {
        let bytes: Vec<u8> = (0u8..20).collect();
        let s = base32(&bytes);
        assert_eq!(s.len(), 32);
        assert_eq!(base32_decode(&s).unwrap(), bytes);
        assert_eq!(base32(b"12345678901234567890"), "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ");
    }

    #[test]
    fn checks_window_and_clock_drift() {
        let secret = b"12345678901234567890";
        let t = 1_800_000_000u64;
        let now_code = format!("{:06}", code_at(secret, t / 30));
        assert_eq!(check(secret, &now_code, t), Check::Ok);
        assert_eq!(check(secret, &format!("{} {}", &now_code[..3], &now_code[3..]), t + 45), Check::Ok, "spasi boleh, 45 dtk masih masuk");
        // jam laptop terlambat 7 jam (zona waktu keliru): kode HP = kode 7 jam ke depan
        let ahead = format!("{:06}", code_at(secret, (t + 7 * 3600) / 30));
        assert_eq!(check(secret, &ahead, t), Check::ClockOff(7 * 3600));
        assert_eq!(check(secret, "12345", t), Check::Wrong);
        assert_eq!(check(secret, "abcdef", t), Check::Wrong);
    }

    #[test]
    fn qr_and_uri() {
        let uri = otpauth_uri("JBSWY3DPEHPK3PXP", "SMP Lab Undiksha");
        assert!(uri.starts_with("otpauth://totp/E-Raport:SMP%20Lab%20Undiksha?secret=JBSWY3DPEHPK3PXP&issuer=E-Raport"));
        assert!(qr_svg(&uri).unwrap().contains("<svg"));
    }
}
