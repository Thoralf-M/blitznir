//! Deleting: the open view's row or chat from its Delete, all of a list
//! from a section's trash or the tray. The sidebar, the view and the
//! tray's Recent follow.

use super::{BUSY, Panel, view};
use blitznir::session::label;
use blitznir_core::{
    lock,
    store::{Chat, HistoryItem, HistoryKind},
};

/// A sidebar list: history rows of one kind, or the saved chats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum List {
    Rows(HistoryKind),
    Chats,
}

/// "dictation", "replacement" or "chat", with an s unless `n` is 1.
pub fn noun(list: List, n: usize) -> String {
    let one = match list {
        List::Rows(HistoryKind::Dictation) => "dictation",
        List::Rows(HistoryKind::Replacement) => "replacement",
        List::Chats => "chat",
    };
    format!("{one}{}", if n == 1 { "" } else { "s" })
}

/// The list's name in state.json's `closed`.
pub fn list_name(list: List) -> &'static str {
    match list {
        List::Rows(HistoryKind::Dictation) => "dictations",
        List::Rows(HistoryKind::Replacement) => "replacements",
        List::Chats => "chats",
    }
}

impl Panel {
    /// A sidebar list folded away or back, kept in state.json.
    pub fn fold_list(&self, list: List) {
        let name = list_name(list).to_string();
        let flip = |closed: &mut Vec<String>| match closed.iter().position(|c| *c == name) {
            Some(i) => drop(closed.remove(i)),
            None => closed.push(name.clone()),
        };
        self.with_ui(|ui| flip(&mut ui.closed));
        if let Err(e) = self.store.update_state(|s| flip(&mut s.closed)) {
            self.fail(e.to_string());
        }
    }

    /// The sidebar's History unfolded or folded, kept in state.json.
    pub fn open_history(&self, open: bool) {
        self.with_ui(|ui| ui.history_open = open);
        if let Err(e) = self.store.update_state(|s| s.history_open = open) {
            self.fail(e.to_string());
        }
    }

    /// The open view's Delete asks first, in its head; Esc or a press
    /// elsewhere takes the question back.
    pub fn ask_delete(&self, on: bool) {
        self.with_ui(|ui| ui.asking = on);
    }

    /// The open view's Delete, answered Yes: the history row being read, or
    /// the saved chat; the view goes back to the selection.
    pub fn delete_viewed(&self) {
        let (item, chat) = {
            let mut v = lock(&self.view);
            v.ui.asking = false;
            if v.ui.busy.is_some() {
                drop(v);
                return self.say(BUSY);
            }
            let item = v.ui.about.clone().filter(|_| view::viewing(&v.ui));
            let saved = !v.chat.messages.is_empty();
            (
                item,
                saved.then(|| (v.chat.id.clone(), v.chat.name.clone())),
            )
        };
        let gone = match (item, chat) {
            (Some(item), _) => self.delete_entry(&item),
            (None, Some((id, name))) => self.delete_chat(&id, &name),
            (None, None) => false,
        };
        if gone {
            let mut v = lock(&self.view);
            v.chat = Chat::new();
            v.ui.chatting = false;
            self.sync_chat(&mut v);
            drop(v);
            self.notify();
        }
    }

    /// False: it could not be deleted (said why).
    fn delete_entry(&self, item: &HistoryItem) -> bool {
        match self.store.remove_history(item) {
            Ok(_) => self.history_changed(format!("Deleted “{}”", label(&item.text))),
            Err(e) => {
                self.fail(format!("Delete failed: {e}"));
                return false;
            }
        }
        true
    }

    /// A section's Delete all, or the tray's. False: not now (said why).
    pub fn clear(&self, list: List) -> bool {
        let kind = match list {
            List::Rows(kind) => kind,
            List::Chats => return self.clear_chats(),
        };
        match self.store.clear_history(kind) {
            Ok(n) => self.history_changed(format!("Deleted {n} {}", noun(list, n))),
            Err(e) => self.fail(format!("Delete failed: {e}")),
        }
        true
    }

    /// Every saved chat, with its images; one open goes from the view. Not
    /// while a job runs: its answer would save its chat again.
    fn clear_chats(&self) -> bool {
        if lock(&self.view).ui.busy.is_some() {
            self.say(BUSY);
            return false;
        }
        let n = match self.store.clear_chats() {
            Ok(n) => n,
            Err(e) => {
                self.fail(format!("Delete failed: {e}"));
                return false;
            }
        };
        let chats = self.rows();
        let mut v = lock(&self.view);
        if !v.chat.messages.is_empty() {
            v.chat = Chat::new();
            v.ui.chatting = false;
            self.sync_chat(&mut v);
        }
        v.ui.chats = chats;
        v.ui.status
            .set(format!("Deleted {n} {}", noun(List::Chats, n)));
        drop(v);
        self.notify();
        self.tray.refresh();
        true
    }

    /// history.json changed: the rows anew; a row being read that went is
    /// left (a chat about it keeps its own copy); the tray's Recent follows.
    fn history_changed(&self, said: String) {
        let recent = self.store.history();
        let mut v = lock(&self.view);
        let gone = v.ui.about.as_ref().is_some_and(|h| !recent.contains(h));
        if gone && v.ui.busy.is_none() && view::viewing(&v.ui) {
            v.chat = Chat::new();
            v.ui.chatting = v.before;
            self.sync_chat(&mut v);
        }
        v.ui.recent = recent;
        v.ui.status.set(said);
        drop(v);
        self.notify();
        self.tray.refresh();
    }
}
