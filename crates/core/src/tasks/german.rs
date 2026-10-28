//! The fallback language guess when the language check failed.

pub(super) const GERMAN_WORDS: &[&str] = &[
    "aber", "als", "auch", "auf", "bei", "das", "dass", "dem", "den", "der", "des", "die", "du",
    "durch", "ein", "eine", "einem", "einen", "er", "für", "haben", "hat", "hatte", "hier", "ich",
    "ihr", "im", "in", "ist", "jetzt", "mehr", "mit", "nach", "nicht", "noch", "nur", "oder",
    "ohne", "schon", "sehr", "sein", "sich", "sie", "sind", "und", "unter", "von", "vor", "war",
    "waren", "weil", "wenn", "werden", "wir", "wird", "wurde", "zu", "über",
];

/// Fallback language guess: umlauts/ß, or ≥ 15 % German function words.
pub fn looks_german(text: &str) -> bool {
    const UMLAUTS: &str = "äöüÄÖÜß";
    if text.chars().any(|c| UMLAUTS.contains(c)) {
        return true;
    }
    let lower = text.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !(c.is_ascii_alphabetic() || UMLAUTS.contains(c)))
        .filter(|w| !w.is_empty())
        .collect();
    let hits = words.iter().filter(|w| GERMAN_WORDS.contains(w)).count();
    !words.is_empty() && hits as f64 / words.len() as f64 >= 0.15
}
