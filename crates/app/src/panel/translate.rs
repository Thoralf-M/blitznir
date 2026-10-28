//! Translate with nothing selected: what is typed in the panel's field
//! becomes a new chat, "Translate" and the text as its first turn, the
//! translation streaming in as the answer. The original stays in sight;
//! Esc takes the turn back and the text returns to the field.

use super::chat::message;
use super::job::Doing;
use super::view::Busy;
use super::{BUSY, Panel, Said};
use blitznir_core::llm::CancellationToken;
use blitznir_core::lock;
use blitznir_core::store::{self, Chat};
use blitznir_core::tasks;
use std::sync::Arc;

impl Panel {
    pub(super) fn translate_typed(self: &Arc<Self>, text: String) -> Said {
        let cfg = self.cfg();
        let (ep, model) = self.target(&cfg)?;
        let mut v = lock(&self.view);
        let (job, cancel) = v.job.reserve(None).ok_or(BUSY)?;
        let mut chat = Chat::new();
        chat.name = store::chat_title(&format!("Translate: {text}"));
        chat.messages
            .push(message("user", &tasks::translation_turn(&text), vec![]));
        v.chat = chat;
        v.asked = Some(v.chat.id.clone());
        v.pending = Some((text.clone(), vec![])); // back in the field on Esc
        v.ui.composer.set("");
        (v.ui.streaming, v.ui.chatting, v.ui.settings) = (Some(String::new()), true, false);
        self.sync_chat(&mut v);
        let id = v.chat.id.clone();
        drop(v);
        log::info!("translate: {} typed chars, as a chat", text.chars().count());
        let work = move |me: &Panel, cancel: &CancellationToken| {
            let c = &me.client;
            let i = me.block_on(c.direction(&ep, &cfg, &model, &text, cancel))?;
            let body = tasks::rewrite_body(&cfg, &model, &text, "", Some(&i), &[]);
            me.block_on(c.stream(&ep, body, &mut me.deltas(cancel), cancel))
        };
        // streamed as a chat answer is (Busy::Chat): the thinking wave, the text
        let doing = Doing::new(Busy::Chat, "Translating…", "Translating…");
        self.start(job, cancel, doing, Box::new(work), move |me, out| {
            me.answer(&id, out)
        })
    }
}
