//! The quick hotkey, the panel's first action button without the panel, and
//! Undo last replace. Both answer in the tray.

use super::{Mode, Panel, spawn};
use blitznir_core::platform::CopyToken;
use blitznir_core::store::Chat;
use blitznir_core::tasks::Kind;
use blitznir_core::{jobs, lock};
use std::sync::Arc;

impl Panel {
    /// The tray's Undo item follows whether there is something to undo.
    pub fn can_undo(&self) -> bool {
        self.last.ready()
    }

    /// `[hotkeys] quick`: the button Return runs (else the first) on the selection, with
    /// the panel's delivery (clipboard, history, paste-back); `button`: that one, by its
    /// own `hotkey`; `[hotkeys] translate` (`Kind::Translate`): Translate so, which the
    /// panel shows where the app takes no text.
    pub fn quick(self: &Arc<Self>, kind: Kind, button: Option<usize>) {
        let me = self.clone();
        if let Err(e) = spawn("quick", move || me.quick_now(kind, button)) {
            log::error!("quick action: {e}");
        }
    }

    fn quick_now(self: &Arc<Self>, kind: Kind, button: Option<usize>) {
        let opened = lock(&self.view).ui.opened;
        // before anything of ours shows: the app in front gets the ⌘C
        let prev = self.platform.focused_app();
        let text = self.copy_quietly(self.platform.begin_copy());
        if text.trim().is_empty() {
            log::info!("quick action: nothing selected in {:?}", prev.0);
            self.tray.flash("No selection");
            return;
        }
        let editable = self.platform.focused_editable(&prev);
        let bounds = (self.platform.selection_bounds(&prev)).filter(|b| b.fits(&text));
        if lock(&self.view).ui.opened != opened {
            log::info!("quick action: the panel opened meanwhile");
            return;
        }
        self.cancel(); // its paste target is about to change
        self.hide(); // the view is the quick action's now
        {
            let mut v = lock(&self.view);
            v.ui.opened += 1;
            (v.prev, v.ui.editable, v.quick) = (prev, editable, true);
            v.bounds = bounds;
            (v.ui.chatting, v.ui.settings) = (false, false);
            v.ui.selection = Some(text.clone());
            v.ui.composer.set("");
            v.ui.images.clear();
            v.chat = Chat::new();
            self.sync_chat(&mut v);
        }
        self.notify();
        let (button, label) = {
            let cfg = lock(&self.cfg);
            let i = button.unwrap_or_else(|| cfg.quick_action());
            let label = cfg.actions.get(i).map(|b| b.label.clone());
            (i, label.filter(|_| kind == Kind::Rewrite))
        };
        log::info!(
            "quick action {:?}: {} chars selected in {:?}, no panel",
            label.unwrap_or_else(|| "Translate".into()),
            text.chars().count(),
            prev.0
        );
        if let Err(e) = self.try_run(kind, button) {
            log::warn!("quick action: {e}");
            self.tray.flash(match kind {
                Kind::Translate => "Translation failed: see blitznir.log",
                _ => "Rewrite failed: see blitznir.log",
            });
        }
    }

    /// The selection, without the panel on macOS. On Linux PRIMARY reaches
    /// only a focused window of ours: the panel shows until it is read, then
    /// hides (made and destroyed there). Not after copy keys: their copy
    /// needs no focus, and our window in front would take the keys.
    fn copy_quietly(self: &Arc<Self>, copy: CopyToken) -> String {
        let flash = cfg!(target_os = "linux") && !copy.sent();
        if flash {
            self.show();
        }
        let text = self.platform.finish_copy(copy).unwrap_or_default();
        if flash {
            self.hide();
        }
        text
    }

    /// Undo last replace (the tray, `[hotkeys] undo`): the original on the
    /// clipboard, the app's own undo in the app the paste went to.
    pub fn undo(self: &Arc<Self>) {
        let Some(last) = self.last.take() else {
            self.tray.flash("Nothing to undo");
            return;
        };
        self.tray.refresh();
        let me = self.clone();
        let spawned = spawn("undo", move || {
            log::info!(
                "undo: {} chars back in place of {} in {:?}",
                last.original.chars().count(),
                last.result.chars().count(),
                last.target.0
            );
            match jobs::undo(&*me.platform, &last) {
                Ok(true) => me.tray.flash("Undone, original on the clipboard"),
                Ok(false) => me.tray.flash("Original on the clipboard"),
                Err(e) => {
                    log::error!("undo: {e}");
                    me.tray.flash("Undo failed: see blitznir.log");
                }
            }
        });
        if let Err(e) = spawned {
            log::error!("undo: {e}");
        }
    }

    /// The tray's items that open the panel.
    pub fn open_from_tray(self: &Arc<Self>, settings: bool) {
        let mode = if settings {
            Mode::Settings
        } else {
            Mode::Rewrite
        };
        self.open(mode, None);
    }
}
