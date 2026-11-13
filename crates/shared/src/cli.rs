//! The command line. Output goes to stdout, log lines to stderr.

use crate::{
    audio::{self, Source},
    hands::Hands,
    platform,
    session::{Dictator, Note},
    stt,
};
use blitznir_core::{
    Error, Result,
    config::{Config, Paths, SttModel},
    dictation::Phase,
    platform::{Platform, Recognizer, fake::FakePlatform},
    segmenter::{SAMPLE_RATE, SegConfig, Segmenter},
    store::Store,
    vocab::Vocab,
};
use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};

const USAGE: &str = "usage: blitznir            the tray app (hotkeys, dictation)
       blitznir --check    data dir, config, model, permissions, microphone
       blitznir transcribe <file.wav> [--model fp32|int8] [--raw]
         16 kHz WAV → text, the way batch dictation would type it
         (--raw: one decode of the whole file, no segmenting)
       blitznir dictate --mode live|batch [--seconds N] [--wav <file.wav>]
         the hotkey's dictation, started and stopped from here: after a
         3 s countdown it types into the focused app. --wav replays a
         16 kHz file in real time instead of the microphone.
       blitznir listen [--seconds N] [--wav FILE] [--type]
         hands-free, typing nothing: listens N s (default 60) for [wake]'s
         phrases, on whatever `enabled` says, dictates into a text kept in
         memory, and prints what it would have typed and each Return;
         --wav hears a recording instead of the microphone; --type types
         into the focused app and presses its Return for real.
       blitznir --help     this text

Every command but --help writes the default config.toml if there is none;
transcribe and dictate first download the speech model if it is missing
(fp32 2.6 GB, int8 670 MB, from Hugging Face).

BLITZNIR_DATA_DIR=<dir> keeps everything in <dir> instead of the usual
data folder; BLITZNIR_OPEN_PANEL=<text> starts the tray app with its
panel open on <text> as the selection; RUST_LOG=debug logs more.";

/// Seconds `dictate` records from the microphone without --seconds.
const DEFAULT_SECONDS: f32 = 10.0;
/// How long the microphone check records.
const PROBE_S: f32 = 0.6;

pub fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("--help" | "-h") => out(USAGE),
        Some("--check") if args.len() == 1 => check(),
        Some("transcribe") => transcribe(&args[1..]),
        Some("dictate") => dictate(&args[1..]),
        Some("listen") => listen(&args[1..]),
        _ => Err(usage()),
    }
}

fn usage() -> Error {
    Error::Config(USAGE.into())
}

fn out(line: &str) -> Result<()> {
    Ok(writeln!(std::io::stdout().lock(), "{line}")?)
}

fn check() -> Result<()> {
    let paths = Paths::user()?;
    out(&format!("data dir     {}", paths.dir.display()))?;
    let cfg = Config::load(&paths);
    let parsed = cfg.as_ref().map_or_else(Error::detailed, |_| "ok".into());
    out(&format!(
        "config       {} ({parsed})",
        paths.config().display()
    ))?;
    let model = cfg.map(|c| c.dictation.model).unwrap_or(SttModel::Fp32);
    let dir = stt::model_dir(&paths, model);
    let have = if stt::model_present(&dir, model) {
        "present"
    } else {
        "missing: downloaded on first start"
    };
    out(&format!("model        {} ({have})", dir.display()))?;
    let missing = platform::current().missing_permissions();
    for name in platform::PERMISSIONS {
        let state = if missing.contains(name) {
            "MISSING"
        } else {
            "granted"
        };
        out(&format!("permission   {name}: {state}"))?;
    }
    let mic = match audio::probe(PROBE_S) {
        Ok(s) if s.samples == 0 => format!("{} opens but delivered no audio", s.device),
        Ok(s) if s.silent() => format!(
            "{} opens but delivered only silence: Microphone permission?",
            s.device
        ),
        Ok(s) => format!(
            "{} opens: {PROBE_S} s → {} samples at 16 kHz, peak {:.4}",
            s.device, s.samples, s.peak
        ),
        Err(e) => format!("does not open: {e}"),
    };
    out(&format!("microphone   {mic}"))
}

fn model_arg(v: Option<&String>) -> Result<SttModel> {
    match v.map(String::as_str) {
        Some("fp32") => Ok(SttModel::Fp32),
        Some("int8") => Ok(SttModel::Int8),
        _ => Err(usage()),
    }
}

fn load(paths: &Paths, model: SttModel) -> Result<stt::Parakeet> {
    let dir = stt::model_dir(paths, model);
    stt::ensure_model(&dir, model, &mut |pct| {
        if pct % 10 == 0 {
            log::info!("… {pct}%");
        }
    })?;
    stt::Parakeet::load(&dir)
}

fn transcribe(args: &[String]) -> Result<()> {
    let (mut wav, mut model, mut raw) = (None, None, false);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--model" => model = Some(model_arg(it.next())?),
            "--raw" => raw = true,
            file if wav.is_none() && !file.starts_with('-') => wav = Some(PathBuf::from(file)),
            _ => return Err(usage()),
        }
    }
    let wav = wav.ok_or_else(usage)?;
    let paths = Paths::user()?;
    let cfg = Config::load(&paths)?;
    let model = model.unwrap_or(cfg.dictation.model);
    let pcm = audio::read_wav(&wav)?;
    let mut rec = load(&paths, model)?;
    let started = Instant::now();
    let text = if raw {
        let tokens = rec.transcribe(&pcm)?;
        tokens
            .iter()
            .map(|t| t.text.as_str())
            .collect::<String>()
            .trim()
            .to_string()
    } else {
        let text = segmented(&mut rec, &pcm, SegConfig::from(&cfg.dictation))?;
        let vocab = Vocab::new(&cfg.vocabulary).map_err(Error::Config)?;
        vocab.apply(&text).into_owned()
    };
    let audio_s = pcm.len() as f32 / SAMPLE_RATE as f32;
    log::info!(
        "{} {audio_s:.1} s of audio in {:.1} s",
        model.as_str(),
        started.elapsed().as_secs_f32()
    );
    out(&text)
}

/// Feed the file like a microphone would, in 50 ms blocks.
fn segmented(rec: &mut dyn Recognizer, pcm: &[f32], cfg: SegConfig) -> Result<String> {
    let mut seg = Segmenter::new(cfg);
    for block in pcm.chunks(SAMPLE_RATE / 20) {
        seg.push(rec, block)?;
    }
    seg.finish(rec, &[])
}

struct DictateArgs {
    live: bool,
    seconds: Option<f32>,
    wav: Option<PathBuf>,
}

fn dictate_args(args: &[String]) -> Result<DictateArgs> {
    let (mut live, mut seconds, mut wav) = (None, None, None);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let value = it.next().map(String::as_str);
        match (a.as_str(), value) {
            ("--mode", Some("live")) => live = Some(true),
            ("--mode", Some("batch")) => live = Some(false),
            // a Duration of a negative, NaN or endless count panics
            ("--seconds", Some(n)) => match n.parse::<f32>() {
                Ok(s) if s.is_finite() && s >= 0.0 => seconds = Some(s),
                _ => return Err(usage()),
            },
            ("--wav", Some(f)) => wav = Some(PathBuf::from(f)),
            _ => return Err(usage()),
        }
    }
    Ok(DictateArgs {
        live: live.ok_or_else(usage)?,
        seconds,
        wav,
    })
}

/// Start a session like the hotkey does, wait, stop it like the hotkey does.
fn dictate(args: &[String]) -> Result<()> {
    let args = dictate_args(args)?;
    let paths = Paths::user()?;
    let cfg = Config::load(&paths)?;
    let (source, seconds) = match &args.wav {
        Some(file) => {
            let pcm = audio::read_wav(Path::new(file))?;
            let len = pcm.len() as f32 / SAMPLE_RATE as f32;
            // the replay hands over its last block up to one block late
            (
                Source::Wav(Arc::new(pcm)),
                args.seconds.unwrap_or(len + 0.8),
            )
        }
        None => (Source::Mic, args.seconds.unwrap_or(DEFAULT_SECONDS)),
    };
    let rec = load(&paths, cfg.dictation.model)?;
    let store = Arc::new(Store::new(paths, cfg.dictation.history_max)?);
    let (tx, notes) = mpsc::channel();
    let d = Dictator::new(platform::current(), store, cfg, source, move |n| {
        log::info!("{n:?}");
        let _ = tx.send(n);
    });
    d.ready(Box::new(rec));
    for n in (1..=3).rev() {
        log::info!("typing into the focused app in {n}…");
        std::thread::sleep(Duration::from_secs(1));
    }
    d.press(args.live);
    std::thread::sleep(Duration::from_secs_f32(seconds));
    d.press(args.live);
    // the final decode and typing; the flush gives up after 30 s by itself
    let mut deadline = Instant::now() + Duration::from_secs(60);
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        match notes.recv_timeout(left) {
            Ok(Note::Dictated(text, _)) => return out(&text),
            // idle without a text: the recording never started
            Ok(Note::Phase(Phase::Idle)) => deadline = Instant::now() + Duration::from_secs(1),
            Ok(_) => {}
            Err(_) => break,
        }
    }
    Err(Error::Stt(format!(
        "the session did not end ({:?})",
        d.phase()
    )))
}

/// Hands-free on a fake screen: the real microphone, ear and model, a
/// throwaway history.
fn listen(args: &[String]) -> Result<()> {
    let (mut seconds, mut source, mut typing) = (60.0_f32, Source::Mic, false);
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        if flag == "--type" {
            typing = true;
            continue;
        }
        match (flag.as_str(), it.next()) {
            ("--seconds", Some(n)) => seconds = n.parse().map_err(|_| usage())?,
            // a recording instead of the microphone, in real time, then quiet
            ("--wav", Some(file)) => {
                source = Source::Wav(Arc::new(audio::read_wav(Path::new(file))?))
            }
            _ => return Err(usage()),
        }
    }
    let paths = Paths::user()?;
    let mut cfg = Config::load(&paths)?;
    cfg.wake.enabled = true;
    let rec = load(&paths, cfg.dictation.model)?;
    let tmp = std::env::temp_dir().join(format!("blitznir-listen-{}", std::process::id()));
    let store = Arc::new(Store::new(Paths::new(&tmp), cfg.dictation.history_max)?);
    let screen = Arc::new(FakePlatform::new());
    // --type: into the focused app, Return and all (a demo's take)
    let target: Arc<dyn Platform> = match typing {
        true => platform::current(),
        false => screen.clone(),
    };
    let (tx, notes) = mpsc::channel();
    let send = move |n| drop(tx.send(n));
    let d = Dictator::new(target.clone(), store, cfg.clone(), Source::Mic, send);
    d.ready(Box::new(rec));
    let hands = Hands::start(&paths, &cfg, target, d, source, |m| {
        log::info!("flash: {m}")
    })?;
    let t0 = Instant::now();
    let at = || t0.elapsed().as_secs_f32();
    out(&format!("listening {seconds} s for {:?}", cfg.wake.wake))?;
    let (end, mut sent) = (t0 + Duration::from_secs_f32(seconds), 0);
    while Instant::now() < end {
        match notes.recv_timeout(Duration::from_millis(200)) {
            Ok(n) => {
                hands.note(&n);
                match n {
                    Note::Phase(p) => out(&format!("{:7.1}s  {p:?}", at()))?,
                    Note::Dictated(text, _) => out(&format!("{:7.1}s  dictated: {text:?}", at()))?,
                    Note::Flash(_) => {}
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        let now = blitznir_core::lock(&screen.sent_in).len();
        if now > sent {
            (sent, _) = (now, out(&format!("{:7.1}s  Return pressed", at()))?);
        }
    }
    let _ = std::fs::remove_dir_all(&tmp);
    out(&format!(
        "screen: {:?}; Return pressed {sent} times",
        screen.screen()
    ))
}

#[cfg(test)]
mod tests {
    use super::run;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn help_is_an_answer_and_unknown_flags_are_refused() {
        assert!(run(&args(&["--help"])).is_ok());
        assert!(run(&args(&["-h"])).is_ok());
        assert!(
            run(&args(&["--open-panel"])).is_err(),
            "an env var, not a flag"
        );
        assert!(run(&args(&["--check", "extra"])).is_err());
    }

    #[test]
    fn dictate_refuses_seconds_no_duration_can_hold() {
        let secs = |n: &str| super::dictate_args(&args(&["--mode", "live", "--seconds", n]));
        for bad in ["-1", "NaN", "inf", "ten"] {
            assert!(secs(bad).is_err(), "--seconds {bad}");
        }
        assert_eq!(secs("2.5").map(|a| a.seconds).ok(), Some(Some(2.5)));
    }
}
