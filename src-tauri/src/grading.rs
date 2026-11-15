//! Perhitungan nilai akhir dan konversi angka → predikat (EE/ME/AE/BE).
//!
//! Aturan:
//! - Nilai akhir = rata-rata berbobot PH dan PTS (bobot di Pengaturan).
//!   Komponen yang bobotnya > 0 wajib ada; kalau kosong → "belum lengkap".
//! - Nilai akhir manual (kalau diisi) mengalahkan rumus.
//! - Konversi memakai BATAS BAWAH tiap predikat, jadi tidak ada celah
//!   (mis. 85,7 tanpa pembulatan tetap masuk BE, bukan "tidak terdefinisi").
//! - Nilai di bawah batas predikat terendah tetap diberi predikat terendah,
//!   tapi ditandai `below_scale` supaya operator bisa mengecek.

use crate::model::{Band, GradingConfig, ScoreEntry};
use serde::Serialize;
use std::cmp::Ordering;

const EPS: f64 = 1e-9;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Ada predikat yang sah.
    Ok,
    /// Belum diisi sama sekali.
    #[default]
    Empty,
    /// Sebagian komponen nilai belum ada.
    Incomplete,
    /// Nilai di bawah rentang predikat terendah (perlu dicek).
    BelowScale,
    /// Isian tidak bisa dibaca.
    Invalid,
}

/// Hasil akhir sebuah isian/nilai setelah dikonversi.
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct Resolved {
    pub code: Option<String>,
    pub label: Option<String>,
    /// Nilai angka yang dipakai untuk konversi (kalau isiannya angka).
    pub score: Option<f64>,
    pub status: Status,
    /// Penjelasan singkat (bahasa Indonesia) untuk ditampilkan di aplikasi.
    pub message: String,
}

/// Bulatkan setengah ke atas: 85,5 → 86; 85,49 → 85.
/// Dibulatkan dulu ke 6 desimal supaya galat floating point
/// (mis. 85,49999999999999 yang seharusnya 85,5) tidak mengubah hasil.
pub fn round_half_up(v: f64) -> f64 {
    let v6 = (v * 1e6).round() / 1e6;
    (v6 + 0.5).floor()
}

fn apply_rounding(v: f64, cfg: &GradingConfig) -> f64 {
    if cfg.round_final {
        round_half_up(v)
    } else {
        (v * 100.0).round() / 100.0
    }
}

/// Nilai akhir sebuah mapel. `missing` berisi komponen yang belum ada.
#[derive(Debug, Clone, PartialEq)]
pub struct FinalScore {
    pub value: Option<f64>,
    pub missing: Vec<&'static str>,
    pub from_manual: bool,
}

pub fn final_score(entry: Option<&ScoreEntry>, cfg: &GradingConfig) -> FinalScore {
    let e = entry.cloned().unwrap_or_default();
    if let Some(m) = e.manual {
        return FinalScore {
            value: Some(apply_rounding(m, cfg)),
            missing: vec![],
            from_manual: true,
        };
    }
    let mut sum = 0.0;
    let mut wsum = 0.0;
    let mut missing = Vec::new();
    for (label, w, v) in [("PH", cfg.weight_ph, e.ph), ("PTS", cfg.weight_pts, e.pts)] {
        if w <= 0.0 {
            continue;
        }
        match v {
            Some(x) => {
                sum += w * x;
                wsum += w;
            }
            None => missing.push(label),
        }
    }
    let value = if missing.is_empty() && wsum > 0.0 {
        Some(apply_rounding(sum / wsum, cfg))
    } else {
        None
    };
    FinalScore {
        value,
        missing,
        from_manual: false,
    }
}

/// Predikat untuk sebuah nilai angka.
pub fn classify(score: f64, bands: &[Band]) -> Option<(&Band, bool)> {
    let mut sorted: Vec<&Band> = bands.iter().collect();
    sorted.sort_by(|a, b| b.min.partial_cmp(&a.min).unwrap_or(Ordering::Equal));
    for b in &sorted {
        if score + EPS >= b.min {
            return Some((b, false));
        }
    }
    sorted.last().map(|b| (*b, true))
}

fn resolved_from_score(score: f64, cfg: &GradingConfig) -> Resolved {
    match classify(score, &cfg.bands) {
        Some((band, below)) => Resolved {
            code: Some(band.code.clone()),
            label: Some(band.label.clone()),
            score: Some(score),
            status: if below { Status::BelowScale } else { Status::Ok },
            message: if below {
                format!(
                    "Nilai {} di bawah batas {} ({}); aturannya belum ditentukan",
                    fmt_num(score),
                    band.code,
                    fmt_num(band.min)
                )
            } else {
                String::new()
            },
        },
        None => Resolved {
            score: Some(score),
            status: Status::Invalid,
            message: "Rentang predikat belum diatur".into(),
            ..Default::default()
        },
    }
}

/// Nilai akhir + predikat sebuah mapel.
pub fn resolve_subject(entry: Option<&ScoreEntry>, cfg: &GradingConfig) -> Resolved {
    let fs = final_score(entry, cfg);
    let nothing_filled =
        entry.is_none_or(|e| e.ph.is_none() && e.pts.is_none() && e.manual.is_none());
    match fs.value {
        Some(v) => resolved_from_score(v, cfg),
        None if nothing_filled => Resolved {
            status: Status::Empty,
            message: "Nilai belum diisi".into(),
            ..Default::default()
        },
        None => Resolved {
            status: Status::Incomplete,
            message: format!("Nilai {} belum ada", fs.missing.join(" & ")),
            ..Default::default()
        },
    }
}

/// Angka dengan koma atau titik desimal ("85,5" / "85.5").
pub fn parse_number(t: &str) -> Option<f64> {
    t.trim()
        .replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
}

/// Isian bebas untuk ATL/proyek: kode predikat ("ME", "me") ATAU angka ("92").
pub fn resolve_input(raw: &str, cfg: &GradingConfig) -> Resolved {
    let t = raw.trim();
    if t.is_empty() {
        return Resolved::default();
    }
    if let Some(b) = cfg.bands.iter().find(|b| b.code.eq_ignore_ascii_case(t)) {
        return Resolved {
            code: Some(b.code.clone()),
            label: Some(b.label.clone()),
            status: Status::Ok,
            ..Default::default()
        };
    }
    match parse_number(t) {
        Some(v) if (0.0..=100.0).contains(&v) => resolved_from_score(apply_rounding(v, cfg), cfg),
        Some(_) => Resolved {
            status: Status::Invalid,
            message: "Nilai harus 0–100".into(),
            ..Default::default()
        },
        None => Resolved {
            status: Status::Invalid,
            message: format!("Isi angka 0–100 atau kode {}", codes(cfg)),
            ..Default::default()
        },
    }
}

fn codes(cfg: &GradingConfig) -> String {
    cfg.bands
        .iter()
        .map(|b| b.code.as_str())
        .collect::<Vec<_>>()
        .join("/")
}

/// Cek nilai angka yang diketik operator.
pub fn validate_score(v: Option<f64>, what: &str) -> Result<(), String> {
    match v {
        Some(x) if !x.is_finite() || !(0.0..=100.0).contains(&x) => {
            Err(format!("{what}: nilai harus di antara 0 dan 100"))
        }
        _ => Ok(()),
    }
}

/// Cek pengaturan penilaian sebelum disimpan.
pub fn validate_grading(cfg: &GradingConfig) -> Result<(), String> {
    if cfg.bands.is_empty() {
        return Err("Minimal harus ada satu predikat".into());
    }
    for w in [cfg.weight_ph, cfg.weight_pts] {
        if !w.is_finite() || w < 0.0 {
            return Err("Bobot nilai tidak boleh negatif".into());
        }
    }
    if cfg.weight_ph + cfg.weight_pts <= 0.0 {
        return Err("Total bobot PH + PTS harus lebih dari 0".into());
    }
    let mut seen_codes: Vec<String> = Vec::new();
    let mut seen_mins: Vec<f64> = Vec::new();
    for b in &cfg.bands {
        let code = b.code.trim();
        if code.is_empty() {
            return Err("Kode predikat tidak boleh kosong".into());
        }
        if parse_number(code).is_some() {
            return Err(format!("Kode predikat \"{code}\" tidak boleh berupa angka"));
        }
        if seen_codes.iter().any(|c| c.eq_ignore_ascii_case(code)) {
            return Err(format!("Kode predikat \"{code}\" dipakai dua kali"));
        }
        if !(0.0..=100.0).contains(&b.min) || !(0.0..=100.0).contains(&b.max) {
            return Err(format!("Rentang {code} harus di antara 0 dan 100"));
        }
        if b.max < b.min {
            return Err(format!("Rentang {code}: batas atas lebih kecil dari batas bawah"));
        }
        if seen_mins.iter().any(|m| (m - b.min).abs() < EPS) {
            return Err(format!("Batas bawah {code} sama dengan predikat lain"));
        }
        seen_codes.push(code.to_string());
        seen_mins.push(b.min);
    }
    Ok(())
}

/// 96.0 → "96", 85.5 → "85.5".
pub fn fmt_num(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        let s = format!("{v:.2}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> GradingConfig {
        GradingConfig::default()
    }

    fn code(score: f64) -> String {
        classify(score, &cfg().bands).unwrap().0.code.clone()
    }

    #[test]
    fn boundaries_follow_principal_scale() {
        assert_eq!(code(100.0), "EE");
        assert_eq!(code(96.0), "EE");
        assert_eq!(code(95.0), "ME");
        assert_eq!(code(91.0), "ME");
        assert_eq!(code(90.0), "AE");
        assert_eq!(code(86.0), "AE");
        assert_eq!(code(85.0), "BE");
        assert_eq!(code(80.0), "BE");
    }

    #[test]
    fn below_lowest_band_is_flagged() {
        let c = cfg();
        let (b, below) = classify(56.0, &c.bands).unwrap();
        assert_eq!(b.code, "BE");
        assert!(below);
        let r = resolve_subject(Some(&ScoreEntry { manual: Some(56.0), ..Default::default() }), &c);
        assert_eq!(r.status, Status::BelowScale);
    }

    #[test]
    fn rounding_half_up() {
        assert_eq!(round_half_up(85.5), 86.0);
        assert_eq!(round_half_up(85.49), 85.0);
        assert_eq!(round_half_up(92.00000000000001), 92.0);
        assert_eq!(round_half_up(85.49999999999999), 86.0);
    }

    #[test]
    fn decimal_between_bands_depends_on_rounding_setting() {
        let mut c = cfg();
        let e = ScoreEntry { ph: Some(85.0), pts: Some(86.0), manual: None }; // 85,5
        assert_eq!(resolve_subject(Some(&e), &c).code.as_deref(), Some("AE"));
        c.round_final = false;
        assert_eq!(resolve_subject(Some(&e), &c).code.as_deref(), Some("BE"));
    }

    #[test]
    fn weighted_average() {
        let c = cfg();
        let e = ScoreEntry { ph: Some(87.0), pts: Some(95.0), manual: None };
        let fs = final_score(Some(&e), &c);
        assert_eq!(fs.value, Some(91.0));
        let mut c2 = cfg();
        c2.weight_ph = 40.0;
        c2.weight_pts = 60.0;
        // 0.4*80 + 0.6*90 = 86
        let e2 = ScoreEntry { ph: Some(80.0), pts: Some(90.0), manual: None };
        assert_eq!(final_score(Some(&e2), &c2).value, Some(86.0));
    }

    #[test]
    fn missing_component_is_incomplete() {
        let c = cfg();
        let e = ScoreEntry { ph: Some(90.0), pts: None, manual: None };
        let r = resolve_subject(Some(&e), &c);
        assert_eq!(r.status, Status::Incomplete);
        assert!(r.message.contains("PTS"));
        assert_eq!(resolve_subject(None, &c).status, Status::Empty);
    }

    #[test]
    fn zero_weight_component_not_required() {
        let mut c = cfg();
        c.weight_ph = 0.0;
        c.weight_pts = 100.0;
        let e = ScoreEntry { ph: None, pts: Some(92.0), manual: None };
        assert_eq!(resolve_subject(Some(&e), &c).code.as_deref(), Some("ME"));
    }

    #[test]
    fn manual_final_overrides_formula() {
        let c = cfg();
        let e = ScoreEntry { ph: Some(60.0), pts: Some(60.0), manual: Some(97.0) };
        assert_eq!(resolve_subject(Some(&e), &c).code.as_deref(), Some("EE"));
    }

    #[test]
    fn free_input_accepts_code_or_number() {
        let c = cfg();
        assert_eq!(resolve_input("me", &c).code.as_deref(), Some("ME"));
        assert_eq!(resolve_input(" EE ", &c).code.as_deref(), Some("EE"));
        assert_eq!(resolve_input("92", &c).code.as_deref(), Some("ME"));
        assert_eq!(resolve_input("85,5", &c).code.as_deref(), Some("AE"));
        assert_eq!(resolve_input("", &c).status, Status::Empty);
        assert_eq!(resolve_input("101", &c).status, Status::Invalid);
        assert_eq!(resolve_input("bagus", &c).status, Status::Invalid);
    }

    #[test]
    fn grading_validation() {
        assert!(validate_grading(&cfg()).is_ok());
        let mut c = cfg();
        c.bands[1].code = "ee".into();
        assert!(validate_grading(&c).is_err());
        let mut c = cfg();
        c.weight_ph = 0.0;
        c.weight_pts = 0.0;
        assert!(validate_grading(&c).is_err());
        let mut c = cfg();
        c.bands[0].code = "90".into();
        assert!(validate_grading(&c).is_err());
        let mut c = cfg();
        c.bands[2].min = 91.0;
        assert!(validate_grading(&c).is_err());
    }

    #[test]
    fn number_formatting() {
        assert_eq!(fmt_num(96.0), "96");
        assert_eq!(fmt_num(85.5), "85.5");
        assert_eq!(fmt_num(83.416666), "83.42");
    }
}
