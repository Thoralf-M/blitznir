//! `[wake]`, hands-free: the ear (shared's wake.rs) feeds core's
//! [`Listener`]; what it decides goes to the dictator and the platform. The
//! dictator's notes say when a dictation starts and ends. One thread, so
//! the listener's state needs no lock. The tray app and `blitznir listen`
//! both run it.

mod commands;

use crate::{
    audio::Source,
    session::{Dictator, Note},
    wake::{self, Ear, Heard},
};
use blitznir_core::{
    config::{Config, Paths, Wake},
    dictation::Phase,
    platform::Platform,
    wake::{Act, Kind, Listener},
};
use commands::After;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Sender},
    },
    thread,
    time::{Duration, Instant},
};

/// A word this recent means someone is still dictating: the live text
/// grows every tick or two (350 ms each) while words come.
const FRESH: Duration = Duration::from_secs(1);
/// The live text ending in the stop phrase stops it once no word came for
/// this long ("blitz stop the music" goes on).
const SETTLED: Duration = Duration::from_millis(600);

enum Msg {
    Heard(Heard),
    /// A dictation started (by the wake phrase or a hotkey).
    Started,
    /// It ended; `typed`: `text` went onto the screen.
    Ended {
        typed: bool,
        text: String,
    },
    /// No dictation runs (one that never started ends without a text).
    Idle,
    Config(Box<Config>),
}

pub struct Hands {
    tx: Sender<Msg>,
}

impl Hands {
    /// Listening at once if `[wake] enabled`; the model is downloaded first
    /// (17 MB, once).
    pub fn start(
        paths: &Paths,
        cfg: &Config,
        platform: Arc<dyn Platform>,
        dictator: Arc<Dictator>,
        source: Source,
        flash: impl Fn(&'static str) + Send + 'static,
    ) -> std::io::Result<Arc<Hands>> {
        let (tx, rx) = mpsc::channel();
        let mut d = Driver {
            dir: wake::model_dir(paths),
            source,
            platform,
            dictator,
            flash: Box::new(flash),
            tx: tx.clone(),
            ear: None,
            on: None,
            listener: Listener::new(&cfg.wake),
            ours: false,
            woke: false,
            feed: None,
            armed: Arc::default(),
            last: None,
            alone: None,
            heard: String::new(),
            cut: None,
            then: None,
        };
        // its first download must not hold up the caller (the app's start)
        let _ = tx.send(Msg::Config(Box::new(cfg.clone())));
        thread::Builder::new().name("hands".into()).spawn(move || {
            for msg in rx {
                d.on(msg);
            }
        })?;
        Ok(Arc::new(Hands { tx }))
    }

    /// The dictator's note: a dictation's start and end.
    pub fn note(&self, note: &Note) {
        let msg = match note {
            Note::Phase(Phase::Live | Phase::Batch) => Msg::Started,
            Note::Phase(Phase::Idle) => Msg::Idle,
            Note::Dictated(text, shown) => Msg::Ended {
                typed: *shown && !text.trim().is_empty(),
                text: text.clone(),
            },
            _ => return,
        };
        let _ = self.tx.send(msg);
    }

    /// config.toml was saved.
    pub fn configure(&self, cfg: &Config) {
        let _ = self.tx.send(Msg::Config(Box::new(cfg.clone())));
    }
}

struct Driver {
    dir: PathBuf,
    /// What the ear hears: the microphone (a WAV for `blitznir listen --wav`).
    source: Source,
    platform: Arc<dyn Platform>,
    dictator: Arc<Dictator>,
    /// A short message for the tray.
    flash: Box<dyn Fn(&'static str) + Send>,
    tx: Sender<Msg>,
    ear: Option<Ear>,
    /// The `[wake]` and `[dictation] silence_rms` the ear listens with; None: off.
    on: Option<(Wake, f32)>,
    listener: Listener,
    /// The next dictation to start is the wake phrase's.
    ours: bool,
    /// The one running is: a send phrase may follow it.
    woke: bool,
    /// The frames from the wake phrase just heard, for the dictation it starts.
    feed: Option<mpsc::Receiver<Vec<f32>>>,
    /// Shared with the ear: a send phrase could follow (utterances go out).
    armed: Arc<AtomicBool>,
    /// The last hands-free dictation's app and text, for "scratch that"
    /// after it ended.
    last: Option<(blitznir_core::platform::Focus, String)>,
    /// When an utterance between pauses last ended in a dictation of ours:
    /// the live text may end in a command now.
    alone: Option<Instant>,
    /// The wake phrase as the transcript that just caught it wrote it.
    heard: String,
    /// A command said at the dictation's end: what the final pass takes off,
    /// and what to do then (moved to `then` once that end is pressed).
    cut: Option<(String, After)>,
    /// What to do once it ended: the command said at its end.
    then: Option<After>,
}

impl Driver {
    fn on(&mut self, msg: Msg) {
        let now = Instant::now();
        let act = match msg {
            Msg::Heard(Heard::Hit(kind @ Kind::Wake, clip, _)) if !self.confirmed(kind, &clip) => {
                None
            }
            Msg::Heard(Heard::Hit(kind, _, feed)) => {
                self.feed = feed;
                self.heard.clear(); // its feed starts 0.8 s before the hit, not at the phrase
                self.listener.hit(kind, now)
            }
            Msg::Heard(Heard::Utterance(clip, feed)) => self.utterance(&clip, feed, now),
            Msg::Heard(Heard::Block(loud)) => {
                self.armed
                    .store(self.listener.armed(now), Ordering::Release);
                // while dictating, speech is words arriving: music is loud too
                let dictating = matches!(self.dictator.phase(), Phase::Live | Phase::Batch);
                let words = self.dictator.words_at();
                let speech = match dictating {
                    true => words.is_some_and(|t| t.elapsed() < FRESH),
                    false => loud,
                };
                // the stop phrase as the transcript has it, once no word follows
                let settled = words.is_some_and(|t| t.elapsed() >= SETTLED);
                match dictating && self.woke && settled && self.said_at_end(now) {
                    true => self.listener.hit(Kind::Stop, now),
                    false => self.listener.block(speech, now),
                }
            }
            Msg::Started => {
                // nothing of the last one carries over
                (self.cut, self.then, self.alone) = (None, None, None);
                self.woke = std::mem::take(&mut self.ours);
                self.listener.started(self.woke, now);
                None
            }
            Msg::Ended { typed, text } => {
                self.ended(typed, text, now);
                None
            }
            Msg::Idle => {
                self.listener.idle();
                None
            }
            Msg::Config(cfg) => {
                self.configure(&cfg);
                None
            }
        };
        match act {
            Some(Act::Start) => {
                // set first: the dictator's note of the start comes through this thread
                self.ours = true;
                let heard = std::mem::take(&mut self.heard);
                let started =
                    (self.feed.take()).is_some_and(|f| self.dictator.start_if_idle(f, &heard));
                self.ours = started;
                if started {
                    log::info!("wake: dictation started by the wake phrase");
                }
            }
            Some(Act::Stop { said })
                if matches!(self.dictator.phase(), Phase::Live | Phase::Batch) =>
            {
                if let (true, Some((w, _))) = (said, &self.on) {
                    // a command's cut and deed go together, or neither
                    let cut = self.cut.take();
                    self.then = cut.as_ref().map(|c| c.1);
                    self.dictator
                        .stopped_by(cut.as_ref().map_or(&w.stop, |c| &c.0), cut.is_some());
                }
                log::info!(
                    "wake: dictation ended by the {}",
                    match (said, self.then) {
                        (true, Some(_)) => "command said at its end",
                        (true, None) => "stop phrase",
                        (false, _) => "pause",
                    }
                );
                self.dictator.press(true);
            }
            Some(Act::Send(to)) => self.send(to),
            _ => {}
        }
        self.feed = None; // a wake phrase's frames nothing started with
    }

    /// A short utterance, or any while a send phrase could follow: the
    /// transcript has a phrase right where the spotter missed it (a
    /// speaker's clear "hey blitz" five times in a row).
    fn utterance(
        &mut self,
        clip: &[f32],
        feed: Option<mpsc::Receiver<Vec<f32>>>,
        now: Instant,
    ) -> Option<Act> {
        let w = self.on.as_ref()?.0.clone();
        let armed = self.listener.armed(now);
        if !armed && self.dictator.phase() != Phase::Idle {
            // dictating: the live text has it; a command ends ours
            self.alone = self.woke.then_some(now);
            return None;
        }
        let Some(text) = self.dictator.transcribe(clip) else {
            log::info!("wake: an utterance the speech model could not take");
            return None;
        };
        use blitznir_core::wake::{is_command, said};
        // a command is all of it: a longer one's clip is only its end
        let whole = clip.len() < wake::CLIP;
        let armed = armed && whole;
        // the take-back first, and any time: it removes the sentence before
        // the caret, dictated or not; a send only after a dictation of ours
        if whole && is_command(&text, &w.scratch) {
            log::info!("wake: scratch phrase in {text:?}");
            self.scratch_back(self.listener.armed(now));
            return None;
        }
        if armed && is_command(&text, &w.send) {
            log::info!("wake: send phrase in {text:?}");
            return self.listener.hit(Kind::Send, now);
        }
        if said(&text, &w.wake) {
            log::info!("wake: wake phrase in {text:?}");
            self.feed = feed;
            // its clip goes to the dictation: the phrase comes off as written here
            self.heard = blitznir_core::wake::said_prefix(&text, &w.wake).unwrap_or_default();
            return self.listener.hit(Kind::Wake, now);
        }
        if armed {
            log::info!("wake: send armed, heard {text:?}: no phrase");
        }
        None
    }

    /// A dictation ended: a wake-started one that typed `text` arms the
    /// send phrase (and "scratch that" after it).
    fn ended(&mut self, typed: bool, text: String, now: Instant) {
        let arm = std::mem::take(&mut self.woke) && typed;
        let to = self.platform.focused_app();
        self.last = arm.then_some((to, text));
        self.listener.ended(to, arm, now);
        match self.then.take() {
            Some(After::Scratch) => self.scratch_back(true),
            Some(After::Send) => match self.listener.hit(Kind::Send, now) {
                Some(Act::Send(to)) => self.send(to),
                _ => log::info!("wake: send phrase at the end, but nothing typed to send"),
            },
            None => {}
        }
    }

    /// The speech model's second opinion on a wake phrase the
    /// spotter heard (false while a dictation runs: nothing would follow).
    fn confirmed(&self, kind: Kind, clip: &[f32]) -> bool {
        let (Some(text), Some((w, _))) = (self.dictator.transcribe(clip), &self.on) else {
            return false;
        };
        let yes = blitznir_core::wake::said(&text, w.phrase(kind));
        if !yes {
            log::info!("wake: the spotter heard {kind:?}, the speech model {text:?}");
        }
        yes
    }

    /// Listen as `cfg` says: the ear is made anew only when its settings changed.
    fn configure(&mut self, cfg: &Config) {
        let want = cfg
            .wake
            .enabled
            .then(|| (cfg.wake.clone(), cfg.dictation.silence_rms));
        if want == self.on {
            return;
        }
        self.ear = None; // closes the old one's microphone
        self.listener = Listener::new(&cfg.wake);
        self.on = want.clone();
        let Some((w, rms)) = want else {
            return log::info!("wake: off");
        };
        // the ear's own channel: forwarded into this thread's
        let (htx, hrx) = mpsc::channel::<Heard>();
        let tx = self.tx.clone();
        let forward = move || {
            for h in hrx {
                if tx.send(Msg::Heard(h)).is_err() {
                    break;
                }
            }
        };
        if let Err(e) = thread::Builder::new()
            .name("wake-forward".into())
            .spawn(forward)
        {
            return log::error!("wake: {e}");
        }
        let armed = self.armed.clone();
        match wake::ensure_model(&self.dir)
            .and_then(|()| wake::listen(&self.dir, &self.source, &w, rms, htx, armed))
        {
            Ok(ear) => self.ear = Some(ear),
            Err(e) => {
                self.on = None; // the next save tries again
                log::error!("wake: not listening: {e}");
                (self.flash)("Hands-free is off: see blitznir.log");
            }
        }
    }
}
