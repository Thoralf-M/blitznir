//! Text shaping for transcripts: stable prefixes, joins, punctuation tidying.
//!
//! Every length and index here counts Unicode code points (not bytes, not
//! UTF-16 units), so NFC "ö" and NFD "o" + U+0308 differ. Golden cases:
//! `fixtures/textproc.json`.

use regex::{Captures, Regex};
use std::sync::LazyLock;
use unicode_segmentation::UnicodeSegmentation;

/// How many Backspaces take `text` back: one per character as the eye sees
/// it (a grapheme cluster). 🇩🇪 or 👍🏽 is one, though two or more code points:
/// counted by code point, the extra Backspaces ate the user's own text.
pub fn backspaces(text: &str) -> usize {
    text.graphemes(true).count()
}

static DOTS: LazyLock<Regex> = LazyLock::new(|| re(r"\.{2,}"));
static SPACE_BEFORE_PUNCT: LazyLock<Regex> = LazyLock::new(|| re(r" +([.,!?;])"));
static SPACES: LazyLock<Regex> = LazyLock::new(|| re(r"  +"));
static DOT_LOWER: LazyLock<Regex> = LazyLock::new(|| re(r"\.( +\p{Lowercase})"));

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("constant regex")
}

/// Code points `a` and `b` share from the start.
fn common_prefix(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
}

/// The first `n` code points of `s`.
fn head(s: &str, n: usize) -> &str {
    s.char_indices().nth(n).map_or(s, |(i, _)| &s[..i])
}

/// Text two consecutive hypotheses agree on, cut at a word boundary (an
/// ASCII space, which the recognizer emits between words).
pub fn stable_prefix(prev: &str, cur: &str) -> String {
    let agreed = head(cur, common_prefix(prev, cur));
    agreed
        .rfind(' ')
        .map_or_else(String::new, |cut| cur[..=cut].to_string())
}

/// How many code points of `b` would have to be deleted to get to a prefix of `a`.
pub fn divergence(a: &str, b: &str) -> usize {
    b.chars().count() - common_prefix(a, b)
}

/// What turns the screen from `typed` into `target`: backspaces, then text.
pub fn typing_diff<'a>(typed: &str, target: &'a str) -> (usize, &'a str) {
    let p = common_prefix(typed, target);
    (typed.chars().count() - p, &target[head(target, p).len()..])
}

/// A word whose dot belongs to it: "e.g", "z.B" (letters, one or two between
/// the dots) or a common abbreviation; its dot ends no sentence.
fn abbreviation(word: &str) -> bool {
    const KNOWN: [&str; 10] = [
        "etc", "vs", "ca", "approx", "incl", "bzw", "usw", "vgl", "ggf", "evtl",
    ];
    let word = word.trim_start_matches(|c: char| !c.is_alphanumeric());
    let short =
        |p: &str| (1..=2).contains(&p.chars().count()) && p.chars().all(char::is_alphabetic);
    (word.contains('.') && word.split('.').all(short)) || KNOWN.contains(&&*word.to_lowercase())
}

/// Fix pause-punctuation artifacts. A period followed by a lowercase word
/// means the model closed the sentence at a small pause but then continued
/// it — that reads better as a comma, unless the period ends an abbreviation.
/// Also collapse stray repeated dots.
pub fn tidy(text: &str) -> String {
    let text = DOTS.replace_all(text, ".");
    let text = SPACE_BEFORE_PUNCT.replace_all(&text, "${1}");
    let text = SPACES.replace_all(&text, " ");
    let comma = |c: &Captures<'_>| {
        let at = c.get(0).map_or(0, |m| m.start());
        let word = text[..at].rsplit(char::is_whitespace).next();
        let mark = if word.is_some_and(abbreviation) {
            "."
        } else {
            ","
        };
        format!("{mark}{}", c.get(1).map_or("", |m| m.as_str()))
    };
    DOT_LOWER.replace_all(&text, comma).into_owned()
}

/// `prev` + `new`, decapitalizing new's first word when it continues an
/// unfinished sentence — a freshly restarted recognizer always capitalizes
/// its first word, which would make sentence boundaries look off.
pub fn smart_join(prev: &str, new: &str) -> String {
    if prev.is_empty() || new.is_empty() {
        return tidy(&format!("{prev}{new}"));
    }
    let stripped = prev.trim_end();
    // a continuation segment never legitimately STARTS with punctuation —
    // it duplicates the boundary punctuation already handled at commit
    let mut new = new.to_string();
    while let Some(rest) = new.strip_prefix(['.', '!', '?', ',', ';']) {
        new = rest.trim_start().to_string();
    }
    let mut new = drop_overlap(stripped, new);
    if stripped
        .chars()
        .last()
        .is_some_and(|c| !".!?…:".contains(c))
    {
        new = decapitalize(&new);
    }
    // a join never glues two words together
    let glue = prev.ends_with(char::is_whitespace)
        || new.is_empty()
        || new.starts_with(char::is_whitespace);
    tidy(&format!("{prev}{}{new}", if glue { "" } else { " " }))
}

/// Drop leading words of `new` that duplicate the tail of `prev` — leftovers
/// of the re-transcribed overlap that the lenient timestamp skip let pass.
/// Case- and edge-punctuation-insensitive, longest match (≤ 4 words) first.
// ponytail: this also eats a genuine repeat across a segment boundary ("we
// met at the station " + "the station was busy" → one "the station"). The
// ceiling is textual: only timestamps could tell a re-heard word from a
// repeated one, and they are too coarse (80 ms, lenient 0.4 s skip) to do it.
fn drop_overlap(prev: &str, new: String) -> String {
    let norm = |w: &str| {
        w.trim_matches(['.', ',', '!', '?', ';', ':'])
            .to_lowercase()
    };
    let pw: Vec<&str> = prev.split_whitespace().collect();
    let nw: Vec<&str> = new.split_whitespace().collect();
    for k in (1..=4.min(pw.len()).min(nw.len())).rev() {
        let same = nw[..k]
            .iter()
            .zip(&pw[pw.len() - k..])
            .all(|(a, b)| norm(a) == norm(b));
        if same {
            return nw[k..].join(" ");
        }
    }
    new
}

/// "That works" → "that works"; "USA", "iPhone", "A1" stay. German nouns and
/// names are lowercased too ("Anna" → "anna"): nothing but a language model
/// can tell them apart, and the LLM cleanup restores them.
fn decapitalize(new: &str) -> String {
    let (first, rest) = new
        .split_once(' ')
        .map_or((new, None), |(f, r)| (f, Some(r)));
    let mut chars = first.chars();
    let Some(c0) = chars.next() else {
        return new.to_string();
    };
    let tail = chars.as_str();
    let cased = |c: &char| c.is_lowercase() || c.is_uppercase();
    let tail_lower =
        tail.chars().any(|c| cased(&c)) && tail.chars().filter(cased).all(char::is_lowercase);
    if !(c0.is_uppercase() && tail_lower) {
        return new.to_string();
    }
    let word: String = c0.to_lowercase().chain(tail.chars()).collect();
    match rest {
        Some(r) if !r.is_empty() => format!("{word} {r}"),
        _ => word,
    }
}

#[cfg(test)]
mod tests;
