//! Audio in, as 16 kHz mono f32 chunks over a channel: the microphone (cpal,
//! the CURRENT default device, opened per recording so a mic plugged in later
//! is used), or a WAV file replayed at real time for a deterministic test.

use blitznir_core::{Error, Result, segmenter::SAMPLE_RATE};
use cpal::{
    FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use rubato::{Fft, FixedSync, Resampler, audioadapter_buffers::direct::InterleavedSlice};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Debug, Clone)]
pub enum Source {
    Mic,
    /// 16 kHz samples, sent in real time; silence after the end, like a quiet room.
    Wav(Arc<Vec<f32>>),
}

/// A denied microphone delivers exact zeros; no real room is this quiet.
const SILENT_PEAK: f32 = 0.0005;
/// The WAV replay's block: one segmenter tick.
const REPLAY_BLOCK: Duration = Duration::from_millis(350);

#[derive(Debug, Default)]
pub struct Stats {
    pub device: String,
    pub peak: f32,
    /// 16 kHz samples delivered.
    pub samples: usize,
}

impl Stats {
    pub fn silent(&self) -> bool {
        self.samples > 0 && self.peak < SILENT_PEAK
    }

    fn add(&mut self, block: &[f32]) {
        self.samples += block.len();
        self.peak = block.iter().fold(self.peak, |m, x| m.max(x.abs()));
    }
}

/// A running capture. It drops its sender (ending the session's audio) once
/// `stop` is set and the last audio went out.
pub struct Recorder(pub JoinHandle<Stats>);

impl Recorder {
    /// Wait up to `within` for the thread to end. None: it is stuck (a
    /// driver that never returns from closing the stream) and is left behind.
    pub fn finish(self, within: Duration) -> Option<Stats> {
        let end = Instant::now() + within;
        while !self.0.is_finished() {
            if Instant::now() >= end {
                return None; // dropping the handle detaches the thread
            }
            thread::sleep(Duration::from_millis(10));
        }
        Some(self.0.join().unwrap_or_else(|_| {
            log::error!("the audio thread panicked");
            Stats::default()
        }))
    }
}

pub fn start(source: &Source, tx: Sender<Vec<f32>>, stop: Arc<AtomicBool>) -> Result<Recorder> {
    match source {
        Source::Mic => mic(tx, stop),
        Source::Wav(pcm) => replay(pcm.clone(), tx, stop),
    }
}

fn replay(pcm: Arc<Vec<f32>>, tx: Sender<Vec<f32>>, stop: Arc<AtomicBool>) -> Result<Recorder> {
    let block = SAMPLE_RATE * REPLAY_BLOCK.as_millis() as usize / 1000;
    let t0 = Instant::now();
    let run = move || {
        let mut stats = Stats {
            device: "WAV replay".into(),
            ..Stats::default()
        };
        for i in 0.. {
            // a microphone hands over a block once it has been spoken
            thread::sleep((t0 + REPLAY_BLOCK * (i + 1)).saturating_duration_since(Instant::now()));
            if stop.load(Ordering::Acquire) {
                break;
            }
            let at = |n: usize| (n * block).min(pcm.len());
            let mut chunk = pcm[at(i as usize)..at(i as usize + 1)].to_vec();
            chunk.resize(block, 0.0);
            stats.add(&chunk);
            if tx.send(chunk).is_err() {
                break;
            }
        }
        stats
    };
    Ok(Recorder(
        thread::Builder::new().name("wav".into()).spawn(run)?,
    ))
}

fn mic(tx: Sender<Vec<f32>>, stop: Arc<AtomicBool>) -> Result<Recorder> {
    let (ready_tx, ready) = mpsc::sync_channel::<Result<String>>(1);
    let flag = stop.clone();
    let run = move || {
        let (raw_tx, raw) = mpsc::channel::<Vec<f32>>();
        let (stream, device, rate) = match open(raw_tx) {
            Ok(opened) => opened,
            Err(e) => {
                let _ = ready_tx.send(Err(e));
                return Stats::default();
            }
        };
        let mut to16k = match To16k::new(rate) {
            Ok(r) => r,
            Err(e) => {
                let _ = ready_tx.send(Err(e));
                return Stats::default();
            }
        };
        let _ = ready_tx.send(Ok(device.clone()));
        let mut stats = Stats {
            device,
            ..Stats::default()
        };
        let mut forward = |block: Vec<f32>| {
            let out = to16k.push(block);
            stats.add(&out);
            if !out.is_empty() {
                let _ = tx.send(out);
            }
        };
        loop {
            let stopped = flag.load(Ordering::Acquire);
            match raw.recv_timeout(Duration::from_millis(20)) {
                Ok(block) => forward(block),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            if stopped {
                break;
            }
        }
        drop(stream);
        raw.try_iter().for_each(&mut forward); // what arrived before the stream stopped
        stats
    };
    let thread = thread::Builder::new().name("mic".into()).spawn(run)?;
    match ready.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(device)) => {
            log::info!("recording from {device}");
            Ok(Recorder(thread))
        }
        Ok(Err(e)) => Err(e),
        Err(_) => {
            stop.store(true, Ordering::Release); // if it opens after all, it ends at once
            Err(Error::Platform(
                "the microphone did not open within 5 s".into(),
            ))
        }
    }
}

/// Frames from `feed` instead of a microphone: hands-free's ear (wake.rs)
/// hands its own on from the wake phrase, so nothing said while the
/// dictation starts (and the phrase is checked) is lost.
pub fn forward(
    feed: Receiver<Vec<f32>>,
    tx: Sender<Vec<f32>>,
    stop: Arc<AtomicBool>,
) -> Result<Recorder> {
    let run = move || {
        let mut stats = Stats {
            device: "the hands-free listener".into(),
            ..Stats::default()
        };
        while !stop.load(Ordering::Acquire) {
            match feed.recv_timeout(Duration::from_millis(20)) {
                Ok(block) => {
                    stats.add(&block);
                    if tx.send(block).is_err() {
                        break;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        stats
    };
    Ok(Recorder(
        thread::Builder::new().name("feed".into()).spawn(run)?,
    ))
}

fn audio_err(e: impl std::fmt::Display) -> Error {
    Error::Platform(format!("microphone: {e}"))
}

/// The default input at its current rate. Asking CoreAudio for 16 kHz would
/// re-clock the device for every app using it, so we resample instead.
fn open(raw: Sender<Vec<f32>>) -> Result<(Stream, String, u32)> {
    let dev = cpal::default_host()
        .default_input_device()
        .ok_or_else(|| audio_err("no input device"))?;
    let sup = dev.default_input_config().map_err(audio_err)?;
    let cfg = sup.config();
    let stream = match sup.sample_format() {
        SampleFormat::F32 => build::<f32>(&dev, &cfg, raw),
        SampleFormat::I16 => build::<i16>(&dev, &cfg, raw),
        SampleFormat::I32 => build::<i32>(&dev, &cfg, raw),
        SampleFormat::U16 => build::<u16>(&dev, &cfg, raw),
        other => Err(audio_err(format!("unsupported sample format {other}"))),
    }?;
    stream.play().map_err(audio_err)?;
    let name = format!("\"{dev}\" ({} Hz, {} ch)", cfg.sample_rate, cfg.channels);
    Ok((stream, name, cfg.sample_rate))
}

fn build<T>(dev: &cpal::Device, cfg: &StreamConfig, raw: Sender<Vec<f32>>) -> Result<Stream>
where
    T: SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    let ch = usize::from(cfg.channels.max(1));
    let data = move |data: &[T], _: &cpal::InputCallbackInfo| {
        // light work only here: downmix and hand over; resampling runs on the mic thread
        let mono = data
            .chunks(ch)
            .map(|f| f.iter().map(|s| f32::from_sample(*s)).sum::<f32>() / ch as f32)
            .collect();
        let _ = raw.send(mono);
    };
    let error = |e| log::warn!("microphone stream: {e}");
    dev.build_input_stream(*cfg, data, error, None)
        .map_err(audio_err)
}

/// Device rate → 16 kHz; a pass-through when the device already runs at 16 kHz.
struct To16k {
    fft: Option<Fft<f32>>,
    pending: Vec<f32>,
}

impl To16k {
    fn new(rate: u32) -> Result<To16k> {
        let fft = if rate as usize == SAMPLE_RATE {
            None
        } else {
            let fft = Fft::new(rate as usize, SAMPLE_RATE, 1024, 1, FixedSync::Input);
            Some(fft.map_err(|e| audio_err(format!("resampler for {rate} Hz: {e}")))?)
        };
        Ok(To16k {
            fft,
            pending: vec![],
        })
    }

    fn push(&mut self, block: Vec<f32>) -> Vec<f32> {
        let Some(fft) = self.fft.as_mut() else {
            return block;
        };
        self.pending.extend(block);
        let mut out = vec![];
        let need = fft.input_frames_next();
        while self.pending.len() >= need {
            let done = InterleavedSlice::new(&self.pending[..need], 1, need)
                .map_err(audio_err)
                .and_then(|input| fft.process(&input, None).map_err(audio_err));
            match done {
                Ok(o) => out.extend(o.take_data()),
                Err(e) => log::warn!("{e}"),
            }
            self.pending.drain(..need);
        }
        out
    }
}

/// For `--check`: record from the default input for `secs`.
pub fn probe(secs: f32) -> Result<Stats> {
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    let rec = mic(tx, stop.clone())?;
    thread::sleep(Duration::from_secs_f32(secs));
    stop.store(true, Ordering::Release);
    rx.iter().for_each(drop); // until the mic thread hangs up
    rec.finish(Duration::from_secs(2))
        .ok_or_else(|| audio_err("the device did not close within 2 s"))
}

/// A 16 kHz WAV, any sample format; channels are averaged.
pub fn read_wav(path: &Path) -> Result<Vec<f32>> {
    let bad = |e: hound::Error| Error::Stt(format!("{}: {e}", path.display()));
    let mut r = hound::WavReader::open(path).map_err(bad)?;
    let spec = r.spec();
    if spec.sample_rate as usize != SAMPLE_RATE {
        return Err(Error::Stt(format!(
            "{}: {} Hz, need 16 kHz (afconvert -f WAVE -d LEI16@16000 -c 1 in.wav out.wav)",
            path.display(),
            spec.sample_rate
        )));
    }
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1_i64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>()
                .map(|s| s.map(|v| v as f32 * scale))
                .collect::<Result<_, _>>()
                .map_err(bad)?
        }
        hound::SampleFormat::Float => r.samples::<f32>().collect::<Result<_, _>>().map_err(bad)?,
    };
    let ch = usize::from(spec.channels.max(1));
    Ok(samples
        .chunks(ch)
        .map(|f| f.iter().sum::<f32>() / ch as f32)
        .collect())
}
