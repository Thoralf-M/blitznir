//! The open chat's title: typed in its head, or asked of the picked model
//! from what was said. Either way it is the chat's name in the sidebar.

use super::{Panel, spawn};
use blitznir_core::llm::CancellationToken;
use blitznir_core::lock;
use blitznir_core::tasks::{self, Turn};
use std::sync::Arc;

impl Panel {
    /// The head's rename: `name`, cleaned up, becomes the open chat's name;
    /// false = nothing usable was typed, the old name stays.
    pub fn rename_chat(&self, name: &str) -> bool {
        self.rename(name, None)
    }

    /// Rename the open chat; with `only`, only while that chat is still
    /// open, checked under the lock the rename holds.
    fn rename(&self, name: &str, only: Option<&str>) -> bool {
        let Some(name) = tasks::clean_title(name) else {
            self.fail("A title needs at least one letter");
            return false;
        };
        let mut v = lock(&self.view);
        if only.is_some_and(|id| v.chat.id != id) {
            log::info!("title for {only:?}: the chat was closed meanwhile");
            return false;
        }
        v.chat.name.clone_from(&name);
        if let Err(e) = self.store.save_chat(&mut v.chat) {
            drop(v);
            self.fail(format!("Rename failed: {e}"));
            return false;
        }
        self.sync_chat(&mut v);
        v.ui.status.set(format!("Renamed to “{name}”"));
        drop(v);
        let rows = self.rows(); // reads chats.json: never under the lock
        self.with_ui(|ui| ui.chats = rows);
        true
    }

    /// The head's Suggest: the picked model names the chat from what was
    /// said. The chat may have been closed meanwhile: then nothing changes.
    pub fn suggest_title(self: &Arc<Self>) {
        let (id, turns) = {
            let v = lock(&self.view);
            let said = v.chat.messages.iter();
            let turns: Vec<(String, String)> =
                said.map(|m| (m.role.clone(), m.content.clone())).collect();
            (v.chat.id.clone(), turns)
        };
        if turns.is_empty() {
            return self.fail("Say something first: a title comes from the chat");
        }
        let cfg = self.cfg();
        let (ep, model) = match self.target(&cfg) {
            Ok(t) => t,
            Err(e) => return self.fail(e),
        };
        self.with_ui(|ui| ui.titling = true);
        let me = self.clone();
        let spawned = spawn("title", move || {
            let turns: Vec<Turn<'_>> = (turns.iter())
                .map(|(role, content)| Turn {
                    role,
                    content,
                    images: &[],
                })
                .collect();
            let body = tasks::title_body(&model, &turns);
            let asked = me.block_on(me.client.stream(
                &ep,
                body,
                &mut |_: &str| {},
                &CancellationToken::default(),
            ));
            me.with_ui(|ui| ui.titling = false);
            match asked.map(|t| tasks::clean_title(&t)) {
                Ok(Some(title)) => {
                    log::info!("title for {id}: {title:?} from {model}");
                    me.rename(&title, Some(&id));
                }
                Ok(None) => me.fail("The model gave no title"),
                Err(e) => me.fail(format!("Title failed: {e}")),
            }
        });
        if let Err(e) = spawned {
            self.with_ui(|ui| ui.titling = false);
            self.fail(e);
        }
    }
}
