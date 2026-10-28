//! `[dictation] rewrite`: every finished dictation rewritten with that
//! prompt, as the quick action runs a button: no panel, the working effect,
//! the result pasted into the app it was dictated into. Batch typed nothing
//! (core's Opts::hold); a live one typed as it went, and those characters
//! are taken back just before the result is pasted. A dictation into the
//! panel's own composer stays as said unless `rewrite_in_panel` is on.

use super::job::Doing;
use super::view::Busy;
use super::{BUSY, Panel, spawn};
use blitznir_core::llm::CancellationToken;
use blitznir_core::lock;
use blitznir_core::platform::Focus;
use blitznir_core::store::Chat;
use blitznir_core::tasks::{self, Kind};
use blitznir_core::textproc;
use std::sync::Arc;

impl Panel {
    /// A dictation records (live or batch) or not: the composer's wave
    /// shows while it types into our own window.
    pub fn set_dictating(&self, on: bool) {
        let changed = lock(&self.view).ui.dictating != on;
        if changed {
            self.with_ui(|ui| ui.dictating = on);
        }
    }

    /// A dictation ended with `text` (live: typed as it went). Nothing to
    /// do without a rewrite prompt.
    pub fn rewrite_dictation(self: &Arc<Self>, text: String, live: bool) {
        let prompt = self.cfg().dictation.rewrite;
        if prompt.trim().is_empty() || text.trim().is_empty() {
            return;
        }
        let me = self.clone();
        if let Err(e) = spawn("dictation-rewrite", move || {
            me.rewrite_now(text, live, prompt)
        }) {
            log::error!("dictation rewrite: {e}");
        }
    }

    fn rewrite_now(self: &Arc<Self>, text: String, live: bool, prompt: String) {
        if lock(&self.view).job.busy() {
            log::info!("dictation rewrite: a job is running, the dictation stays as it is");
            self.tray.flash("Not rewritten: busy");
            if !live {
                self.fallback_type(&text, self.platform.focused_app());
            }
            return;
        }
        if self.window_focused() {
            if !live {
                // batch held it back for the rewrite: in as said first, so
                // a failed or cancelled rewrite leaves it, as a live one
                self.put_in_composer(None, &text);
            }
            if self.cfg().dictation.rewrite_in_panel {
                self.rewrite_in_composer(text, prompt);
            }
            return;
        }
        let prev = self.platform.focused_app();
        let editable = self.platform.focused_editable(&prev);
        let bounds = self.platform.selection_bounds(&prev);
        self.cancel();
        self.hide();
        {
            let mut v = lock(&self.view);
            v.ui.opened += 1;
            (v.prev, v.ui.editable, v.quick, v.bounds) = (prev, editable, true, bounds);
            (v.ui.chatting, v.ui.settings) = (false, false);
            v.ui.selection = Some(text.clone());
            v.ui.composer.set("");
            v.ui.images.clear();
            let typed = if live { textproc::backspaces(&text) } else { 0 };
            v.dictation = Some((prompt, typed));
            v.chat = Chat::new();
            self.sync_chat(&mut v);
        }
        self.notify();
        log::info!(
            "dictation rewrite: {} chars ({}) in {:?}",
            text.chars().count(),
            if live { "live, typed" } else { "batch, held" },
            prev.0
        );
        if let Err(e) = self.try_run(Kind::Rewrite, 0) {
            log::warn!("dictation rewrite: {e}");
            lock(&self.view).dictation = None;
            self.tray.flash("Rewrite failed: see blitznir.log");
            if !live {
                // batch held its text back: it still goes where it was dictated
                self.fallback_type(&text, prev);
            }
        }
    }

    /// `rewrite_in_panel`: the dictation went into our own composer (the
    /// panel had the focus), and its rewrite takes its place there. The panel stays, nothing is pasted.
    fn rewrite_in_composer(self: &Arc<Self>, text: String, prompt: String) {
        let cfg = self.cfg();
        let (ep, model) = match self.target(&cfg) {
            Ok(t) => t,
            Err(e) => return self.fail(e),
        };
        let Some((job, cancel)) = lock(&self.view).job.reserve(None) else {
            return self.fail(BUSY);
        };
        log::info!(
            "dictation rewrite: {} chars, in the composer",
            text.chars().count()
        );
        let dictated = text.clone();
        let work = move |me: &Panel, cancel: &CancellationToken| {
            let body = tasks::rewrite_body(&cfg, &model, &text, &prompt, None, &[]);
            me.block_on(me.client.stream(&ep, body, &mut |_: &str| {}, cancel))
        };
        let doing = Doing::new(Busy::Rewrite, "Rewriting the dictation…", "Rewriting…");
        let finish = move |me: &Panel, out: String| {
            me.put_in_composer(Some(&dictated), &out);
            if let Some(me) = me.me.upgrade() {
                me.show(); // the effect's overlay had the key focus
            }
        };
        self.zap_composer();
        if let Err(e) = self.start(job, cancel, doing, Box::new(work), finish) {
            self.fail(e);
        }
    }

    /// The effect over the composer, the panel left in its place: the text
    /// it rewrites is there.
    pub(super) fn zap_composer(&self) {
        let Some(effect) = self.effect() else {
            return;
        };
        let at = lock(&self.view).composer;
        let Some(rect) = at.and_then(|r| self.on_screen(r)) else {
            return;
        };
        let at = Some(blitznir_core::platform::Bounds { rect, text: true });
        self.zap.start(at, None, effect);
    }

    /// `out` in the composer where `dictated` was typed, else (None: a
    /// batch dictation typed nothing) after what is there.
    pub(super) fn put_in_composer(&self, dictated: Option<&str>, out: &str) {
        let mut v = lock(&self.view);
        let before = v.ui.composer.text.clone();
        let after = replace_dictated(&before, dictated, out);
        v.ui.composer.set(after.clone());
        v.composer_undo = Some((before, after));
        drop(v);
        self.notify();
    }

    /// ⌘Z (`redo`: ⇧⌘Z) in the composer while it holds exactly a rewrite's
    /// result (its dictation): the rewrite taken back or put again. False:
    /// anything else, the input's own undo.
    pub fn undo_composer(&self, redo: bool) -> bool {
        let mut v = lock(&self.view);
        let Some((before, after)) = v.composer_undo.clone() else {
            return false;
        };
        let (from, to) = if redo {
            (&before, &after)
        } else {
            (&after, &before)
        };
        if v.ui.composer.text != *from {
            return false;
        }
        v.ui.composer.set(to.clone());
        drop(v);
        self.notify();
        true
    }

    /// A live dictation's typed characters, taken back before its rewrite
    /// is pasted, when the app it was typed into is still in front (else
    /// they stay and the result lands beside them: never delete elsewhere).
    /// How many characters a live dictation typed that its rewrite takes
    /// back (batch: 0; None: no dictation rewrite); asked once, as it lands.
    pub(super) fn dictated_typed(&self) -> Option<usize> {
        lock(&self.view).dictation.take().map(|(_, typed)| typed)
    }

    /// Take `typed` characters back in `to` (our overlay has the focus until then).
    pub(super) fn erase_dictation(&self, to: Focus, typed: usize) {
        if typed == 0 {
            return;
        }
        match self.platform.backspace_into(&to, typed) {
            Ok(true) => {}
            Ok(false) => {
                log::warn!("dictation rewrite: its app never came back, the dictation stays")
            }
            Err(e) => log::error!("dictation rewrite: taking the dictation back: {e}"),
        }
    }

    /// The running job is a batch dictation's rewrite: what was said and
    /// where, for Esc to put back (it typed nothing; live typed it already).
    pub(super) fn held_dictation(&self) -> Option<(String, Focus)> {
        let v = lock(&self.view);
        match v.dictation {
            Some((_, 0)) => Some((v.ui.selection.clone()?, v.prev)),
            _ => None,
        }
    }

    /// Esc on (or the failure of) a batch dictation's rewrite: what was said
    /// is pasted where it was dictated. Pasted, not typed: the effect's
    /// overlay has the key focus, and the paste brings `to` back.
    pub(super) fn put_back(&self, text: String, to: Focus) {
        let platform = self.platform.clone();
        let paste = move || {
            let pasted = platform
                .set_clipboard(&text)
                .and_then(|()| platform.paste_into(&to));
            match pasted {
                Ok(true) => log::info!("dictation rewrite cancelled: the dictation pasted as said"),
                Ok(false) => log::warn!("dictation rewrite cancelled: its app never came back"),
                Err(e) => log::error!("dictation rewrite cancelled: pasting the dictation: {e}"),
            }
        };
        if let Err(e) = spawn("dictation-back", paste) {
            log::error!("dictation rewrite cancelled: {e}");
        }
    }

    /// A held batch dictation whose rewrite never started (no overlay took
    /// the focus): typed as it was, while its app is in front.
    pub(super) fn fallback_type(&self, text: &str, to: Focus) {
        if self.platform.focused_app() == to
            && let Err(e) = self.platform.type_text(text)
        {
            log::error!("dictation rewrite: typing the dictation: {e}");
        }
    }
}

/// `now` with its last `dictated` replaced by `out`; without it (or None:
/// nothing was typed), `out` after what is there.
fn replace_dictated(now: &str, dictated: Option<&str>, out: &str) -> String {
    let at = dictated.and_then(|d| Some((now.rfind(d)?, d.len())));
    match at {
        Some((i, n)) => format!("{}{out}{}", &now[..i], &now[i + n..]),
        None if now.trim().is_empty() => out.to_string(),
        None => format!("{} {out}", now.trim_end()),
    }
}

#[cfg(test)]
mod tests {
    use super::replace_dictated;

    #[test]
    fn the_rewrite_takes_the_dictations_place_in_the_composer() {
        let said = "um so the the release is ready";
        let out = "The release is ready.";
        assert_eq!(
            replace_dictated(said, Some(said), out),
            out,
            "live: typed as said"
        );
        let around = format!("Note: {said} (see below)");
        assert_eq!(
            replace_dictated(&around, Some(said), out),
            format!("Note: {out} (see below)")
        );
        assert_eq!(replace_dictated("", None, out), out, "batch: nothing typed");
        assert_eq!(replace_dictated("Hi. ", None, out), format!("Hi. {out}"));
    }

    #[test]
    fn a_batch_dictation_goes_after_the_text_that_holds_its_words() {
        // typed nothing: the "the" in "there" is not it
        assert_eq!(
            replace_dictated("check there", None, "the"),
            "check there the"
        );
    }
}
