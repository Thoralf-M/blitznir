//! The transcript's hold on its bottom. Every layout that grows the text
//! (a streamed token, a new message) asks one thing first: was the reader at
//! the bottom of the old text, within [`SLACK`]? Then it follows; a wheel,
//! trackpad, scrollbar drag or key that moved away leaves it where it is.
//! Another chat (or entering one) starts at its bottom. Freya reports a size
//! on every layout, a scroll's too: only a changed one counts, or a slow
//! scroll up near the bottom is pulled back down at every step. A selecting
//! drag past its top or bottom edge scrolls the text that way.

use freya::prelude::*;
use std::{cell::RefCell, rc::Rc, time::Duration};

/// How far above the bottom still counts as at the bottom, in pixels.
const SLACK: f32 = 40.0;

/// A drag scrolls a pixel a frame per this many pixels past the edge, up
/// to [`DRAG_MAX`]: a browser's pace, slow at the edge, a page a second
/// at 80 px out.
const DRAG_PER: f32 = 4.0;
const DRAG_MAX: f32 = 40.0;
const FRAME: Duration = Duration::from_millis(16);

#[derive(Default)]
struct Seen {
    content: f32,
    viewport: f32,
    /// The chat laid out last; None while the band shows the selection.
    chat: Option<String>,
    /// A jump to the end is on its way: the position is not there yet.
    going: bool,
    /// The next change of size is a fold opened or closed ([`Follow::hold`]).
    held: bool,
    /// The view's top and bottom edge in the window.
    edges: (f32, f32),
    /// The pointer's height in the window while a selecting drag runs.
    drag: Option<f32>,
}

/// Sizes come in logical pixels, divided from physical ones.
fn same(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.5
}

#[derive(Clone)]
pub struct Follow {
    scroll: ScrollController,
    seen: Rc<RefCell<Seen>>,
    /// The drag's frames, scrolling past an edge.
    edge: Rc<RefCell<Option<TaskHandle>>>,
}

pub fn use_follow() -> Follow {
    let scroll = use_scroll_controller(ScrollConfig::default);
    let seen = use_hook(|| Rc::new(RefCell::new(Seen::default())));
    let edge = use_hook(|| Rc::new(RefCell::new(None)));
    Follow { scroll, seen, edge }
}

impl Follow {
    pub fn controller(&self) -> ScrollController {
        self.scroll
    }

    fn at_bottom(&self, s: &Seen) -> bool {
        let (_, y): (i32, i32) = self.scroll.into();
        s.going || s.content - s.viewport + y as f32 <= SLACK
    }

    /// The text laid out at `height`, in `chat` (None: the selection).
    pub fn content(&self, height: f32, chat: Option<&str>) {
        let mut s = self.seen.borrow_mut();
        let other = s.chat.as_deref() != chat;
        if !other && same(s.content, height) {
            return;
        }
        let held = std::mem::take(&mut s.held);
        let stick = chat.is_some() && (other || (!held && self.at_bottom(&s)));
        s.content = height;
        s.chat = chat.map(str::to_string);
        if stick {
            self.to_end(&mut s);
        }
    }

    /// A fold opens or closes in the text: its next change of size keeps
    /// the view where it is, at the bottom too, so the fold's header stays
    /// under the pointer.
    pub fn hold(&self) {
        self.seen.borrow_mut().held = true;
    }

    /// The window around the text, laid out at `area`.
    pub fn viewport(&self, area: Area) {
        let mut s = self.seen.borrow_mut();
        s.edges = (area.min_y(), area.max_y());
        let height = area.height();
        if same(s.viewport, height) {
            return;
        }
        let stick = s.chat.is_some() && self.at_bottom(&s);
        s.viewport = height;
        if stick {
            self.to_end(&mut s);
        }
    }

    /// After this layout: the scroll view clamps to the old size before it.
    fn to_end(&self, s: &mut Seen) {
        s.going = true;
        let (mut scroll, seen) = (self.scroll, self.seen.clone());
        spawn(async move {
            scroll.scroll_to(ScrollPosition::End, Direction::Vertical);
            seen.borrow_mut().going = false;
        });
    }

    /// A selecting drag starts at `y`: until [`Follow::released`], each
    /// frame with the pointer past an edge scrolls the text that way; the
    /// selection's end follows the text moving under it.
    pub fn pressed(&self, y: f32) {
        self.seen.borrow_mut().drag = Some(y);
        let (seen, scroll) = (self.seen.clone(), self.scroll);
        let task = spawn(async move {
            loop {
                let Some(step) = past_edge(&seen.borrow()) else {
                    return;
                };
                let (_, y): (i32, i32) = scroll.into();
                let mut scroll = scroll;
                scroll.scroll_to_y(y - step);
                timer(FRAME).await;
            }
        });
        if let Some(old) = self.edge.replace(Some(task)) {
            old.cancel();
        }
    }

    pub fn dragged(&self, y: f32) {
        let mut s = self.seen.borrow_mut();
        if s.drag.is_some() {
            s.drag = Some(y);
        }
    }

    pub fn released(&self) {
        self.seen.borrow_mut().drag = None;
    }

    /// PageUp/PageDown by a page, Home/End to the ends; the arrows are the
    /// scroll view's. True when the key was taken.
    pub fn key(&self, e: &Event<KeyboardEventData>) -> bool {
        let page = (self.seen.borrow().viewport * 0.9).max(40.0) as i32;
        let (_, y): (i32, i32) = self.scroll.into();
        let mut scroll = self.scroll;
        match e.key {
            Key::Named(NamedKey::PageUp) => scroll.scroll_to_y(y + page),
            Key::Named(NamedKey::PageDown) => scroll.scroll_to_y(y - page),
            Key::Named(NamedKey::Home) => scroll.scroll_to_y(0),
            Key::Named(NamedKey::End) => {
                scroll.scroll_to(ScrollPosition::End, Direction::Vertical);
                true
            }
            _ => return false,
        };
        true
    }
}

/// The drag's scroll this frame, down positive; None once it ended.
fn past_edge(s: &Seen) -> Option<i32> {
    let y = s.drag?;
    let (top, bottom) = s.edges;
    let past = if y < top {
        y - top
    } else if y > bottom {
        y - bottom
    } else {
        0.0
    };
    let step = (past / DRAG_PER).clamp(-DRAG_MAX, DRAG_MAX);
    Some(if past != 0.0 && step.abs() < 1.0 {
        step.signum()
    } else {
        step
    } as i32)
}
