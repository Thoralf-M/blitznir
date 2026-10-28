//! `[vocabulary]`: spoken forms replaced by their written forms in every
//! transcript, and the spellings the clean-up and instruction prompts are
//! told to keep (tasks.rs).

use crate::config::Vocabulary;
use regex::{Captures, Regex, RegexBuilder};
use std::borrow::Cow;

/// `[vocabulary] replace`, compiled once into one regex: one group per
/// spoken form, the longest first.
#[derive(Debug, Clone, Default)]
pub struct Vocab {
    /// None: nothing to replace.
    re: Option<Regex>,
    /// The written form of group `i + 1`.
    written: Vec<String>,
}

/// A spoken form as a pattern: its words with any whitespace between them,
/// on a word boundary at each end that is a word character.
fn pattern(spoken: &str) -> String {
    let spoken = spoken.trim();
    let words: Vec<String> = spoken.split_whitespace().map(regex::escape).collect();
    let edge = |c: Option<char>| match c {
        Some(c) if c.is_alphanumeric() || c == '_' => r"\b",
        _ => "",
    };
    let (first, last) = (spoken.chars().next(), spoken.chars().last());
    format!("({}{}{})", edge(first), words.join(r"\s+"), edge(last))
}

impl Vocab {
    /// Refuses a spoken form without a word in it.
    pub fn new(v: &Vocabulary) -> Result<Vocab, String> {
        if v.replace.keys().any(|s| s.trim().is_empty()) {
            return Err("vocabulary.replace: a spoken form is empty".into());
        }
        if v.replace.is_empty() {
            return Ok(Vocab::default());
        }
        let mut pairs: Vec<(&String, &String)> = v.replace.iter().collect();
        // alternatives match leftmost-first: "o m l x" must win over "o m"
        pairs.sort_by_key(|(s, _)| std::cmp::Reverse(s.trim().chars().count()));
        let alts: Vec<String> = pairs.iter().map(|(s, _)| pattern(s)).collect();
        let re = RegexBuilder::new(&alts.join("|"))
            .case_insensitive(true)
            .build()
            .map_err(|e| format!("vocabulary.replace: {e}"))?;
        let written = pairs.into_iter().map(|(_, w)| w.clone()).collect();
        Ok(Vocab {
            re: Some(re),
            written,
        })
    }

    /// `text` with every spoken form replaced; what follows it stays as it is.
    pub fn apply<'a>(&self, text: &'a str) -> Cow<'a, str> {
        let Some(re) = &self.re else {
            return Cow::Borrowed(text);
        };
        re.replace_all(text, |c: &Captures<'_>| {
            let group = (1..c.len()).find(|&i| c.get(i).is_some());
            group.map_or_else(|| c[0].to_string(), |i| self.written[i - 1].clone())
        })
    }
}

/// What the prompts are told to keep as written: `terms`, then the written
/// forms, each once.
pub fn spellings(v: &Vocabulary) -> Vec<&str> {
    let mut out: Vec<&str> = vec![];
    for s in v.terms.iter().chain(v.replace.values()).map(|s| s.trim()) {
        if !s.is_empty() && !out.contains(&s) {
            out.push(s);
        }
    }
    out
}

/// A vocabulary of `(spoken, written)` pairs, for tests.
#[cfg(test)]
pub(crate) fn of(pairs: &[(&str, &str)]) -> Vocab {
    let replace = pairs
        .iter()
        .map(|(s, w)| ((*s).to_string(), (*w).to_string()))
        .collect();
    let terms = vec![];
    Vocab::new(&Vocabulary { replace, terms }).expect("a test vocabulary")
}

#[cfg(test)]
mod tests {
    use super::{of as vocab, *};

    #[test]
    fn whole_words_are_replaced_whatever_their_case() {
        let v = vocab(&[("o m l x", "oMLX"), ("blitz near", "Blitznir")]);
        assert_eq!(
            v.apply("I run O M L X with blitz near"),
            "I run oMLX with Blitznir"
        );
        assert_eq!(
            v.apply("Blitz Near and BLITZ  NEAR"),
            "Blitznir and Blitznir"
        );
    }

    #[test]
    fn nothing_inside_a_word_is_replaced() {
        let v = vocab(&[("near", "NEAR"), ("Müller", "MÜLLER-TEST")]);
        assert_eq!(v.apply("nearby is near"), "nearby is NEAR");
        assert_eq!(
            v.apply("Müllerstraße, Frau müller!"),
            "Müllerstraße, Frau MÜLLER-TEST!"
        );
        assert_eq!(v.apply("unnear"), "unnear");
    }

    #[test]
    fn the_longest_spoken_form_wins_and_punctuation_stays() {
        let v = vocab(&[("o m", "OM"), ("o m l x", "oMLX")]);
        assert_eq!(v.apply("o m l x, then o m."), "oMLX, then OM.");
        assert_eq!(v.apply("o m l x? Yes: o m l x!"), "oMLX? Yes: oMLX!");
    }

    #[test]
    fn an_empty_vocabulary_changes_nothing() {
        let v = Vocab::new(&Vocabulary::default()).unwrap();
        let text = "o m l x, unchanged.";
        assert!(matches!(v.apply(text), Cow::Borrowed(t) if t == text));
        assert!(spellings(&Vocabulary::default()).is_empty());
    }

    #[test]
    fn a_spoken_form_needs_a_word() {
        let mut v = Vocabulary::default();
        v.replace.insert("  ".into(), "x".into());
        assert!(Vocab::new(&v).unwrap_err().contains("spoken form is empty"));
        // odd characters are taken literally
        let v = vocab(&[("c++", "C++"), ("(x)", "X")]);
        assert_eq!(v.apply("c++ and (x) but abc++"), "C++ and X but abc++");
    }

    #[test]
    fn spellings_are_the_terms_then_the_written_forms_once() {
        let mut v = Vocabulary {
            terms: vec!["PostgreSQL".into(), " ".into(), "oMLX".into()],
            ..Vocabulary::default()
        };
        v.replace.insert("o m l x".into(), "oMLX".into());
        v.replace.insert("blitz near".into(), "Blitznir".into());
        assert_eq!(spellings(&v), ["PostgreSQL", "oMLX", "Blitznir"]);
    }
}
