//! The model picker, the sidebar's fold and the Settings editor: config.toml
//! as text, saved only if it parses and validates.

use super::job::Doing;
use super::view::{Busy, Editor, MOD, ModelRow};
use super::{Panel, spawn};
use blitznir_core::config::{Config, DEFAULT_TOML, form};
use blitznir_core::llm::{CancellationToken, Listing, vision};
use blitznir_core::lock;
use blitznir_core::store::State;
use blitznir_core::tasks::{self, Kind};
use std::sync::Arc;

/// The picker's rows. A dead endpoint is one row that says why; the picked
/// model stays listed even when its server is down.
fn rows(listings: Vec<Listing>, current: &State, cfg: &Config) -> Vec<ModelRow> {
    let mut rows = vec![];
    for l in listings {
        let broken = |label: String| ModelRow {
            endpoint: l.endpoint.clone(),
            model: String::new(),
            label,
            error: true,
            vision: false,
        };
        match l.models {
            Ok(models) if models.is_empty() => {
                rows.push(broken(format!("{}: no models", l.endpoint)))
            }
            Ok(models) => rows.extend(models.into_iter().map(|m| ModelRow {
                label: format!("{m} · {}", l.endpoint),
                endpoint: l.endpoint.clone(),
                vision: l.vision.contains(&m),
                model: m,
                error: false,
            })),
            Err(e) => rows.push(broken(format!("{}: {e}", l.endpoint))),
        }
    }
    let listed = rows
        .iter()
        .any(|r| r.endpoint == current.endpoint && r.model == current.model);
    if !current.model.is_empty() && !listed {
        let label = format!("{} · {}", current.model, current.endpoint);
        let (endpoint, model) = (current.endpoint.clone(), current.model.clone());
        let vision = vision::unlisted(cfg, &endpoint, &model);
        rows.insert(
            0,
            ModelRow {
                endpoint,
                model,
                label,
                error: false,
                vision,
            },
        );
    }
    rows
}

/// How many lines `after` has that `before` lacks, and the other way round.
fn changed_lines(before: &str, after: &str) -> (usize, usize) {
    let mut left: Vec<&str> = before.lines().collect();
    let mut added = 0;
    for line in after.lines() {
        match left.iter().position(|l| *l == line) {
            Some(i) => drop(left.swap_remove(i)),
            None => added += 1,
        }
    }
    (added, left.len())
}

impl Panel {
    /// Ask every endpoint (up to 5 s), on a thread of its own. A list for
    /// servers config.toml no longer names is dropped: the save that
    /// changed them asked again, and that list may be in first.
    pub(super) fn list_models(self: &Arc<Self>) {
        let me = self.clone();
        let spawned = spawn("models", move || {
            let cfg = me.cfg();
            let listings = me.block_on(me.client.list_models(&cfg));
            let now = me.cfg();
            if (&now.endpoints, &now.llm) != (&cfg.endpoints, &cfg.llm) {
                return;
            }
            let current = me.store.state();
            let rows = rows(listings, &current, &cfg);
            me.with_ui(|ui| {
                ui.models = rows;
                ui.picked = (current.endpoint, current.model);
            });
        });
        if let Err(e) = spawned {
            log::error!("cannot list the models: {e}");
        }
    }

    pub fn set_model(&self, row: &ModelRow) {
        if !self.cfg().endpoints.iter().any(|e| e.name == row.endpoint) {
            return self.fail(format!("Endpoint {:?} is not in config.toml", row.endpoint));
        }
        log::info!("model: {} on {}", row.model, row.endpoint);
        let (endpoint, model) = (row.endpoint.clone(), row.model.clone());
        let picked = |s: &mut State| (s.endpoint, s.model) = (endpoint, model);
        match self.store.update_state(picked) {
            Ok(()) => self.with_ui(|ui| {
                ui.picked = (row.endpoint.clone(), row.model.clone());
                ui.status
                    .set(format!("Model: {} on {}", row.model, row.endpoint));
            }),
            Err(e) => self.fail(e.to_string()),
        }
    }

    /// The data folder, for Settings' About.
    pub fn data_dir(&self) -> String {
        self.paths.dir.display().to_string()
    }

    /// The sidebar's fold, kept in state.json.
    pub fn set_folded(&self, folded: bool) {
        self.with_ui(|ui| ui.folded = folded);
        if let Err(e) = self.store.update_state(|s| s.folded = folded) {
            self.fail(e.to_string());
        }
    }

    /// The sidebar's width at the end of a drag, kept in state.json.
    pub fn set_side_width(&self, width: f32) {
        self.with_ui(|ui| ui.side_width = width);
        if let Err(e) = self.store.update_state(|s| s.side_width = width) {
            self.fail(e.to_string());
        }
    }

    /// The desktop's dark preference where the window system gives none.
    pub fn prefers_dark(&self) -> Option<bool> {
        self.platform.prefers_dark()
    }

    /// Cmd + / Cmd - (`up`), Cmd 0 (None): the next zoom, kept in
    /// state.json; the window applies what this returns.
    pub fn zoom(&self, up: Option<bool>) -> f64 {
        let now = self.store.state().zoom();
        let z = up.map_or(1.0, |up| State::zoom_step(now, up));
        if let Err(e) = self.store.update_state(|s| s.zoom = z) {
            self.fail(e.to_string());
        }
        self.say(format!("Zoom {:.0} %", z * 100.0));
        f64::from(z)
    }

    /// The gear, Cmd+, and Back: the editor stands in for the band and composer.
    pub fn show_settings(&self, on: bool) {
        let mut v = lock(&self.view);
        v.ui.settings = on;
        v.ui.status.clear();
        if on {
            self.load_config(&mut v.ui.config);
        }
        drop(v);
        self.notify();
    }

    /// config.toml into the editor, unless it holds unsaved edits.
    pub(super) fn load_config(&self, ed: &mut Editor) {
        if ed.dirty {
            return;
        }
        match self.config_get() {
            Ok(text) => {
                ed.text.set(text);
                (ed.undoable, ed.loaded) = (false, true);
                ed.msg.clear();
            }
            Err(e) => ed.msg = e,
        }
    }

    fn config_get(&self) -> Result<String, String> {
        match std::fs::read_to_string(self.paths.config()) {
            Ok(text) => Ok(text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(DEFAULT_TOML.into()),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Typing in the editor.
    pub fn edit_config(&self, text: String) {
        let mut v = lock(&self.view);
        let ed = &mut v.ui.config;
        if ed.text.text != text {
            ed.text.text = text;
            ed.dirty = true;
        }
    }

    /// The editor's whole text replaced as one edit, which its undo takes back.
    pub(super) fn replace_config(ed: &mut Editor, text: String) {
        ed.text.set(text);
        (ed.undoable, ed.dirty, ed.loaded) = (true, true, true);
    }

    /// Save: nothing is written unless the text parses and validates. A
    /// blank text is valid TOML but means no endpoints: an accident, never a wish.
    pub fn save_config(self: &Arc<Self>) {
        let text = {
            let v = lock(&self.view);
            if !v.ui.config.loaded {
                return; // never save a text that was not shown
            }
            v.ui.config.text.text.clone()
        };
        let said = self.config_set(&text);
        let saved = said.is_ok();
        self.with_ui(|ui| {
            ui.config.msg = said.unwrap_or_else(|e| format!("Not saved: {e}"));
            if saved {
                ui.config.dirty = false;
                ui.set_buttons(&lock(&self.cfg));
            }
        });
        if saved {
            self.list_models(); // endpoints may have changed
        }
    }

    /// One key of config.toml from a switch outside the Settings tab (the
    /// header's Think, the box's Replace/Suggest), written as the form's
    /// Save writes it, off the UI thread, under the config's lock from the
    /// read to the swap (two quick clicks never lose one); then the window
    /// shows what config.toml holds, and a failure says so. A Settings
    /// editor with unsaved edits keeps them.
    pub(super) fn write_key(
        self: &Arc<Self>,
        (section, key): (&'static str, &'static str),
        on: bool,
    ) {
        let me = self.clone();
        let write = move || -> Result<String, String> {
            let mut cfg = lock(&me.cfg);
            let field = form::field(section, key).ok_or("not in the form")?;
            let text = std::fs::read_to_string(me.paths.config()).map_err(|e| e.to_string())?;
            let text = form::set(&text, field, if on { "true" } else { "false" })?;
            me.config_swap(&mut cfg, &text)
        };
        let me = self.clone();
        let set = move || {
            match write() {
                Ok(_) => log::info!("[{section}] {key} = {on}, from a switch"),
                Err(e) => {
                    log::error!("[{section}] {key} from a switch: {e}");
                    me.fail(format!("Not saved to config.toml: {e}"));
                }
            }
            let cfg = me.cfg();
            me.with_ui(|ui| ui.set_buttons(&cfg));
        };
        if let Err(e) = spawn("config-key", set) {
            log::error!("[{section}] {key}: {e}");
        }
    }

    /// The header's Think: `[llm] chat_reasoning`, shown at once.
    pub fn set_chat_reasoning(self: &Arc<Self>, on: bool) {
        self.with_ui(|ui| ui.reasons = on);
        self.write_key(("llm", "chat_reasoning"), on);
    }

    pub(super) fn config_set(&self, text: &str) -> Result<String, String> {
        self.config_swap(&mut lock(&self.cfg), text)
    }

    /// Write `text` to config.toml and make it the config, `cfg` being the
    /// config's lock, held: one write at a time, the file and the memory alike.
    fn config_swap(&self, cfg: &mut Config, text: &str) -> Result<String, String> {
        if text.trim().is_empty() {
            return Err("config.toml would be empty: nothing saved".into());
        }
        let new = Config::write_text(&self.paths.config(), text).map_err(|e| e.detailed())?;
        let old = std::mem::replace(cfg, new.clone());
        (self.hooks.config)(&new);
        if old.linux != new.linux {
            // Linux: the typing layout, without a restart; off the UI thread,
            // the desktop's keymap takes a roundtrip and typing holds the keyboard
            let (platform, cfg) = (self.platform.clone(), new.clone());
            if let Err(e) = spawn("configure", move || platform.configure(&cfg)) {
                log::error!("typing: {e}");
            }
        }
        log::info!("config.toml saved and applied");
        let (a, b) = (&old.dictation, &new.dictation);
        Ok(if a.model != b.model || a.history_max != b.history_max {
            "Saved. The speech model and history size change on the next start.".into()
        } else {
            "Saved and applied.".into()
        })
    }

    /// Reset to defaults: the commented default file into the editor; only Save writes it.
    pub fn reset_config(&self) {
        let mut v = lock(&self.view);
        if v.ui.busy.is_some() {
            return;
        }
        Self::replace_config(&mut v.ui.config, DEFAULT_TOML.into());
        v.ui.config.msg = format!("Defaults loaded. Save to apply, or {MOD}+Z to go back");
        drop(v);
        self.notify();
    }

    /// Ask model: the picked model edits the editor's text. Its answer only
    /// reaches the editor if it parses and validates; Save is still the user's.
    pub fn ask_config(self: &Arc<Self>) {
        let (instruction, current, loaded) = {
            let v = lock(&self.view);
            let ed = &v.ui.config;
            (
                v.ui.ask.text.trim().to_string(),
                ed.text.text.clone(),
                ed.loaded,
            )
        };
        if !loaded || lock(&self.view).ui.busy.is_some() {
            return;
        }
        if let Err(e) = self.try_ask_config(&instruction, current) {
            self.fail(e);
        }
    }

    fn try_ask_config(self: &Arc<Self>, instruction: &str, current: String) -> Result<(), String> {
        if let Some(why) = tasks::cannot_run(Kind::Settings, None, instruction, 0) {
            return Err(why.into());
        }
        let cfg = self.cfg();
        let (ep, model) = self.target(&cfg)?;
        log::info!("settings: {model} on {}, {} chars", ep.name, current.len());
        self.with_ui(|ui| ui.config.msg.clear());
        let body = tasks::settings_body(&model, &current, instruction);
        let work = move |me: &Panel, cancel: &CancellationToken| {
            let mut on = me.deltas(cancel);
            me.block_on(me.client.stream(&ep, body, &mut on, cancel))
        };
        let doing = Doing::new(
            Busy::Settings,
            "Asking the model for the new file…",
            "Editing settings…",
        );
        self.job(doing, work, move |me, out| me.checked(&current, out))
    }

    fn checked(&self, before: &str, out: String) {
        if let Err(e) = Config::parse(&out) {
            log::warn!("settings: the model's file does not parse: {e}");
            let msg = e.detailed();
            return self.with_ui(|ui| {
                ui.config.msg = format!("the model's file does not parse, nothing changed: {msg}");
                ui.status
                    .fail(format!("Settings edit failed: {}", ui.config.msg));
            });
        }
        // the answer comes trimmed
        let text = if out.ends_with('\n') { out } else { out + "\n" };
        let (added, removed) = changed_lines(before, &text);
        self.with_ui(|ui| {
            Self::replace_config(&mut ui.config, text);
            ui.ask.set("");
            ui.status.clear();
            ui.config.msg = if added + removed > 0 {
                format!(
                    "Review the changes, then Save: {added} line(s) new or changed, {removed} gone. {MOD}+Z undoes."
                )
            } else {
                "The model changed nothing.".into()
            };
        });
    }
}
