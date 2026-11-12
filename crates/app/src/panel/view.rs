//! What the panel window shows, kept in the panel and never in the window:
//! Linux destroys the window on every hide (Wayland can neither hide nor
//! focus one), and the one made anew must show the same.

use blitznir_core::{
    config::Config,
    jobs::Slot,
    platform::{Bounds, Focus, Rect},
    store::{Chat, HistoryItem, State, Unanswered, Unthought},
};
use std::time::{Duration, Instant};

mod shot;
pub use shot::Shot;

/// A confirmation above the composer (Copied, Deleted …) stands this long;
/// an error stands until the next message or its x.
pub const NOTE_FOR: Duration = Duration::from_secs(2);

/// The keys as the keyboard names them.
pub const RET: &str = if cfg!(target_os = "macos") {
    "Return"
} else {
    "Enter"
};
pub const MOD: &str = if cfg!(target_os = "macos") {
    "Cmd"
} else {
    "Ctrl"
};

/// What runs: the activity strip says it, the buttons wait for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Busy {
    Rewrite,
    Translate,
    Chat,
    Write,
    Settings,
}

impl Busy {
    /// "… failed: why" names the job so.
    pub fn noun(self) -> &'static str {
        match self {
            Busy::Rewrite => "Rewrite",
            Busy::Translate => "Translation",
            Busy::Chat => "Chat",
            Busy::Write => "Write-out",
            Busy::Settings => "Settings edit",
        }
    }
}

/// A text the panel may replace: `rev` moves when it does, and the window
/// takes the text over; typing in the window only updates `text`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Field {
    pub rev: u64,
    pub text: String,
}

impl Field {
    pub fn set(&mut self, text: impl Into<String>) {
        self.rev += 1;
        self.text = text.into();
    }
}

/// The last message and when it was said: a confirmation, or an error.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Note {
    pub text: String,
    pub at: Option<Instant>,
    pub error: bool,
}

impl Note {
    /// A confirmation: gone after [`NOTE_FOR`].
    pub fn set(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.at = Some(Instant::now());
        self.error = false;
    }

    /// An error: stands until the next message or a clear.
    pub fn fail(&mut self, text: impl Into<String>) {
        self.set(text);
        self.error = true;
    }

    pub fn clear(&mut self) {
        *self = Note::default();
    }

    /// An error, or a confirmation said less than [`NOTE_FOR`] before `now`.
    pub fn fresh(&self, now: Instant) -> bool {
        !self.text.is_empty()
            && (self.error || self.at.is_some_and(|at| now.duration_since(at) < NOTE_FOR))
    }

    /// How long a fresh confirmation still stands; None for an error.
    pub fn stale_in(&self, now: Instant) -> Option<Duration> {
        let at = self.at.filter(|_| !self.error && self.fresh(now))?;
        Some(NOTE_FOR.saturating_sub(now - at))
    }
}

/// How the activity strip says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// A job running, a confirmation: small grey text.
    Quiet,
    /// The picked model may not take the attached images.
    Warn,
    /// Critical text with an x that dismisses it.
    Error,
}

/// The line above the composer; none, and it takes no room.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Strip {
    pub text: String,
    pub tone: Tone,
}

/// A chat message as shown.
#[derive(Clone, PartialEq)]
pub struct Msg {
    pub role: String,
    pub text: String,
    pub ts: String,
    pub images: Vec<Shot>,
    /// An answer's reasoning ("" for none), and why it was left off.
    pub thinking: String,
    pub unthought: Unthought,
    /// A user's turn that got no answer, and why.
    pub unanswered: Option<Unanswered>,
}

/// One picker row; a broken endpoint is one row that says why, with no model.
#[derive(Clone, PartialEq)]
pub struct ModelRow {
    pub endpoint: String,
    pub model: String,
    pub label: String,
    pub error: bool,
    /// It accepts images (core's llm::vision): the picker's eye.
    pub vision: bool,
}

#[derive(Clone, PartialEq)]
pub struct ChatRow {
    pub id: String,
    pub name: String,
    pub updated: String,
    pub messages: usize,
    /// The name and every message, lowercased: what the sidebar's search reads.
    pub haystack: String,
}

/// The Settings editor: config.toml as text.
#[derive(Clone, Default, PartialEq)]
pub struct Editor {
    pub text: Field,
    /// The last replacement is one undoable edit (an answer, the defaults; not a load).
    pub undoable: bool,
    /// Never save a text that was not shown.
    pub loaded: bool,
    /// Unsaved edits: kept until the next Save, across opens.
    pub dirty: bool,
    pub msg: String,
}

#[derive(Clone, Default, PartialEq)]
pub struct Ui {
    /// Moves on every open: the window focuses the composer again.
    pub opened: u64,
    /// The window is on screen; a hidden one lets go of key focus, or the
    /// cursor keeps blinking into a window nobody sees.
    pub shown: bool,
    /// The band shows the transcript and Return sends.
    pub chatting: bool,
    /// The Settings editor stands in for the band and the composer.
    pub settings: bool,
    /// Settings shows the form, not the text; kept across opens.
    pub form: bool,
    /// The form's last refused change: which input ("hotkeys.chat") and why.
    pub form_error: Option<(String, String)>,
    /// What Settings' Check for updates said (updates.rs).
    pub updates: super::updates::Updates,
    /// A hotkey field recording (form_keys.rs), and whether a key was refused.
    pub recording: Option<(super::form::Target, bool)>,
    /// None while the copy is still out.
    pub selection: Option<String>,
    pub editable: Option<bool>,
    pub busy: Option<Busy>,
    /// The running job's line: "Translating… · Esc cancels".
    pub doing: String,
    /// The last message; [`strip_for`] decides whether it shows.
    pub status: Note,
    /// The activity strip as shown: [`strip_for`], redone on every change.
    pub strip: Option<Strip>,
    /// A rewrite's text as it streams, then its result.
    pub preview: String,
    /// The chat answer so far, while it streams.
    pub streaming: Option<String>,
    /// The streaming answer's reasoning so far.
    pub trace: String,
    pub chat_id: String,
    pub chat_name: String,
    /// The model is naming the open chat.
    pub titling: bool,
    pub messages: Vec<Msg>,
    /// The Recent row the open chat is about, shown above its messages.
    pub about: Option<HistoryItem>,
    /// The open view's Delete asks in its head: "Delete this …? Yes / No".
    pub asking: bool,
    pub chats: Vec<ChatRow>,
    pub recent: Vec<HistoryItem>,
    pub models: Vec<ModelRow>,
    /// The picked endpoint and model; "" for none.
    pub picked: (String, String),
    /// config.toml's [[actions]] labels: the buttons left of Chat.
    pub actions: Vec<String>,
    /// The button Return runs; None = Return starts a chat.
    pub enter: Option<usize>,
    /// The button Return runs in a chat; None = Return sends.
    pub chat_enter: Option<usize>,
    /// `[panel] thinking_effect`: the wave in an answer's place until its first word.
    pub thinking: bool,
    /// `[llm] chat_reasoning`: the header's Think.
    pub reasons: bool,
    /// `[panel] dictation_effect`: the wave under a dictation into the composer.
    pub dictation_wave: bool,
    pub dictating: bool,
    pub folded: bool,
    /// The sidebar's folds, History's share as dragged, its width (state.json).
    pub closed: Vec<String>,
    pub history_open: bool,
    pub history_share: Option<f32>,
    pub side_width: f32,
    /// Attached for the next Return.
    pub images: Vec<Shot>,
    pub composer: Field,
    /// The Settings editor's instruction field.
    pub ask: Field,
    pub config: Editor,
}

impl Ui {
    /// config.toml's buttons, which one each Return runs, and the waves.
    pub fn set_buttons(&mut self, cfg: &Config) {
        self.actions = cfg.actions.iter().map(|a| a.label.trim().into()).collect();
        (self.enter, self.chat_enter) = (cfg.enter(), cfg.chat_enter());
        self.thinking = cfg.panel.thinking_effect;
        self.reasons = cfg.llm.chat_reasoning;
        self.dictation_wave = cfg.panel.dictation_effect;
    }

    /// The window as state.json and config.toml left it: sidebar, model, buttons.
    pub fn restore(&mut self, state: State, cfg: &Config) {
        (self.folded, self.side_width) = (state.folded, state.side_width());
        (self.history_open, self.history_share) = (state.history_open, state.history_share());
        self.closed.clone_from(&state.closed);
        self.picked = (state.endpoint, state.model);
        self.set_buttons(cfg);
    }
}

/// Everything the panel knows: the open's flow state and what the window shows.
pub struct View {
    /// The app to paste back into.
    pub prev: Focus,
    /// Where the selection is on screen: lightning strikes there.
    pub bounds: Option<Bounds>,
    /// The composer, in the window's points: a rewrite in it is aimed there.
    pub composer: Option<Rect>,
    /// A rewrite in the composer, before and after: Freya's input forgets
    /// its undo history when its text is set, so ⌘Z takes this one back.
    pub composer_undo: Option<(String, String)>,
    /// The open conversation; fresh on every open.
    pub chat: Chat,
    pub job: Slot,
    /// The chat whose last turn, the user's, the running job answers.
    pub asked: Option<String>,
    /// The quick hotkey's: no panel to come back to, the tray says it all.
    pub quick: bool,
    /// A dictation's rewrite ([dictation] rewrite): its prompt, in place of a
    /// button's, and how many characters a live one typed, taken back
    /// before the result is pasted.
    pub dictation: Option<(String, usize)>,
    /// A typed translation's text and images: back into the composer if it fails.
    pub pending: Option<(String, Vec<Shot>)>,
    /// Whether the band showed a chat before a Recent row was opened: Esc goes back there.
    pub before: bool,
    pub ui: Ui,
}

impl View {
    pub(crate) fn new() -> View {
        View {
            prev: Focus::default(),
            bounds: None,
            chat: Chat::new(),
            job: Slot::default(),
            asked: None,
            quick: false,
            dictation: None,
            pending: None,
            before: false,
            composer: None,
            composer_undo: None,
            // Settings opens on the form after a start, then on the tab last left
            ui: Ui {
                form: true,
                ..Ui::default()
            },
        }
    }

    /// The first action reads "Write out" while a chat has an answer and
    /// nothing is typed.
    pub fn writes_out(ui: &Ui) -> bool {
        ui.chatting && ui.composer.text.trim().is_empty() && newest_answer(ui).is_some()
    }
}

/// A Recent row is shown and nothing is asked about it yet: nothing is saved either.
pub fn viewing(ui: &Ui) -> bool {
    ui.chatting && ui.about.is_some() && ui.messages.is_empty() && ui.streaming.is_none()
}

/// The picked model has the eye.
fn sees(ui: &Ui) -> bool {
    let (endpoint, model) = &ui.picked;
    ui.models
        .iter()
        .any(|r| r.vision && &r.endpoint == endpoint && &r.model == model)
}

/// The activity strip, the first that holds: an error, a fresh confirmation,
/// the running job, the selection still being copied, or the warning that
/// the picked model may not take the attached images; else nothing.
pub fn strip_for(ui: &Ui, now: Instant) -> Option<Strip> {
    let say = |text: &str, tone| {
        Some(Strip {
            text: text.into(),
            tone,
        })
    };
    let note = &ui.status;
    if note.error && note.fresh(now) {
        return say(&note.text, Tone::Error);
    }
    if note.fresh(now) {
        return say(&note.text, Tone::Quiet);
    }
    if ui.busy.is_some() {
        return say(&ui.doing, Tone::Quiet);
    }
    if ui.selection.is_none() && ui.shown && !ui.settings {
        return say("Copying the selection…", Tone::Quiet);
    }
    if !ui.images.is_empty() && !ui.settings && !sees(ui) {
        return say("The picked model may not accept images", Tone::Warn);
    }
    None
}

/// The text of the newest answer shown.
pub fn newest_answer(ui: &Ui) -> Option<&str> {
    let m = ui.messages.iter().rev().find(|m| m.role == "assistant")?;
    Some(m.text.as_str())
}

/// "16:10" from a stamp of today, else "09-24".
pub fn short_time(ts: &str) -> String {
    let today = blitznir_core::now_stamp();
    match (ts.get(..10), ts.get(11..16)) {
        (Some(day), Some(time)) if today.starts_with(day) => time.into(),
        (Some(day), Some(_)) => day[5..].into(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests;
