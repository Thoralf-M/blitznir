//! The Settings form's changes: each goes into the editor's text through
//! core's config::form (comments kept, checked as Save checks), so the
//! form and the text are one; the form saves by itself once the changes
//! rest ([`AUTOSAVE`]), the Text tab with Save.

use super::{Panel, spawn};
use blitznir_core::{
    config::form::{self, Column, Field, Rows},
    lock,
    platform::KeyEvent,
};
use std::{sync::atomic::Ordering, thread, time::Duration};

/// The form saves this long after its last change: typing "600" passes
/// through 6 and 60, and a server's URL would be asked for models at every
/// keystroke.
const AUTOSAVE: Duration = Duration::from_millis(700);

/// What one input of the form changes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Target {
    Field(&'static Field),
    Cell(Rows, usize, &'static Column),
}

impl Target {
    /// Names the input in `Ui::form_error`.
    pub fn id(self) -> String {
        match self {
            Target::Field(f) => format!("{}.{}", f.table, f.key),
            Target::Cell(r, i, c) => format!("{r:?}.{i}.{}", c.key),
        }
    }

    pub fn kind(self) -> form::Kind {
        match self {
            Target::Field(f) => f.kind,
            Target::Cell(_, _, c) => c.kind,
        }
    }
}

impl Panel {
    /// The Form and Text tabs of Settings.
    pub fn show_form(&self, on: bool) {
        self.with_ui(|ui| ui.form = on);
    }

    /// The composer's +: Settings on the form, whose first table is the buttons.
    pub fn open_buttons(&self) {
        self.with_ui(|ui| ui.form = true);
        self.show_settings(true);
    }

    /// `change` on the editor's text: taken (true), or refused with its
    /// reason beside the input `id`; the autosave writes it once the changes rest.
    fn form_change(&self, id: String, change: impl FnOnce(&str) -> Result<String, String>) -> bool {
        let mut v = lock(&self.view);
        if !v.ui.config.loaded || v.ui.busy.is_some() {
            return false;
        }
        let taken = match change(&v.ui.config.text.text) {
            Ok(text) => {
                if text != v.ui.config.text.text {
                    Self::replace_config(&mut v.ui.config, text);
                    v.ui.config.msg = "Changed: saving…".into();
                    self.autosave(v.ui.config.text.rev);
                }
                if v.ui.form_error.as_ref().is_some_and(|(at, _)| *at == id) {
                    v.ui.form_error = None;
                }
                true
            }
            Err(e) => {
                v.ui.form_error = Some((id, e));
                false
            }
        };
        drop(v);
        self.notify();
        taken
    }

    pub fn form_set(&self, target: Target, input: String) {
        self.form_change(target.id(), |text| match target {
            Target::Field(f) => form::set(text, f, &input),
            Target::Cell(rows, i, c) => form::set_cell(text, rows, i, c, &input),
        });
    }

    /// A Return column's pick: row `i`, or None for Chat / Send.
    pub fn form_pick(&self, col: &'static Column, i: Option<usize>) {
        let id = format!("{:?}.{}", Rows::Actions, col.key);
        self.form_change(id, |text| form::pick(text, Rows::Actions, col, i));
    }

    pub fn form_add(&self, rows: Rows) {
        self.form_change(format!("{rows:?}"), |text| form::add_row(text, rows));
    }

    /// True: row `i` is gone, and the rows below it moved up.
    pub fn form_remove(&self, rows: Rows, i: usize) -> bool {
        self.form_change(format!("{rows:?}"), |text| form::remove_row(text, rows, i))
    }

    /// Save once the form's changes rest for [`AUTOSAVE`], and no job runs:
    /// one that started meanwhile delays the save to its end. Not when the
    /// text was replaced since (`rev` moved: the model's answer, the
    /// defaults): that waits for Save. A refused input never reached the
    /// text, so it holds nothing back: what was taken saves.
    fn autosave(&self, rev: u64) {
        let n = self.edits.fetch_add(1, Ordering::SeqCst) + 1;
        let me = self.me.clone();
        let wait = move || loop {
            thread::sleep(AUTOSAVE);
            let Some(me) = me.upgrade() else {
                return;
            };
            if me.edits.load(Ordering::SeqCst) != n {
                return; // a later change saves
            }
            let (dirty, now, busy) = {
                let v = lock(&me.view);
                let ed = &v.ui.config;
                (ed.dirty, ed.text.rev, v.ui.busy.is_some())
            };
            if !busy {
                if dirty && now == rev {
                    me.save_config();
                }
                return;
            }
        };
        if let Err(e) = spawn("form-autosave", wait) {
            log::warn!("form autosave: {e}");
        }
    }

    /// A hotkey field's Record (None: stop): the next key press is its value.
    pub fn record(&self, target: Option<Target>) {
        self.with_ui(|ui| ui.recording = target.map(|t| (t, false)));
    }

    /// While a field records, the key listener's press goes in by its config
    /// name, left and right told apart as the hotkeys tell them (Freya
    /// reports no Command key's side); true: taken here. Not once the form
    /// is gone from the window (Esc, Back): the record ends there, or a key
    /// pressed in another app would become the hotkey.
    pub(super) fn recorded(&self, ev: KeyEvent) -> bool {
        let (recording, form) = {
            let ui = &lock(&self.view).ui;
            (ui.recording, ui.settings && ui.form)
        };
        let Some((target, _)) = recording else {
            return false;
        };
        if !form {
            self.record(None);
            return false;
        }
        let name = match ev {
            KeyEvent::Modifier { key, down: true } => key.to_string(),
            KeyEvent::FKey(n) => format!("F{n}"),
            KeyEvent::Other => {
                self.with_ui(|ui| ui.recording = Some((target, true)));
                return true;
            }
            _ => return false,
        };
        self.form_set(target, name);
        self.record(None);
        true
    }
}
