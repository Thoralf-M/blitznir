//! The working effect, on Freya: while a rewrite,
//! a translation that replaces the selection or a write-out runs, the
//! menu-bar hammer glows and sends light from it down onto the selected
//! text (Platform::selection_bounds), else the mouse, which just made the
//! selection (core's sky): a bolt at every glow's peak ([`strikes`]), or a
//! beam that stays until the text lands ([`beam`]). [`overlay`] draws it in
//! a transparent, click-through window over the target's screen that takes
//! the hidden panel's key focus, so esc still cancels and stray keys cannot
//! reach the selection. It never touches text: the paste is jobs::deliver's,
//! and comes only after [`Zap::finish`] returns. The window is made for
//! each effect and destroyed after it; on Linux it is maximized (Wayland
//! lets no window place itself, see [`overlay`]).

mod beam;
mod bolt;
pub mod caret;
mod overlay;
mod strikes;
pub mod wave;

use crate::tray::{GLOW_PEAK, Tray};
pub use beam::{Hum, HumSound};
use blitznir_core::lock;
use blitznir_core::platform::{Bounds, Rect};
use blitznir_core::sky::Sky;
use bolt::{Bolt, Rng};
use freya::prelude::{LaunchProxy, RendererContext};
use freya::winit::window::WindowId;
pub(crate) use overlay::screen;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, Weak};
use std::thread;
use std::time::{Duration, Instant};
use thunder_synth::Thunder;

/// One glow frame per tick, up to the peak (the bolt fires) and back down.
const TICK: Duration = Duration::from_micros(33_333);
const BLOW: [usize; 10] = [1, 2, 3, 4, GLOW_PEAK, 4, 3, 2, 1, 0];
/// Seconds between blows: lightning is not a metronome, and it gets busier
/// once the answer streams in.
const GAP_WAITING: (f32, f32) = (0.6, 1.3);
const GAP_WRITING: (f32, f32) = (0.25, 0.65);
/// The last strike is seen landing before the paste: the paste's app switch
/// (`NSApp.hide`) takes every window of ours off the screen.
const SEEN: Duration = Duration::from_millis(100);
/// A strike's flicker, and the last one's; the window goes after that.
const SHORT: Duration = Duration::from_millis(380);
const FINAL: Duration = Duration::from_millis(600);
/// [`Zap::finish`] never holds the paste back longer than this.
const FINISH_MAX: Duration = Duration::from_millis(600);
/// The event loop places the overlay within this, or the effect is off.
const LOOP_WAIT: Duration = Duration::from_secs(1);
/// A window shows after its first frame; key focus is asked for then.
const REFOCUS: Duration = Duration::from_millis(250);

enum Msg {
    /// The answer has started to stream in.
    Progress,
    /// The answer is in: strike once more, then answer on the sender.
    Finish(Sender<()>),
    Cancel,
}

/// One strike as the overlay draws it.
pub struct Strike {
    pub bolt: Bolt,
    pub at: Instant,
    /// Its flicker: [`SHORT`], or [`FINAL`] for the last.
    pub lasts: Duration,
}

/// What the overlay shows: where the bolts land (lit when it is the
/// selected text itself) and the newest strike, or the beam.
#[derive(Clone, Default)]
pub struct Scene {
    pub aim: Rect,
    pub exact: bool,
    pub strike: Option<Arc<Strike>>,
    pub beam: Option<beam::Beam>,
}

/// Which effect a job gets, with its sound (None: silent).
pub enum Effect {
    Lightning(Option<Sound>),
    Beam(Option<HumSound>),
}

pub struct Zap {
    me: Weak<Zap>,
    tray: Arc<Tray>,
    proxy: LaunchProxy,
    /// Esc in the overlay: the panel cancels the job.
    esc: Box<dyn Fn() + Send + Sync>,
    /// The running effect's session, the line to its thread, and whether
    /// it is the beam (its thread takes it down itself).
    run: Mutex<Option<(u64, Sender<Msg>, bool)>>,
    /// Bumped by every start: an older thread's late strikes and hide must
    /// never reach the newer effect.
    session: AtomicU64,
    /// The overlay window and the session it belongs to.
    shown: Mutex<Option<(u64, WindowId)>>,
    scene: Mutex<(u64, Scene)>,
    /// Wakes the overlay's component when the scene changed.
    wake: Mutex<Option<async_channel::Sender<()>>>,
}

impl Zap {
    pub fn new(
        tray: Arc<Tray>,
        proxy: LaunchProxy,
        esc: impl Fn() + Send + Sync + 'static,
    ) -> Arc<Zap> {
        Arc::new_cyclic(|me| Zap {
            me: me.clone(),
            tray,
            proxy,
            esc: Box::new(esc),
            run: Mutex::new(None),
            session: AtomicU64::new(0),
            shown: Mutex::new(None),
            scene: Mutex::default(),
            wake: Mutex::new(None),
        })
    }

    /// Start `effect` at `sel`, the selection's bounds, else `mouse`. Never
    /// waits for the event loop, so it may run on it. True: the overlay is
    /// coming and takes the key focus; the caller hides the panel.
    pub fn start(&self, sel: Option<Bounds>, mouse: Option<(f64, f64)>, effect: Effect) -> bool {
        self.cancel();
        let Some(me) = self.me.upgrade() else {
            return false;
        };
        let id = self.session.fetch_add(1, Ordering::SeqCst) + 1;
        let (tx, rx) = mpsc::channel();
        *lock(&self.run) = Some((id, tx, matches!(effect, Effect::Beam(_))));
        let began = Instant::now();
        let run = move || match (me.open(id, sel, mouse), effect) {
            (Some(sky), Effect::Lightning(sound)) => {
                let rng = Rng::new();
                strikes::Run {
                    zap: me,
                    sky,
                    id,
                    rng,
                    sound,
                }
                .pace(&rx);
            }
            (Some(sky), Effect::Beam(sound)) => {
                beam::Run::new(me, &sky, id, began, sound).pace(&rx)
            }
            (None, _) => {
                log::warn!("effect: no overlay (no screen, or the loop did not answer)")
            }
        };
        if let Err(e) = thread::Builder::new().name("zap".into()).spawn(run) {
            log::error!("effect: {e}");
            *lock(&self.run) = None;
            return false;
        }
        true
    }

    fn send(&self, msg: Msg) -> Option<u64> {
        let run = lock(&self.run);
        let (id, tx, _) = run.as_ref()?;
        tx.send(msg).ok().map(|()| *id)
    }

    /// The first streamed words: strike faster.
    pub fn progress(&self) {
        self.send(Msg::Progress);
    }

    /// The answer is in: returns once the last, bigger strike has landed,
    /// so the paste comes after it; a [`Zap::cancel`] meanwhile still counts.
    /// Nothing running: returns at once.
    pub fn finish(&self) {
        let (ack, landed) = mpsc::channel();
        let Some(id) = self.send(Msg::Finish(ack)) else {
            return;
        };
        let _ = landed.recv_timeout(FINISH_MAX);
        let mut run = lock(&self.run);
        if run.as_ref().is_some_and(|(i, _, _)| *i == id) {
            *run = None;
        }
    }

    /// Esc, an error, a new open: gone at once (the beam in a quick
    /// collapse). True if it ran, and so the panel has to come back.
    pub fn cancel(&self) -> bool {
        let Some((id, tx, beam)) = lock(&self.run).take() else {
            return false;
        };
        let _ = tx.send(Msg::Cancel); // the finale does not listen: it ends by itself
        if !beam {
            self.close(id);
        }
        true
    }

    /// Esc in the overlay, while an effect runs.
    fn escape(&self) {
        let running = lock(&self.run).is_some();
        if running {
            (self.esc)();
        }
    }

    /// What the overlay shows now.
    fn scene(&self) -> Scene {
        lock(&self.scene).1.clone()
    }

    /// The overlay's component listens here.
    fn attach(&self, wake: async_channel::Sender<()>) {
        *lock(&self.wake) = Some(wake);
    }

    fn strike(&self, id: u64, strike: Strike) {
        self.show(id, |s| s.strike = Some(Arc::new(strike)));
    }

    /// Session `id`'s scene changed by `change`, and the overlay woken.
    fn show(&self, id: u64, change: impl FnOnce(&mut Scene)) {
        let mut scene = lock(&self.scene);
        if scene.0 != id {
            return;
        }
        change(&mut scene.1);
        drop(scene);
        if let Some(wake) = lock(&self.wake).as_ref() {
            let _ = wake.try_send(());
        }
    }

    /// Make the overlay over the target's screen for session `id`; its
    /// [`Sky`] once the event loop has placed it. On the effect's thread.
    fn open(&self, id: u64, sel: Option<Bounds>, mouse: Option<(f64, f64)>) -> Option<Sky> {
        let me = self.me.clone();
        let (tx, rx) = mpsc::sync_channel(1);
        drop(self.proxy.post_callback(move |ctx| {
            let _ = tx.send(me.upgrade().and_then(|me| me.open_in(ctx, id, sel, mouse)));
        }));
        let sky = rx.recv_timeout(LOOP_WAIT).ok().flatten()?;
        let Rect { x, y, w, h } = sky.aim;
        log::info!(
            "effect: at {x:.0}, {y:.0}, {w:.0}×{h:.0} (the text: {}) on {:?}",
            sky.exact,
            sky.area
        );
        let (me, proxy) = (self.me.clone(), self.proxy.clone());
        thread::spawn(move || {
            thread::sleep(REFOCUS);
            drop(proxy.post_callback(move |ctx| {
                let Some(me) = me.upgrade() else {
                    return;
                };
                // a job that ended meanwhile (the beam still fading) gave
                // the focus back: the overlay must not take it again
                let running = lock(&me.run).as_ref().map(|(i, _, _)| *i);
                let Some((_, win)) = *lock(&me.shown) else {
                    return;
                };
                if running != Some(id) {
                    return;
                }
                if let Some(w) = ctx.windows_mut().get_mut(&win) {
                    w.window_mut().focus_window();
                }
            }));
        });
        Some(sky)
    }

    fn open_in(
        self: Arc<Self>,
        ctx: &mut RendererContext,
        id: u64,
        sel: Option<Bounds>,
        mouse: Option<(f64, f64)>,
    ) -> Option<Sky> {
        if self.session.load(Ordering::SeqCst) != id {
            return None; // cancelled or replaced before the loop got here
        }
        let sky = overlay::cover(ctx.active_event_loop, sel, mouse)?;
        let (aim, exact) = (sky.aim, sky.exact);
        *lock(&self.scene) = (
            id,
            Scene {
                aim,
                exact,
                ..Scene::default()
            },
        );
        let old = lock(&self.shown).take();
        if let Some((_, win)) = old {
            ctx.windows_mut().remove(&win); // an older finale's, still fading
        }
        let win = ctx.launch_window(overlay::config(self.clone(), sky.area));
        *lock(&self.shown) = Some((id, win));
        Some(sky)
    }

    /// Destroy session `id`'s overlay, later, on the event loop.
    fn close(&self, id: u64) {
        let me = self.me.clone();
        drop(self.proxy.post_callback(move |ctx| {
            let Some(me) = me.upgrade() else {
                return;
            };
            let mut shown = lock(&me.shown);
            if let Some((sid, win)) = *shown
                && sid == id
            {
                *shown = None;
                *lock(&me.wake) = None;
                ctx.windows_mut().remove(&win);
            }
        }));
    }
}

/// Plays thunder (`[panel] effect_volume`): far rolls while it strikes, the clap
/// with the last strike; None silences it (a cancel).
pub type Sound = Arc<dyn Fn(Option<Thunder>) + Send + Sync>;
