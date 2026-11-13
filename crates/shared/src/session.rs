//! The dictation owner: turns live/batch hotkey presses into sessions of
//! `core::dictation` on their own thread. The tray daemon and
//! `blitznir dictate` both drive this, so the CLI tests the hotkey path.

use crate::audio::{self, Recorder, Source};
use blitznir_core::{
    config::Config,
    dictation::{self, Opts, Phase, Step},
    lock,
    platform::{Platform, Recognizer},
    store::Store,
};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

/// How long a finished session waits for the audio thread to let go of the
/// microphone. The phase is Idle by then; this only decides whether the
/// "Microphone silent" check still gets its numbers.
const MIC_RELEASE: Duration = Duration::from_secs(2);

/// How long a tray message replaces the state title.
pub const FLASH: Duration = Duration::from_secs(3);
/// Menu labels of recent dictations are cut here.
const LABEL_CHARS: usize = 40;

/// The tray title of each phase (both shells show the same words).
pub fn title(phase: Phase) -> &'static str {
    match phase {
        Phase::Loading => "Loading model…",
        Phase::Idle => "",
        Phase::Live => "Listening…",
        Phase::Batch => "Recording…",
        Phase::Finishing => "Finishing…",
    }
}

/// A recent dictation as a menu item: its first line, cut at [`LABEL_CHARS`].
pub fn label(text: &str) -> String {
    let line = text.lines().next().unwrap_or_default();
    let mut s: String = line.chars().take(LABEL_CHARS).collect();
    if s.len() < text.len() {
        s.push('…');
    }
    s
}

/// What the UI hears about.
#[derive(Debug, Clone, PartialEq)]
pub enum Note {
    Phase(Phase),
    /// A short message for the tray.
    Flash(&'static str),
    /// A session ended with this text (already in the history); true: it
    /// is on the screen (live, or batch not held back for a rewrite).
    Dictated(String, bool),
}

type Notify = Box<dyn Fn(Note) + Send + Sync>;

struct Now {
    phase: Phase,
    /// The running session's stop flag.
    stop: Option<Arc<AtomicBool>>,
}

pub struct Dictator {
    platform: Arc<dyn Platform>,
    store: Arc<Store>,
    /// Replaced when config.toml is saved; a session reads it when it starts.
    cfg: Mutex<Config>,
    source: Source,
    rec: Mutex<Option<Box<dyn Recognizer>>>,
    now: Mutex<Now>,
    notify: Notify,
    /// When a session's live text last gained a word (`Opts::words`).
    words: Arc<Mutex<Option<std::time::Instant>>>,
    /// The stop phrase that ended the running session, if one did (`Opts::stop`).
    said_stop: Arc<Mutex<(String, bool)>>,
    /// The wake phrase as the transcript that caught it wrote it.
    heard_head: Mutex<String>,
    /// The running session's live text (`Opts::text`).
    live: Arc<Mutex<String>>,
}

impl Dictator {
    /// Starts in [`Phase::Loading`]; [`Dictator::ready`] hands over the model.
    pub fn new(
        platform: Arc<dyn Platform>,
        store: Arc<Store>,
        cfg: Config,
        source: Source,
        notify: impl Fn(Note) + Send + Sync + 'static,
    ) -> Arc<Dictator> {
        Arc::new(Dictator {
            platform,
            store,
            cfg: Mutex::new(cfg),
            source,
            rec: Mutex::new(None),
            words: Arc::default(),
            said_stop: Arc::default(),
            heard_head: Mutex::default(),
            live: Arc::default(),
            now: Mutex::new(Now {
                phase: Phase::Loading,
                stop: None,
            }),
            notify: Box::new(notify),
        })
    }

    pub fn ready(&self, rec: Box<dyn Recognizer>) {
        *lock(&self.rec) = Some(rec);
        self.set(Phase::Idle, None);
    }

    pub fn set_config(&self, cfg: Config) {
        *lock(&self.cfg) = cfg;
    }

    pub fn phase(&self) -> Phase {
        lock(&self.now).phase
    }

    /// The running session ends because `phrase`, the stop phrase, was
    /// said: it is taken off the text (call before the press that stops it).
    pub fn stopped_by(&self, phrase: &str, command: bool) {
        *lock(&self.said_stop) = (phrase.to_string(), command);
    }

    /// Starts a live session from `feed` if none runs: checked and started
    /// under one lock, so a hotkey press between them cannot be stopped
    /// instead. False: one ran (or the model is not loaded).
    /// `heard`: the wake phrase as its transcript wrote it, taken off too.
    pub fn start_if_idle(self: &Arc<Self>, feed: mpsc::Receiver<Vec<f32>>, heard: &str) -> bool {
        *lock(&self.heard_head) = heard.to_string();
        self.step(true, Some(feed), true)
    }

    /// The running session's live text, as heard.
    pub fn live_text(&self) -> String {
        lock(&self.live).clone()
    }

    /// When the running session's live text last gained a word.
    pub fn words_at(&self) -> Option<std::time::Instant> {
        *lock(&self.words)
    }

    /// The loaded model's text for `pcm` while no dictation uses it; None:
    /// one does, or none is loaded (hands-free's check of a wake phrase).
    pub fn transcribe(&self, pcm: &[f32]) -> Option<String> {
        if self.phase() != Phase::Idle {
            return None;
        }
        // a session that just took the model must not be waited for
        let mut rec = self.rec.try_lock().ok()?;
        let tokens = rec.as_mut()?.transcribe(pcm).ok()?;
        Some(
            tokens
                .iter()
                .map(|t| t.text.as_str())
                .collect::<String>()
                .trim()
                .to_string(),
        )
    }

    fn set(&self, phase: Phase, stop: Option<Arc<AtomicBool>>) {
        *lock(&self.now) = Now { phase, stop };
        (self.notify)(Note::Phase(phase));
    }

    /// A live (`live`) or batch hotkey press.
    pub fn press(self: &Arc<Self>, live: bool) {
        self.press_from(live, None);
    }

    /// As [`Dictator::press`]; a session it starts records from `feed`
    /// (hands-free's listener) instead of opening the microphone.
    pub fn press_from(self: &Arc<Self>, live: bool, feed: Option<mpsc::Receiver<Vec<f32>>>) {
        self.step(live, feed, false);
    }

    /// A press; `start_only`: one that would stop a session does nothing.
    /// True: a session started.
    fn step(
        self: &Arc<Self>,
        live: bool,
        feed: Option<mpsc::Receiver<Vec<f32>>>,
        start_only: bool,
    ) -> bool {
        let mut now = lock(&self.now);
        match dictation::step(now.phase, live) {
            Step::Stop | Step::Ignore if start_only => false,
            Step::Start { live } => {
                // before the note: hands-free reads them as soon as it hears of the start
                lock(&self.words).take();
                lock(&self.live).clear();
                *lock(&self.said_stop) = Default::default();
                let stop = Arc::new(AtomicBool::new(false));
                now.phase = if live { Phase::Live } else { Phase::Batch };
                now.stop = Some(stop.clone());
                let phase = now.phase;
                drop(now);
                (self.notify)(Note::Phase(phase));
                let me = self.clone();
                let spawned = thread::Builder::new()
                    .name("dictation".into())
                    .spawn(move || me.session(live, &stop, feed));
                if let Err(e) = spawned {
                    log::error!("cannot start the dictation thread: {e}");
                    self.set(Phase::Idle, None);
                    return false;
                }
                true
            }
            Step::Stop => {
                if let Some(stop) = &now.stop {
                    stop.store(true, Ordering::Release);
                }
                now.phase = Phase::Finishing;
                drop(now);
                (self.notify)(Note::Phase(Phase::Finishing));
                false
            }
            Step::Ignore => {
                log::info!("hotkey ignored: {:?}", now.phase);
                false
            }
        }
    }

    fn session(&self, live: bool, stop: &Arc<AtomicBool>, feed: Option<mpsc::Receiver<Vec<f32>>>) {
        let mode = if live { "live" } else { "batch" };
        log::info!("{mode} dictation started");
        let (tx, rx) = mpsc::channel();
        let woken = feed.is_some();
        let recording = match feed {
            Some(feed) => audio::forward(feed, tx, stop.clone()),
            None => audio::start(&self.source, tx, stop.clone()),
        };
        let recorder = match recording {
            Ok(recorder) => recorder,
            Err(e) => {
                log::error!("cannot record: {e}");
                (self.notify)(Note::Flash("No microphone"));
                self.set(Phase::Idle, None);
                return;
            }
        };
        // as the recording began: the config may change before it ends
        let mut opts = Opts::new(live, &lock(&self.cfg));
        if woken {
            let w = lock(&self.cfg).wake.clone();
            // it heard the phrase's end, maybe as the transcript did
            let heard = std::mem::take(&mut *lock(&self.heard_head));
            opts.head = match heard.is_empty() {
                true => w.wake,
                false => format!("{}, {heard}", w.wake),
            };
            opts.tail = w.stop;
        }
        opts.words = self.words.clone();
        opts.stop = self.said_stop.clone();
        opts.text = self.live.clone();
        let text = match lock(&self.rec).as_mut() {
            Some(rec) => {
                dictation::run(&rx, stop, rec.as_mut(), &*self.platform, &self.store, &opts)
            }
            None => String::new(), // step() starts nothing before ready()
        };
        drop(rx);
        stop.store(true, Ordering::Release); // a flush timeout ends it too
        self.wrap_up((mode, !opts.hold), text, recorder, MIC_RELEASE);
    }

    /// Idle first, then the audio thread: one stuck in the driver must not
    /// leave the app in Finishing, where every hotkey is ignored.
    fn wrap_up(
        &self,
        (mode, shown): (&str, bool),
        text: String,
        recorder: Recorder,
        within: Duration,
    ) {
        self.set(Phase::Idle, None);
        log::info!("{mode} dictation done: {} chars", text.chars().count());
        (self.notify)(Note::Dictated(text, shown));
        match recorder.finish(within) {
            Some(stats) if stats.silent() => (self.notify)(Note::Flash("Microphone silent")), // core logged why
            Some(_) => {}
            None => log::warn!("the audio thread did not stop within {within:?}; left behind"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::Stats;
    use blitznir_core::{config::Paths, platform::fake::FakePlatform};
    use std::time::Instant;

    #[test]
    fn a_stuck_audio_thread_never_keeps_the_hotkeys_dead() {
        let dir = std::env::temp_dir().join(format!("blitznir-session-{}", std::process::id()));
        let store = Arc::new(Store::new(Paths::new(&dir), 10).unwrap());
        let (tx, notes) = mpsc::channel();
        let d = Dictator::new(
            Arc::new(FakePlatform::new()),
            store,
            Config::default(),
            Source::Mic,
            move |n| drop(tx.send(n)),
        );
        d.set(Phase::Finishing, None);
        // a microphone thread that never returns until the test ends
        let (release, stuck) = mpsc::channel::<()>();
        let recorder = Recorder(thread::spawn(move || {
            let _ = stuck.recv();
            Stats::default()
        }));
        let t0 = Instant::now();
        d.wrap_up(
            ("live", true),
            "Hallo.".into(),
            recorder,
            Duration::from_millis(100),
        );
        assert!(
            t0.elapsed() < Duration::from_secs(5),
            "never waits on the thread"
        );
        assert_eq!(d.phase(), Phase::Idle);
        assert_eq!(dictation::step(d.phase(), true), Step::Start { live: true });
        let got: Vec<Note> = notes.try_iter().collect();
        let want = [
            Note::Phase(Phase::Finishing),
            Note::Phase(Phase::Idle),
            Note::Dictated("Hallo.".into(), true),
        ];
        assert_eq!(got, want);
        drop(release);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_silent_microphone_is_still_reported() {
        let dir = std::env::temp_dir().join(format!("blitznir-silent-{}", std::process::id()));
        let store = Arc::new(Store::new(Paths::new(&dir), 10).unwrap());
        let (tx, notes) = mpsc::channel();
        let d = Dictator::new(
            Arc::new(FakePlatform::new()),
            store,
            Config::default(),
            Source::Mic,
            move |n| drop(tx.send(n)),
        );
        let zeros = Stats {
            samples: 16_000,
            ..Stats::default()
        };
        let recorder = Recorder(thread::spawn(move || zeros));
        d.wrap_up(
            ("batch", true),
            String::new(),
            recorder,
            Duration::from_secs(2),
        );
        let got: Vec<Note> = notes.try_iter().collect();
        assert_eq!(got.last(), Some(&Note::Flash("Microphone silent")));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
