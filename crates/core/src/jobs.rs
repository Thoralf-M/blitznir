//! The panel's jobs (a rewrite, a translation, a chat answer, a write-out),
//! headless: one at a time, a cancelled one never delivers, and where its
//! result goes is fixed when it starts. The last paste-back can be undone.

use crate::{
    Result,
    llm::CancellationToken,
    lock,
    platform::{Focus, Platform},
    store::{Made, Store},
    tasks::{self, Kind},
};
use std::sync::{Arc, Mutex};

/// What a job without an answer keeps of a rewrite, translation or chat
/// turn: the selection and what made it, its prompt with the typed
/// instruction (a translation's once it is known), so a long one is never
/// lost to an Esc, a new open or a failed request.
#[derive(Debug, Clone)]
pub struct Unfinished {
    pub original: String,
    pub made: Arc<Mutex<Made>>,
}

impl Unfinished {
    /// Into the history, as a cancelled row, or failed with that error.
    pub fn keep(&self, store: &Store, failed: Option<&str>) -> Result<()> {
        let made = lock(&self.made).clone();
        store.add_unanswered(&self.original, &made, failed)
    }
}

/// The panel's one job slot.
#[derive(Default)]
pub struct Slot {
    running: Option<(u64, CancellationToken, Option<Unfinished>)>,
    last: u64,
}

impl Slot {
    pub fn busy(&self) -> bool {
        self.running.is_some()
    }

    /// Take the slot for a new job: its id and its cancel; `kept`, what it
    /// leaves in the history should it get no answer. None while one runs.
    pub fn reserve(&mut self, kept: Option<Unfinished>) -> Option<(u64, CancellationToken)> {
        if self.busy() {
            return None;
        }
        self.last += 1;
        let cancel = CancellationToken::default();
        self.running = Some((self.last, cancel.clone(), kept));
        Some((self.last, cancel))
    }

    /// Job `id` came back. Some only while it still holds the slot (an esc
    /// that lands after the answer is in must still stop the paste): what
    /// it kept, should it have failed.
    pub fn end(&mut self, id: u64) -> Option<Option<Unfinished>> {
        if self.running.as_ref().is_none_or(|(j, ..)| *j != id) {
            return None;
        }
        self.running.take().map(|(_, _, kept)| kept)
    }

    /// Esc, or a new open: free the slot and drop the job's connection (the
    /// server stops generating). None: no job ran; else what it kept.
    pub fn cancel(&mut self) -> Option<Option<Unfinished>> {
        let (_, token, kept) = self.running.take()?;
        token.cancel();
        Some(kept)
    }
}

/// Where a finished rewrite, translation or write-out goes. Decided when the
/// job starts, from what the panel knew then.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// Into this app, the one the panel came from.
    Paste(Focus),
    /// Into the panel: the app says the selection takes no text.
    Show,
}

impl Route {
    pub fn new(kind: Kind, editable: Option<bool>, from: Focus) -> Route {
        if kind == Kind::Translate && !tasks::translate_pastes(editable) {
            Route::Show
        } else {
            Route::Paste(from)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Delivered {
    /// The result is on the clipboard.
    pub copied: bool,
    /// And it was pasted into the route's app.
    pub pasted: bool,
}

/// Clipboard first (a paste that goes wrong can still be recovered), the
/// history second, then the paste. ⌘V pastes whatever the clipboard holds,
/// so without our text on it there is no paste. `hide` runs right before
/// the paste: the panel must never see our ⌘V. `original`, the
/// selection the result replaces, goes into the history with it and `made`,
/// what made it.
pub fn deliver(
    platform: &dyn Platform,
    store: &Store,
    out: &str,
    original: &str,
    made: &Made,
    route: Route,
    hide: impl FnOnce(),
) -> Delivered {
    let copied = (platform.set_clipboard(out))
        .inspect_err(|e| log::error!("clipboard: {e}"))
        .is_ok();
    if let Err(e) = store.add_replacement(out, original, made) {
        log::error!("history: {e}");
    }
    let pasted = match route {
        Route::Paste(to) if copied => {
            hide();
            platform.paste_into(&to).unwrap_or_else(|e| {
                log::error!("paste: {e}");
                false
            })
        }
        _ => false,
    };
    Delivered { copied, pasted }
}

/// A paste-back: `result` replaced `original`, the selection, in `target`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastReplace {
    pub original: String,
    pub result: String,
    pub target: Focus,
}

/// The last paste-back, for Undo last replace: in memory only, until the
/// next paste-back takes its place or Undo takes it.
#[derive(Default)]
pub struct Undo(Mutex<Option<LastReplace>>);

impl Undo {
    /// `d` delivered `result` along `route` in place of `original`: a paste
    /// is what Undo takes back now. True if so; anything else keeps the last.
    pub fn keep(&self, route: Route, d: Delivered, original: &str, result: &str) -> bool {
        let Route::Paste(target) = route else {
            return false;
        };
        if d.pasted {
            *lock(&self.0) = Some(LastReplace {
                original: original.into(),
                result: result.into(),
                target,
            });
        }
        d.pasted
    }

    pub fn ready(&self) -> bool {
        lock(&self.0).is_some()
    }

    pub fn take(&self) -> Option<LastReplace> {
        lock(&self.0).take()
    }
}

/// Take `last` back: the original on the clipboard first (where the app's
/// own undo does nothing, it can be pasted by hand), then the app's undo.
/// False: the app never came back to the front.
pub fn undo(platform: &dyn Platform, last: &LastReplace) -> Result<bool> {
    platform.set_clipboard(&last.original)?;
    platform.undo(&last.target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Paths, platform::fake::FakePlatform};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    fn store() -> (Store, Paths) {
        let dir = std::env::temp_dir().join(format!("blitznir-jobs-{}", uuid::Uuid::new_v4()));
        let paths = Paths::new(dir);
        (Store::new(paths.clone(), 10).unwrap(), paths)
    }

    #[test]
    fn a_job_cancelled_before_delivery_never_pastes() {
        let (store, paths) = store();
        let p = FakePlatform::new();
        let mut slot = Slot::default();
        let (id, token) = slot.reserve(None).unwrap();
        assert!(slot.reserve(None).is_none(), "one job at a time");
        // esc while the answer is already on its way back
        assert!(slot.cancel().is_some());
        assert!(token.is_cancelled());
        let (newer, _) = slot.reserve(None).unwrap();
        // the cancelled job's thread comes back with its answer
        if slot.end(id).is_some() {
            deliver(
                &p,
                &store,
                "late",
                "orig",
                &Made::new("rewrite", "Rewrite", None),
                Route::Paste(Focus(Some(7))),
                || {},
            );
        }
        assert_eq!(p.screen(), "");
        assert_eq!(p.clipboard(), "");
        assert!(store.history().is_empty());
        assert!(slot.busy(), "the newer job keeps the slot");
        assert!(slot.end(newer).is_some());
        assert!(!slot.busy() && slot.cancel().is_none());
        std::fs::remove_dir_all(&paths.dir).unwrap();
    }

    #[test]
    fn a_cancelled_rewrite_keeps_its_prompt_in_the_history() {
        let (store, paths) = store();
        let mut slot = Slot::default();
        let made = Made::new("rewrite", "Rewrite", Some("Shorten:\n{text}".into()));
        let made = Arc::new(Mutex::new(made));
        slot.reserve(Some(unfinished("long selection", &made)))
            .unwrap();
        lock(&made).prompt = Some("Shorten more:\n{text}".into()); // known late
        let kept = slot.cancel().unwrap().unwrap();
        kept.keep(&store, None).unwrap();
        kept.keep(&store, None).unwrap(); // a second cancel of the same: its own row
        let rows = store.history();
        assert_eq!(rows.len(), 2);
        let row = &rows[0];
        assert!(row.cancelled && row.text.is_empty());
        assert_eq!(row.unanswered().as_deref(), Some("Cancelled"));
        assert_eq!(row.original.as_deref(), Some("long selection"));
        assert_eq!(row.prompt.as_deref(), Some("Shorten more:\n{text}"));
        // a job that failed hands what it kept to its end, not to a cancel
        let (id, _) = slot.reserve(Some(unfinished("y", &made))).unwrap();
        let kept = slot.end(id).unwrap().unwrap();
        assert!(slot.cancel().is_none());
        kept.keep(&store, Some("server down")).unwrap();
        let row = &store.history()[0];
        assert!(!row.cancelled && row.original.as_deref() == Some("y"));
        assert_eq!(row.unanswered().as_deref(), Some("Failed: server down"));
        std::fs::remove_dir_all(&paths.dir).unwrap();
    }

    fn unfinished(original: &str, made: &Arc<Mutex<Made>>) -> Unfinished {
        Unfinished {
            original: original.into(),
            made: made.clone(),
        }
    }

    #[test]
    fn the_paste_goes_to_the_app_the_job_started_for_after_the_hide() {
        let (store, paths) = store();
        let p = FakePlatform::new();
        let started_in = Focus(Some(7));
        let route = Route::new(Kind::Rewrite, Some(false), started_in);
        assert_ne!(
            p.focused_app(),
            started_in,
            "another app is in front by now"
        );
        let hidden = Arc::new(AtomicBool::new(false));
        let seen = hidden.clone();
        p.on_type(move |_| {
            assert!(seen.load(Ordering::SeqCst), "hidden before the ⌘V");
            Ok(())
        });
        let h = hidden.clone();
        let made = Made::new("rewrite", "Summarize", Some("Shorten:\n{text}".into()));
        let d = deliver(&p, &store, "Fixed.", "fixd", &made, route, move || {
            h.store(true, Ordering::SeqCst);
        });
        assert_eq!(
            d,
            Delivered {
                copied: true,
                pasted: true
            }
        );
        assert_eq!(*p.pasted_into.lock().unwrap(), [started_in]);
        assert_eq!(p.screen(), "Fixed.");
        assert_eq!(store.history()[0].text, "Fixed.");
        let kept = &store.history()[0];
        assert_eq!(kept.original.as_deref(), Some("fixd"));
        assert_eq!(kept.action.as_deref(), Some("Summarize"));
        assert_eq!(kept.prompt.as_deref(), Some("Shorten:\n{text}"));
        std::fs::remove_dir_all(&paths.dir).unwrap();
    }

    #[test]
    fn a_translation_is_shown_only_where_the_app_says_it_takes_no_text() {
        let f = Focus(Some(3));
        assert_eq!(Route::new(Kind::Translate, Some(false), f), Route::Show);
        assert_eq!(Route::new(Kind::Translate, Some(true), f), Route::Paste(f));
        assert_eq!(Route::new(Kind::Translate, None, f), Route::Paste(f));
        assert_eq!(Route::new(Kind::Rewrite, Some(false), f), Route::Paste(f));
        // shown: on the clipboard and in the history, nothing typed
        let (store, paths) = store();
        let p = FakePlatform::new();
        let d = deliver(
            &p,
            &store,
            "Hallo",
            "Hello",
            &Made::new("translate", "Translate", Some("Into German".into())),
            Route::Show,
            || panic!("nothing to hide for"),
        );
        assert_eq!(
            d,
            Delivered {
                copied: true,
                pasted: false
            }
        );
        assert_eq!(p.clipboard(), "Hallo");
        assert_eq!(p.screen(), "");
        assert!(p.pasted_into.lock().unwrap().is_empty());
        assert_eq!(store.history()[0].mode, "translate");
        assert_eq!(store.history()[0].prompt.as_deref(), Some("Into German"));
        std::fs::remove_dir_all(&paths.dir).unwrap();
    }

    #[test]
    fn undo_takes_the_last_paste_back_back_once() {
        let (store, paths) = store();
        let p = FakePlatform::new();
        let undo = Undo::default();
        let pasted = Delivered {
            copied: true,
            pasted: true,
        };
        let (first, second) = (Focus(Some(7)), Focus(Some(8)));
        assert!(!undo.ready() && undo.take().is_none());
        assert!(undo.keep(Route::Paste(first), pasted, "orig 1", "one"));
        // a paste-back that failed, or a shown translation, keeps the last one
        let failed = Delivered {
            pasted: false,
            ..pasted
        };
        assert!(!undo.keep(Route::Paste(second), failed, "x", "y"));
        assert!(!undo.keep(Route::Show, pasted, "x", "y"));
        let d = deliver(
            &p,
            &store,
            "Fixed.",
            "fixd",
            &Made::new("write-out", "Write out", None),
            Route::Paste(second),
            || {},
        );
        assert!(undo.keep(Route::Paste(second), d, "fixd", "Fixed."));
        let out = &store.history()[0];
        assert_eq!(
            (out.action.as_deref(), &out.prompt),
            (Some("Write out"), &None)
        );
        let last = undo.take().unwrap();
        assert_eq!(
            last,
            LastReplace {
                original: "fixd".into(),
                result: "Fixed.".into(),
                target: second,
            }
        );
        assert!(!undo.ready(), "once");
        assert_eq!(p.screen(), "Fixed.");
        assert!(super::undo(&p, &last).unwrap());
        assert_eq!(p.screen(), "", "the app's undo took the paste back");
        assert_eq!(p.clipboard(), "fixd");
        assert_eq!(*p.undone_in.lock().unwrap(), [second]);
        std::fs::remove_dir_all(&paths.dir).unwrap();
    }
}
