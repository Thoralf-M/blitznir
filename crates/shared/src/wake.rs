//! `[wake]`'s ear: a small keyword spotter (k2-fsa's sherpa-onnx, a 3.3 M
//! parameter streaming zipformer trained on GigaSpeech) on a microphone
//! stream of its own, hearing all of it (under 1 % of a core); and its
//! download. What a hit means is core's wake.rs.

use crate::audio::{self, Source};
use blitznir_core::{
    Error, Result,
    config::{Paths, Wake},
    segmenter::SAMPLE_RATE,
    wake::{self, Kind},
};
use sherpa_onnx::{KeywordSpotter, KeywordSpotterConfig};
use std::{
    collections::VecDeque,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::Duration,
};

/// k2-fsa's release, pinned by size and SHA-256.
const URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/kws-models/sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01.tar.bz2";
const SIZE: u64 = 17_626_723;
const SHA256: &str = "f170013b4716e41b62b9bfd809687c207cef798ef9bc6534d524e17af9b6561a";
const IN_TAR: &str = "sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01/";
/// The int8 graphs (5 MB together) and the token table: all the spotter reads.
const FILES: [&str; 4] = [
    "encoder-epoch-12-avg-2-chunk-16-left-64.int8.onnx",
    "decoder-epoch-12-avg-2-chunk-16-left-64.int8.onnx",
    "joiner-epoch-12-avg-2-chunk-16-left-64.int8.onnx",
    "tokens.txt",
];

pub fn model_dir(paths: &Paths) -> PathBuf {
    paths.models().join("kws-gigaspeech-3.3m")
}

pub fn model_present(dir: &Path) -> bool {
    FILES
        .iter()
        .all(|f| std::fs::metadata(dir.join(f)).is_ok_and(|m| m.len() > 0))
}

/// Download the release once, check it, and keep only [`FILES`].
pub fn ensure_model(dir: &Path) -> Result<()> {
    if model_present(dir) {
        return Ok(());
    }
    std::fs::create_dir_all(dir)?;
    let tarball = dir.join("model.tar.bz2");
    crate::stt::download(URL, &tarball, SIZE, &mut |_| {})?;
    let bytes = std::fs::read(&tarball)?;
    let _ = std::fs::remove_file(&tarball);
    let sum = ring::digest::digest(&ring::digest::SHA256, &bytes);
    let hex: String = sum.as_ref().iter().map(|b| format!("{b:02x}")).collect();
    if hex != SHA256 {
        return Err(Error::Stt(format!(
            "{URL}: SHA-256 {hex}, expected {SHA256}"
        )));
    }
    let mut tar = tar::Archive::new(bzip2::read::BzDecoder::new(&bytes[..]));
    for entry in tar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_string_lossy().into_owned();
        let Some(name) = path.strip_prefix(IN_TAR).filter(|n| FILES.contains(n)) else {
            continue;
        };
        let mut data = vec![];
        entry.read_to_end(&mut data)?;
        // a write cut short must not pass for the model: .part, then renamed
        let part = dir.join(format!("{name}.part"));
        std::fs::write(&part, &data)?;
        std::fs::rename(&part, dir.join(name))?;
    }
    match model_present(dir) {
        true => Ok(()),
        false => Err(Error::Stt(format!("{URL}: files missing from the release"))),
    }
}

/// What the ear hears.
#[derive(Debug)]
pub enum Heard {
    /// A phrase, the last [`CLIP`] of audio up to it (for the check), and
    /// for a wake phrase every frame from it on (for the dictation it
    /// starts; dropped, the ear stops sending).
    Hit(Kind, Vec<f32>, Option<Receiver<Vec<f32>>>),
    /// 50 ms of audio, loud or not (the listener times a send's quiet by them).
    Block(bool),
    /// A short utterance between pauses (a phrase said on its own), or any
    /// while a send phrase could follow: its last [`CLIP`], for the
    /// transcript; for a short one, every frame from it on, for the
    /// dictation it may start.
    Utterance(Vec<f32>, Option<Receiver<Vec<f32>>>),
}

/// What a wake phrase's check hears: the phrase and a breath around it.
pub const CLIP: usize = SAMPLE_RATE * 5 / 2;
/// The dictation a wake phrase starts hears from this long before the
/// spotter fired: it fires up to a second after the phrase ("hey blitz,
/// write a short note" lost "write a"). What it hears of the phrase is
/// taken off (core's `wake::strip_head`).
const BEFORE_HIT: usize = SAMPLE_RATE * 4 / 5;
/// An utterance ends after this many quiet 50 ms frames (0.5 s).
const UTTERANCE_END: usize = 10;
/// 50 ms: what `[dictation] silence_rms` is measured over.
const FRAME: usize = SAMPLE_RATE / 20;

/// Speech is loud from half `[dictation] silence_rms` up: Phonon through a
/// MacBook's speaker, across a desk, put 25 % of its 50 ms frames over
/// 0.01 and 40 % over 0.005, a quiet room none over 0.0022. Over 0.01 only,
/// a sentence could seem a pause and a hands-free dictation end mid-word.
const LOUD: f32 = 0.5;

/// The utterance going on: its loud frames, all its frames from the first
/// loud one, and the quiet ones since the last.
#[derive(Default)]
struct Speaking {
    loud: usize,
    long: usize,
    since: usize,
}

impl Speaking {
    /// A frame; Some(frames): an utterance that long (and a little before)
    /// just ended in a pause. Counted by its loud frames alone, a quiet
    /// "blitz send" was a clip of silence and an empty transcript.
    fn frame(&mut self, loud: bool) -> Option<usize> {
        if loud {
            (self.loud, self.since) = (self.loud + 1, 0);
        } else if self.loud > 0 {
            self.since += 1;
        }
        if self.loud > 0 {
            self.long += 1;
        }
        if self.loud == 0 || self.since < UTTERANCE_END {
            return None;
        }
        // 0.2 s of speech at least: a click or a cough is not one
        let ended = (self.loud >= 4).then_some(self.long + LEAD);
        *self = Speaking::default();
        ended
    }
}

/// An utterance this short (2.5 s) may be a wake phrase said on its own:
/// the speech model hears it (the spotter missed a speaker's clear "hey
/// blitz" five times in a row; the model wrote it right four times).
const SHORT: usize = CLIP / FRAME;
/// Frames an utterance's clip starts before its first loud one (0.3 s).
const LEAD: usize = 6;

fn rms(frame: &[f32]) -> f32 {
    (frame.iter().map(|x| x * x).sum::<f32>() / frame.len().max(1) as f32).sqrt()
}

/// The ear's loop: 50 ms frames, each reported loud or not, all of them
/// to the spotter. Fed only while loud (a gate to save CPU it hardly
/// needs: real time × 0.008), a speaker's normal "hey blitz" came through
/// in pieces and was heard only said very clearly.
fn hear(
    kws: &KeywordSpotter,
    rx: Receiver<Vec<f32>>,
    silence_rms: f32,
    heard: &Sender<Heard>,
    armed: &AtomicBool,
) {
    let stream = kws.create_stream();
    let rate = SAMPLE_RATE as i32;
    let mut pending = Vec::new();
    let mut recent = VecDeque::with_capacity(CLIP + FRAME);
    // a wake phrase's frames on to the dictation it starts
    let mut tap = None::<Sender<Vec<f32>>>;
    let mut speaking = Speaking::default();
    for block in rx {
        pending.extend(block);
        while pending.len() >= FRAME {
            let frame: Vec<f32> = pending.drain(..FRAME).collect();
            if tap.as_ref().is_some_and(|t| t.send(frame.clone()).is_err()) {
                tap = None; // that dictation ended, or never started
            }
            recent.extend(frame.iter().copied());
            recent.drain(..recent.len().saturating_sub(CLIP));
            let loud = rms(&frame) >= silence_rms * LOUD;
            if heard.send(Heard::Block(loud)).is_err() {
                return;
            }
            let ended = speaking.frame(loud);
            if let Some(n) = ended.filter(|n| *n <= SHORT || armed.load(Ordering::Acquire)) {
                let n = (n * FRAME).min(recent.len());
                let clip: Vec<f32> = recent.range(recent.len() - n..).copied().collect();
                // the dictation it may start hears it too: "hey blitz, write a
                // note" in one breath lost "write a note" (the phrase comes off
                // the text: core's `wake::strip_head`)
                let feed = (n <= SHORT * FRAME && tap.is_none()).then(|| {
                    let (tx, rx) = mpsc::channel();
                    let _ = tx.send(clip.clone());
                    tap = Some(tx);
                    rx
                });
                let _ = heard.send(Heard::Utterance(clip, feed));
            }
            stream.accept_waveform(rate, &frame);
            while kws.is_ready(&stream) {
                kws.decode(&stream);
                let Some(hit) = kws.get_result(&stream) else {
                    continue;
                };
                if let Some(kind) = Kind::of(&hit.keyword) {
                    log::info!("wake: heard {kind:?}");
                    let clip = recent.iter().copied().collect();
                    // one at a time: a new one would cut the running dictation's
                    let feed = (kind == Kind::Wake && tap.is_none()).then(|| {
                        let (tx, rx) = mpsc::channel();
                        let before = recent.len().saturating_sub(BEFORE_HIT);
                        let _ = tx.send(recent.range(before..).copied().collect());
                        tap = Some(tx);
                        rx
                    });
                    let _ = heard.send(Heard::Hit(kind, clip, feed));
                    kws.reset(&stream);
                }
            }
        }
    }
}

/// A running ear; dropping it closes its microphone.
pub struct Ear {
    stop: Arc<AtomicBool>,
}

impl Drop for Ear {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

fn spotter(dir: &Path, w: &Wake) -> Result<KeywordSpotter> {
    let file = |f: &str| Some(dir.join(f).to_string_lossy().into_owned());
    let mut c = KeywordSpotterConfig::default();
    c.model_config.transducer.encoder = file(FILES[0]);
    c.model_config.transducer.decoder = file(FILES[1]);
    c.model_config.transducer.joiner = file(FILES[2]);
    c.model_config.tokens = file(FILES[3]);
    c.model_config.num_threads = 1;
    c.keywords_buf = Some(wake::keywords(w).map_err(Error::Config)?);
    KeywordSpotter::create(&c).ok_or_else(|| Error::Stt("the keyword spotter did not load".into()))
}

/// Listen for `w`'s phrases with the model in `dir`; what is heard goes
/// to `heard`. Fails when the model or the microphone does not open.
/// `armed`: a send phrase could follow, so utterances go out too.
pub fn listen(
    dir: &Path,
    source: &Source,
    w: &Wake,
    silence_rms: f32,
    heard: Sender<Heard>,
    armed: Arc<AtomicBool>,
) -> Result<Ear> {
    let stop = Arc::new(AtomicBool::new(false));
    let (ready_tx, ready) = mpsc::sync_channel::<Result<()>>(1);
    let phrase = w.wake.clone();
    let (dir, w, flag, source) = (dir.to_path_buf(), w.clone(), stop.clone(), source.clone());
    let run = move || {
        // made on this thread: the spotter's handle is not shared
        let kws = match spotter(&dir, &w) {
            Ok(k) => k,
            Err(e) => return drop(ready_tx.send(Err(e))),
        };
        let (tx, rx) = mpsc::channel();
        let recorder = match audio::start(&source, tx, flag.clone()) {
            Ok(r) => r,
            Err(e) => return drop(ready_tx.send(Err(e))),
        };
        let _ = ready_tx.send(Ok(()));
        hear(&kws, rx, silence_rms, &heard, &armed);
        flag.store(true, Ordering::Release);
        drop(recorder.finish(Duration::from_secs(2)));
    };
    thread::Builder::new().name("wake".into()).spawn(run)?;
    ready
        .recv()
        .map_err(|_| Error::Stt("the listener thread ended".into()))??;
    log::info!("wake: listening for {phrase:?}");
    Ok(Ear { stop })
}
