//! Finding your way in Settings: the sidebar has a search (the form shows
//! what matches on every page, core's form::cards; in config.toml it finds
//! the text, ui/find.rs) and, with the form, lists its pages: a press shows
//! that one. About is a page of its own, below a rule. ⌘F is the search.

use super::keys::field_keys;
use super::style::{Look, Palette, button, pressable, use_palette};
use crate::panel::view::Ui;
use blitznir_core::config::form::{SECTIONS, TABLES, cards};
use freya::code_editor::CodeEditorData;
use freya::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

/// The form's pages in order: the two tables, then SECTIONS.
fn titles() -> Vec<&'static str> {
    (0..2 + SECTIONS.len()).map(|i| page_title(i).0).collect()
}

/// Page `i`'s title and what it is for.
pub fn page_title(i: usize) -> (&'static str, &'static str) {
    match i {
        0 => (
            TABLES[0],
            "Up to 6, shown under the text field, left of the Chat button",
        ),
        1 => (
            TABLES[1],
            "OpenAI-compatible endpoints; the model picker lists their models",
        ),
        _ => (SECTIONS[i - 2].title, SECTIONS[i - 2].about),
    }
}

/// Wheel points that turn the sidebar one page: a trackpad sends many small
/// steps, a mouse wheel few large ones.
const WHEEL_STEP: f64 = 40.0;
/// At most one page per this long: a scroll goes on page by page, and the
/// steps in between are dropped, so a trackpad swipe's momentum turns a few
/// more and stops rather than racing on to About.
const TURN_GAP: Duration = Duration::from_millis(120);

/// What the sidebar and the form share: the page, the search, the scroll.
#[derive(Clone, PartialEq)]
pub struct FormNav {
    pub scroll: ScrollController,
    /// The page shown: an index of [`titles`].
    pub page: State<usize>,
    /// The page's Advanced settings unfolded.
    pub advanced: State<bool>,
    /// The search: what the form shows.
    pub query: State<String>,
    /// The search field, for ⌘F, and ⌘F's ask while the sidebar was away:
    /// the field made next focuses itself ([`use_find_focus`]).
    pub search_id: AccessibilityId,
    pub find: State<bool>,
    /// About is shown in the form's place.
    pub about: State<bool>,
    /// config.toml's editor and its scroll, for the search there.
    pub editor: State<CodeEditorData>,
    pub text_scroll: ScrollController,
    /// The search's match in config.toml: which one and how many; None
    /// when nothing matches.
    pub found: State<Option<(usize, usize)>>,
}

/// Made once by the window, above both the sidebar and the form.
pub fn use_form_nav(editor: State<CodeEditorData>) -> FormNav {
    let scroll = use_scroll_controller(ScrollConfig::default);
    let text_scroll = use_scroll_controller(ScrollConfig::default);
    let found = use_state(|| None);
    let page = use_state(|| 0);
    let advanced = use_state(|| false);
    let search_id = use_a11y();
    let find = use_state(|| false);
    let query = use_state(String::new);
    let about = use_state(|| false);
    use_provide_context(|| FormNav {
        scroll,
        page,
        advanced,
        query,
        search_id,
        find,
        about,
        editor,
        text_scroll,
        found,
    })
}

/// A search field, as it is made: when ⌘F asked for it, it takes the focus
/// a frame later, after whatever asked for it in the frame that made it
/// (the composer's own auto focus), and the ask is taken.
pub fn use_find_focus(nav: &FormNav) {
    let (mut find, id) = (nav.find, nav.search_id);
    use_hook(move || {
        if *find.peek() {
            find.set(false);
            spawn(async move {
                timer(std::time::Duration::from_millis(16)).await;
                id.request_focus();
            });
        }
    });
}

impl FormNav {
    /// Show page `i` from its top (from About or a search too).
    pub fn go(&self, i: usize) {
        let (mut about, mut page, mut advanced) = (self.about, self.page, self.advanced);
        let (mut query, mut scroll) = (self.query, self.scroll);
        about.set_if_modified(false);
        query.set_if_modified(String::new());
        if *page.peek() != i {
            advanced.set(false);
        }
        page.set(i);
        scroll.scroll_to_y(0);
    }
}

/// The sidebar while the form is open: its sections.
#[derive(PartialEq)]
pub struct FormIndex {
    pub width: f32,
}

fn entry(p: &Palette, nav: &FormNav, i: usize, title: &'static str, here: bool) -> Element {
    let nav = nav.clone();
    let row = button(p, if here { Look::Default } else { Look::Tool })
        .width(Size::fill())
        .padding((5.0, 8.0))
        .child(rect().width(Size::fill()).child(label().text(title))); // Button centres
    pressable(row, move |_| nav.go(i)).key(i).into_element()
}

/// About, set apart from the pages by a rule when there are any above it:
/// not a setting.
fn about_entry(p: &Palette, nav: &FormNav, apart: bool) -> Vec<Element> {
    let mut about = nav.about;
    let row = button(p, if about() { Look::Default } else { Look::Tool })
        .width(Size::fill())
        .padding((5.0, 8.0))
        .child(rect().width(Size::fill()).child(label().text("About")));
    let rule = rect()
        .width(Size::fill())
        .height(Size::px(1.0))
        .margin((10.0, 0.0, 8.0, 0.0))
        .background(p.line);
    let row = pressable(row, move |_| about.set(true)).into();
    if apart {
        vec![rule.into(), row]
    } else {
        vec![row]
    }
}

impl FormNav {
    /// Select match `n` of the search in config.toml.
    fn find(&self, n: usize) {
        let (mut editor, mut found) = (self.editor, self.found);
        let query = self.query.peek().clone();
        found.set(super::find::select(
            &mut editor.write(),
            &query,
            n,
            self.text_scroll,
        ));
    }
}

/// The search: typing narrows the form (from its top) or finds the first
/// match in config.toml, Return the next there; Esc empties it.
fn search(p: &Palette, nav: &FormNav) -> Element {
    use_find_focus(nav);
    let (mut query, mut scroll, mut about) = (nav.query, nav.scroll, nav.about);
    let ui = use_consume::<State<Ui>>();
    let last = use_hook(|| Rc::new(RefCell::new((String::new(), true))));
    let finder = nav.clone();
    use_side_effect(move || {
        // only a changed search (or tab): the scroll's bounds, read on the
        // way, rerun this at every layout
        let now = (query.read().clone(), ui.read().form);
        if *last.borrow() != now {
            *last.borrow_mut() = now.clone();
            about.set_if_modified(false); // a search is of the settings
            if now.1 {
                scroll.scroll_to_y(0);
            } else {
                finder.find(0);
            }
        }
    });
    let next = nav.clone();
    let keys = move |e: Event<KeyboardEventData>| {
        field_keys(&e, |e| match e.key {
            Key::Named(NamedKey::Escape) if !query.read().is_empty() => {
                query.set(String::new());
                Some(false)
            }
            Key::Named(NamedKey::Enter) if !ui.peek().form => {
                let n = next.found.peek().map_or(0, |(i, _)| i + 1);
                next.find(n);
                Some(false)
            }
            _ => None,
        })
    };
    Input::new(query)
        .a11y_id(nav.search_id)
        .compact()
        .placeholder("Search settings")
        .width(Size::fill())
        .background(p.field)
        .focus_background(p.field)
        .border_fill(p.line)
        .focus_border_fill(p.accent)
        .color(p.fg)
        .placeholder_color(p.muted)
        .on_pre_key_down(keys)
        .into_element()
}

/// The pages and About, scrolling when the window is too short for them.
/// `room` and `tall` are measured: its height on screen and its own.
fn list(rows: Vec<Element>, (mut room, mut tall): (State<f32>, State<f32>)) -> Rect {
    rect()
        .width(Size::fill())
        .height(Size::flex(1.0))
        .on_sized(move |e: Event<SizedEventData>| room.set_if_modified(e.area.height()))
        .child(
            ScrollView::new()
                .width(Size::fill())
                .height(Size::fill())
                .child(
                    rect()
                        .width(Size::fill())
                        .spacing(2.0)
                        .on_sized(move |e: Event<SizedEventData>| {
                            tall.set_if_modified(e.area.height())
                        })
                        .children(rows),
                ),
        )
}

/// The wheel over the sidebar turns the pages, About last, at most one per
/// [`TURN_GAP`].
/// Not while `turns` is false (a search's list), nor while the list is too
/// tall for its room: then the wheel scrolls it.
fn use_page_wheel(
    nav: &FormNav,
    turns: bool,
    (room, tall): (State<f32>, State<f32>),
) -> impl FnMut(Event<WheelEventData>) + 'static {
    // the wheel's sum since the last page, and when that page turned
    let wheel = use_hook(|| Rc::new(Cell::new((0.0_f64, Instant::now() - TURN_GAP))));
    let turn = nav.clone();
    move |e: Event<WheelEventData>| {
        if !turns {
            return;
        }
        e.stop_propagation();
        if *tall.peek() > *room.peek() + 1.0 {
            return; // the list's own scroll view has it
        }
        let (sum, turned) = wheel.get();
        if turned.elapsed() < TURN_GAP {
            return; // dropped, not saved up for a burst after
        }
        let sum = sum - e.delta_y;
        if sum.abs() < WHEEL_STEP {
            wheel.set((sum, turned));
            return;
        }
        wheel.set((0.0, Instant::now()));
        let last = titles().len(); // About
        let now = if *turn.about.peek() {
            last
        } else {
            *turn.page.peek()
        };
        let to = if sum > 0.0 {
            (now + 1).min(last)
        } else {
            now.saturating_sub(1)
        };
        if to == last {
            let mut about = turn.about;
            about.set_if_modified(true);
        } else if to != now {
            turn.go(to);
        }
    }
}

/// Above the list, and set off from it: a title, the search, a rule.
fn heading(p: &Palette, nav: &FormNav) -> Rect {
    rect()
        .width(Size::fill())
        .spacing(2.0)
        .padding((0.0, 8.0))
        .margin((0.0, 0.0, 8.0, 0.0))
        .child(
            label()
                .text("Settings")
                .font_size(15.0)
                .font_weight(FontWeight::BOLD),
        )
        .child(rect().margin((4.0, 0.0, 0.0, 0.0)).child(search(p, nav)))
        .child(
            rect()
                .width(Size::fill())
                .height(Size::px(1.0))
                .margin((8.0, 0.0, 0.0, 0.0))
                .background(p.line),
        )
}

impl Component for FormIndex {
    fn render(&self) -> impl IntoElement {
        let p = use_palette();
        let nav = use_consume::<FormNav>();
        let searching = !nav.query.read().trim().is_empty();
        let here = (!*nav.about.read() && !searching).then(|| *nav.page.read());
        let form = use_consume::<State<Ui>>().read().form;
        // config.toml has no sections: the search finds in it instead
        let shown = if form {
            cards(&nav.query.read())
        } else {
            Vec::new()
        };
        let query = nav.query.read().trim().to_string();
        let found = (!form && !query.is_empty()).then(|| {
            let text = match *nav.found.read() {
                Some((i, n)) => format!("{} of {n} · {} next", i + 1, crate::panel::view::RET),
                None => "Not in config.toml".to_string(),
            };
            label()
                .text(text)
                .font_size(11.0)
                .color(p.muted)
                .margin((0.0, 8.0))
        });
        let entries: Vec<(usize, &'static str)> = (titles().into_iter().enumerate())
            .filter(|(i, _)| form && shown.get(*i).copied().unwrap_or(true))
            .collect();
        let apart = !entries.is_empty();
        // the list's room and its own height: it scrolls when they differ
        let (room, tall) = (use_state(|| 0.0_f32), use_state(|| 0.0_f32));
        let pages = use_page_wheel(&nav, form && !searching, (room, tall));
        rect()
            .width(Size::px(self.width))
            .height(Size::fill())
            .background(p.side)
            .on_wheel(pages)
            .padding((12.0, 6.0))
            .spacing(2.0)
            .content(Content::flex())
            .child(heading(&p, &nav))
            .maybe_child(found)
            .child(list(
                entries
                    .into_iter()
                    .map(|(i, t)| entry(&p, &nav, i, t, Some(i) == here))
                    .chain(about_entry(&p, &nav, apart))
                    .collect(),
                (room, tall),
            ))
    }
}
