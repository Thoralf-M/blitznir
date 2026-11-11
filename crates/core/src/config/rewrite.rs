//! `[dictation] rewrite` and `rewrite_auto`: when a dictation's rewrite runs.

use super::Dictation;

impl Dictation {
    /// The prompt each finished dictation is rewritten with as it ends:
    /// none while blank or offered on click.
    pub fn rewrite_prompt(&self) -> Option<&str> {
        self.prompt().filter(|_| self.rewrite_auto)
    }

    /// The prompt the box by a finished dictation offers to run.
    pub fn rewrite_on_click(&self) -> Option<&str> {
        self.prompt().filter(|_| !self.rewrite_auto)
    }

    fn prompt(&self) -> Option<&str> {
        Some(self.rewrite.as_str()).filter(|p| !p.trim().is_empty())
    }
}
