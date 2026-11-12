//! The beam's player: core's beam_sound [`Synth`] as one stream through the
//! speaker, from the opening to the landing's end. The landing and a stop
//! reach the stream as messages it takes at its next sample, so the landing
//! comes in without a gap and a stop fades instead of cutting.

use super::sound::{gain, seed};
use crate::zap::{Hum, HumSound};
use blitznir::speaker::Speaker;
use blitznir_core::{
    beam_sound::{RATE, Synth},
    lock,
};
use std::sync::{
    Arc, Mutex,
    mpsc::{self, Receiver, Sender},
};

/// A stop fades out over this: no click where it ends.
const STOP_SECS: f32 = 0.02;

enum Cue {
    Land(f32),
    Stop,
}

/// The stream's side: the synth, its cues, the volume.
struct Stream {
    synth: Synth,
    cues: Receiver<Cue>,
    gain: f32,
    /// Samples left of a stop's fade.
    stopping: Option<usize>,
}

impl Stream {
    fn next(&mut self) -> Option<f32> {
        while let Ok(cue) = self.cues.try_recv() {
            match cue {
                Cue::Land(ran) => self.synth.land(ran),
                Cue::Stop => {
                    let fade = (STOP_SECS * RATE as f32) as usize;
                    self.stopping.get_or_insert(fade);
                }
            }
        }
        if self.synth.done() {
            return None;
        }
        let fade = match self.stopping.as_mut() {
            Some(0) => return None,
            Some(n) => {
                *n -= 1;
                *n as f32 / (STOP_SECS * RATE as f32)
            }
            None => 1.0,
        };
        Some(self.synth.sample() * self.gain * fade)
    }
}

/// The beam's [`HumSound`] for one job, at `volume` %.
pub fn beam(volume: u8) -> HumSound {
    let cues: Mutex<Option<Sender<Cue>>> = Mutex::new(None);
    let gain = gain(volume);
    Arc::new(move |cue: Hum| match cue {
        Hum::Start => {
            let (tx, rx) = mpsc::channel();
            let mut stream = Stream {
                synth: Synth::new(seed()),
                cues: rx,
                gain,
                stopping: None,
            };
            match Speaker::play(RATE, Box::new(move || stream.next())) {
                Ok(Speaker) => *lock(&cues) = Some(tx),
                Err(e) => log::warn!("beam sound: {e}"),
            }
        }
        Hum::Land(ran) => send(&cues, Cue::Land(ran)),
        Hum::Stop => send(&cues, Cue::Stop),
    })
}

fn send(cues: &Mutex<Option<Sender<Cue>>>, cue: Cue) {
    if let Some(tx) = lock(cues).as_ref() {
        let _ = tx.send(cue); // gone: the stream has ended already
    }
}
