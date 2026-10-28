//! `[dictation] revert`: once a dictation's rewrite is pasted, the dictation
//! shows in a small box by the text (ui/revert.rs) with Revert; with
//! `rewrite_replaces = false` the dictation stays and the box offers the
//! rewrite instead (Use rewrite), and switches that setting for next time.
//! What is in the app stays: any key, a click outside the box, the next job or
//! [`OFFER`] ends the offer. A click in the box gives it the keyboard (its
//! text can be selected and copied): from then on only a click outside or
//! the box's Esc ends it. Revert selects the rewrite (else backspaces it) and
//! pastes the dictation, so it is offered only while nothing can have moved
//! the cursor. macOS only: Wayland lets no window place itself.

use super::{Panel, spawn};
use crate::ui::revert::{self, BoxKind};
use blitznir_core::platform::{Focus, KeyEvent, Rect};
use blitznir_core::store::Made;
use blitznir_core::{lock, sky, textproc};
use freya::winit::window::WindowId;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};

/// Long enough to read two lines and decide; any key ends it sooner.
const OFFER: Duration = Duration::from_secs(12);
/// After ⌘⇧4 and the like, clicks are the screenshot's for this long.
const SHOT: Duration = Duration::from_secs(20);
/// The box, in points, and its gap to the text.
const SMALL: (f64, f64) = (400.0, 132.0);
/// Past about a sentence it widens to [`WIDE`] and grows a line at a time to
/// [`TALL`], so two versions can be read and compared without scrolling.
const SENTENCE: usize = 90;
const WIDE: f64 = 680.0;
const TALL: f64 = 420.0;
/// The box's own rows (head, buttons, switch, padding) and its text's line,
/// at 12 pt: about 6.6 pt a character on average.
const CHROME: f64 = 100.0;
/// The Replace/Suggest row, which a translation's box has not.
const SWITCH_ROW: f64 = 26.0;
const LINE: f64 = 16.0;
const CHAR_W: f64 = 6.6;

/// The box for `text`: the small one for a sentence, else wide and as tall
/// as its lines need, up to [`TALL`] (then it scrolls).
fn size(text: &str) -> (f64, f64) {
    if text.chars().count() <= SENTENCE && !text.contains('\n') {
        return SMALL;
    }
    let per_line = ((WIDE - 24.0) / CHAR_W).floor().max(1.0) as usize;
    let lines: usize = (text.lines())
        .map(|l| l.chars().count().div_ceil(per_line).max(1))
        .sum();
    (WIDE, (CHROME + lines as f64 * LINE).clamp(SMALL.1, TALL))
}
const GAP: f64 = 6.0;
/// Below the menu bar: a box above a first line would sit under it.
const TOP: f64 = 30.0;
/// Above the Dock at its default size: winit gives a screen's whole frame,
/// not the part the Dock leaves.
const BOTTOM: f64 = 80.0;

/// The open offer.
pub struct Offer {
    id: u64,
    /// What the app holds now, and what the box offers in its place.
    there: String,
    other: String,
    to: Focus,
    frame: Rect,
    win: Option<WindowId>,
    /// Clicked: the box has the keyboard, and the timer no longer runs.
    held: bool,
    /// A screenshot began then: its region is dragged out with clicks.
    shot: Option<Instant>,
}

/// Above the text where it fits, else below; without its bounds, above the
/// mouse. Never past the screen's bottom, where its buttons could not be
/// clicked: over the text's end, if a tall selection leaves no room. Kept
/// on the screen of the text or mouse (`screens`, in points: one
/// left of or above the main screen has negative coordinates).
fn frame(
    (w, h): (f64, f64),
    text: Option<Rect>,
    mouse: Option<(f64, f64)>,
    screens: &[Rect],
) -> Option<Rect> {
    let at = text.map(|r| (r.x, r.y)).or(mouse)?;
    let all = Rect {
        w: f64::MAX,
        h: f64::MAX,
        ..Rect::default()
    };
    let s = (screens.iter().find(|s| s.contains(at)))
        .or(screens.first())
        .map_or(all, |s| *s);
    let (top, bottom) = (s.y + TOP, s.y + s.h - BOTTOM);
    let (w, h) = (w.min(s.w - 16.0), h.min(bottom - top)); // a small screen
    let (x, y) = match (text, mouse) {
        (Some(r), _) if r.y - h - GAP >= top => (r.x, r.y - h - GAP),
        (Some(r), _) => (r.x, r.y + r.h + GAP),
        (None, Some((mx, my))) => (mx - w / 2.0, (my - h - 2.0 * GAP).max(top)),
        (None, None) => return None,
    };
    let right = (s.x + s.w - w - 8.0).max(s.x + 8.0);
    Some(Rect {
        x: x.clamp(s.x + 8.0, right),
        y: y.clamp(top, (bottom - h).max(top)),
        w,
        h,
    })
}

fn inside(r: &Rect, (x, y): (f64, f64)) -> bool {
    (r.x..=r.x + r.w).contains(&x) && (r.y..=r.y + r.h).contains(&y)
}

impl Panel {
    /// The dictation `original` was replaced by `rewrite` in `to`: offer it back.
    pub(super) fn offer_revert(self: &Arc<Self>, to: Focus, original: &str, rewrite: &str) {
        if cfg!(target_os = "macos") && self.cfg().dictation.revert {
            self.offer(to, rewrite, original, BoxKind::Revert);
        }
    }

    /// `[dictation] rewrite_replaces = false` (macOS): the dictation stays (a
    /// batch one, held back, is pasted now) and the box offers the rewrite.
    pub(super) fn suggest(&self, to: Focus, typed: usize, said: &str, rewrite: &str, made: &Made) {
        if let Err(e) = self.store.add_replacement(rewrite, said, made) {
            log::error!("history: {e}");
        }
        self.hide_and_wait();
        // a live one is typed already: its app only comes back to the front
        let back = if typed == 0 {
            (self.platform.set_clipboard(said)).and_then(|()| self.platform.paste_into(&to))
        } else {
            self.platform.backspace_into(&to, 0)
        };
        if !matches!(back, Ok(true)) {
            log::warn!("dictation rewrite: the dictation's app did not come back: {back:?}");
        }
        let recent = self.store.history();
        self.with_ui(|ui| ui.recent = recent);
        let offered = self
            .me
            .upgrade()
            .is_some_and(|me| me.offer(to, said, rewrite, BoxKind::Suggest));
        if !offered {
            // no place for the box: the rewrite must not end up only in the history
            if let Err(e) = self.platform.set_clipboard(rewrite) {
                log::error!("clipboard: {e}");
            }
            self.tray.flash("Rewrite on the clipboard");
        }
    }

    /// The box over `to`, which holds `there`, offering `other` in its place;
    /// false: nowhere to put it (neither the text's bounds nor the mouse).
    fn offer(self: &Arc<Self>, to: Focus, there: &str, other: &str, kind: BoxKind) -> bool {
        // where the effect aims: a field or a window is no place for a box
        // by the text, the mouse in it, which just selected the text, is
        let mouse = self.platform.mouse();
        let text = sky::aim(lock(&self.view).bounds, mouse).map(|(r, _)| r);
        let screens = self.on_loop(|ctx| {
            let all = ctx.active_event_loop.available_monitors();
            all.map(|m| crate::zap::screen(&m).1).collect::<Vec<_>>()
        });
        let (w, h) = size(other);
        let switch = if kind == BoxKind::Translation {
            SWITCH_ROW
        } else {
            0.0
        };
        let size = (w, h - switch);
        let Some(frame) = frame(size, text, mouse, &screens.unwrap_or_default()) else {
            return false;
        };
        let id = self.shots.fetch_add(1, Ordering::SeqCst);
        let offer = Offer {
            id,
            there: there.into(),
            other: other.into(),
            to,
            frame,
            win: None,
            held: false,
            shot: None,
        };
        if let Some(old) = lock(&self.revert).replace(offer) {
            self.close_box(old.win);
        }
        self.platform.show_quietly(); // the paste hid our windows
        let shown = revert::Shown {
            text: other.to_string(),
            kind,
            replaces: self.cfg().dictation.rewrite_replaces,
        };
        let me = self.clone();
        drop(self.proxy.post_callback(move |ctx| {
            let win = ctx.launch_window(revert::config(me.clone(), shown, frame));
            match lock(&me.revert).as_mut() {
                Some(o) if o.id == id => o.win = Some(win),
                _ => drop(ctx.windows_mut().remove(&win)), // ended before it showed
            }
        }));
        let me = self.me.clone();
        let timer = move || {
            thread::sleep(OFFER);
            let held = |me: &Panel| lock(&me.revert).as_ref().is_some_and(|o| o.held);
            if let Some(me) = me.upgrade()
                && !held(&me)
            {
                me.end_revert(Some(id));
            }
        };
        if let Err(e) = spawn("revert-offer", timer) {
            log::warn!("revert: {e}");
        }
        log::info!("revert offered at {:.0}, {:.0}", frame.x, frame.y);
        true
    }

    /// Every key and click the listener hears (none of ours): a key or a
    /// click outside the box keeps the rewrite.
    pub fn saw(&self, ev: KeyEvent) {
        if self.recorded(ev) {
            return; // a Settings hotkey field took it
        }
        let mut offer = lock(&self.revert);
        let Some(o) = offer.as_mut() else {
            return;
        };
        let ends = match ev {
            KeyEvent::Other | KeyEvent::FKey(_) => !o.held, // held: typed into the box
            KeyEvent::Click => {
                let hit = self.platform.mouse().is_some_and(|m| inside(&o.frame, m));
                o.held |= hit;
                let shooting = o.shot.is_some_and(|t| t.elapsed() < SHOT);
                !hit && !shooting
            }
            // a screenshot of the box: it stays, through the region's drag
            KeyEvent::Screenshot => {
                (o.held, o.shot) = (true, Some(Instant::now()));
                false
            }
            KeyEvent::Modifier { .. } | KeyEvent::Reset => false,
        };
        drop(offer);
        if ends {
            self.end_revert(None);
        }
    }

    /// End the offer (`id`: only that one, for its timer); the rewrite stays.
    pub fn end_revert(&self, id: Option<u64>) {
        let mut offer = lock(&self.revert);
        if offer
            .as_ref()
            .is_some_and(|o| id.is_none_or(|id| o.id == id))
        {
            let win = offer.take().and_then(|o| o.win);
            drop(offer);
            self.close_box(win);
        }
    }

    /// The box's Esc or its x: it may have had the keyboard, which goes back to the app.
    pub fn keep_rewrite(self: &Arc<Self>) {
        let Some(to) = lock(&self.revert).as_ref().map(|o| o.to) else {
            return;
        };
        self.end_revert(None);
        let me = self.clone();
        let back = move || {
            // no characters: only the app brought back to the front
            if let Err(e) = me.platform.backspace_into(&to, 0) {
                log::warn!("revert: back to the app: {e}");
            }
        };
        if let Err(e) = spawn("revert-keep", back) {
            log::warn!("revert: {e}");
        }
    }

    fn close_box(&self, win: Option<WindowId>) {
        if let Some(win) = win {
            drop(self.proxy.post_callback(move |ctx| {
                ctx.windows_mut().remove(&win);
            }));
        }
    }

    /// A translation of text the app cannot take back (a web page), from
    /// the quick hotkey: in the box by the text rather than the whole panel;
    /// false: nowhere to put it (macOS only).
    pub(super) fn offer_translation(self: &Arc<Self>, out: &str) -> bool {
        let to = lock(&self.view).prev;
        // a model's answer often ends in a newline: no empty line in the box
        cfg!(target_os = "macos") && self.offer(to, "", out.trim(), BoxKind::Translation)
    }

    /// The translation box's Copy: its text on the clipboard; the box closes.
    pub fn copy_offer(self: &Arc<Self>) {
        let Some(text) = lock(&self.revert).as_ref().map(|o| o.other.clone()) else {
            return;
        };
        self.end_revert(None);
        match self.platform.set_clipboard(&text) {
            Ok(()) => self.tray.flash("Copied"),
            Err(e) => log::error!("copy: {e}"),
        }
    }

    /// The translation box's Open in Blitznir: the panel, on its chat.
    pub fn open_offer(self: &Arc<Self>) {
        self.end_revert(None);
        self.show();
    }

    /// The box's switch: `[dictation] rewrite_replaces` in config.toml.
    pub fn set_rewrite_replaces(self: &Arc<Self>, on: bool) {
        self.write_key(("dictation", "rewrite_replaces"), on);
    }

    /// The box's Revert or Use rewrite: what the app holds is taken back and
    /// the other text pasted in its place.
    pub fn revert(self: &Arc<Self>) {
        let Some(offer) = lock(&self.revert).take() else {
            return;
        };
        self.close_box(offer.win);
        drop(self.last.take()); // Undo last replace would undo the wrong paste
        self.tray.refresh();
        let me = self.clone();
        let undo = move || {
            let n = textproc::backspaces(&offer.there);
            let back = me.platform.backspace_into(&offer.to, n).and_then(|ok| {
                me.platform.set_clipboard(&offer.other)?;
                Ok(ok && me.platform.paste_into(&offer.to)?)
            });
            match back {
                Ok(true) => log::info!("revert: {n} chars swapped for the box's text"),
                Ok(false) => me.tray.flash("Not swapped: the text is on the clipboard"),
                Err(e) => {
                    log::error!("revert: {e}");
                    me.tray.flash("Revert failed: see blitznir.log");
                }
            }
        };
        if let Err(e) = spawn("revert", undo) {
            log::error!("revert: {e}");
        }
    }
}

#[cfg(test)]
mod tests;
