//! Chat about the selection or a Recent row, the saved chats, and a
//! translation shown as a chat turn.

mod resend;

use super::job::Doing;
use super::view::{self, Busy, ChatRow, Msg, Shot, View};
use super::{BUSY, Panel, Said};
use blitznir_core::config::{Config, Endpoint};
use blitznir_core::llm::{Answer, CancellationToken};
use blitznir_core::store::{self, Chat, Message, Store, Unanswered, Unthought};
use blitznir_core::tasks::{self, Turn};
use blitznir_core::{Error, Result, lock, now_stamp};
use std::sync::{Arc, Mutex};

pub(super) fn message(role: &str, content: &str, images: Vec<String>) -> Message {
    Message {
        role: role.into(),
        content: content.into(),
        ts: now_stamp(),
        images,
        thinking: String::new(),
        unthought: Unthought::No,
        unanswered: None,
    }
}

/// An answer's reasoning, handed from the job's thread to its finish.
type Trace = Arc<Mutex<Answer>>;

/// The request of a chat turn, run on the job's thread.
fn ask(
    mut chat: Chat,
    images: Vec<(String, Shot)>,
    cfg: Config,
    ep: Endpoint,
    model: String,
    selection: String,
    trace: Trace,
) -> impl FnOnce(&Panel, &CancellationToken) -> Result<String> + Send + 'static {
    move |me: &Panel, cancel: &CancellationToken| {
        // the new turn is stored before it is asked: unanswered, it stays marked
        for (name, shot) in &images {
            me.put_shot(name, shot)?;
        }
        {
            // a cancel marks the turn under this lock: the save never lands over its mark
            let _v = lock(&me.view);
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            me.store.save_chat(&mut chat)?;
        }
        let urls: Vec<Vec<String>> = chat.sent().map(|m| me.store.message_images(m)).collect();
        let turns: Vec<Turn<'_>> = chat
            .sent()
            .zip(&urls)
            .map(|(m, images)| Turn {
                role: &m.role,
                content: &m.content,
                images,
            })
            .collect();
        let body = tasks::chat_body(&cfg, &model, &selection, &turns);
        let (mut on, mut thinking) = (me.deltas(cancel), me.thoughts(cancel));
        let a = me.block_on(me.client.chat(&ep, body, &mut on, &mut thinking, cancel))?;
        let text = a.text.clone();
        *lock(&trace) = a;
        Ok(text)
    }
}

impl Panel {
    /// Store pasted/captured data URLs as files in images/.
    fn keep_images(&self, shots: &[Shot]) -> std::result::Result<Vec<String>, String> {
        shots
            .iter()
            .map(|s| {
                let name = Store::image_name();
                (self.put_shot(&name, s).map(|()| name)).map_err(|e| e.to_string())
            })
            .collect()
    }

    pub(super) fn rows(&self) -> Vec<ChatRow> {
        rows(&self.store)
    }
}

/// The sidebar's rows of the saved chats.
pub(super) fn rows(store: &Store) -> Vec<ChatRow> {
    let row = |c: Chat| {
        let said = c.messages.iter().map(|m| m.content.as_str());
        let haystack = std::iter::once(c.name.as_str()).chain(said);
        ChatRow {
            haystack: haystack.collect::<Vec<_>>().join("\n").to_lowercase(),
            messages: c.messages.len(),
            id: c.id,
            name: c.name,
            updated: c.updated,
        }
    };
    store.chats().into_iter().map(row).collect()
}

impl Panel {
    /// A stored image, decoded once.
    fn stored(&self, name: &str) -> Option<Shot> {
        if let Some(shot) = lock(&self.pngs).get(name) {
            return Some(shot.clone());
        }
        let url = self
            .store
            .image(name)
            .map_err(|e| log::warn!("image {name}: {e}"));
        let png = store::data_url_bytes(&url.ok()?)?;
        let mut shot = self.shot(String::new(), png, None);
        shot.stored = Some(name.into()); // its original read when wanted
        lock(&self.pngs).insert(name.into(), shot.clone());
        Some(shot)
    }

    /// The window's transcript from the open chat.
    pub(super) fn sync_chat(&self, v: &mut View) {
        let msg = |m: &Message| Msg {
            role: m.role.clone(),
            text: m.content.clone(),
            ts: m.ts.clone(),
            images: m.images.iter().filter_map(|n| self.stored(n)).collect(),
            thinking: m.thinking.clone(),
            unthought: m.unthought,
            unanswered: m.unanswered.clone(),
        };
        v.ui.messages = v.chat.messages.iter().map(msg).collect();
        v.ui.chat_id.clone_from(&v.chat.id);
        v.ui.chat_name.clone_from(&v.chat.name);
        v.ui.about.clone_from(&v.chat.about);
        v.ui.asking = false;
    }

    /// A click on Recent row `i` (of the list shown): its text in the
    /// transcript; what is typed next starts a chat about it, saved then.
    pub fn open_recent(&self, i: usize) {
        let mut v = lock(&self.view);
        let Some(item) = v.ui.recent.get(i).cloned() else {
            return;
        };
        if v.ui.busy.is_some() {
            return;
        }
        if !view::viewing(&v.ui) {
            v.before = v.ui.chatting;
        }
        v.chat = Chat::about(item);
        (v.ui.chatting, v.ui.settings) = (true, false);
        v.ui.status.clear();
        self.sync_chat(&mut v);
        drop(v);
        self.notify();
    }

    /// Esc while a Recent row is read: back to where the click came from.
    /// False: no row is read.
    pub(super) fn leave_recent(&self) -> bool {
        let mut v = lock(&self.view);
        if v.ui.busy.is_some() || !view::viewing(&v.ui) {
            return false;
        }
        v.chat = Chat::new();
        v.ui.chatting = v.before;
        v.ui.status.clear();
        self.sync_chat(&mut v);
        drop(v);
        self.notify();
        true
    }

    /// A saved chat from the sidebar, or "" for New chat.
    pub fn open_chat(&self, id: &str) {
        let chat = if id.is_empty() {
            Some(Chat::new())
        } else {
            self.store.chat(id)
        };
        let chats = self.rows();
        let mut v = lock(&self.view);
        if v.ui.busy.is_some() {
            return;
        }
        let Some(chat) = chat else {
            drop(v);
            return self.fail("That chat is gone");
        };
        v.ui.status.clear();
        v.chat = chat;
        (v.ui.chatting, v.ui.settings, v.ui.chats) = (true, false, chats);
        self.sync_chat(&mut v);
        drop(v);
        self.notify();
    }

    /// False: it could not be deleted (said why).
    pub(super) fn delete_chat(&self, id: &str, name: &str) -> bool {
        if let Err(e) = self.store.delete_chat(id) {
            self.fail(format!("Delete failed: {e}"));
            return false;
        }
        let chats = self.rows();
        let mut v = lock(&self.view);
        if v.chat.id == id {
            v.chat = Chat::new();
            self.sync_chat(&mut v);
        }
        v.ui.chats = chats;
        v.ui.status.set(format!("Deleted “{name}”"));
        drop(v);
        self.notify();
        self.tray.refresh();
        true
    }

    /// The Chat/Send button: enter the chat, and send what is typed right away.
    pub fn chat_clicked(self: &Arc<Self>) {
        let mut v = lock(&self.view);
        if v.ui.busy.is_some() {
            return;
        }
        let first = !v.ui.chatting;
        (v.ui.chatting, v.ui.settings) = (true, false);
        let has = !v.ui.composer.text.trim().is_empty() || !v.ui.images.is_empty();
        if has || first {
            v.ui.status.clear(); // the placeholder says what Return does
        } else {
            v.ui.status.set("Type a message first");
        }
        drop(v);
        self.notify();
        if has {
            self.send();
        }
    }

    /// Send the composer's text (and images) as the open chat's next turn.
    pub fn send(self: &Arc<Self>) {
        if let Err(e) = self.try_send() {
            self.fail(e);
        }
    }

    fn try_send(self: &Arc<Self>) -> Said {
        let cfg = self.cfg();
        let (ep, model) = self.target(&cfg)?;
        let mut v = lock(&self.view);
        let typed = v.ui.composer.text.trim().to_string();
        let shots = v.ui.images.clone();
        let text = match typed.as_str() {
            "" if shots.is_empty() => return Err("Type a message first".into()),
            "" => tasks::IMAGE_QUESTION.to_string(),
            t => t.to_string(),
        };
        if v.job.busy() {
            return Err(BUSY.into());
        }
        // a chat about a Recent row has it as context; others the selection
        let selection = match v.ui.selection.as_deref() {
            None if v.chat.about.is_none() => return Err(tasks::STILL_COPYING.into()),
            s => v.chat.context(s.unwrap_or_default()).to_string(),
        };
        if v.chat.messages.is_empty() {
            v.chat.name = store::chat_title(&text);
        }
        // unanswered, the turn stays in its chat, marked ([`Panel::unask`])
        let (job, cancel) = v.job.reserve(None).ok_or(BUSY)?; // free: checked under this lock
        // named now and shown from memory; written on the job's thread
        let images: Vec<(String, Shot)> = (shots.iter())
            .map(|s| (Store::image_name(), s.clone()))
            .collect();
        for (name, shot) in &images {
            lock(&self.pngs).insert(name.clone(), shot.clone());
        }
        let names = images.iter().map(|(n, _)| n.clone()).collect();
        v.chat.messages.push(message("user", &text, names));
        v.asked = Some(v.chat.id.clone());
        v.ui.composer.set("");
        v.ui.images.clear();
        (v.ui.streaming, v.ui.chatting) = (Some(String::new()), true);
        v.ui.trace.clear();
        self.sync_chat(&mut v);
        let chat = v.chat.clone();
        drop(v);
        log::info!(
            "chat {}: {model} on {}, turn {}",
            chat.id,
            ep.name,
            chat.messages.len()
        );
        let id = chat.id.clone();
        let trace = Trace::default();
        let work = ask(chat, images, cfg, ep, model, selection, trace.clone());
        let doing = Doing::new(Busy::Chat, "Thinking…", "Thinking…");
        self.start(job, cancel, doing, Box::new(work), move |me, out| {
            let a = std::mem::take(&mut *lock(&trace));
            let mut reply = message("assistant", &out, vec![]);
            (reply.thinking, reply.unthought) = (a.thinking, a.unthought);
            me.answer(&id, reply);
        })
    }

    /// Save `reply` into chat `id`, and show that chat.
    pub(super) fn answer(&self, id: &str, reply: Message) {
        let mut v = lock(&self.view);
        let mut chat = if v.chat.id == id {
            v.chat.clone()
        } else {
            self.store.chat(id).unwrap_or_default()
        };
        chat.messages.push(reply);
        if let Err(e) = self.store.save_chat(&mut chat) {
            log::error!("chats.json: {e}");
        }
        v.chat = chat;
        v.pending = None;
        v.ui.streaming = None;
        (v.ui.chatting, v.ui.settings) = (true, false);
        v.ui.status
            .set("Ask another question, or Write out puts this answer into the app");
        self.sync_chat(&mut v);
        drop(v);
        let rows = self.rows(); // reads chats.json: never under the lock
        self.with_ui(|ui| ui.chats = rows);
        self.tray.refresh(); // its Delete all chats counts them
    }

    /// The running job ended without answering chat `asked`: mark its user
    /// turn `why` (a chat's), or take it back (None: a typed translation's),
    /// in the open chat or, if another was opened meanwhile, the stored one.
    pub(super) fn unask(&self, v: &mut View, asked: Option<String>, why: Option<Unanswered>) {
        let Some(id) = asked else { return };
        let open = v.chat.id == id;
        let mut chat = match open {
            true => v.chat.clone(),
            false => match self.store.chat(&id) {
                Some(chat) => chat,
                None => return,
            },
        };
        let kept = match why {
            None => self.store.unask(&mut chat),
            Some(why) => {
                let last = chat.messages.last_mut().filter(|m| m.role == "user");
                last.into_iter()
                    .for_each(|m| m.unanswered = Some(why.clone()));
                self.store.save_chat(&mut chat)
            }
        };
        if let Err(e) = kept {
            log::error!("chats.json: {e}");
        }
        if open {
            v.chat = chat;
        }
    }

    /// The app says the selection cannot be replaced, so the original and
    /// its translation show as a chat turn instead, kept in the history
    /// (jobs::deliver), not as a chat: a question about it saves the chat.
    pub(super) fn show_translation(&self, text: &str, shots: &[Shot], out: String, copied: bool) {
        let names = self.keep_images(shots).unwrap_or_else(|e| {
            log::error!("images: {e}");
            vec![]
        });
        let mut v = lock(&self.view);
        if v.chat.messages.is_empty() {
            v.chat.name = store::chat_title(&format!("Translate: {text}"));
        }
        let turn = tasks::translation_turn(text);
        v.chat.messages.push(message("user", &turn, names));
        v.chat.messages.push(message("assistant", &out, vec![]));
        log::info!("translation shown in the panel, on the clipboard: {copied}");
        (v.ui.chatting, v.ui.settings) = (true, false);
        v.ui.status
            .set("Ask about it, copy it, or Write out pastes it");
        self.sync_chat(&mut v);
    }
}
