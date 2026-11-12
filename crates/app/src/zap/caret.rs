//! The dictation's wave (`[panel] dictation_effect`): while
//! a dictation records, a stream of light 5 points under the caret line of
//! the app it types into, its tip at the caret. A thread asks where the
//! caret is ([`Platform::caret`]) five times a second; the wave is drawn in
//! a small transparent window the size of the line ([`window`]), always on
//! top, click-through and shown without key focus: live dictation types
//! into the app under it. It never covers the text and never takes a key or
//! a click; it fades out when the recording ends. macOS only: Wayland lets
//! no window place itself, and Linux has no caret to ask for.

pub mod window;

#[cfg(test)]
mod tests;

use blitznir_core::{
    dictation::Phase,
    lock,
    platform::{Platform, Rect},
    wave::{CARET_OUT_SECS, Spot, spot},
};
use freya::{
    prelude::LaunchProxy,
    winit::{
        dpi::{LogicalPosition, LogicalSize},
        window::WindowId,
    },
};
use std::{
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

/// How often the caret is asked for: a word lands every 400 ms or so, and
/// the spring glides between the answers.
const POLL: Duration = Duration::from_millis(200);

/// What the window draws, from the thread that asks for the caret.
#[derive(Debug, Clone, Copy)]
pub struct Scene {
    pub id: u64,
    pub start: Instant,
    pub end: Option<Instant>,
    /// Where the wave goes; None: the app does not say where its caret is.
    pub spot: Option<Spot>,
    /// The window's place on the screen.
    pub frame: Rect,
    /// When the caret last moved.
    pub moved: Instant,
}

impl Scene {
    /// Faded out after the end.
    pub fn gone(&self, now: Instant) -> bool {
        self.end
            .is_some_and(|e| now.saturating_duration_since(e).as_secs_f32() >= CARET_OUT_SECS)
    }
}

pub struct Caret {
    me: Weak<Caret>,
    platform: Arc<dyn Platform>,
    proxy: LaunchProxy,
    on: AtomicBool,
    /// Bumped by every recording: an older thread never touches a newer wave.
    session: AtomicU64,
    scene: Mutex<Option<Scene>>,
    shown: Mutex<Option<(u64, WindowId)>>,
}

impl Caret {
    pub fn new(platform: Arc<dyn Platform>, proxy: LaunchProxy, on: bool) -> Arc<Caret> {
        Arc::new_cyclic(|me| Caret {
            me: me.clone(),
            platform,
            proxy,
            on: AtomicBool::new(on),
            session: AtomicU64::new(0),
            scene: Mutex::new(None),
            shown: Mutex::new(None),
        })
    }

    /// `[panel] dictation_effect`, from the next recording on.
    pub fn set_on(&self, on: bool) {
        self.on.store(on, Ordering::SeqCst);
    }

    /// The dictation's phase: recording shows the wave, anything else ends it.
    pub fn phase(&self, phase: Phase) {
        match phase {
            Phase::Live | Phase::Batch => self.start(),
            Phase::Loading | Phase::Idle | Phase::Finishing => self.stop(),
        }
    }

    pub fn scene(&self) -> Option<Scene> {
        *lock(&self.scene)
    }

    fn start(&self) {
        if !cfg!(target_os = "macos") || !self.on.load(Ordering::SeqCst) {
            return;
        }
        let Some(me) = self.me.upgrade() else { return };
        let id = self.session.fetch_add(1, Ordering::SeqCst) + 1;
        let now = Instant::now();
        *lock(&self.scene) = Some(Scene {
            id,
            start: now,
            end: None,
            spot: None,
            frame: Rect::default(),
            moved: now,
        });
        if let Err(e) = thread::Builder::new()
            .name("caret-wave".into())
            .spawn(move || me.follow(id))
        {
            log::warn!("dictation wave: {e}");
        }
    }

    fn stop(&self) {
        let id = {
            let mut scene = lock(&self.scene);
            let Some(s) = scene.as_mut().filter(|s| s.end.is_none()) else {
                return;
            };
            s.end = Some(Instant::now());
            s.id
        };
        let me = self.me.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_secs_f32(CARET_OUT_SECS + 0.05));
            if let Some(me) = me.upgrade() {
                me.close(id);
            }
        });
    }

    fn live(&self, id: u64) -> bool {
        self.scene().is_some_and(|s| s.id == id && s.end.is_none())
    }

    /// Session `id`'s thread: where the caret is, until the recording ends.
    fn follow(&self, id: u64) {
        while self.live(id) {
            let focus = self.platform.focused_app();
            let at = self.platform.caret(&focus).map(|line| spot(&line));
            let place = {
                let mut scene = lock(&self.scene);
                let Some(s) = scene.as_mut().filter(|s| s.id == id) else {
                    return;
                };
                if at.map(|a| (a.caret, a.y)) != s.spot.map(|a| (a.caret, a.y)) {
                    s.moved = Instant::now();
                }
                let moves = at.is_some_and(|a| a.frame != s.frame);
                if let Some(a) = at {
                    s.spot = Some(a);
                    s.frame = a.frame;
                } else {
                    s.spot = None; // the window stays where it was; the wave fades
                }
                moves.then_some(s.frame)
            };
            if let Some(frame) = place {
                self.place(id, frame);
            }
            thread::sleep(POLL);
        }
    }

    /// Session `id`'s window onto `frame`: made the first time, moved after.
    fn place(&self, id: u64, frame: Rect) {
        let first = lock(&self.shown).is_none_or(|(sid, _)| sid != id);
        if first {
            self.platform.show_quietly(); // a paste hid our windows
        }
        let me = self.me.clone();
        drop(self.proxy.post_callback(move |ctx| {
            let Some(me) = me.upgrade() else { return };
            if !me
                .scene()
                .is_some_and(|s| s.id == id && !s.gone(Instant::now()))
            {
                return;
            }
            let mut shown = lock(&me.shown);
            match *shown {
                Some((sid, win)) if sid == id => {
                    if let Some(w) = ctx.windows_mut().get_mut(&win) {
                        let w = w.window_mut();
                        w.set_outer_position(LogicalPosition::new(frame.x, frame.y));
                        let _ = w.request_inner_size(LogicalSize::new(frame.w, frame.h));
                    }
                }
                old => {
                    if let Some((_, win)) = old {
                        ctx.windows_mut().remove(&win);
                    }
                    let win = ctx.launch_window(window::config(me.clone(), id, frame));
                    *shown = Some((id, win));
                }
            }
        }));
    }

    /// Session `id`'s window goes, on the event loop.
    fn close(&self, id: u64) {
        let me = self.me.clone();
        drop(self.proxy.post_callback(move |ctx| {
            let Some(me) = me.upgrade() else { return };
            let mut shown = lock(&me.shown);
            if let Some((sid, win)) = *shown
                && sid == id
            {
                *shown = None;
                ctx.windows_mut().remove(&win);
            }
        }));
    }
}
