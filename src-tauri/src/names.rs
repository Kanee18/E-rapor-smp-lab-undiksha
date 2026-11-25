//! Pencocokan nama siswa antara file nilai dan data siswa.
//!
//! Di file nilai, nama sering ditulis berbeda dengan Dapodik:
//! - huruf besar semua ("DESAK KETUT ..." vs "Desak Ketut ...")
//! - disingkat ("ASTU P." vs "Astu Pradnyana", "D. D." vs "Divma Djelantik")
//! - salah ketik kecil ("WIDHAYANTI" vs "WIDAYANTI")
//!
//! Pencocokan dilakukan bertahap: persis → singkatan → mirip. Tiap siswa
//! hanya boleh dipakai satu kali, dan kalau ada dua kandidat yang sama-sama
//! cocok, nama itu TIDAK dicocokkan (dilaporkan ke operator).

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchKind {
    Exact,
    Abbreviation,
    Similar,
}

/// Huruf besar, tanda baca jadi spasi, spasi ganda dirapikan.
pub fn normalize_name(s: &str) -> String {
    let mapped: String = s
        .chars()
        .flat_map(|c| {
            if c.is_alphanumeric() {
                c.to_uppercase().collect::<Vec<_>>()
            } else {
                vec![' ']
            }
        })
        .collect();
    mapped.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Sama jumlah katanya, dan setiap kata sama persis ATAU salah satunya
/// berupa inisial satu huruf dari kata pasangannya.
fn abbreviation_match(a: &[&str], b: &[&str]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut used = false;
    for (x, y) in a.iter().zip(b) {
        if x == y {
            continue;
        }
        if x.chars().count() == 1 && y.starts_with(x) || y.chars().count() == 1 && x.starts_with(y) {
            used = true;
            continue;
        }
        return false;
    }
    used
}

/// Nama dipendekkan: minimal 2 kata, dan semua katanya = kata-kata AWAL nama lengkap
/// ("Luh Sari" → "Luh Sari Dewi") atau kata-kata AKHIR-nya ("Ayu Lestari" → "I Made Ayu Lestari").
fn shortened_match(q: &[&str], full: &[&str]) -> bool {
    q.len() >= 2 && q.len() < full.len() && (full.starts_with(q) || full.ends_with(q))
}

fn similarity(a: &str, b: &str) -> (f64, usize) {
    let d = levenshtein(a, b);
    let m = a.chars().count().max(b.chars().count());
    if m == 0 {
        (1.0, 0)
    } else {
        (1.0 - d as f64 / m as f64, d)
    }
}

/// Cocokkan setiap nama di `queries` ke salah satu `candidates`.
/// Hasil: untuk tiap query, `Some((indeks kandidat, jenis))` atau `None`.
pub fn match_names(queries: &[String], candidates: &[String]) -> Vec<Option<(usize, MatchKind)>> {
    let qn: Vec<String> = queries.iter().map(|s| normalize_name(s)).collect();
    let cn: Vec<String> = candidates.iter().map(|s| normalize_name(s)).collect();
    let mut result: Vec<Option<(usize, MatchKind)>> = vec![None; queries.len()];
    let mut taken = vec![false; candidates.len()];

    // Tahap 1: persis sama.
    for (qi, q) in qn.iter().enumerate() {
        if q.is_empty() {
            continue;
        }
        if let Some(ci) = (0..cn.len()).find(|&ci| !taken[ci] && cn[ci] == *q) {
            taken[ci] = true;
            result[qi] = Some((ci, MatchKind::Exact));
        }
    }

    // Tahap 2: singkatan/inisial.
    for qi in 0..qn.len() {
        if result[qi].is_some() || qn[qi].is_empty() {
            continue;
        }
        let qt: Vec<&str> = qn[qi].split(' ').collect();
        let hits: Vec<usize> = (0..cn.len())
            .filter(|&ci| {
                !taken[ci] && abbreviation_match(&qt, &cn[ci].split(' ').collect::<Vec<_>>())
            })
            .collect();
        if let [only] = hits[..] {
            taken[only] = true;
            result[qi] = Some((only, MatchKind::Abbreviation));
        }
    }

    // Tahap 2b: nama dipendekkan (kata depan/belakang dihilangkan), harus satu-satunya kandidat.
    for qi in 0..qn.len() {
        if result[qi].is_some() || qn[qi].is_empty() {
            continue;
        }
        let qt: Vec<&str> = qn[qi].split(' ').collect();
        let hits: Vec<usize> = (0..cn.len())
            .filter(|&ci| shortened_match(&qt, &cn[ci].split(' ').collect::<Vec<_>>()))
            .collect();
        if let [only] = hits[..] {
            if !taken[only] {
                taken[only] = true;
                result[qi] = Some((only, MatchKind::Abbreviation));
            }
        }
    }

    // Tahap 3: mirip (salah ketik 1–3 huruf), harus jelas lebih mirip dari kandidat lain.
    for qi in 0..qn.len() {
        if result[qi].is_some() || qn[qi].is_empty() {
            continue;
        }
        let mut scored: Vec<(usize, f64, usize)> = (0..cn.len())
            .filter(|&ci| !taken[ci])
            .map(|ci| {
                let (sim, dist) = similarity(&qn[qi], &cn[ci]);
                (ci, sim, dist)
            })
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        if let Some(&(ci, sim, dist)) = scored.first() {
            let second = scored.get(1).map_or(0.0, |s| s.1);
            if sim >= 0.9 && dist <= 3 && sim - second >= 0.05 {
                taken[ci] = true;
                result[qi] = Some((ci, MatchKind::Similar));
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn normalizes() {
        assert_eq!(normalize_name("  Astu  P. "), "ASTU P");
        assert_eq!(normalize_name("I Gusti Bagus D.D."), "I GUSTI BAGUS D D");
    }

    #[test]
    fn levenshtein_basic() {
        assert_eq!(levenshtein("WIDHAYANTI", "WIDAYANTI"), 1);
        assert_eq!(levenshtein("", "ABC"), 3);
        assert_eq!(levenshtein("SAMA", "SAMA"), 0);
    }

    #[test]
    fn matches_real_world_variants() {
        // Pola nama yang ada di file contoh (nama disamarkan).
        let queries = s(&[
            "KADEK ARYA P.",
            "I GUSTI BAGUS WIRA D. D.",
            "NI PUTU SARI WIDHAYANTI",
            "KADEK BIANCA GIOVANI",
            "MADE DEWI",
        ]);
        let candidates = s(&[
            "Made Dewi",
            "Ni Putu Sari Widayanti",
            "Kadek Bianca Giovanni",
            "Kadek Arya Pradnyana",
            "I Gusti Bagus Wira Divma Djelantik",
        ]);
        let r = match_names(&queries, &candidates);
        assert_eq!(r[0], Some((3, MatchKind::Abbreviation)));
        assert_eq!(r[1], Some((4, MatchKind::Abbreviation)));
        assert_eq!(r[2], Some((1, MatchKind::Similar)));
        assert_eq!(r[3], Some((2, MatchKind::Similar)));
        assert_eq!(r[4], Some((0, MatchKind::Exact)));
    }

    #[test]
    fn shortened_names_match_only_when_unique() {
        let cand = s(&["Luh Sari Dewi", "I Made Ayu Lestari", "Putu Ayu Lestari", "Kadek Bayu"]);
        let r = match_names(&s(&["LUH SARI", "MADE AYU LESTARI", "AYU LESTARI", "BAYU"]), &cand);
        assert_eq!(r[0], Some((0, MatchKind::Abbreviation)));
        assert_eq!(r[1], Some((1, MatchKind::Abbreviation)));
        assert_eq!(r[2], None, "dua siswa bernama ...Ayu Lestari: jangan menebak");
        assert_eq!(r[3], None, "satu kata saja terlalu umum");
    }

    #[test]
    fn ambiguous_abbreviation_is_not_matched() {
        let r = match_names(&s(&["KADEK A."]), &s(&["Kadek Arya", "Kadek Ayu"]));
        assert_eq!(r[0], None);
    }

    #[test]
    fn different_people_are_not_matched() {
        let r = match_names(&s(&["PUTU GEDE"]), &s(&["Komang Ayu Lestari"]));
        assert_eq!(r[0], None);
    }

    #[test]
    fn each_candidate_used_once() {
        let r = match_names(&s(&["MADE DEWI", "MADE DEWI"]), &s(&["Made Dewi"]));
        assert_eq!(r[0], Some((0, MatchKind::Exact)));
        assert_eq!(r[1], None);
    }
}
