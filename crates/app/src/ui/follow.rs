//! The transcript's hold on its bottom. Every layout that grows the text
//! (a streamed token, a new message) asks one thing first: was the reader at
//! the bottom of the old text, within [`SLACK`]? Then it follows; a wheel,
//! trackpad, scrollbar drag or key that moved away leaves it where it is.
//! Another chat (or entering one) starts at its bottom. Freya reports a size
//! on every layout, a scroll's too: only a changed one counts, or a slow
//! scroll up near the bottom is pulled back down at every step.

use freya::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// How far above the bottom still counts as at the bottom, in pixels.
const SLACK: f32 = 40.0;

#[derive(Default)]
struct Seen {
    content: f32,
    viewport: f32,
    /// The chat laid out last; None while the band shows the selection.
    chat: Option<String>,
    /// A jump to the end is on its way: the position is not there yet.
    going: bool,
}

/// Sizes come in logical pixels, divided from physical ones.
fn same(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.5
}

#[derive(Clone)]
pub struct Follow {
    scroll: ScrollController,
    seen: Rc<RefCell<Seen>>,
}

pub fn use_follow() -> Follow {
    let scroll = use_scroll_controller(ScrollConfig::default);
    let seen = use_hook(|| Rc::new(RefCell::new(Seen::default())));
    Follow { scroll, seen }
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
        let stick = chat.is_some() && (other || self.at_bottom(&s));
        s.content = height;
        s.chat = chat.map(str::to_string);
        if stick {
            self.to_end(&mut s);
        }
    }

    /// The window around the text laid out at `height`.
    pub fn viewport(&self, height: f32) {
        let mut s = self.seen.borrow_mut();
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
