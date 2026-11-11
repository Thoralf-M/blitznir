//! Return, the action buttons and Translate on the selection, Write out,
//! and where their result goes: core's delivery along the route fixed when
//! the job started.

use super::job::Doing;
use super::view::{self, Busy, MOD, Shot, View};
use super::{Panel, Said};
use blitznir_core::jobs::{self, Delivered, Route, Unfinished};
use blitznir_core::llm::CancellationToken;
use blitznir_core::lock;
use blitznir_core::store::Made;
use blitznir_core::tasks::{self, Kind};
use std::sync::{Arc, Mutex};

/// What a rewrite or translation shows while it runs, and what the history
/// keeps of what made it (its prompt comes later): Return and the buttons
/// by their label, the quick hotkey as the quick action.
fn doing(translate: bool, quick: bool, label: &str) -> (Doing, Made) {
    let (doing, mode, action) = if translate {
        let doing = Doing::new(Busy::Translate, "Translating…", "Translating…");
        (doing, "translate", "Translate")
    } else {
        let doing = Doing::new(Busy::Rewrite, "Rewriting…", "Rewriting…");
        (doing, "rewrite", label.trim())
    };
    let action = if quick && !translate {
        "Quick action"
    } else {
        action
    };
    (doing, Made::new(mode, action, None))
}

impl Panel {
    /// Return: the button config.toml marks for it (`enter`, in a chat
    /// `chat_enter`), else Chat, in a chat Send.
    pub fn submit(self: &Arc<Self>) {
        let (chatting, enter, chat_enter) = {
            let v = lock(&self.view);
            (v.ui.chatting, v.ui.enter, v.ui.chat_enter)
        };
        match (chatting, enter, chat_enter) {
            (true, _, Some(i)) => self.run(Kind::Rewrite, i),
            (true, _, None) => self.send(),
            (false, Some(i), _) => self.run(Kind::Rewrite, i),
            (false, None, _) => self.chat_clicked(),
        }
    }

    /// Action button `i`: the first reads Write out while a chat has an answer.
    pub fn action(self: &Arc<Self>, i: usize) {
        if i == 0 && View::writes_out(&lock(&self.view).ui) {
            self.write_out(None);
        } else {
            self.run(Kind::Rewrite, i);
        }
    }

    /// Action button `action` (Return: 0) or Translate on the selection, with
    /// the composer's text as the instruction and the attached images.
    pub fn run(self: &Arc<Self>, kind: Kind, action: usize) {
        if let Err(e) = self.try_run(kind, action) {
            self.fail(e);
        }
    }

    pub(super) fn try_run(self: &Arc<Self>, kind: Kind, action: usize) -> Said {
        self.end_revert(None); // its Backspaces would hit this job's paste
        let (selection, editable, prev, instruction, shots, quick) = {
            let v = lock(&self.view);
            let (ui, typed) = (&v.ui, v.ui.composer.text.trim().to_string());
            let (selection, editable) = (ui.selection.clone(), ui.editable);
            (
                selection,
                editable,
                v.prev,
                typed,
                ui.images.clone(),
                v.quick,
            )
        };
        let images: Vec<String> = shots.iter().map(|s| s.url.clone()).collect();
        if tasks::translates_typed(kind, selection.as_deref(), &instruction, images.len()) {
            return self.translate_typed(instruction);
        }
        if let Some(why) = tasks::cannot_run(kind, selection.as_deref(), &instruction, images.len())
        {
            return Err(why.into());
        }
        let text = selection.unwrap_or_default();
        let cfg = self.cfg();
        let (ep, model) = self.target(&cfg)?;
        let dictated = lock(&self.view).dictation.clone();
        let button = match &dictated {
            Some((prompt, _)) => Some(blitznir_core::config::Button {
                label: "Dictation rewrite".into(),
                prompt: prompt.clone(),
                enter: false,
                chat_enter: false,
                hotkey: vec![],
            }),
            None => cfg.actions.get(action).cloned(),
        };
        let button = button.ok_or("That button is gone from the settings")?;
        let translate = kind == Kind::Translate;
        let route = Route::new(kind, editable, prev);
        let (doing, made) = doing(translate, quick, &button.label);
        log::info!(
            "{} by {:?}: {model} on {}, {} chars, {} image(s), shown here: {}",
            made.mode,
            made.action.as_deref().unwrap_or_default(),
            ep.name,
            text.chars().count(),
            images.len(),
            route == Route::Show
        );
        let own = button.prompt.clone();
        let i = Some(instruction.as_str());
        let prompt =
            (!translate).then(|| tasks::rewrite_prompt(&cfg, &text, &own, i, !images.is_empty()));
        let made = Arc::new(Mutex::new(Made { prompt, ..made }));
        let shown = (route == Route::Show).then(|| (text.clone(), shots));
        let original = text.clone();
        self.with_ui(|ui| ui.preview.clear());
        let kept = made.clone();
        let doing = doing.keeping(Unfinished {
            original: original.clone(),
            made: made.clone(),
        });
        // on the job's own thread, where blocking is allowed
        let work = move |me: &Panel, cancel: &CancellationToken| {
            let mut on = me.deltas(cancel);
            let c = &me.client;
            let (own, i) = if translate {
                let i = me.block_on(c.direction(&ep, &cfg, &model, &text, cancel))?;
                lock(&kept).prompt = Some(i.clone());
                (String::new(), i)
            } else {
                (own, instruction)
            };
            let body = tasks::rewrite_body(&cfg, &model, &text, &own, Some(&i), &images);
            me.block_on(c.stream(&ep, body, &mut on, cancel))
        };
        self.job(doing, work, move |me, out| {
            let made = lock(&made).clone();
            me.deliver(out, &made, route, shown, &original);
        })
    }

    /// Paste `text` into the app the panel came from; None = the open chat's
    /// newest answer.
    pub fn write_out(self: &Arc<Self>, text: Option<String>) {
        let (text, prev, original) = {
            let v = lock(&self.view);
            let newest = view::newest_answer(&v.ui).unwrap_or_default().to_string();
            let original = v.ui.selection.clone().unwrap_or_default();
            (text.unwrap_or(newest), v.prev, original)
        };
        if text.trim().is_empty() {
            return self.say("Nothing to write out yet");
        }
        let doing = Doing::new(Busy::Write, "Writing out…", "Writing out…");
        let done = self.job(
            doing,
            move |_, _| Ok(text),
            move |me, out| {
                let made = Made::new("write-out", "Write out", None);
                me.deliver(out, &made, Route::Paste(prev), None, &original);
            },
        );
        if let Err(e) = done {
            self.fail(e);
        }
    }

    /// core's [`jobs::deliver`] along `route`, fixed when the job started;
    /// a shown translation (`shown`: its text and images) becomes a chat turn.
    /// A paste over `original` becomes what Undo last replace takes back; a
    /// dictation's is offered back right there too (revert.rs).
    fn deliver(
        &self,
        out: String,
        made: &Made,
        route: Route,
        shown: Option<(String, Vec<Shot>)>,
        original: &str,
    ) {
        let typed = self.dictated_typed();
        // a rewrite asked for by a click (not automatic) always replaces
        let d = self.cfg().dictation;
        let replaces = !cfg!(target_os = "macos") || !d.rewrite_auto || d.rewrite_replaces;
        if let (Some(typed), Route::Paste(to), false) = (typed, route, replaces) {
            return self.suggest(to, typed, original, &out, made);
        }
        let d = jobs::deliver(
            &*self.platform,
            &self.store,
            &out,
            original,
            made,
            route,
            || {
                self.hide_and_wait();
                if let Route::Paste(to) = route {
                    // our overlay had the focus until now
                    self.erase_dictation(to, typed.unwrap_or(0));
                }
            },
        );
        if let (Some(_), Route::Paste(to), true, Some(me)) =
            (typed, route, d.pasted, self.me.upgrade())
        {
            me.offer_revert(to, original, &out);
        }
        if self.last.keep(route, d, original, &out) {
            self.tray.refresh();
        }
        let recent = self.store.history();
        self.with_ui(|ui| ui.recent = recent);
        match shown {
            Some((text, shots)) => {
                self.show_translation(&text, &shots, out.clone(), d.copied);
                // a quick translation the app takes no text for: the box by
                // the text shows it, else the panel (its chat either way)
                let quick = std::mem::take(&mut lock(&self.view).quick);
                if let Some(me) = self.me.upgrade().filter(|_| quick)
                    && !me.offer_translation(&out)
                {
                    me.show();
                }
            }
            None => self.pasted(out, d),
        }
    }

    fn pasted(&self, out: String, d: Delivered) {
        log::info!("pasted {} chars: {}", out.chars().count(), d.pasted);
        let quick = lock(&self.view).quick;
        if !d.pasted && quick {
            self.tray
                .flash("Not pasted: the result is on the clipboard");
        } else if !d.pasted
            && let Some(me) = self.me.upgrade()
        {
            me.show(); // to say where the text is
        }
        self.with_ui(|ui| {
            ui.preview = out;
            if d.pasted {
                ui.composer.set("");
                ui.images.clear();
                ui.status.clear();
            } else if d.copied {
                ui.status
                    .set(format!("On the clipboard: paste it with {MOD}+V"));
            } else {
                ui.status
                    .fail("Not pasted: the clipboard refused it, copy it from above");
            }
        });
    }
}
