//! The history: dictations and replacements, newest first, capped per kind.

use super::{Store, read_json, write_json};
use crate::{Result, now_stamp};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryItem {
    pub text: String,
    pub mode: String,
    pub ts: String,
    /// A replacement's selection before the paste; None for a dictation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original: Option<String>,
    /// A replacement's button ("Rewrite", "Translate", "Write out"); None
    /// for a dictation and for rows stored before it was kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// The prompt that made a replacement, `{text}` left as the placeholder
    /// (the original is stored once, above); None for a write-out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// A rewrite, translation or chat turn cancelled before its answer: no
    /// text, kept for its prompt and selection.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cancelled: bool,
    /// As `cancelled`, for one whose request failed: the error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed: Option<String>,
}

/// What made a replacement: its mode, its button and the prompt it used.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Made {
    pub mode: String,
    pub action: Option<String>,
    pub prompt: Option<String>,
}

impl Made {
    pub fn new(mode: &str, action: &str, prompt: Option<String>) -> Made {
        Made {
            mode: mode.into(),
            action: Some(action.into()),
            prompt,
        }
    }
}

/// What a history row is, from its mode: something said, or a text the
/// panel put in place of the selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryKind {
    /// live, batch, recovered.
    Dictation,
    /// rewrite, translate, write-out.
    Replacement,
}

impl HistoryItem {
    pub fn kind(&self) -> HistoryKind {
        match self.mode.as_str() {
            "rewrite" | "translate" | "write-out" | "chat" => HistoryKind::Replacement,
            _ => HistoryKind::Dictation,
        }
    }

    /// What the row is, as its card names it.
    pub fn what(&self) -> &'static str {
        match self.mode.as_str() {
            "rewrite" => "Rewrite",
            "translate" => "Translation",
            "write-out" => "Write-out",
            "chat" => "Chat",
            _ => "Dictation",
        }
    }

    /// What a chat about this row is about: its text; for a row without an
    /// answer, what it was asked on (its text is empty).
    pub fn subject(&self) -> &str {
        match self.unanswered() {
            Some(_) => self.original.as_deref().unwrap_or_default(),
            None => &self.text,
        }
    }

    /// What stands where the answer would be, for a row that has none.
    pub fn unanswered(&self) -> Option<String> {
        match &self.failed {
            Some(e) => Some(format!("Failed: {e}")),
            None => self.cancelled.then(|| "Cancelled".into()),
        }
    }
}

/// A row of `text`, over `original` ("" for none), made as `made` says.
fn row(text: &str, original: &str, made: &Made) -> HistoryItem {
    let original = original.trim();
    HistoryItem {
        text: text.into(),
        mode: made.mode.clone(),
        ts: now_stamp(),
        original: (!original.is_empty()).then(|| original.into()),
        action: made.action.clone(),
        prompt: made.prompt.clone(),
        cancelled: false,
        failed: None,
    }
}

impl Store {
    /// Newest first; at most `history_max` of each kind.
    pub fn history(&self) -> Vec<HistoryItem> {
        let mut items: Vec<HistoryItem> = read_json(&self.paths.history());
        self.cap(&mut items);
        items
    }

    /// The cap is per kind: a run of rewrites never pushes a dictation out
    /// of the safety net.
    fn cap(&self, items: &mut Vec<HistoryItem>) {
        let (mut said, mut replaced) = (0, 0);
        items.retain(|i| {
            let n = match i.kind() {
                HistoryKind::Dictation => &mut said,
                HistoryKind::Replacement => &mut replaced,
            };
            *n += 1;
            *n <= self.history_max
        });
    }

    /// Keep a finished transcript. This is the safety net for when typing
    /// into the focused app fails: nothing dictated lives only there.
    pub fn add_history(&self, text: &str, mode: &str) -> Result<()> {
        let made = Made {
            mode: mode.into(),
            ..Made::default()
        };
        self.add_replacement(text, "", &made)
    }

    /// Keep `text` with the selection it replaced, `original` ("" for none),
    /// and what made it.
    pub fn add_replacement(&self, text: &str, original: &str, made: &Made) -> Result<()> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(());
        }
        self.insert(row(text, original, made))
    }

    /// A rewrite, translation or chat turn that got no answer: cancelled
    /// (`failed` None) or failed with that error. Its selection and prompt,
    /// never merged with another row.
    pub fn add_unanswered(&self, original: &str, made: &Made, failed: Option<&str>) -> Result<()> {
        self.insert(HistoryItem {
            cancelled: failed.is_none(),
            failed: failed.map(str::to_string),
            ..row("", original, made)
        })
    }

    /// `item` first in its kind; the same text again moves up rather than
    /// repeating (rows without an answer never merge).
    fn insert(&self, item: HistoryItem) -> Result<()> {
        let kind = item.kind();
        let answered = item.unanswered().is_none();
        let same = |i: &HistoryItem| {
            answered && i.unanswered().is_none() && i.kind() == kind && i.text == item.text
        };
        let _g = self.guard();
        let mut items = self.history();
        let newest = items.iter().find(|i| i.kind() == kind);
        if newest.is_some_and(|i| same(i) && i.original == item.original) {
            return Ok(()); // the same text again (a recovered recording)
        }
        items.retain(|i| !same(i)); // an older copy of its kind moves up
        items.insert(0, item);
        self.cap(&mut items);
        write_json(&self.paths.history(), &items)
    }

    /// Delete the row equal to `item`, not an index: a dictation that lands
    /// between the render and the click shifts every index. False: gone already.
    pub fn remove_history(&self, item: &HistoryItem) -> Result<bool> {
        let _g = self.guard();
        let mut items = self.history();
        let before = items.len();
        items.retain(|i| i != item);
        if items.len() == before {
            return Ok(false);
        }
        write_json(&self.paths.history(), &items)?;
        Ok(true)
    }

    /// Delete every row of `kind`; how many went.
    pub fn clear_history(&self, kind: HistoryKind) -> Result<usize> {
        let _g = self.guard();
        let mut items = self.history();
        let before = items.len();
        items.retain(|i| i.kind() != kind);
        let gone = before - items.len();
        if gone > 0 {
            write_json(&self.paths.history(), &items)?;
        }
        Ok(gone)
    }
}

#[cfg(test)]
mod tests;
