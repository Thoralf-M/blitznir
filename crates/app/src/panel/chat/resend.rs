//! Send again a turn that got no answer: its text and images go as a new
//! turn at the chat's end, and the marked one leaves; what was typed in the
//! composer meanwhile stays there.

use super::super::Panel;
use super::super::view::Shot;
use blitznir_core::{lock, store};
use std::sync::Arc;

impl Panel {
    /// The open chat's message `i`, unanswered, sent again.
    pub fn resend(self: &Arc<Self>, i: usize) {
        let mut v = lock(&self.view);
        let turn = v.chat.messages.get(i).filter(|m| m.unanswered.is_some());
        let Some(turn) = turn.cloned().filter(|_| v.ui.busy.is_none()) else {
            return;
        };
        let shots = turn
            .images
            .iter()
            .filter_map(|n| self.image_shot(n))
            .collect();
        let typed = std::mem::take(&mut v.ui.composer.text);
        let draft = std::mem::replace(&mut v.ui.images, shots);
        v.chat.messages.remove(i);
        v.ui.composer.set(turn.content.clone());
        drop(v);
        let sent = self.try_send();
        let mut v = lock(&self.view);
        v.ui.composer.set(typed);
        v.ui.images = draft;
        match sent {
            Ok(()) => {
                drop(v);
                self.store.remove_images([&turn]); // sent as new copies
                self.notify();
            }
            Err(e) => {
                v.chat.messages.insert(i, turn);
                self.sync_chat(&mut v);
                drop(v);
                self.fail(e);
            }
        }
    }

    /// A stored image as the composer holds one: its data URL to send again,
    /// from memory if it was sent this run (its file may never have been
    /// written), else from its file.
    fn image_shot(&self, name: &str) -> Option<Shot> {
        let sent = lock(&self.pngs)
            .get(name)
            .filter(|s| !s.url.is_empty())
            .cloned();
        if sent.is_some() {
            return sent;
        }
        let url = self.store.image(name);
        let url = url.map_err(|e| log::warn!("image {name}: {e}")).ok()?;
        let png = store::data_url_bytes(&url)?;
        Some(self.shot(url, png))
    }
}
