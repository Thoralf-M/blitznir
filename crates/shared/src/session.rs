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

    fn set(&self, phase: Phase, stop: Option<Arc<AtomicBool>>) {
        *lock(&self.now) = Now { phase, stop };
        (self.notify)(Note::Phase(phase));
    }

    /// A live (`live`) or batch hotkey press.
    pub fn press(self: &Arc<Self>, live: bool) {
        let mut now = lock(&self.now);
        match dictation::step(now.phase, live) {
            Step::Start { live } => {
                let stop = Arc::new(AtomicBool::new(false));
                now.phase = if live { Phase::Live } else { Phase::Batch };
                now.stop = Some(stop.clone());
                let phase = now.phase;
                drop(now);
                (self.notify)(Note::Phase(phase));
                let me = self.clone();
                let spawned = thread::Builder::new()
                    .name("dictation".into())
                    .spawn(move || me.session(live, &stop));
                if let Err(e) = spawned {
                    log::error!("cannot start the dictation thread: {e}");
                    self.set(Phase::Idle, None);
                }
            }
            Step::Stop => {
                if let Some(stop) = &now.stop {
                    stop.store(true, Ordering::Release);
                }
                now.phase = Phase::Finishing;
                drop(now);
                (self.notify)(Note::Phase(Phase::Finishing));
            }
            Step::Ignore => log::info!("hotkey ignored: {:?}", now.phase),
        }
    }

    fn session(&self, live: bool, stop: &Arc<AtomicBool>) {
        let mode = if live { "live" } else { "batch" };
        log::info!("{mode} dictation started");
        let (tx, rx) = mpsc::channel();
        let recorder = match audio::start(&self.source, tx, stop.clone()) {
            Ok(recorder) => recorder,
            Err(e) => {
                log::error!("cannot record: {e}");
                (self.notify)(Note::Flash("No microphone"));
                self.set(Phase::Idle, None);
                return;
            }
        };
        // as the recording began: the config may change before it ends
        let opts = Opts::new(live, &lock(&self.cfg));
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
