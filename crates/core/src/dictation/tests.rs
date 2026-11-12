use super::*;
use crate::config::{Config, Paths};
use crate::platform::Token;
use crate::platform::fake::{FakePlatform, FakeRecognizer, Timeline};
use crate::vocab::of as vocab;
use std::sync::mpsc::{Receiver, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

const BLOCK: usize = 800; // 50 ms, like the microphone

struct Env {
    store: Store,
    paths: Paths,
    platform: FakePlatform,
    stop: Arc<AtomicBool>,
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.paths.dir);
    }
}

fn env() -> Env {
    let paths = Paths::new(
        std::env::temp_dir().join(format!("blitznir-dict-{}", uuid::Uuid::new_v4().simple())),
    );
    let store = Store::new(paths.clone(), 4).unwrap();
    Env {
        store,
        paths,
        platform: FakePlatform::new(),
        stop: Arc::new(AtomicBool::new(false)),
    }
}

fn opts(live: bool) -> Opts {
    Opts {
        type_every: Duration::ZERO,
        ..Opts::new(live, &Config::default())
    }
}

fn words_from(text: &str, start: f64, each: f64) -> Vec<(&str, f64, f64)> {
    text.split(' ')
        .enumerate()
        .map(|(i, w)| (w, start + i as f64 * each, start + (i + 1) as f64 * each))
        .collect()
}

/// Two sentences with a long pause between them: two segments.
const SPOKEN: &str = "Hello my name is Anna. I live in Berlin.";

fn speech() -> (Timeline, Vec<f32>) {
    let mut w = words_from("Hello my name is Anna.", 0.0, 0.64);
    w.extend(words_from("I live in Berlin.", 4.8, 0.6));
    let t = Timeline::new(&w);
    let pcm = t.audio(8.5);
    (t, pcm)
}

/// A microphone at 50× real time: `pcm` in 50 ms blocks, 1 ms apart (so
/// the session sees them one by one), then stop. With `hold`, the channel
/// stays open until `hold` is dropped.
fn mic(
    pcm: Vec<f32>,
    stop: Arc<AtomicBool>,
    hold: Option<Receiver<()>>,
) -> (Receiver<Vec<f32>>, JoinHandle<Instant>) {
    let (tx, rx) = sync_channel(0);
    let feeder = std::thread::spawn(move || {
        for block in pcm.chunks(BLOCK) {
            if tx.send(block.to_vec()).is_err() {
                break; // the session ended early
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        stop.store(true, Ordering::Release);
        let stopped = Instant::now();
        if let Some(hold) = hold {
            let _ = hold.recv();
        }
        stopped
    });
    (rx, feeder)
}

fn session(e: &Env, rec: &mut FakeRecognizer, pcm: Vec<f32>, o: &Opts) -> String {
    let (rx, feeder) = mic(pcm, e.stop.clone(), None);
    let text = run(&rx, &e.stop, rec, &e.platform, &e.store, o);
    drop(rx);
    feeder.join().unwrap();
    text
}

#[test]
fn live_typing_converges_to_the_final_text() {
    let e = env();
    let calls = Arc::new(Mutex::new(0));
    let c = calls.clone();
    e.platform.on_type(move |_| {
        *c.lock().unwrap() += 1;
        Ok(())
    });
    let (t, pcm) = speech();
    let text = session(&e, &mut t.recognizer(), pcm, &opts(true));
    assert_eq!(text, SPOKEN);
    assert_eq!(e.platform.screen(), SPOKEN);
    assert!(
        *calls.lock().unwrap() > 5,
        "typed live, not once at the end"
    );
    assert_eq!(e.store.history()[0].text, SPOKEN);
    assert_eq!(e.store.history()[0].mode, "live");
}

#[test]
fn batch_types_once_after_the_history_has_it() {
    let e = env();
    let seen = Arc::new(Mutex::new(vec![]));
    let (s, history) = (seen.clone(), e.paths.history());
    e.platform.on_type(move |typed| {
        let saved = std::fs::read_to_string(&history).unwrap_or_default();
        s.lock()
            .unwrap()
            .push((typed.to_string(), saved.contains(typed)));
        Ok(())
    });
    let (t, pcm) = speech();
    let text = session(&e, &mut t.recognizer(), pcm, &opts(false));
    assert_eq!(text, SPOKEN);
    assert_eq!(*seen.lock().unwrap(), [(SPOKEN.to_string(), true)]);
    assert_eq!(e.platform.screen(), SPOKEN);
}

#[test]
fn recording_txt_holds_committed_text_and_is_cleared_after() {
    let e = env();
    let snapshots = Arc::new(Mutex::new(vec![]));
    let (s, path, t) = (snapshots.clone(), e.paths.recording(), speech().0);
    let mut rec = FakeRecognizer::new(move |pcm| {
        s.lock()
            .unwrap()
            .push(std::fs::read_to_string(&path).unwrap_or_default());
        Ok(t.tokens(pcm))
    });
    let text = session(&e, &mut rec, speech().1, &opts(true));
    assert_eq!(text, SPOKEN);
    assert!(
        snapshots
            .lock()
            .unwrap()
            .iter()
            .any(|s| s == "Hello my name is Anna "),
        "the first segment was on disk"
    );
    assert_eq!(std::fs::read_to_string(e.paths.recording()).unwrap(), "");
}

#[test]
fn the_final_pass_never_truncates_what_is_typed() {
    let e = env();
    let t = Timeline::new(&words_from(
        "one two three four five six seven eight",
        0.0,
        0.5,
    ));
    let stop = e.stop.clone();
    // after stop, the last decode drops the last three words
    let mut rec = FakeRecognizer::new(move |pcm| {
        let mut toks: Vec<Token> = t.tokens(pcm);
        if stop.load(Ordering::Acquire) {
            toks.truncate(toks.len().saturating_sub(3));
        }
        Ok(toks)
    });
    let pcm = Timeline::new(&words_from("x", 0.0, 4.0)).audio(4.2);
    let text = session(&e, &mut rec, pcm, &opts(true));
    assert!(text.starts_with("one two three four five six"), "{text:?}");
    assert_eq!(e.platform.screen(), text, "what a rewrite takes back");
    assert_eq!(e.store.history()[0].text, text);
}

#[test]
fn a_shorter_final_pass_that_differs_early_keeps_the_typed_words() {
    let e = env();
    let t = Timeline::new(&words_from(
        "one two three four five six seven eight",
        0.0,
        0.5,
    ));
    let stop = e.stop.clone();
    // after stop, the last decode drops three words and mishears the first
    let mut rec = FakeRecognizer::new(move |pcm| {
        let mut toks: Vec<Token> = t.tokens(pcm);
        if stop.load(Ordering::Acquire) {
            toks.truncate(toks.len().saturating_sub(3));
            if let Some(first) = toks.first_mut() {
                first.text = " won".into();
            }
        }
        Ok(toks)
    });
    let pcm = Timeline::new(&words_from("x", 0.0, 4.0)).audio(4.2);
    let text = session(&e, &mut rec, pcm, &opts(true));
    assert!(text.starts_with("one two three four five six"), "{text:?}");
    assert_eq!(e.platform.screen(), text, "what a rewrite takes back");
    assert_eq!(e.store.history()[0].text, text);
}

#[test]
fn a_typing_error_mid_live_leaves_the_rest_to_the_final_pass() {
    let e = env();
    let calls = Arc::new(Mutex::new(0));
    let c = calls.clone();
    e.platform.on_type(move |_| {
        let mut n = c.lock().unwrap();
        *n += 1;
        if *n == 3 {
            return Err(crate::Error::Platform("the app went away".into()));
        }
        Ok(())
    });
    let (t, pcm) = speech();
    let o = Opts {
        auto_copy: true,
        ..opts(true)
    };
    let text = session(&e, &mut t.recognizer(), pcm, &o);
    assert_eq!(text, SPOKEN);
    assert_eq!(e.store.history()[0].text, SPOKEN);
    assert_eq!(e.platform.screen(), SPOKEN, "the final pass typed the rest");
    assert_eq!(
        *calls.lock().unwrap(),
        4,
        "two live, the failed one, one at the end"
    );
    assert_eq!(e.platform.clipboard(), SPOKEN);
}

#[test]
fn a_crashed_sessions_words_are_kept_before_the_next_one_starts() {
    let e = env();
    e.store
        .write_recording("words from before the crash ")
        .unwrap();
    let (t, pcm) = speech();
    session(&e, &mut t.recognizer(), pcm, &opts(false));
    let h = e.store.history();
    assert_eq!(h[0].text, SPOKEN);
    assert_eq!(
        (h[1].text.as_str(), h[1].mode.as_str()),
        ("words from before the crash", "recovered")
    );
}

#[test]
fn a_stuck_flush_finalizes_with_what_it_has() {
    let e = env();
    let (t, pcm) = speech();
    let (hold_tx, hold_rx) = sync_channel::<()>(0);
    let (rx, feeder) = mic(pcm, e.stop.clone(), Some(hold_rx));
    let o = Opts {
        flush_timeout: Duration::from_millis(300),
        ..opts(true)
    };
    let text = run(&rx, &e.stop, &mut t.recognizer(), &e.platform, &e.store, &o);
    let returned = Instant::now();
    drop(hold_tx);
    let waited = returned - feeder.join().unwrap();
    assert!(
        waited >= Duration::from_millis(250) && waited < Duration::from_secs(10),
        "{waited:?}"
    );
    assert_eq!(text, SPOKEN);
    assert_eq!(e.platform.screen(), SPOKEN);
}

#[test]
fn a_failing_recognizer_keeps_the_committed_text() {
    let e = env();
    let t = speech().0;
    let mut rec = FakeRecognizer::new(move |pcm| {
        if Timeline::offset(pcm) > 1.0 && pcm.len() > 40_000 {
            return Err(crate::Error::Stt("model crashed".into()));
        }
        Ok(t.tokens(pcm))
    });
    let text = session(&e, &mut rec, speech().1, &opts(true));
    assert_eq!(text, "Hello my name is Anna");
    assert_eq!(e.store.history()[0].text, "Hello my name is Anna");
    assert_eq!(std::fs::read_to_string(e.paths.recording()).unwrap(), "");
}

#[test]
fn live_corrections_stay_small_unless_the_screen_went_stale() {
    let (p, o, t0) = (
        FakePlatform::new(),
        Opts {
            type_every: Duration::from_millis(400),
            ..opts(true)
        },
        Instant::now(),
    );
    let at = |ms| t0 + Duration::from_millis(ms);
    let mut ty = Typer::default();
    let first = "Hello world, this is a long sentence";
    ty.offer(&p, first, at(0), &o).unwrap();
    ty.offer(&p, "Hello world, this is a long sentence and", at(100), &o)
        .unwrap(); // too soon
    assert_eq!(p.screen(), first);
    let fixed = "Hello world, this is a long sentinel";
    ty.offer(&p, fixed, at(500), &o).unwrap(); // 4 chars of backspace
    assert_eq!(p.screen(), fixed);
    ty.offer(&p, "Hello", at(1000), &o).unwrap(); // never shrinks
    assert_eq!(p.screen(), fixed);
    let rewrite = "Hi there, this is a completely different sentence";
    ty.offer(&p, rewrite, at(1500), &o).unwrap(); // 35 chars of backspace
    assert_eq!(p.screen(), fixed);
    ty.offer(&p, rewrite, at(2600), &o).unwrap(); // nothing typed for 2 s: allowed
    assert_eq!(p.screen(), rewrite);
}

#[test]
fn either_hotkey_stops_and_nothing_starts_while_busy() {
    for live in [true, false] {
        assert_eq!(step(Phase::Idle, live), Step::Start { live });
        assert_eq!(step(Phase::Live, live), Step::Stop);
        assert_eq!(step(Phase::Batch, live), Step::Stop);
        assert_eq!(step(Phase::Loading, live), Step::Ignore);
        assert_eq!(step(Phase::Finishing, live), Step::Ignore);
    }
}

mod vocab;
