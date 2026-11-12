//! Text to select and copy, as Freya's SelectableText (v0.5.0-rc.9,
//! freya-components/src/selectable_text.rs) but with the I-beam only over
//! the glyphs: a text that fills its row takes a press beside a short line,
//! and the pointer there stays an arrow, as in a browser. In the band
//! ([`Selectable::in_band`]), a drag past its edge scrolls it
//! ([`Follow::pressed`]) and the selection follows the text moving under a
//! still pointer.

use super::follow::Follow;
use freya::{prelude::*, text_edit::*};
use std::{cell::Cell, rc::Rc};

#[derive(Clone, PartialEq)]
pub struct Selectable {
    spans: Vec<Span<'static>>,
    layout: LayoutData,
    accessibility: AccessibilityData,
    text_style_data: TextStyleData,
    event_handlers: EventHandlers,
    layer: Layer,
    max_lines: Option<usize>,
    in_band: bool,
    key: DiffKey,
}

impl KeyExt for Selectable {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl LayoutExt for Selectable {
    fn layout_mut(&mut self) -> &mut LayoutData {
        &mut self.layout
    }
}

impl ContainerExt for Selectable {}

impl TextOverflowExt for Selectable {}

impl AccessibilityExt for Selectable {
    fn accessibility_data_mut(&mut self) -> &mut AccessibilityData {
        &mut self.accessibility
    }
}

impl TextStyleExt for Selectable {
    fn text_style_data_mut(&mut self) -> &mut TextStyleData {
        &mut self.text_style_data
    }
}

impl EventHandlersExt for Selectable {
    fn event_handlers_mut(&mut self) -> &mut EventHandlers {
        &mut self.event_handlers
    }
}

impl LayerExt for Selectable {
    fn layer_mut(&mut self) -> &mut Layer {
        &mut self.layer
    }
}

impl Selectable {
    pub fn new() -> Self {
        Self {
            spans: Vec::new(),
            layout: LayoutData::default(),
            accessibility: AccessibilityData::default(),
            text_style_data: TextStyleData::default(),
            event_handlers: EventHandlers::default(),
            layer: Layer::default(),
            max_lines: None,
            in_band: false,
            key: DiffKey::None,
        }
    }

    pub fn span(mut self, span: impl Into<Span<'static>>) -> Self {
        self.spans.push(span.into());
        self
    }

    /// Text in the band's own scroll: a drag past its edge scrolls it.
    /// Not in a box with a scroll of its own (a trace): its size reports
    /// shifted that box's hold on its newest line.
    pub fn in_band(mut self) -> Self {
        self.in_band = true;
        self
    }

    pub fn max_lines(mut self, max_lines: impl Into<Option<usize>>) -> Self {
        self.max_lines = max_lines.into();
        self
    }

    fn text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_ref()).collect()
    }
}

/// Whether `at` (in the element) is over a line's glyphs: the line is the
/// first whose bottom is below `at`, so the gap between lines counts as the
/// line below it and the pointer does not flicker there.
fn over_glyphs(holder: &ParagraphHolder, at: CursorPoint) -> bool {
    let at = holder.visible_location(at);
    let laid = holder.0.borrow();
    let Some(laid) = laid.as_ref() else {
        return false;
    };
    let (x, y) = (at.x * laid.scale_factor, at.y * laid.scale_factor);
    let lines = laid.paragraph.get_line_metrics();
    lines
        .iter()
        .find(|l| y < l.baseline + l.descent)
        .is_some_and(|l| y >= 0.0 && x >= l.left && x <= l.left + l.width)
}

/// A drag above the text selects to its start and one below it to its end,
/// as on a page; Skia would keep the column and stop mid-line.
fn past_ends(holder: &ParagraphHolder, at: CursorPoint) -> CursorPoint {
    let laid = holder.0.borrow();
    let Some(laid) = laid.as_ref() else {
        return at;
    };
    let pad = laid.padding;
    let top = f64::from(pad.top()) / laid.scale_factor;
    let bottom = top + f64::from(laid.paragraph.height()) / laid.scale_factor;
    if at.y < top {
        CursorPoint::new(f64::from(pad.left()) / laid.scale_factor, top)
    } else if at.y >= bottom {
        CursorPoint::new(1e6, bottom - 1.0) // past any line's end
    } else {
        at
    }
}

/// Copying and moving the selection by keys, and the end of a press.
fn keys(p: Paragraph, mut editable: UseEditable, holder: State<ParagraphHolder>) -> Paragraph {
    p.on_mouse_up(move |_| editable.process_event(EditableEvent::Release))
        .on_key_down(move |e: Event<KeyboardEventData>| {
            editable.process_event(EditableEvent::KeyDown {
                key: &e.key,
                modifiers: e.modifiers,
                editor_line: Some(EditorLine::SingleParagraph),
                holder: Some(&holder.read()),
            });
        })
        .on_key_up(move |e: Event<KeyboardEventData>| {
            editable.process_event(EditableEvent::KeyUp { key: &e.key });
        })
}

/// The pointer over the text: the I-beam over its glyphs (and all through a
/// drag), and a press elsewhere clears the selection.
fn hover(
    p: Paragraph,
    mut editable: UseEditable,
    holder: State<ParagraphHolder>,
    mut hovering: State<bool>,
    mut beam: State<bool>,
    drag_origin: State<Option<impl Copy + 'static>>,
) -> Paragraph {
    p.on_pointer_move(move |e: Event<PointerEventData>| {
        let over =
            drag_origin.read().is_some() || over_glyphs(&holder.read(), e.element_location());
        if *beam.read() != over {
            beam.set(over);
        }
    })
    .on_pointer_enter(move |_| hovering.set(true))
    .on_pointer_leave(move |_| {
        hovering.set(false);
        beam.set(false);
    })
    .on_global_pointer_down(move |_: Event<PointerEventData>| {
        if !hovering() {
            editable.editor_mut().write().clear_selection();
        }
    })
}

/// A press, its drag and its end: the selection's end under the pointer,
/// also as a scroll moves the text under a still one; in the band, a drag
/// past its edge scrolls it.
fn drag(
    p: Paragraph,
    mut editable: UseEditable,
    holder: State<ParagraphHolder>,
    a11y_id: AccessibilityId,
    (mut drag_origin, hovering, in_band): (State<Option<()>>, State<bool>, bool),
) -> Paragraph {
    // where it is laid out, and the drag's pointer, in the window: a
    // scroll moves the text under a still pointer
    let at = use_hook(|| Rc::new(Cell::new(CursorPoint::default())));
    let pointer = use_hook(|| Rc::new(Cell::new(None::<CursorPoint>)));
    let band = use_try_consume::<Follow>().filter(|_| in_band);
    let drag_to = {
        let (at, pointer) = (at.clone(), pointer.clone());
        move || {
            let (Some(g), true) = (pointer.get(), a11y_id.is_focused()) else {
                return;
            };
            let local = g - at.get().to_vector();
            editable.process_event(EditableEvent::Move {
                location: past_ends(&holder.read(), local),
                editor_line: EditorLine::SingleParagraph,
                holder: &holder.read(),
            });
        }
    };

    let (pressed, origin) = (pointer.clone(), at.clone());
    let on_pointer_down = move |e: Event<PointerEventData>| {
        if !e.data().is_primary() {
            return;
        }
        e.stop_propagation();
        drag_origin.set(Some(()));
        pressed.set(Some(e.global_location()));
        origin.set((e.global_location() - e.element_location()).to_point());
        if let Some(band) = &band {
            band.pressed(e.global_location().y as f32);
        }
        editable.process_event(EditableEvent::Down {
            location: e.element_location(),
            editor_line: EditorLine::SingleParagraph,
            holder: &holder.read(),
        });
        a11y_id.request_focus();
    };

    let (moved, mut follow) = (pointer.clone(), drag_to.clone());
    let on_global_pointer_move = move |e: Event<PointerEventData>| {
        if drag_origin.read().is_some() {
            moved.set(Some(e.global_location()));
            follow();
        }
    };
    let (laid, mut scrolled) = (at.clone(), drag_to);
    let on_sized = move |e: Event<SizedEventData>| {
        let o = e.area.origin;
        laid.set(CursorPoint::new(f64::from(o.x), f64::from(o.y)));
        if drag_origin.read().is_some() {
            scrolled();
        }
    };

    let on_global_pointer_up = move |_: Event<PointerEventData>| {
        if hovering() || a11y_id.is_focused() {
            editable.process_event(EditableEvent::Release);
        }
        if drag_origin.read().is_some() {
            drag_origin.set(None);
            pointer.set(None);
        } else if a11y_id.is_focused() {
            a11y_id.request_unfocus();
        }
    };
    let p = p
        .on_global_pointer_move(on_global_pointer_move)
        .on_pointer_down(on_pointer_down)
        .on_global_pointer_up(on_global_pointer_up);
    if in_band { p.on_sized(on_sized) } else { p }
}

impl Component for Selectable {
    fn render(&self) -> impl IntoElement {
        let value = self.text();
        let holder = use_state(ParagraphHolder::default);
        let mut editable = use_editable(
            || self.text(),
            move || EditableConfig::new().with_allow_changes(false),
        );
        let hovering = use_state(|| false);
        let beam = use_state(|| false);
        let a11y_id = use_a11y();
        let drag_origin = use_state(|| None::<()>);

        if value.as_str() != editable.editor().read().rope() {
            editable.editor_mut().write().set(value.as_str());
            editable.editor_mut().write().editor_history_mut().clear();
        }

        let highlights = editable
            .editor()
            .read()
            .get_visible_selection(EditorLine::SingleParagraph);

        let paragraph = paragraph()
            .layout(self.layout.clone())
            .accessibility(self.accessibility.clone())
            .text_style(self.text_style_data.clone())
            .event_handlers(self.event_handlers.clone())
            .layer(self.layer)
            .max_lines(self.max_lines)
            .a11y_id(a11y_id)
            // no Tab stop (Tab takes Enabled only), yet a press focuses it
            // (request_focus refuses Disabled), and the focus brings it ⌘C
            // and the arrows
            .a11y_focusable(Focusable::Unknown)
            .holder(holder.read().clone())
            .cursor([CursorIcon::Default, CursorIcon::Text][usize::from(beam())])
            .cursor_color(Color::BLACK)
            .highlights(highlights.map(|h| vec![h]));
        let mut paragraph = drag(
            paragraph,
            editable,
            holder,
            a11y_id,
            (drag_origin, hovering, self.in_band),
        );
        paragraph = keys(paragraph, editable, holder);
        paragraph = hover(paragraph, editable, holder, hovering, beam, drag_origin);
        for span in &self.spans {
            paragraph = paragraph.span(span.clone());
        }
        paragraph
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
