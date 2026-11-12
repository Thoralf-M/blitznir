//! A chat's text that selects like any other app's: one drag from anywhere
//! to anywhere, across messages, text, code, quote and table, and ⌘C
//! copies it. freya's MarkdownViewer selects nothing, and SelectableText
//! stops at its own paragraph, so the chat keeps one selection of its own
//! over plain paragraphs (core's markdown::select does the arithmetic).
//! A link is its text plus a small button that opens it.

use super::{
    icons::{self, icon_button},
    keys::command,
    spans::span,
    style::{Palette, use_palette},
};
use crate::panel::Panel;
use blitznir_core::markdown::{
    Block, Run,
    select::{self, Piece, Point, len16, range_in},
};
use freya::{elements::paragraph::ParagraphCursorExt, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

pub(super) const SIZE: f32 = 13.0;
const CODE_SIZE: f32 = 12.0;

/// Where a piece of the answer was laid out, for hit-testing a drag.
#[derive(Default)]
struct Geo {
    holder: ParagraphHolder,
    area: Area,
}

type Geos = Rc<RefCell<Vec<Geo>>>;

/// The piece under `at` and the offset in it. Between pieces, or beside
/// one, the nearest piece above counts, so a drag through a gap goes on.
fn hit(geos: &[Geo], lens: &[usize], at: CursorPoint) -> Point {
    let (x, y) = (at.x as f32, at.y as f32);
    let inside = geos.iter().position(|g| g.area.contains((x, y).into()));
    let Some(k) = inside.or_else(|| geos.iter().rposition(|g| g.area.min_y() <= y)) else {
        return (0, 0);
    };
    let (g, len) = (&geos[k], lens.get(k).copied().unwrap_or(0));
    let (lx, ly) = (x - g.area.min_x(), y - g.area.min_y());
    if ly >= g.area.height() {
        return (k, len);
    }
    let at = g
        .holder
        .visible_location(CursorPoint::new(f64::from(lx), f64::from(ly)));
    let laid = g.holder.0.borrow();
    let Some(laid) = laid.as_ref() else {
        return (k, 0);
    };
    let scale = laid.scale_factor;
    let i = laid
        .paragraph
        .cursor_index_at_point((at.x * scale, at.y * scale));
    (k, i.min(len))
}

/// Builds the pieces in document order, each a paragraph that reports its
/// layout and draws its share of the selection.
struct Pieces<'a> {
    p: &'a Palette,
    panel: &'a Arc<Panel>,
    selection: &'a Selection,
    lens: &'a [usize],
    sel: Option<(Point, Point)>,
    next: Cell<usize>,
    /// The text's colour: muted for what a replacement replaced.
    fg: Color,
}

impl Pieces<'_> {
    fn paragraph(&self) -> Paragraph {
        let k = self.next.replace(self.next.get() + 1);
        let len = self.lens.get(k).copied().unwrap_or(0);
        let lit = self.sel.and_then(|(a, b)| range_in(k, len, a, b));
        let holder = (self.selection.geos.borrow().get(k)).map(|g| g.holder.clone());
        let selection = self.selection.clone();
        paragraph()
            .width(Size::fill())
            .holder(holder.unwrap_or_default())
            .highlights(lit.map(|r| vec![r]))
            .on_sized(move |e: Event<SizedEventData>| {
                if let Some(g) = selection.geos.borrow_mut().get_mut(k) {
                    g.area = e.area;
                }
                selection.follow(); // a scroll mid-drag moved the text, not the pointer
            })
    }

    /// `runs` as one paragraph, an open button after each link.
    fn text(&self, runs: &[Run]) -> Paragraph {
        let p = self.p;
        let mut t = self.paragraph().font_size(SIZE).color(self.fg);
        for (i, r) in runs.iter().enumerate() {
            t = t.span(span(p, r, self.fg));
            let Some(url) = &r.style.link else { continue };
            if runs.get(i + 1).and_then(|n| n.style.link.as_ref()) == Some(url) {
                continue; // the link goes on: its button comes after its last run
            }
            let (panel, url) = (self.panel.clone(), url.clone());
            let tip = format!("Open {url}");
            t = t.child(icon_button(p, icons::LINK, tip, true, move |_| {
                panel.open_link(&url);
            }));
        }
        t
    }

    fn code(&self, code: &str) -> Rect {
        let text = self
            .paragraph()
            .span(Span::new(code.to_string()))
            .font_family("monospace") // Menlo on macOS: daemon.rs registers it
            .font_size(CODE_SIZE)
            .color(self.fg);
        rect()
            .width(Size::fill())
            .padding((8.0, 10.0))
            .corner_radius(6.0)
            .background(self.p.soft)
            .child(text)
    }

    fn quote(&self, runs: &[Run]) -> Rect {
        let bar = rect()
            .width(Size::px(3.0))
            .height(Size::fill_minimum()) // the text's height, with Content::fit below
            .background(self.p.line);
        let body = rect()
            .width(Size::fill())
            .padding((8.0, 10.0))
            .child(self.text(runs).font_slant(FontSlant::Italic));
        rect()
            .width(Size::fill())
            .horizontal()
            .content(Content::fit())
            .background(self.p.soft)
            .child(bar)
            .child(body)
    }

    fn table(&self, rows: &[Vec<Vec<Run>>]) -> Table {
        let row = |(i, cells): (usize, &Vec<Vec<Run>>)| {
            TableRow::new().key(i).children(cells.iter().map(|c| {
                let cell = self.text(c).font_size(CODE_SIZE);
                let cell = if i == 0 {
                    cell.font_weight(FontWeight::BOLD)
                } else {
                    cell
                };
                cell.into_element()
            }))
        };
        Table::new().children(rows.iter().enumerate().map(row))
    }

    fn block(&self, i: usize, b: &Block) -> Element {
        match b {
            Block::Text(runs) => self.text(runs).key(i).into_element(),
            Block::Code(c) => self.code(c).key(i).into_element(),
            Block::Quote(runs) => self.quote(runs).key(i).into_element(),
            Block::Table(rows) => self.table(rows).key(i).into_element(),
            Block::Rule => rect()
                .key(i)
                .width(Size::fill())
                .height(Size::px(1.0))
                .background(self.p.line)
                .into_element(),
        }
    }
}

/// The chat's own run of quick presses, apart from a text field's.
#[derive(Clone, PartialEq)]
struct ChatPress;

/// One laid-out line of text, in the window's points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Line {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub base: f32,
    pub bottom: f32,
}

/// One selection over a whole chat, yours and the answers' words alike:
/// the transcript owns it (its drag, its focus, its keys) and every
/// [`Body`] draws its share. Pieces are numbered across the chat.
#[derive(Clone)]
pub struct Selection {
    geos: Geos,
    pieces: Rc<RefCell<Vec<Piece>>>,
    sel: State<Option<(Point, Point)>>,
    dragging: State<bool>,
    /// Where the pointer was last seen during a drag, in the window.
    pointer: Rc<Cell<Option<CursorPoint>>>,
    /// The press picked a word or paragraph: its drag keeps it.
    held: Rc<Cell<bool>>,
}

pub fn use_selection() -> Selection {
    let sel = use_state(|| None);
    let dragging = use_state(|| false);
    let geos: Geos = use_hook(Geos::default);
    let pieces = use_hook(|| Rc::new(RefCell::new(Vec::new())));
    let pointer = use_hook(Rc::default);
    let held = use_hook(Rc::default);
    let s = Selection {
        geos,
        pieces,
        sel,
        dragging,
        pointer,
        held,
    };
    use_provide_context(|| s.clone())
}

impl Selection {
    /// The chat's pieces, in order, before its bodies are drawn.
    pub fn set_pieces(&self, pieces: Vec<Piece>) {
        self.geos
            .borrow_mut()
            .resize_with(pieces.len(), Geo::default);
        *self.pieces.borrow_mut() = pieces;
    }

    /// The lines of pieces `from..` as laid out, in order: where an
    /// answer's text is, for the thinking wave.
    pub fn lines(&self, from: usize) -> Vec<Line> {
        let mut out = vec![];
        for g in self.geos.borrow().iter().skip(from) {
            let laid = g.holder.0.borrow();
            let Some(laid) = laid.as_ref() else { continue };
            let s = laid.scale_factor as f32;
            for m in laid.paragraph.get_line_metrics() {
                let base = g.area.min_y() + m.baseline as f32 / s;
                let left = g.area.min_x() + m.left as f32 / s;
                out.push(Line {
                    left,
                    right: left + m.width as f32 / s,
                    top: base - m.ascent as f32 / s,
                    base,
                    bottom: base + m.descent as f32 / s,
                });
            }
        }
        out
    }

    fn lens(&self) -> Vec<usize> {
        self.pieces
            .borrow()
            .iter()
            .map(|p| len16(&p.text))
            .collect()
    }

    fn hit(&self, at: CursorPoint) -> Point {
        hit(&self.geos.borrow(), &self.lens(), at)
    }

    /// A press starts a drag; a second quick one there selects the word
    /// under it, a third its paragraph, and holds it (its drag moves nothing).
    pub fn down(&self, e: &Event<PointerEventData>) {
        if !e.data().is_primary() {
            return;
        }
        let g = e.global_location();
        let (k, at) = self.hit(g);
        let text = (self.pieces.borrow().get(k)).map_or(String::new(), |p| p.text.clone());
        let (from, to, drag) = match EventsCombos::<ChatPress>::pressed(g) {
            PressEventType::Single => (at, at, true),
            PressEventType::Double => {
                let (a, b) = select::word(&text, at);
                (a, b, false)
            }
            _ => (0, len16(&text), false),
        };
        self.pointer.set(drag.then_some(g));
        self.held.set(!drag);
        let (mut sel, mut dragging) = (self.sel, self.dragging);
        sel.set(Some(((k, from), (k, to))));
        dragging.set(true);
    }

    pub fn moved(&self, e: &Event<PointerEventData>) {
        if (self.dragging)() && !self.held.get() {
            self.pointer.set(Some(e.global_location()));
            self.follow();
        }
    }

    /// The drag's end under the pointer's last place, also after a scroll moved
    /// the text under a still pointer; only a change is written (it lays out again).
    fn follow(&self) {
        let Some(at) = self.pointer.get().filter(|_| *self.dragging.peek()) else {
            return;
        };
        let at = self.hit(at);
        if self.sel.peek().is_some_and(|(_, to)| to != at) {
            let mut sel = self.sel;
            sel.with_mut(|mut s| {
                if let Some((_, to)) = s.as_mut() {
                    *to = at;
                }
            });
        }
    }

    pub fn released(&self) {
        let mut dragging = self.dragging;
        dragging.set(false);
        self.pointer.set(None);
    }

    /// A press elsewhere, or another chat, clears; the transcript's own press
    /// sets `dragging` first or right after, in either order.
    pub fn clear(&self) {
        if !*self.dragging.peek() {
            let mut sel = self.sel;
            sel.set(None);
        }
    }

    /// ⌘C copies the selection, ⌘A selects the chat: true = taken.
    pub fn key(&self, e: &Event<KeyboardEventData>, panel: &Arc<Panel>) -> bool {
        let Key::Character(c) = &e.key else {
            return false;
        };
        if !command(e.modifiers) {
            return false;
        }
        match c.as_str() {
            "c" => {
                let text = (self.sel)().map(|(a, b)| select::selected(&self.pieces.borrow(), a, b));
                let Some(text) = text.filter(|t| !t.is_empty()) else {
                    return false;
                };
                panel.copy(text, "Copied".into());
                true
            }
            "a" => {
                let lens = self.lens();
                let Some(last) = lens.len().checked_sub(1) else {
                    return false;
                };
                let mut sel = self.sel;
                sel.set(Some(((0, 0), (last, lens[last]))));
                true
            }
            _ => false,
        }
    }
}

/// A message's text: an answer's markdown or the user's literal words,
/// its pieces numbered from `base` in the chat's [`Selection`].
#[derive(PartialEq)]
pub struct Body {
    pub text: String,
    pub markdown: bool,
    pub base: usize,
    pub muted: bool,
}

impl Component for Body {
    fn render(&self) -> impl IntoElement {
        let p = use_palette();
        let panel = use_consume::<Arc<Panel>>();
        let s = use_consume::<Selection>();
        let lens = s.lens();
        let build = Pieces {
            p: &p,
            panel: &panel,
            selection: &s,
            lens: &lens,
            sel: (s.sel)(),
            next: Cell::new(self.base),
            fg: if self.muted { p.muted } else { p.fg },
        };
        let blocks = select::message(&self.text, self.markdown);
        rect().width(Size::fill()).spacing(10.0).children(
            (blocks.iter().enumerate())
                .map(|(i, b)| build.block(i, b))
                .collect::<Vec<Element>>(),
        )
    }
}
