//! A chat's text that selects like any other app's: one drag from anywhere
//! to anywhere, across messages, text, code, quote and table, and ⌘C
//! copies it. freya's MarkdownViewer selects nothing, and SelectableText
//! stops at its own paragraph, so the chat keeps one selection of its own
//! over plain paragraphs (core's markdown::select does the arithmetic).
//! A link is its text plus a small button that opens it.

use super::icons::{self, icon_button};
use super::keys::command;
use super::style::{Palette, use_palette};
use crate::panel::Panel;
use blitznir_core::markdown::select::{self, Piece, Point, len16, range_in};
use blitznir_core::markdown::{Block, Run};
use freya::elements::paragraph::ParagraphCursorExt;
use freya::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

const SIZE: f32 = 13.0;
const CODE_SIZE: f32 = 12.0;

fn heading_size(level: u8) -> f32 {
    match level {
        1 => 20.0,
        2 => 18.0,
        3 => 16.0,
        4 => 14.0,
        _ => SIZE,
    }
}

fn span(p: &Palette, r: &Run) -> Span<'static> {
    let s = &r.style;
    let mut span = Span::new(r.text.clone()).color(if s.link.is_some() { p.accent } else { p.fg });
    if s.heading > 0 {
        span = span
            .font_size(heading_size(s.heading))
            .font_weight(FontWeight::BOLD);
    }
    if s.bold {
        span = span.font_weight(FontWeight::BOLD);
    }
    if s.italic {
        span = span.font_slant(FontSlant::Italic);
    }
    if s.strike {
        span = span.text_decoration(TextDecoration::LineThrough);
    } else if s.link.is_some() {
        span = span.text_decoration(TextDecoration::Underline);
    }
    if s.code {
        span = span.font_family("monospace").color(p.accent);
    }
    span
}

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
        let mut t = self.paragraph().font_size(SIZE).color(p.fg);
        for (i, r) in runs.iter().enumerate() {
            t = t.span(span(p, r));
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
            .color(self.p.fg);
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
}

pub fn use_selection() -> Selection {
    let sel = use_state(|| None);
    let dragging = use_state(|| false);
    let geos: Geos = use_hook(Geos::default);
    let pieces = use_hook(|| Rc::new(RefCell::new(Vec::new())));
    let pointer = use_hook(Rc::default);
    let s = Selection {
        geos,
        pieces,
        sel,
        dragging,
        pointer,
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

    pub fn down(&self, e: &Event<PointerEventData>) {
        if e.data().is_primary() {
            self.pointer.set(Some(e.global_location()));
            let at = self.hit(e.global_location());
            let (mut sel, mut dragging) = (self.sel, self.dragging);
            sel.set(Some((at, at)));
            dragging.set(true);
        }
    }

    pub fn moved(&self, e: &Event<PointerEventData>) {
        if (self.dragging)() {
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
        };
        let blocks = select::message(&self.text, self.markdown);
        rect().width(Size::fill()).spacing(10.0).children(
            (blocks.iter().enumerate())
                .map(|(i, b)| build.block(i, b))
                .collect::<Vec<Element>>(),
        )
    }
}
