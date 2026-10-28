//! The beam: a thread of light runs from the menu-bar hammer along under
//! the bar, and a wide soft column drops from there onto the selected text
//! and stays, streaming, for as long as the job runs; when the answer is
//! in, a small lift where it lands, the paste, and the column collapses
//! from the top. Its timeline is core's beam, its drawing [`draw`], its
//! sound core's beam_sound (panel/beam_sound.rs plays it). This thread only
//! marks the start and the end in the scene; the overlay animates from them.

mod draw;
mod motes;
mod paint;

/// Where the beam lands along the selection, as a share of its width: the
/// middle, where a reader looks.
const LAND_AT: f64 = 0.5;

use super::{Msg, SEEN, TICK, Zap};
use crate::tray::GLOW_PEAK;
use blitznir_core::beam::{CANCEL_SECS, End, LAND_SECS, Phase};
use blitznir_core::sky::Sky;
pub use draw::{Cache, draw};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

pub type Pt = (f32, f32);

/// The beam as the overlay draws it, in the overlay's points.
#[derive(Clone, Copy)]
pub struct Beam {
    pub start: Instant,
    /// The hammer's face, and the menu bar's bottom edge under it.
    pub src: Pt,
    pub bar: f32,
    /// The selected text: never drawn over.
    pub aim: [f32; 4],
    pub end: Option<(Instant, End)>,
}

impl Beam {
    pub fn phase(&self, now: Instant) -> Phase {
        let run = now.saturating_duration_since(self.start).as_secs_f32();
        let end = self
            .end
            .map(|(at, how)| (now.saturating_duration_since(at).as_secs_f32(), how));
        Phase::at(run, end)
    }
}

/// What the beam's sound does: the working loop from the start, the
/// landing of a job that ran this many seconds, silence (a cancel).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hum {
    Start,
    Land(f32),
    Stop,
}

/// Plays [`Hum`]s (`[panel] effect_volume` is its volume).
pub type HumSound = Arc<dyn Fn(Hum) + Send + Sync>;

/// One beam's thread: it owns the hammer until the job ends or a newer
/// effect starts, and takes its overlay down after the collapse.
pub(super) struct Run {
    zap: Arc<Zap>,
    id: u64,
    /// When the job started: how long it ran decides the landing's sound.
    began: Instant,
    beam: Beam,
    sound: Option<HumSound>,
    glowing: usize,
}

impl Run {
    pub fn new(zap: Arc<Zap>, sky: &Sky, id: u64, began: Instant, sound: Option<HumSound>) -> Run {
        let hammer = zap.tray.rect().and_then(|(at, h)| sky.hammer(at, h));
        if hammer.is_none() {
            log::warn!("beam: the hammer is on no screen of the overlay: from the top edge");
        }
        let a = sky.aim;
        let aim = [a.x as f32, a.y as f32, a.w as f32, a.h as f32];
        // no hammer: straight down from the top edge onto where it lands
        let ((x, y), bar) = hammer.unwrap_or(((a.x + a.w * LAND_AT, 0.0), 0.0));
        let beam = Beam {
            start: Instant::now(),
            src: (x as f32, y as f32),
            bar: bar as f32,
            aim,
            end: None,
        };
        Run {
            zap,
            id,
            began,
            beam,
            sound,
            glowing: 0,
        }
    }

    fn live(&self) -> bool {
        self.zap.session.load(Ordering::SeqCst) == self.id
    }

    fn hum(&self, cue: Hum) {
        if let Some(s) = &self.sound {
            s(cue);
        }
    }

    /// The hammer at the beam's glow now; the tray only hears changes.
    fn glow(&mut self) {
        let level = (self.beam.phase(Instant::now()).hammer() * GLOW_PEAK as f32).round() as usize;
        if level != self.glowing && self.live() {
            self.glowing = level;
            self.zap.tray.glow(level);
        }
    }

    /// The scene's beam, now with how it ended.
    fn end(&mut self, how: End) {
        self.beam.end = Some((Instant::now(), how));
        let beam = self.beam;
        self.zap.show(self.id, |s| s.beam = Some(beam));
    }

    /// The glow follows the beam until `secs` after its end, then the
    /// overlay goes.
    fn fade(mut self, secs: f32) {
        let until = Instant::now() + Duration::from_secs_f32(secs);
        while Instant::now() < until {
            self.glow();
            thread::sleep(TICK);
        }
        if self.live() {
            self.zap.tray.glow(0);
            self.zap.close(self.id);
        }
    }

    /// From the start until the job ends: its answer, or a cancel (esc, an
    /// error, a new open).
    pub fn pace(mut self, rx: &Receiver<Msg>) {
        self.beam.start = Instant::now();
        let beam = self.beam;
        self.zap.show(self.id, |s| s.beam = Some(beam));
        self.hum(Hum::Start);
        loop {
            match rx.recv_timeout(TICK) {
                Ok(Msg::Progress) => {}
                Ok(Msg::Finish(ack)) => return self.land(&ack),
                Ok(Msg::Cancel) | Err(RecvTimeoutError::Disconnected) => {
                    self.hum(Hum::Stop);
                    self.end(End::Cancel);
                    return self.fade(CANCEL_SECS);
                }
                Err(RecvTimeoutError::Timeout) => self.glow(),
            }
        }
    }

    /// The lift where it lands and the landing's sound, the paste may go
    /// (`ack`), then the collapse.
    fn land(mut self, ack: &Sender<()>) {
        self.end(End::Land);
        self.hum(Hum::Land(self.began.elapsed().as_secs_f32()));
        if self.live() {
            self.zap.tray.glow(GLOW_PEAK);
            self.glowing = GLOW_PEAK;
        }
        log::info!("beam: landed");
        thread::sleep(SEEN);
        #[cfg(target_os = "linux")]
        if self.live() {
            self.zap.close(self.id); // it may hold the focus the paste goes to
        }
        let _ = ack.send(());
        self.fade(LAND_SECS - SEEN.as_secs_f32());
    }
}
