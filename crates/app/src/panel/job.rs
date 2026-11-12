//! One job at a time: a streamed LLM answer, or a write-out, on a thread of
//! its own. core's [`blitznir_core::jobs::Slot`] decides whether its result
//! is still wanted, so an esc that comes late never lets a cancelled rewrite paste.

use super::{
    BUSY, Panel, Said, spawn,
    view::{Busy, View},
};
use crate::zap::Effect;
use blitznir_core::{
    Error, Result, config, jobs::Unfinished, llm::CancellationToken, lock, store::Unanswered, tasks,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

/// A streamed answer reaches the window at most this often.
const DELTA_EVERY: Duration = Duration::from_millis(40);

/// What a job is, for the window and the tray.
pub struct Doing {
    busy: Busy,
    /// The activity strip while it runs.
    says: String,
    /// The tray title while it runs.
    title: &'static str,
    /// What a cancel keeps in the history.
    kept: Option<Unfinished>,
}

impl Doing {
    pub fn new(busy: Busy, says: &str, title: &'static str) -> Doing {
        let says = says.into();
        let kept = None;
        Doing {
            busy,
            says,
            title,
            kept,
        }
    }

    /// A cancel keeps `kept` as a cancelled history row.
    pub fn keeping(self, kept: Unfinished) -> Doing {
        let kept = Some(kept);
        Doing { kept, ..self }
    }
}

type Work = dyn FnOnce(&Panel, &CancellationToken) -> Result<String> + Send;

impl Panel {
    /// Stream updates to the window, throttled; none once the job is
    /// cancelled (looked at under the lock `cancel` takes): a late one would
    /// land in the next job's place.
    pub(super) fn deltas(&self, cancel: &CancellationToken) -> impl FnMut(&str) + Send + '_ {
        let cancel = cancel.clone();
        let mut last: Option<Instant> = None;
        move |text: &str| {
            let early = last.is_some_and(|t| t.elapsed() < DELTA_EVERY);
            if text.is_empty() || early || cancel.is_cancelled() {
                return;
            }
            if last.is_none() {
                self.zap.progress();
            }
            last = Some(Instant::now());
            let mut v = lock(&self.view);
            if cancel.is_cancelled() {
                return;
            }
            let ui = &mut v.ui;
            match ui.busy {
                Some(Busy::Chat) => ui.streaming = Some(text.into()),
                Some(Busy::Settings) => {
                    let n = text.lines().count();
                    ui.doing = format!("Writing the new file… {n} lines · Esc cancels");
                }
                _ => ui.preview = text.into(),
            }
            drop(v);
            self.notify();
        }
    }

    /// A chat answer's reasoning to the window as it streams, throttled as
    /// [`Panel::deltas`] is: a trace comes at ~200 tokens a second.
    pub(super) fn thoughts(&self, cancel: &CancellationToken) -> impl FnMut(&str) + Send + '_ {
        let cancel = cancel.clone();
        let mut last: Option<Instant> = None;
        move |text: &str| {
            if last.is_some_and(|t| t.elapsed() < DELTA_EVERY) || cancel.is_cancelled() {
                return;
            }
            last = Some(Instant::now());
            let mut v = lock(&self.view);
            if cancel.is_cancelled() || v.ui.busy != Some(Busy::Chat) {
                return;
            }
            v.ui.trace = text.into();
            drop(v);
            self.notify();
        }
    }

    /// Run `work` on its own thread; hand its answer to `finish` unless the
    /// job was cancelled meanwhile. A rewrite, translation or write-out:
    /// where the selection is about to be replaced, the effect takes the
    /// panel's place ([`Zap`](crate::zap::Zap)); not where the app says it
    /// takes no text (a shown translation, a paste that lands nowhere).
    pub(super) fn job(
        self: &Arc<Self>,
        mut doing: Doing,
        work: impl FnOnce(&Panel, &CancellationToken) -> Result<String> + Send + 'static,
        finish: impl FnOnce(&Panel, String) + Send + 'static,
    ) -> Said {
        let (id, cancel, replaces, at) = {
            let mut v = lock(&self.view);
            let (id, cancel) = v.job.reserve(doing.kept.take()).ok_or(BUSY)?;
            let replaces = doing.busy != Busy::Settings && tasks::translate_pastes(v.ui.editable);
            (id, cancel, replaces, v.bounds)
        };
        if replaces
            && let Some(effect) = self.effect()
            && self.zap.start(at, self.platform.mouse(), effect)
        {
            self.hide();
        }
        self.start(id, cancel, doing, Box::new(work), finish)
    }

    /// The configured effect with its sound (none at volume 0); None: `effect = "off"`.
    pub(super) fn effect(&self) -> Option<Effect> {
        let (panel, p) = (self.cfg().panel, self.platform.clone());
        let loud = panel.effect_volume > 0;
        Some(match panel.effect {
            config::Effect::Beam => {
                Effect::Beam(loud.then(|| super::beam_sound::beam(panel.effect_volume)))
            }
            config::Effect::Lightning => Effect::Lightning(
                loud.then(|| super::sound::thunder(p, panel.effect_volume, panel.rain)),
            ),
            config::Effect::Off => return None,
        })
    }

    /// The effect's overlay goes at once and the panel is back in its
    /// place, unless the quick hotkey ran the job: there is no panel then.
    fn unzap(&self) {
        if self.zap.cancel()
            && !lock(&self.view).quick
            && let Some(me) = self.me.upgrade()
        {
            me.show();
        }
    }

    /// A job without the panel (the quick hotkey, a dictation's rewrite)
    /// ended unpasted, on Esc or an error: the app it came from gets the key
    /// focus back, which the effect's overlay took. Not on a cancel by a new
    /// open: the panel is about to take the focus itself.
    pub(super) fn give_back(&self) {
        let (quick, prev) = {
            let v = lock(&self.view);
            (v.quick, v.prev)
        };
        if !quick || self.platform.focused_app() == prev {
            return;
        }
        let platform = self.platform.clone();
        let back = move || {
            if !platform.refocus(&prev) {
                log::warn!("focus back: {:?} never came to the front", prev.0);
            }
        };
        if let Err(e) = spawn("refocus", back) {
            log::error!("focus back: {e}");
        }
    }

    /// [`Panel::job`] for a slot the caller took itself.
    pub(super) fn start(
        self: &Arc<Self>,
        id: u64,
        cancel: CancellationToken,
        doing: Doing,
        work: Box<Work>,
        finish: impl FnOnce(&Panel, String) + Send + 'static,
    ) -> Said {
        (self.hooks.busy)(Some(doing.title));
        let busy = doing.busy;
        self.with_ui(|ui| {
            ui.busy = Some(busy);
            ui.doing = format!("{} · Esc cancels", doing.says);
            ui.status.clear(); // a new action: the last error goes
        });
        let me = self.clone();
        let title = doing.title;
        let spawned = spawn("panel-job", move || {
            let out = work(&me, &cancel);
            if out.is_ok() {
                me.zap.finish(); // the last strike lands first; an esc meanwhile still stops the paste
            }
            let why = out
                .as_ref()
                .err()
                .map(|e| Unanswered::Failed(e.to_string()));
            let Some(kept) = me.end_job(id, why) else {
                return; // cancelled: cancel() already said so
            };
            match out {
                Ok(text) => finish(&me, text),
                Err(e) => {
                    me.unanswered(kept, Some(&e.to_string()));
                    if busy == Busy::Chat {
                        me.chats_changed();
                    }
                    me.unzap();
                    me.give_back();
                    me.failed(busy, title, &e);
                }
            }
            me.idle();
        });
        spawned.inspect_err(|_| {
            self.end_job(id, Some(Unanswered::Failed("it could not start".into())));
            self.unzap();
            self.idle();
        })
    }

    fn failed(&self, busy: Busy, title: &str, e: &Error) {
        log::warn!("{title} failed: {e}");
        let mut v = lock(&self.view);
        if v.quick {
            self.tray.flash(match busy {
                Busy::Translate => "Translation failed: see blitznir.log",
                _ => "Rewrite failed: see blitznir.log",
            });
        }
        let msg = e.detailed();
        if v.ui.settings {
            v.ui.config.msg.clone_from(&msg);
        }
        v.ui.status.fail(format!("{} failed: {msg}", busy.noun()));
        let held = v.dictation.take().filter(|(_, typed)| *typed == 0);
        let (text, to) = (v.ui.selection.clone().unwrap_or_default(), v.prev);
        drop(v);
        if held.is_some() {
            // a batch dictation still lands, pasted as on Esc: typed, it
            // waited for its app in front, where the effect's overlay was
            self.put_back(text, to);
        }
        self.notify();
    }

    /// Clear job `id`; Some (what it kept) if it was still the running one.
    /// Unanswered (`why`), the chat turn it was for is marked here, under
    /// the same lock as the clear, so a next send never finds it unmarked.
    fn end_job(&self, id: u64, why: Option<Unanswered>) -> Option<Option<Unfinished>> {
        let mut v = lock(&self.view);
        let current = v.job.end(id);
        if let Some(why) = why.filter(|_| current.is_some()) {
            self.unasked(&mut v, why);
        }
        current
    }

    /// The sidebar's chats and the tray's count, after a turn was kept
    /// unanswered (a new chat is stored by then). Reads chats.json: never
    /// under the lock.
    fn chats_changed(&self) {
        let rows = self.rows();
        self.with_ui(|ui| ui.chats = rows);
        self.tray.refresh();
    }

    /// A job without an answer: what it kept goes into the history,
    /// cancelled or `failed` with that error.
    /// On a thread of its own: Esc calls this on the UI thread, and
    /// history.json is read and written whole.
    fn unanswered(&self, kept: Option<Unfinished>, failed: Option<&str>) {
        let (Some(kept), Some(me)) = (kept, self.me.upgrade()) else {
            return;
        };
        let failed = failed.map(str::to_string);
        let keep = move || match kept.keep(&me.store, failed.as_deref()) {
            Ok(()) => {
                let recent = me.store.history();
                me.with_ui(|ui| ui.recent = recent);
            }
            Err(e) => log::error!("history: {e}"),
        };
        if let Err(e) = spawn("history", keep) {
            log::error!("history: {e}");
        }
    }

    /// A job ended: its busy goes, unless a newer job holds the slot by now
    /// (one started while this one still pasted).
    pub(super) fn idle(&self) {
        {
            let mut v = lock(&self.view);
            if v.job.busy() {
                return;
            }
            v.ui.busy = None;
        }
        (self.hooks.busy)(None);
        self.notify();
    }

    pub fn cancel(&self) {
        let (cancelled, chat) = {
            let mut v = lock(&self.view);
            let chat = v.asked.is_some();
            // a dictation rewrite ends with its job: kept, the next paste-back
            // would take its characters back
            v.dictation = None;
            let cancelled = v.job.cancel(); // drops the connection: the server stops
            if cancelled.is_some() {
                self.unasked(&mut v, Unanswered::Cancelled);
            }
            (cancelled, chat)
        };
        let Some(kept) = cancelled else {
            return;
        };
        log::info!("job cancelled");
        self.unanswered(kept, None);
        if chat {
            self.chats_changed();
        }
        self.unzap();
        self.idle();
    }

    /// The job ended without an answer (`why`): a chat's turn stays in it,
    /// marked; a typed translation's leaves its chat, and its text and
    /// images go back into the composer.
    fn unasked(&self, v: &mut View, why: Unanswered) {
        let asked = v.asked.take();
        let pending = v.pending.take();
        self.unask(v, asked, pending.is_none().then_some(why));
        v.ui.streaming = None;
        if let Some((text, mut shots)) = pending {
            let typed = std::mem::take(&mut v.ui.composer.text);
            let text = if typed.is_empty() {
                text
            } else {
                format!("{text}\n{typed}")
            };
            v.ui.composer.set(text);
            shots.append(&mut v.ui.images);
            v.ui.images = shots;
        }
        self.sync_chat(v);
    }
}
