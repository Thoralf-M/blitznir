//! The lightning's thread: the hammer glows up and a bolt strikes at every
//! peak, faster once the answer streams in; a last, bigger strike when it
//! is in, and the paste after it.

use super::bolt::{self, Rng};
use super::{BLOW, FINAL, GAP_WAITING, GAP_WRITING, Msg, SEEN, SHORT, Sound, Strike, TICK, Zap};
use crate::tray::GLOW_PEAK;
use blitznir_core::sky::Sky;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};
use thunder_synth::Thunder;

/// One effect's thread. It owns the overlay and the hammer until the job
/// ends or a newer effect starts.
pub(super) struct Run {
    pub zap: Arc<Zap>,
    pub sky: Sky,
    pub id: u64,
    pub rng: Rng,
    pub sound: Option<Sound>,
}

impl Run {
    fn live(&self) -> bool {
        self.zap.session.load(Ordering::SeqCst) == self.id
    }

    fn glow(&self, level: usize) {
        if self.live() {
            self.zap.tray.glow(level);
        }
    }

    fn strike(&mut self, last: bool) {
        if !self.live() {
            return;
        }
        let src = (self.zap.tray.rect()).and_then(|(at, h)| self.sky.source(at, h));
        if src.is_none() {
            log::warn!("lightning: the hammer is on no screen of the overlay: from the top edge");
        }
        let bolt = bolt::strike(self.sky.aim, src, last, &mut self.rng);
        let lasts = if last { FINAL } else { SHORT };
        let at = Instant::now();
        self.zap.strike(self.id, Strike { bolt, at, lasts });
    }

    fn hide(&self) {
        if self.live() {
            self.zap.close(self.id);
        }
    }

    /// Blow after blow until the job ends: its answer, or a cancel (esc, an
    /// error, a new open), which has already closed the overlay.
    pub fn pace(mut self, rx: &Receiver<Msg>) {
        let mut next: Option<usize> = None; // into BLOW; None: resting
        let mut at = Instant::now();
        let mut gap = GAP_WAITING;
        loop {
            let shown = next.map_or(0, |i| BLOW[i - 1]);
            match rx.recv_timeout(at.saturating_duration_since(Instant::now())) {
                Ok(Msg::Progress) => gap = GAP_WRITING,
                Ok(Msg::Finish(ack)) => return self.finale(shown, &ack),
                Ok(Msg::Cancel) | Err(RecvTimeoutError::Disconnected) => {
                    self.sound.as_ref().inspect(|s| s(None));
                    return self.glow(0);
                }
                Err(RecvTimeoutError::Timeout) => {
                    let i = next.unwrap_or(0);
                    self.glow(BLOW[i]);
                    if BLOW[i] == GLOW_PEAK {
                        self.sound.as_ref().inspect(|s| s(Some(Thunder::Roll)));
                        self.strike(false);
                    }
                    next = (i + 1 < BLOW.len()).then_some(i + 1);
                    at = match next {
                        Some(_) => at + TICK,
                        None => {
                            Instant::now() + Duration::from_secs_f32(self.rng.range(gap.0, gap.1))
                        }
                    };
                }
            }
        }
    }

    /// From glow `from` up to the peak, one bigger strike, the paste may go
    /// (`ack`), then the glow dies down and the overlay goes.
    fn finale(mut self, from: usize, ack: &Sender<()>) {
        for level in from + 1..GLOW_PEAK {
            self.glow(level);
            thread::sleep(TICK);
        }
        self.glow(GLOW_PEAK);
        self.sound
            .as_ref()
            .inspect(|clap| clap(Some(Thunder::Clap)));
        self.strike(true);
        let struck = Instant::now();
        log::info!("lightning: last strike");
        thread::sleep(SEEN);
        #[cfg(target_os = "linux")]
        self.hide(); // it may hold the focus the paste goes to
        let _ = ack.send(());
        for level in (0..GLOW_PEAK).rev() {
            thread::sleep(TICK);
            self.glow(level);
        }
        thread::sleep(FINAL.saturating_sub(struck.elapsed()));
        self.hide();
    }
}
