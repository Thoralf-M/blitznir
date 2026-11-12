//! `[translate] on_select` (macOS): text selected with the mouse gets a
//! Translate button above the pointer, in the box by the text
//! (panel/revert.rs), which any key, a click elsewhere or a few seconds
//! close. The selection is only read (Accessibility), never copied, until
//! the click: then its app comes back to the front and `[hotkeys]
//! translate`'s path copies and translates it.

use super::{Panel, spawn};
use crate::ui::revert::BoxKind;
use blitznir_core::lock;
use blitznir_core::platform::{Focus, KeyEvent};
use blitznir_core::tasks::Kind;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// The listener hears the mouse-up before the app under it handles it: its
/// selection is there this much later.
const SETTLE: Duration = Duration::from_millis(80);

/// What the mouse selections so far left behind.
#[derive(Default)]
pub struct Seen {
    /// The app and the text (a hash: no copy of it is kept) the button was
    /// last shown for: the same again, with no key since, is a drag that
    /// selected nothing (a window moved, a scrollbar, a second click on the
    /// button), so no button.
    last: Option<(Focus, u64)>,
    /// Keys pressed and clicks: one while a selection is looked at means the
    /// user went on, and the button would come too late.
    inputs: u64,
}

fn digest(text: &str) -> u64 {
    let mut h = DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

impl Panel {
    /// Every key and click the listener hears (revert.rs's `saw`).
    pub(super) fn heard(self: &Arc<Self>, ev: KeyEvent) {
        let mut seen = lock(&self.pick);
        match ev {
            KeyEvent::Selected => {
                drop(seen);
                self.selected_by_mouse();
            }
            KeyEvent::Modifier { down: false, .. } | KeyEvent::Reset => {}
            // a key changes or replaces the selection: the same text, selected
            // again after it, is selected anew
            KeyEvent::Other | KeyEvent::FKey(_) => {
                (seen.inputs, seen.last) = (seen.inputs + 1, None)
            }
            _ => seen.inputs += 1,
        }
    }

    /// The button's box was pressed beside its button, which made us the
    /// active app, and closed untranslated by a key or a quick action (a
    /// click elsewhere took the focus itself): the app gets the keyboard
    /// back; `now`: before this returns.
    pub(super) fn unpick(&self, to: Focus, now: bool) {
        let platform = self.platform.clone();
        let back = move || {
            if platform.focused_app().0.is_none() && !platform.refocus(&to) {
                log::warn!("translate button: the app did not come back");
            }
        };
        if now {
            back();
        } else if let Err(e) = spawn("pick-back", back) {
            log::warn!("translate button: {e}");
        }
    }

    /// A drag or a double or triple click ended: the button, if it selected text.
    fn selected_by_mouse(self: &Arc<Self>) {
        if !cfg!(target_os = "macos") || !self.cfg().translate.on_select {
            return;
        }
        let me = self.clone();
        let inputs = lock(&self.pick).inputs;
        let look = move || {
            thread::sleep(SETTLE);
            let to = me.platform.focused_app(); // never ours
            let Some(text) = to.0.and_then(|_| me.platform.selected_text(&to)) else {
                return;
            };
            let now = Some((to, digest(&text)));
            let (fresh, late) = {
                let seen = lock(&me.pick);
                (seen.last != now, seen.inputs != inputs)
            };
            // an open box: the selection was made in it
            let taken = lock(&me.revert).is_some() || lock(&me.view).job.busy();
            // what the box holds is the text: the click translates that
            if fresh && !late && !taken && me.offer(to, &text, "", BoxKind::Pick) {
                lock(&me.pick).last = now;
            }
        };
        if let Err(e) = spawn("pick", look) {
            log::warn!("translate button: {e}");
        }
    }

    /// The button: the text read when it showed is translated, no copy. The
    /// click made us the active app, so its app comes back to the front; an
    /// app that lost the selection meanwhile (an Electron editor, seen) gets the
    /// translation in the box by the text instead of a paste over nothing.
    pub fn translate_picked(self: &Arc<Self>) {
        let Some((text, to)) = self.take_offer() else {
            return;
        };
        let me = self.clone();
        let run = move || {
            let opened = lock(&me.view).ui.opened;
            let back = me.platform.refocus(&to);
            let kept = back && me.platform.selected_text(&to).as_deref() == Some(text.as_str());
            let editable = if kept {
                me.platform.focused_editable(&to)
            } else {
                log::info!("translate button: the selection is gone, shown not pasted");
                Some(false)
            };
            me.quick_on((Kind::Translate, None), to, text, editable, opened);
        };
        if let Err(e) = spawn("pick-translate", run) {
            log::error!("translate button: {e}");
        }
    }
}
