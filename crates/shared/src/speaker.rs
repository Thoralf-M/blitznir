//! Sound out as one stream: the default output device pulls mono samples
//! from a closure on its own thread, for as long as the closure has any.
//! For the beam's sound, which must run on without a gap from its opening
//! to its landing's end: separate files through a player start late and
//! leave a hole where one hands over to the next.

use blitznir_core::{Error, Result};
use cpal::{
    FromSample, SampleFormat, SizedSample, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

/// The next sample, -1..1; None: it has played out, the stream closes.
pub type Fill = Box<dyn FnMut() -> Option<f32> + Send>;

/// A stream playing. Dropping it lets it play out; the closure ends it.
pub struct Speaker;

fn out_err(e: impl std::fmt::Display) -> Error {
    Error::Platform(format!("sound out: {e}"))
}

impl Speaker {
    /// Play `fill`, mono at `rate`, on the default output device.
    pub fn play(rate: u32, fill: Fill) -> Result<Speaker> {
        let (ready_tx, ready) = mpsc::sync_channel::<Result<()>>(1);
        let run = move || {
            let over = Arc::new(AtomicBool::new(false));
            let stream = match open(rate, fill, over.clone()) {
                Ok(s) => s,
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };
            let _ = ready_tx.send(Ok(()));
            while !over.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(20));
            }
            // the device may still hold the last buffer: let it drain
            thread::sleep(Duration::from_millis(100));
            drop(stream);
        };
        thread::Builder::new().name("speaker".into()).spawn(run)?;
        match ready.recv_timeout(Duration::from_secs(2)) {
            Ok(Ok(())) => Ok(Speaker),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(out_err("the output did not open within 2 s")),
        }
    }
}

/// The default output at its own rate; `fill`'s `rate` resampled to it.
fn open(rate: u32, fill: Fill, over: Arc<AtomicBool>) -> Result<Stream> {
    let dev = cpal::default_host()
        .default_output_device()
        .ok_or_else(|| out_err("no output device"))?;
    let sup = dev.default_output_config().map_err(out_err)?;
    let cfg = sup.config();
    let src = Resample::new(fill, rate, cfg.sample_rate);
    let stream = match sup.sample_format() {
        SampleFormat::F32 => build::<f32>(&dev, &cfg, src, over),
        SampleFormat::I16 => build::<i16>(&dev, &cfg, src, over),
        SampleFormat::I32 => build::<i32>(&dev, &cfg, src, over),
        SampleFormat::U16 => build::<u16>(&dev, &cfg, src, over),
        other => Err(out_err(format!("unsupported sample format {other}"))),
    }?;
    stream.play().map_err(out_err)?;
    Ok(stream)
}

fn build<T>(
    dev: &cpal::Device,
    cfg: &StreamConfig,
    mut src: Resample,
    over: Arc<AtomicBool>,
) -> Result<Stream>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let ch = usize::from(cfg.channels.max(1));
    let data = move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
        for frame in data.chunks_mut(ch) {
            let v = src.next().unwrap_or_else(|| {
                over.store(true, Ordering::Release);
                0.0
            });
            frame.fill(T::from_sample(v));
        }
    };
    let error = |e| log::warn!("sound out: {e}");
    dev.build_output_stream(*cfg, data, error, None)
        .map_err(out_err)
}

/// `fill` at the device's rate: linear between its samples (a pass-through
/// when the rates agree). Fine for noise and a low hum; nothing tonal and
/// high rides on it.
struct Resample {
    fill: Fill,
    step: f64,
    pos: f64,
    last: f32,
    now: f32,
    done: bool,
}

impl Resample {
    fn new(fill: Fill, from: u32, to: u32) -> Resample {
        Resample {
            fill,
            step: f64::from(from) / f64::from(to.max(1)),
            pos: 1.0,
            last: 0.0,
            now: 0.0,
            done: false,
        }
    }

    fn next(&mut self) -> Option<f32> {
        if self.done {
            return None; // played out: `fill` is not asked again
        }
        while self.pos >= 1.0 {
            self.pos -= 1.0;
            self.last = self.now;
            let Some(v) = (self.fill)() else {
                self.done = true;
                return None;
            };
            self.now = v;
        }
        let v = self.last + (self.now - self.last) * self.pos as f32;
        self.pos += self.step;
        Some(v)
    }
}
