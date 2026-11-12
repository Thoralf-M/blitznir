//! `[dictation] rewrite` and `rewrite_auto`: when a dictation's rewrite runs.

use super::Dictation;

impl Dictation {
    /// The prompt each finished dictation is rewritten with as it ends:
    /// none while blank or offered on click.
    pub fn rewrite_prompt(&self) -> Option<&str> {
        self.prompt().filter(|_| self.automatic())
    }

    /// The prompt the box by a finished dictation offers to run.
    pub fn rewrite_on_click(&self) -> Option<&str> {
        self.prompt().filter(|_| !self.automatic())
    }

    /// `rewrite_auto`; on Linux always: no box there could offer the
    /// rewrite, so it would never run.
    fn automatic(&self) -> bool {
        self.rewrite_auto || !cfg!(target_os = "macos")
    }

    fn prompt(&self) -> Option<&str> {
        Some(self.rewrite.as_str()).filter(|p| !p.trim().is_empty())
    }
}
