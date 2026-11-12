//! The panel's flows on Freya: open with the selection, rewrite or translate
//! and paste back, chat, capture, the model picker and the Settings tab.
//! core does the work (jobs, llm, store); this sequences it and keeps what
//! the window shows in a [`view::Ui`], which the window only renders.

mod attach;
mod beam_sound;
mod chat;
mod dictation;
pub mod form;
mod history;
mod job;
mod pick;
mod quick;
mod revert;
mod rewrite;
mod settings;
mod sound;
mod title;
mod translate;
pub mod updates;
pub mod view;
pub use history::{List, list_name, noun};
mod window;

use crate::{tray::Tray, zap::Zap};
use blitznir_core::{
    Result,
    config::{Config, Endpoint, Paths},
    jobs::Undo,
    llm::{self, Client},
    lock,
    platform::{Focus, Platform},
    store::{Chat, Store},
};
use freya::{prelude::LaunchProxy, winit::window::WindowId};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use view::{RET, Shot, Ui, View};

/// The previous app gets this long to act on our ⌘C before we take focus.
const COPY_HEAD_START: Duration = Duration::from_millis(120);
const BUSY: &str = "Still working: Esc cancels";

/// What the daemon wires in.
pub struct Hooks {
    /// A job started (the tray title to show) or ended (None).
    pub busy: Box<dyn Fn(Option<&'static str>) + Send + Sync>,
    /// config.toml was saved: hotkeys and dictation follow it.
    pub config: Box<dyn Fn(&Config) + Send + Sync>,
}

/// How an open of the panel starts: Return rewrites, Return sends, or the
/// Settings editor (the tray's "Settings…": no ⌘C, nothing to paste back).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Rewrite,
    Chat,
    Settings,
}

pub struct Panel {
    me: Weak<Panel>,
    platform: Arc<dyn Platform>,
    store: Arc<Store>,
    paths: Paths,
    client: Client,
    /// reqwest's futures need a runtime; core leaves it to the app.
    rt: tokio::runtime::Runtime,
    cfg: Mutex<Config>,
    view: Mutex<View>,
    hooks: Hooks,
    /// The last paste-back, for Undo last replace.
    last: Undo,
    tray: Arc<Tray>,
    /// The working effect (beam or lightning) while a job replaces the selection.
    zap: Arc<Zap>,
    proxy: LaunchProxy,
    /// The window, while it exists (window.rs).
    window: Mutex<Option<WindowId>>,
    /// Wakes the window's component when [`Ui`] changed.
    wake: Mutex<Option<async_channel::Sender<()>>>,
    /// Stored chat images, decoded once, by file name.
    pngs: Mutex<HashMap<String, Shot>>,
    shots: AtomicU64,
    /// The form's changes so far: an autosave waits for the last (form.rs).
    edits: AtomicU64,
    /// A timer waits for the strip's confirmation to go stale.
    stale: AtomicBool,
    /// The revert offer after a dictation's rewrite (revert.rs).
    revert: Mutex<Option<revert::Offer>>,
    /// The mouse selections' Translate button (pick.rs).
    pick: Mutex<pick::Seen>,
}

fn spawn(name: &str, f: impl FnOnce() + Send + 'static) -> std::result::Result<(), String> {
    thread::Builder::new()
        .name(name.into())
        .spawn(f)
        .map(drop)
        .map_err(|e| e.to_string())
}

/// What Return, a button, an open says above the composer when it fails.
type Said = std::result::Result<(), String>;

pub struct Parts {
    pub platform: Arc<dyn Platform>,
    pub store: Arc<Store>,
    pub paths: Paths,
    pub cfg: Config,
    pub tray: Arc<Tray>,
    pub proxy: LaunchProxy,
}

impl Panel {
    pub fn new(p: Parts, hooks: Hooks) -> Result<Arc<Panel>> {
        let client = Client::new()?;
        let rt = tokio::runtime::Runtime::new()?;
        Ok(Arc::new_cyclic(|me: &Weak<Panel>| {
            let esc = me.clone();
            let zap = Zap::new(p.tray.clone(), p.proxy.clone(), move || {
                if let Some(panel) = esc.upgrade() {
                    panel.escape();
                }
            });
            Panel::with(me.clone(), p, hooks, client, rt, zap)
        }))
    }

    fn with(
        me: Weak<Panel>,
        p: Parts,
        hooks: Hooks,
        client: Client,
        rt: tokio::runtime::Runtime,
        zap: Arc<Zap>,
    ) -> Panel {
        // the quick action and a dictation rewrite show it without an open
        let mut view = View::new();
        view.ui.restore(p.store.state(), &p.cfg);
        (view.ui.recent, view.ui.chats) = (p.store.history(), chat::rows(&p.store));
        Panel {
            me,
            platform: p.platform,
            store: p.store,
            paths: p.paths,
            client,
            rt,
            cfg: Mutex::new(p.cfg),
            view: Mutex::new(view),
            hooks,
            last: Undo::default(),
            tray: p.tray,
            zap,
            proxy: p.proxy,
            window: Mutex::new(None),
            wake: Mutex::new(None),
            pngs: Mutex::new(HashMap::new()),
            shots: AtomicU64::new(1),
            edits: AtomicU64::new(0),
            stale: AtomicBool::new(false),
            revert: Mutex::new(None),
            pick: Mutex::default(),
        }
    }

    fn cfg(&self) -> Config {
        lock(&self.cfg).clone()
    }

    /// What the window shows now.
    pub fn ui(&self) -> Ui {
        lock(&self.view).ui.clone()
    }

    /// The window's component listens here; a window made anew takes over.
    pub fn attach(&self, wake: async_channel::Sender<()>) {
        *lock(&self.wake) = Some(wake);
    }

    /// Redo the activity strip and wake the window. A confirmation goes
    /// stale by itself: one timer comes back then.
    fn notify(&self) {
        let now = Instant::now();
        let stale_in = {
            let mut v = lock(&self.view);
            v.ui.strip = view::strip_for(&v.ui, now);
            v.ui.status.stale_in(now)
        };
        if let Some(wait) = stale_in
            && !self.stale.swap(true, Ordering::SeqCst)
        {
            let me = self.me.clone();
            let _ = spawn("status", move || {
                thread::sleep(wait + Duration::from_millis(50));
                if let Some(me) = me.upgrade() {
                    me.stale.store(false, Ordering::SeqCst);
                    me.notify();
                }
            });
        }
        if let Some(wake) = lock(&self.wake).as_ref() {
            let _ = wake.try_send(());
        }
    }

    /// Change what the window shows, and tell it.
    fn with_ui(&self, f: impl FnOnce(&mut Ui)) {
        f(&mut lock(&self.view).ui);
        self.notify();
    }

    /// A confirmation above the composer, gone after [`view::NOTE_FOR`].
    pub fn say(&self, status: impl Into<String>) {
        let status = status.into();
        self.with_ui(|ui| ui.status.set(status));
    }

    /// An error above the composer, until the next message or its x.
    /// "Still working" is no error: it would outlast the job.
    pub fn fail(&self, status: impl Into<String>) {
        let status = status.into();
        if status == BUSY {
            return self.say(status);
        }
        self.with_ui(|ui| ui.status.fail(status));
    }

    /// The strip's x.
    pub fn dismiss(&self) {
        self.with_ui(|ui| ui.status.clear());
    }

    /// A hidden panel records no hotkey (form.rs).
    fn set_shown(&self, shown: bool) {
        self.with_ui(|ui| (ui.shown, ui.recording) = (shown, ui.recording.filter(|_| shown)));
    }

    /// Typing in the composer; the panel reads it on Return.
    pub fn set_composer(&self, text: String) {
        lock(&self.view).ui.composer.text = text;
    }

    pub fn set_ask(&self, text: String) {
        lock(&self.view).ui.ask.text = text;
    }

    /// The rewrite or chat hotkey, the tray, `BLITZNIR_OPEN_PANEL`: `preset`
    /// stands in for the selection; None copies it from the app in front.
    pub fn open(self: &Arc<Self>, mode: Mode, preset: Option<String>) {
        let me = self.clone();
        if let Err(e) = spawn("panel-open", move || me.open_now(mode, preset)) {
            log::error!("cannot open the panel: {e}");
        }
    }

    fn open_now(self: &Arc<Self>, mode: Mode, preset: Option<String>) {
        let settings = mode == Mode::Settings;
        if !settings && self.window_focused() {
            self.show(); // in front already: nothing to copy
            return;
        }
        self.cancel(); // its paste target is about to change
        // both BEFORE our window shows: the app in front gets the ⌘C
        let prev = self.platform.focused_app();
        let preset = if settings {
            Some(String::new())
        } else {
            preset
        };
        let copy = preset.is_none().then(|| self.platform.begin_copy());
        let opened = self.reset(mode, prev, preset.clone());
        self.list_models();
        if copy.is_some() {
            thread::sleep(COPY_HEAD_START);
        }
        self.show();
        let text = match copy {
            Some(t) => self.platform.finish_copy(t).unwrap_or_default(),
            None => preset.unwrap_or_default(),
        };
        let editable = self.platform.focused_editable(&prev);
        // where the paste will land; the previous app answers from the background
        let bounds = (!settings)
            .then(|| self.platform.selection_bounds(&prev))
            .flatten()
            .filter(|b| b.fits(&text)); // else the mouse, by the text it just selected
        {
            let mut v = lock(&self.view);
            if v.ui.opened != opened {
                return;
            }
            v.ui.selection = Some(text.clone());
            v.ui.editable = editable;
            v.bounds = bounds;
        }
        self.notify();
        log::info!(
            "panel: {} chars selected, editable {editable:?}, previous app {:?}",
            text.chars().count(),
            prev.0
        );
    }

    /// A fresh open: forget the last one's view. The open's number.
    fn reset(&self, mode: Mode, prev: Focus, selection: Option<String>) -> u64 {
        self.end_revert(None);
        let cfg = self.cfg();
        let state = self.store.state();
        let (recent, chats) = (self.store.history(), self.rows());
        let mut v = lock(&self.view);
        (v.prev, v.bounds) = (prev, None);
        v.chat = Chat::new();
        (v.quick, v.pending, v.dictation) = (false, None, None);
        let ui = &mut v.ui;
        ui.opened += 1;
        (ui.chatting, ui.settings) = (mode == Mode::Chat, mode == Mode::Settings);
        ui.form_error = None;
        (ui.selection, ui.editable, ui.busy) = (selection, None, None);
        (ui.streaming, ui.recent, ui.chats) = (None, recent, chats);
        ui.status.clear();
        ui.preview.clear();
        ui.images.clear();
        ui.composer.set("");
        ui.restore(state, &cfg);
        let opened = ui.opened;
        if mode == Mode::Settings {
            self.load_config(&mut v.ui.config);
        }
        self.sync_chat(&mut v);
        drop(v);
        self.notify();
        opened
    }

    /// The endpoint and model picked in the panel.
    fn target(&self, cfg: &Config) -> std::result::Result<(Endpoint, String), String> {
        let state = self.store.state();
        llm::target(cfg, &state)
            .map(|(ep, model)| (ep.clone(), model.to_string()))
            .map_err(|e| e.to_string())
    }

    fn block_on<F: Future>(&self, f: F) -> F::Output {
        self.rt.block_on(f)
    }

    /// Esc: take back a Delete's question, cancel the running job, leave a
    /// Recent row being read, leave Settings, or else close.
    pub fn escape(&self) {
        if lock(&self.view).ui.asking {
            self.ask_delete(false);
        } else if lock(&self.view).job.busy() {
            let held = self.held_dictation();
            self.cancel();
            match held {
                Some((text, to)) => self.put_back(text, to), // the paste brings `to` back
                None => self.give_back(),
            }
            self.say(format!(
                "Cancelled: edit and press {RET} again, or Esc to close"
            ));
        } else if lock(&self.view).ui.settings {
            self.show_settings(false); // as Back: unsaved edits stay
        } else if !self.leave_recent() {
            self.close();
        }
    }

    /// Esc with nothing running, or the window's close button: hide, never quit.
    pub fn close(&self) {
        self.cancel();
        self.hide();
    }
}
