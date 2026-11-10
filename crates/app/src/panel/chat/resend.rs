//! Send again a turn that got no answer, or the last turn again for a new
//! answer (Retry): its text and images go as a new turn at the chat's end,
//! and the old one leaves; what was typed in the composer meanwhile stays
//! there.

use super::super::Panel;
use super::super::view::{Shot, View};
use blitznir_core::{lock, store, tasks};
use std::sync::{Arc, MutexGuard};

impl Panel {
    /// The open chat's message `i`, unanswered, sent again.
    pub fn resend(self: &Arc<Self>, i: usize) {
        let v = lock(&self.view);
        let marked = v
            .chat
            .messages
            .get(i)
            .is_some_and(|m| m.unanswered.is_some());
        if marked && v.ui.busy.is_none() {
            self.send_again(v, i);
        }
    }

    /// Retry on the chat's last answer, message `i`: it goes, and the turn
    /// it answered is asked again. Back where it was if that fails.
    pub fn retry(self: &Arc<Self>, i: usize) {
        let mut v = lock(&self.view);
        let m = &v.chat.messages;
        let roles = |i: usize| m.get(i).map(|m| (m.role.as_str(), m.content.as_str()));
        let asked = i.checked_sub(1).and_then(roles);
        let last = i + 1 == m.len() && roles(i).is_some_and(|(r, _)| r == "assistant");
        let again = asked.is_some_and(|(r, q)| r == "user" && tasks::retryable(q));
        if !last || !again || v.ui.busy.is_some() {
            return;
        }
        let answer = v.chat.messages.remove(i);
        if !self.send_again(v, i - 1) {
            let mut v = lock(&self.view);
            v.chat.messages.insert(i, answer);
            self.sync_chat(&mut v);
            drop(v);
            self.notify();
        }
    }

    /// The open chat's message `i` sent again (`v` holds the view); false:
    /// it could not be (said why), and it is back in its place.
    fn send_again(self: &Arc<Self>, mut v: MutexGuard<'_, View>, i: usize) -> bool {
        let turn = v.chat.messages[i].clone();
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
                true
            }
            Err(e) => {
                v.chat.messages.insert(i, turn);
                self.sync_chat(&mut v);
                drop(v);
                self.fail(e);
                false
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
        Some(self.shot(url, png, self.store.original(name)))
    }
}
