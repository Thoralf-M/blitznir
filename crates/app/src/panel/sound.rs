//! The thunder's player: far rolls for as long as the lightning goes on,
//! then the clap, which takes the roll still playing over (thunder-synth's
//! `clap_over` plays its rest, fading) so nothing cuts off; a cancel
//! silences it all. Each roll is rendered and started on a thread of its own.
//! With `[panel] rain`, a rain clip starts beside every roll as a sound of
//! its own: the clap stops the rolls, never the rain, whose last clip runs
//! out under it.

use super::spawn;
use crate::zap::Sound;
use blitznir_core::lock;
use blitznir_core::platform::Platform;
use std::process::Child;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use thunder_synth::{self as thunder, Thunder};

/// A far roll starts when the last one has this long left: they overlap,
/// never a gap in the sound while the lightning goes on.
const OVERLAP_SECS: f32 = thunder::ROLL_OVERLAP;
/// About how long rendering the clap takes (measured 20 to 60 ms in a
/// release build): where the far roll will be when the clap starts. A few
/// tens of ms off in a rumble cannot be heard.
const RENDER_SECS: f32 = 0.05;

struct Roll {
    child: Option<Child>,
    seed: u64,
    started: Instant,
}

#[derive(Default)]
struct Player {
    rolls: Mutex<Vec<Roll>>,
    clap: Mutex<Option<Child>>,
    /// The rain clips playing, with their start (`[panel] rain`).
    rain: Mutex<Vec<(Option<Child>, Instant)>>,
    /// Bumped by the clap and by a silence: a roll still being rendered
    /// from before must not start after them.
    generation: AtomicU64,
    /// The far rolls' loop runs.
    rolling: AtomicBool,
    /// Every sample times this: `[panel] effect_volume`, the volume.
    gain: f32,
    /// `[panel] rain`: a rain clip beside every far roll.
    raining: bool,
}

/// The volume in % as a gain: squared, as loudness is heard (50 % is about
/// -12 dB), 0 silent.
pub(super) fn gain(volume: u8) -> f32 {
    (f32::from(volume.min(100)) / 100.0).powi(2)
}

/// `wav` with its samples times `gain`: thunder-synth's, a 16-bit mono WAV,
/// whose samples start after the 44-byte header.
fn scaled(wav: &[u8], gain: f32) -> Vec<u8> {
    let (head, pcm) = wav.split_at(wav.len().min(44));
    let quiet = pcm.as_chunks::<2>().0.iter().flat_map(|b| {
        let s = f32::from(i16::from_le_bytes(*b)) * gain;
        (s as i16).to_le_bytes()
    });
    head.iter().copied().chain(quiet).collect()
}

fn stop(child: Option<Child>) {
    if let Some(mut c) = child {
        let _ = c.kill();
        let _ = c.wait();
    }
}

pub(super) fn seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos() as u64)
}

/// `wav` written to the temp dir and played; the player's process.
fn play(platform: &dyn Platform, name: &str, wav: &[u8], gain: f32) -> Option<Child> {
    let path = std::env::temp_dir().join(format!("blitznir-{}-{name}.wav", std::process::id()));
    let played = std::fs::write(&path, scaled(wav, gain))
        .map_err(|e| e.to_string())
        .and_then(|()| platform.play_sound(&path).map_err(|e| e.to_string()));
    played.unwrap_or_else(|e| {
        log::warn!("thunder: {e}");
        None
    })
}

impl Player {
    /// A strike: the far rolls start, unless they already go. From then
    /// on they keep themselves going (the next one [`OVERLAP_SECS`] before
    /// the last ends, whatever the strikes do) until the clap or a silence.
    fn far(self: &Arc<Self>, platform: &Arc<dyn Platform>) {
        if self.rolling.swap(true, Ordering::SeqCst) {
            return;
        }
        let (me, platform) = (self.clone(), platform.clone());
        let born = self.generation.load(Ordering::SeqCst);
        let every = Duration::from_secs_f32(thunder::secs(Thunder::Roll) - OVERLAP_SECS);
        let started = spawn("thunder", move || {
            loop {
                let seed = seed();
                let wav = thunder::wav(Thunder::Roll, seed);
                let rain = me.raining.then(|| thunder::rain(seed));
                let t0 = Instant::now();
                if me.generation.load(Ordering::SeqCst) != born {
                    break; // the clap or a silence
                }
                let child = play(&*platform, &format!("far-{seed}"), &wav, me.gain);
                if let Some(rain) = rain {
                    let drop = play(&*platform, &format!("rain-{seed}"), &rain, me.gain);
                    let mut clips = lock(&me.rain);
                    // a clip is over when the next one has faded in
                    clips.retain_mut(|(c, at)| {
                        let done = at.elapsed() > every + Duration::from_secs_f32(OVERLAP_SECS);
                        if done {
                            stop(c.take());
                        }
                        !done
                    });
                    clips.push((drop, t0));
                }
                let mut rolls = lock(&me.rolls);
                // done ones are only reaped; the newest stays for the clap
                rolls.retain_mut(|r| {
                    let done = r.started.elapsed() > every + Duration::from_secs_f32(OVERLAP_SECS);
                    if done {
                        stop(r.child.take());
                    }
                    !done
                });
                rolls.push(Roll {
                    child,
                    seed,
                    started: t0,
                });
                drop(rolls);
                thread::sleep(every.saturating_sub(t0.elapsed()));
            }
            me.rolling.store(false, Ordering::SeqCst);
        });
        if let Err(e) = started {
            self.rolling.store(false, Ordering::SeqCst);
            log::warn!("thunder: {e}");
        }
    }

    /// Stop the far rolls: none still being rendered starts after this.
    fn stop_rolls(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        let rolls = std::mem::take(&mut *lock(&self.rolls));
        rolls.into_iter().for_each(|r| stop(r.child));
    }

    /// The clap over the rest of the newest far roll: rendered while the
    /// roll still plays (where it will be by then, [`RENDER_SECS`] on),
    /// then the roll stops and the clap starts, so there is no gap.
    fn clap(self: &Arc<Self>, platform: &Arc<dyn Platform>) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        let (me, platform) = (self.clone(), platform.clone());
        let started = spawn("thunder", move || {
            let newest = (lock(&me.rolls).iter())
                .max_by_key(|r| r.started)
                .map(|r| (r.seed, r.started.elapsed().as_secs_f32() + RENDER_SECS));
            let far = newest.filter(|(_, at)| *at < thunder::secs(Thunder::Roll));
            let wav = thunder::clap_over(seed(), far);
            me.stop_rolls();
            stop(lock(&me.clap).take());
            *lock(&me.clap) = play(&*platform, "clap", &wav, me.gain);
        });
        if let Err(e) = started {
            log::warn!("thunder: {e}");
        }
    }

    fn silence(&self) {
        self.stop_rolls();
        stop(lock(&self.clap).take());
        let clips = std::mem::take(&mut *lock(&self.rain));
        clips.into_iter().for_each(|(c, _)| stop(c));
    }
}

/// The lightning's [`Sound`] for one job, with rain if `rain`.
pub fn thunder(platform: Arc<dyn Platform>, volume: u8, rain: bool) -> Sound {
    let player = Arc::new(Player {
        gain: gain(volume),
        raining: rain,
        ..Player::default()
    });
    Arc::new(move |kind: Option<Thunder>| match kind {
        Some(Thunder::Roll) => player.far(&platform),
        Some(Thunder::Clap) => player.clap(&platform),
        // a kind thunder-synth may add later: nothing plays it yet
        Some(_) => {}
        None => player.silence(),
    })
}
