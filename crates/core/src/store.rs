//! What the app keeps on disk: dictation history, the recording in progress,
//! saved chats, their images, and the selected model. Every write is atomic
//! (temp file + rename) and private (0600), so a crash never leaves half a file.

use crate::{Result, config::Paths, now_stamp};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{io::Write, os::unix::fs::OpenOptionsExt, path::Path, sync::Mutex};

mod data_url;
mod history;
mod images;
mod marks;
pub use data_url::{data_url, data_url_bytes};
pub use history::{HistoryItem, HistoryKind, Made};
pub use marks::{Unanswered, Unthought};

/// Conversations kept on disk; the cap is the clean-up.
pub const CHATS_MAX: usize = 50;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chat {
    pub id: String,
    pub name: String,
    pub created: String,
    pub updated: String,
    #[serde(default)]
    pub messages: Vec<Message>,
    /// The Recent row this chat asks about: its text is the chat's context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub about: Option<HistoryItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub ts: String,
    /// File names in `images/`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<String>,
    /// An answer's reasoning, shown folded above it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub thinking: String,
    /// Why the answer came without the thinking it was asked for.
    #[serde(default, skip_serializing_if = "Unthought::is_no")]
    pub unthought: Unthought,
    /// A turn of the user's that got no answer: cancelled, or failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unanswered: Option<Unanswered>,
}

/// state.json: app-owned, never edited by hand.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub endpoint: String,
    pub model: String,
    /// The panel's sidebar is folded away.
    pub folded: bool,
    /// The sidebar's width as last dragged, in points; 0 = never dragged.
    pub side_width: f32,
    /// The panel's zoom (Cmd +/-), 1 = 100 %; 0 = never zoomed.
    pub zoom: f32,
    /// The sidebar's lists folded away: "dictations", "replacements", "chats".
    pub closed: Vec<String>,
    /// The sidebar's History (dictations, replacements) is unfolded; it starts folded.
    pub history_open: bool,
    /// The share of the window each open History list may take, as its edge
    /// was last dragged; 0 = never dragged.
    pub history_share: f32,
}

impl State {
    /// The sidebar's default width, and the range a drag keeps it in.
    pub const SIDE_DEFAULT: f32 = 200.0;
    pub const SIDE_MIN: f32 = 160.0;
    pub const SIDE_MAX: f32 = 420.0;

    /// Cmd +/- steps through these, as a browser does; Cmd 0 is 1.
    const ZOOMS: [f32; 8] = [0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0];

    /// The panel's zoom: as set, 1 when never.
    pub fn zoom(&self) -> f32 {
        if self.zoom > 0.0 {
            self.zoom
                .clamp(Self::ZOOMS[0], Self::ZOOMS[Self::ZOOMS.len() - 1])
        } else {
            1.0
        }
    }

    /// The next step from `now` up (`up`) or down, staying at the ends.
    pub fn zoom_step(now: f32, up: bool) -> f32 {
        let z = &Self::ZOOMS;
        let next = if up {
            z.iter().find(|s| **s > now + 1e-3)
        } else {
            z.iter().rev().find(|s| **s < now - 1e-3)
        };
        next.copied()
            .unwrap_or(if up { z[z.len() - 1] } else { z[0] })
    }

    /// History's share as dragged, in range; None: never dragged.
    pub fn history_share(&self) -> Option<f32> {
        (self.history_share > 0.0).then(|| self.history_share.min(1.0))
    }

    /// The sidebar's width to draw: as dragged, kept in range.
    pub fn side_width(&self) -> f32 {
        if self.side_width > 0.0 {
            self.side_width.clamp(Self::SIDE_MIN, Self::SIDE_MAX)
        } else {
            Self::SIDE_DEFAULT
        }
    }
}

impl Chat {
    /// A blank conversation; not stored until it has a message.
    pub fn new() -> Chat {
        let now = now_stamp();
        let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        Chat {
            id,
            name: "New chat".into(),
            created: now.clone(),
            updated: now,
            messages: vec![],
            about: None,
        }
    }

    /// A blank conversation about a Recent row (a click in the sidebar);
    /// stored with its first message, like any chat.
    pub fn about(item: HistoryItem) -> Chat {
        Chat {
            about: Some(item),
            ..Chat::new()
        }
    }

    /// The messages a request sends: a turn left unanswered is not one
    /// (two user turns in a row break strict chat templates).
    pub fn sent(&self) -> impl Iterator<Item = &Message> {
        self.messages.iter().filter(|m| m.unanswered.is_none())
    }

    /// What the chat's system prompt is about: the Recent row it was started
    /// from, else `selection`, the text selected when the panel opened.
    pub fn context<'a>(&'a self, selection: &'a str) -> &'a str {
        self.about.as_ref().map_or(selection, HistoryItem::subject)
    }
}

impl Default for Chat {
    fn default() -> Self {
        Chat::new()
    }
}

/// A saved chat's name: its first six words.
pub fn chat_title(first_message: &str) -> String {
    let title = first_message
        .split_whitespace()
        .take(6)
        .collect::<Vec<_>>()
        .join(" ");
    title.chars().take(60).collect()
}

/// Write `data` to `path` via `path.tmp` + rename, mode 0600.
pub fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)?;
    f.write_all(data)?;
    f.sync_all()?;
    std::fs::rename(&tmp, path)
}

/// A JSON file's value, or the default when it is missing. One that cannot
/// be read is moved to `<name>.bad` first: the next write would replace it,
/// and what is in it may still be recovered by hand.
fn read_json<T: DeserializeOwned + Default>(path: &Path) -> T {
    let why = match std::fs::read(path) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(value) => return value,
            Err(e) => e.to_string(),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return T::default(),
        Err(e) => e.to_string(),
    };
    let mut bad = path.as_os_str().to_owned();
    bad.push(".bad");
    let bad = std::path::PathBuf::from(bad);
    match std::fs::rename(path, &bad) {
        Ok(()) => log::error!(
            "cannot read {} ({why}); moved it to {}",
            path.display(),
            bad.display()
        ),
        Err(e) => log::error!(
            "cannot read {} ({why}), nor move it aside: {e}",
            path.display()
        ),
    }
    T::default()
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    Ok(write_atomic(path, &serde_json::to_vec_pretty(value)?)?)
}

/// The data directory's files. Read-modify-write is serialized by a lock, so
/// the dictation thread and the panel can share one `Store`.
pub struct Store {
    paths: Paths,
    history_max: usize,
    lock: Mutex<()>,
}

impl Store {
    pub fn new(paths: Paths, history_max: usize) -> Result<Store> {
        std::fs::create_dir_all(&paths.dir)?;
        Ok(Store {
            paths,
            history_max,
            lock: Mutex::new(()),
        })
    }

    fn guard(&self) -> std::sync::MutexGuard<'_, ()> {
        crate::lock(&self.lock)
    }

    /// The text of the recording in progress, rewritten at every segment
    /// commit, so a crash or kill -9 still leaves the words on disk.
    pub fn write_recording(&self, text: &str) -> Result<()> {
        Ok(write_atomic(&self.paths.recording(), text.as_bytes())?)
    }

    /// Move the text of a recording that never finished into the history.
    pub fn recover_recording(&self) -> Result<Option<String>> {
        let text = std::fs::read_to_string(self.paths.recording()).unwrap_or_default();
        let text = text.trim();
        if text.is_empty() {
            return Ok(None);
        }
        log::info!(
            "recovered {} chars from an unfinished recording",
            text.chars().count()
        );
        self.add_history(text, "recovered")?;
        self.write_recording("")?;
        Ok(Some(text.to_string()))
    }

    /// Saved chats, newest first.
    pub fn chats(&self) -> Vec<Chat> {
        read_json(&self.paths.chats())
    }

    pub fn chat(&self, id: &str) -> Option<Chat> {
        self.chats().into_iter().find(|c| c.id == id)
    }

    /// Insert or update `chat` at the front. A chat without messages is not
    /// stored; the oldest beyond [`CHATS_MAX`] go, with their images.
    pub fn save_chat(&self, chat: &mut Chat) -> Result<()> {
        if chat.messages.is_empty() {
            return Ok(());
        }
        chat.updated = now_stamp();
        let _g = self.guard();
        let mut chats = self.chats();
        chats.retain(|c| c.id != chat.id);
        chats.insert(0, chat.clone());
        let evicted = chats.split_off(chats.len().min(CHATS_MAX));
        write_json(&self.paths.chats(), &chats)?;
        self.remove_images(evicted.iter().flat_map(|c| &c.messages));
        Ok(())
    }

    pub fn delete_chat(&self, id: &str) -> Result<()> {
        let _g = self.guard();
        let (gone, kept): (Vec<Chat>, Vec<Chat>) =
            self.chats().into_iter().partition(|c| c.id == id);
        if !gone.is_empty() {
            write_json(&self.paths.chats(), &kept)?;
            self.remove_images(gone.iter().flat_map(|c| &c.messages));
        }
        Ok(())
    }

    /// Delete every chat, with its images; how many went.
    pub fn clear_chats(&self) -> Result<usize> {
        let _g = self.guard();
        let gone = self.chats();
        if !gone.is_empty() {
            write_json(&self.paths.chats(), &Vec::<Chat>::new())?;
            self.remove_images(gone.iter().flat_map(|c| &c.messages));
        }
        Ok(gone.len())
    }

    /// Take `chat`'s last turn back if it is the user's: unanswered, it goes
    /// back where it was typed (a turn kept in its chat is marked instead,
    /// [`Message::unanswered`]). Its images go with it; a chat left empty is
    /// deleted.
    pub fn unask(&self, chat: &mut Chat) -> Result<()> {
        if chat.messages.last().is_none_or(|m| m.role != "user") {
            return Ok(());
        }
        let turn = chat.messages.pop();
        let saved = if chat.messages.is_empty() {
            self.delete_chat(&chat.id)
        } else {
            self.save_chat(chat)
        };
        self.remove_images(&turn);
        saved
    }

    pub fn state(&self) -> State {
        read_json(&self.paths.state())
    }

    /// Change some of state.json: the picker and the sidebar each set their part.
    pub fn update_state(&self, change: impl FnOnce(&mut State)) -> Result<()> {
        let _g = self.guard();
        let mut state = self.state();
        change(&mut state);
        write_json(&self.paths.state(), &state)
    }
}

#[cfg(test)]
mod tests;
