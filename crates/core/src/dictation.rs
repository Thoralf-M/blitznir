//! One dictation session, on one thread: audio in, text typed, history kept.
//!
//! Never-lose-text rules: `recording.txt` holds the committed text while the
//! session runs (a crash leaves it for the next start or session to recover);
//! the final text reaches the history BEFORE it is typed (typing is the step
//! that can go wrong); a stuck flush gives up after `flush_timeout` and keeps
//! what it has.

use crate::{
    Result,
    platform::{Platform, Recognizer},
    segmenter::{Out, SegConfig, Segmenter},
    store::Store,
    textproc::{divergence, typing_diff},
    vocab::Vocab,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, RecvTimeoutError},
    },
    time::{Duration, Instant},
};

#[derive(Debug, Clone)]
pub struct Opts {
    /// Type while speaking; false = batch, type once at the end.
    pub live: bool,
    pub seg: SegConfig,
    /// Batch with a rewrite prompt: the text is kept, not typed; the
    /// rewrite types its result instead.
    pub hold: bool,
    pub auto_copy: bool,
    /// Max time finishing may take after stop before it types what it has.
    pub flush_timeout: Duration,
    /// Live typing at most this often.
    pub type_every: Duration,
    /// Nothing reached the screen for this long: a big correction is allowed.
    pub stale_after: Duration,
    /// `[vocabulary]`: applied to everything typed or kept.
    pub vocab: Vocab,
}

/// Live corrections are at most this many characters of backspace.
const MAX_CORRECTION: usize = 30;

/// Where the app is between sessions; the tray shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// The model is not ready yet.
    Loading,
    Idle,
    Live,
    Batch,
    /// Stopped; the last decode and the typing are still running.
    Finishing,
}

/// What a dictation hotkey (live or batch) does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Start { live: bool },
    Stop,
    Ignore,
}

/// Either key stops a running recording, whichever mode it is in; nothing
/// starts before the model is loaded or while the last session finishes.
pub fn step(phase: Phase, live: bool) -> Step {
    match phase {
        Phase::Idle => Step::Start { live },
        Phase::Live | Phase::Batch => Step::Stop,
        Phase::Loading | Phase::Finishing => Step::Ignore,
    }
}

impl Opts {
    pub fn new(live: bool, cfg: &crate::config::Config) -> Opts {
        let d = &cfg.dictation;
        Opts {
            live,
            seg: SegConfig::from(d),
            hold: !live && d.rewrite_prompt().is_some(),
            auto_copy: d.auto_copy,
            flush_timeout: Duration::from_secs(30),
            type_every: Duration::from_millis(400),
            stale_after: Duration::from_secs(2),
            // valid: Config::parse compiled it already
            vocab: Vocab::new(&cfg.vocabulary).unwrap_or_default(),
        }
    }
}

/// Keeps the focused field in sync with the evolving text.
#[derive(Default)]
struct Typer {
    typed: String,
    last: Option<Instant>,
}

impl Typer {
    fn update(&mut self, p: &dyn Platform, target: &str) -> Result<()> {
        if target == self.typed {
            return Ok(());
        }
        let (back, text) = typing_diff(&self.typed, target);
        p.wait_modifiers_released();
        p.backspace(back)?;
        // the shared prefix is what the screen holds, should typing fail now
        self.typed = target[..target.len() - text.len()].to_string();
        p.type_text(text)?;
        self.typed = target.to_string();
        Ok(())
    }

    /// Live mode: type `target` unless that shrinks the text or is a big
    /// correction — allowed only once nothing reached the screen for a while.
    /// True if it was typed.
    fn offer(&mut self, p: &dyn Platform, target: &str, now: Instant, o: &Opts) -> Result<bool> {
        let since = self.last.map(|t| now.saturating_duration_since(t));
        if since.is_some_and(|s| s < o.type_every) {
            return Ok(false);
        }
        let stale = since.is_none_or(|s| s > o.stale_after);
        let grows = target.chars().count() >= self.typed.chars().count();
        let small =
            target.starts_with(&self.typed) || divergence(target, &self.typed) <= MAX_CORRECTION;
        let typed = grows && (small || stale);
        if typed {
            self.update(p, target)?;
            self.last = Some(now);
        }
        Ok(typed)
    }
}

/// Run a session until `stop` is set and the audio channel is drained and
/// closed (or `flush_timeout` passed). Returns the final text.
pub fn run(
    audio: &Receiver<Vec<f32>>,
    stop: &AtomicBool,
    rec: &mut dyn Recognizer,
    platform: &dyn Platform,
    store: &Store,
    opts: &Opts,
) -> String {
    // A crash (of the daemon or of `blitznir dictate`) left words here:
    // into the history with them before this session overwrites the file.
    if let Err(e) = store.recover_recording() {
        log::error!("cannot recover recording.txt: {e}");
    }
    let mut s = Session {
        platform,
        store,
        opts,
        seg: Segmenter::new(opts.seg),
        typer: Typer::default(),
        heard: String::new(),
        typing_live: opts.live,
    };
    match s
        .listen(audio, stop, rec)
        .and_then(|tail| s.seg.finish(rec, &tail))
    {
        Ok(text) => s.finish(text),
        Err(e) => {
            log::error!("dictation failed: {e}; keeping the text committed so far");
            let kept = opts.vocab.apply(s.seg.committed().trim()).into_owned();
            s.keep(&kept);
            kept
        }
    }
}

struct Session<'a> {
    platform: &'a dyn Platform,
    store: &'a Store,
    opts: &'a Opts,
    seg: Segmenter,
    typer: Typer,
    /// The words as heard, before the vocabulary, of what is on screen.
    heard: String,
    /// Live and no typing error yet; after one, the final pass types the rest.
    typing_live: bool,
}

impl Session<'_> {
    fn offer(&mut self, heard: &str) {
        let target = self.opts.vocab.apply(heard);
        let typed = self
            .typer
            .offer(self.platform, &target, Instant::now(), self.opts);
        match typed {
            Ok(true) => self.heard = heard.to_string(),
            Ok(false) => {}
            Err(e) => {
                log::error!("typing failed: {e}; the rest of this session is typed when it ends");
                self.typing_live = false;
                self.heard = self.typer.typed.clone(); // part of a target: as typed will do
            }
        }
    }

    /// Read audio and tick the segmenter until the session ends. Returns the
    /// audio that arrived after stop, for the final decode.
    fn listen(
        &mut self,
        audio: &Receiver<Vec<f32>>,
        stop: &AtomicBool,
        rec: &mut dyn Recognizer,
    ) -> Result<Vec<f32>> {
        let mut deadline: Option<Instant> = None;
        let mut tail = vec![];
        let mut peak = 0.0_f32;
        loop {
            if stop.load(Ordering::Acquire) {
                let d = *deadline.get_or_insert_with(|| Instant::now() + self.opts.flush_timeout);
                if Instant::now() >= d {
                    // a mic that refuses to close keeps the channel open forever
                    log::warn!("flush timed out — finalizing with what we have");
                    break;
                }
            }
            // Drain EVERYTHING pending: a decode can take longer than a block.
            let mut got = match audio.recv_timeout(Duration::from_millis(100)) {
                Ok(chunk) => chunk,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => break,
            };
            got.extend(audio.try_iter().flatten());
            peak = got.iter().fold(peak, |m, x| m.max(x.abs()));
            if deadline.is_some() {
                tail.extend(got); // finishing: no more ticks, one final decode
                continue;
            }
            for out in self.seg.push(rec, &got)? {
                match out {
                    Out::Target(t) if self.typing_live => self.offer(&t),
                    Out::Target(_) => {}
                    Out::Committed(c) => self.recording(&self.opts.vocab.apply(&c)),
                }
            }
        }
        if peak < 0.0005 {
            log::warn!(
                "the microphone delivered silence — check the Microphone permission and the input device"
            );
        }
        Ok(tail)
    }

    fn finish(&mut self, heard: String) -> String {
        // The final pass corrects content; it never takes words away. If the
        // last decode (often mostly trailing silence) came back shorter than
        // what is on screen, the typed words stay, and the history gets them.
        // Both as heard: a replacement that joins words is no loss.
        let on_screen = self.heard.trim_end();
        let words = |s: &str| s.split_whitespace().count();
        let keep_screen = !on_screen.is_empty()
            && (on_screen.starts_with(&heard) || words(&heard) < words(on_screen));
        let text = if keep_screen {
            self.typer.typed.trim_end().to_string()
        } else {
            self.opts.vocab.apply(&heard).into_owned()
        };
        self.keep(&text);
        if self.opts.auto_copy
            && !text.is_empty()
            && let Err(e) = self.platform.set_clipboard(&text)
        {
            log::warn!("auto copy failed: {e}");
        }
        if self.opts.hold {
            return text; // the rewrite types it
        }
        // kept from the screen, only its trailing space goes: the screen then
        // holds exactly `text`, as many characters as a rewrite takes back
        if let Err(e) = self.typer.update(self.platform, &text) {
            log::error!("typing failed: {e}; the text is in the history");
        }
        text
    }

    /// History first; recording.txt is cleared only once the text is safe there.
    fn keep(&self, text: &str) {
        let mode = if self.opts.live { "live" } else { "batch" };
        match self.store.add_history(text, mode) {
            Ok(()) => self.recording(""),
            Err(e) => log::error!("cannot save the history: {e}; recording.txt keeps the text"),
        }
    }

    fn recording(&self, text: &str) {
        if let Err(e) = self.store.write_recording(text) {
            log::warn!("cannot write recording.txt: {e}");
        }
    }
}

#[cfg(test)]
mod tests;
