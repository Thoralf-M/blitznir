//! What hands-free does on a command: Return, or the last sentence taken
//! back, and spotting one said at a dictation's end.

use super::Driver;
use std::time::{Duration, Instant};

/// A command said at a dictation's end, done once it is typed.
#[derive(Clone, Copy)]
pub(super) enum After {
    Scratch,
    Send,
}

/// A command's words reach the live text this long after its pause at most.
const ALONE: Duration = Duration::from_secs(3);

impl Driver {
    /// Whether ours ends in the stop phrase, or in a command said on its own
    /// (an utterance between pauses just ended): then what it is, and what
    /// the final pass takes off.
    pub(super) fn said_at_end(&mut self, now: Instant) -> bool {
        let Some((w, _)) = &self.on else {
            return false;
        };
        let live = self.dictator.live_text();
        if !w.stop.trim().is_empty() && blitznir_core::wake::said_last(&live, &w.stop) {
            return true;
        }
        if !self.alone.is_some_and(|t| now.duration_since(t) < ALONE) {
            return false;
        }
        let commands = [(&w.scratch, After::Scratch), (&w.send, After::Send)];
        let Some((tail, setting, then)) = commands.into_iter().find_map(|(setting, then)| {
            blitznir_core::wake::command_tail(&live, setting).map(|t| (t, setting, then))
        }) else {
            return false;
        };
        log::info!("wake: command at the dictation's end: {tail:?}");
        self.cut = Some((format!("{tail}, {setting}"), then));
        self.alone = None;
        true
    }

    /// The take-back phrase: the last sentence before the caret comes off.
    pub(super) fn scratch_back(&mut self, armed: bool) {
        let to = self.platform.focused_app();
        // a Backspace would take the selection, then more
        if self.platform.selected_text(&to).is_some() {
            return log::info!("wake: scratch phrase heard, but text is selected");
        }
        // the field's own text, typed or dictated; else, right after a
        // dictation of ours (Linux; a field that will not say), what it typed
        let field = self.platform.text_before_caret(&to);
        let ours = (self.last.as_ref())
            .filter(|(app, _)| armed && *app == to)
            .map(|(_, t)| t.clone());
        let Some(text) = field.or(ours) else {
            return log::info!("wake: scratch phrase heard, but no text before the caret");
        };
        let Some(kept) = blitznir_core::wake::drop_last_sentence(&text) else {
            return log::info!("wake: scratch phrase heard, but the sentence is too long");
        };
        self.last = Some((to, text));
        if self.take_back(kept) {
            (self.flash)("Scratched");
        }
    }

    /// The last dictation's text down to `kept`, backspaced in its app if
    /// that is still in front; whether it was.
    fn take_back(&mut self, kept: String) -> bool {
        let Some((to, text)) = self.last.as_mut() else {
            return false;
        };
        if to.0.is_some() && self.platform.focused_app() != *to {
            log::info!("wake: a command heard, but another app is in front");
            return false;
        }
        // as the eye counts: by code point an emoji took two Backspaces
        let n = blitznir_core::textproc::backspaces(&text[kept.len()..]);
        if n == 0 {
            log::info!("wake: nothing of ours left to take back");
            return false;
        }
        match self.platform.backspace_into(to, n) {
            Ok(true) => {
                log::info!("wake: took back {n} characters");
                *text = kept;
                true
            }
            Ok(false) => {
                log::warn!("wake: the app did not come back; nothing taken back");
                false
            }
            Err(e) => {
                log::error!("wake: take back: {e}");
                false
            }
        }
    }

    /// Return in `to`, the app the dictation went to, if it is still in
    /// front (Linux names no app: wherever the focus is).
    pub(super) fn send(&self, to: blitznir_core::platform::Focus) {
        if to.0.is_some() && self.platform.focused_app() != to {
            log::info!("wake: send phrase heard, but another app is in front: not sent");
            return;
        }
        match self.platform.send(&to) {
            Ok(true) => {
                log::info!("wake: sent");
                (self.flash)("Sent");
            }
            Ok(false) => log::warn!("wake: the app did not come back; not sent"),
            Err(e) => log::error!("wake: send: {e}"),
        }
    }
}
