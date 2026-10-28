//! Tooltips that stay inside the window. Freya's TooltipContainer centres
//! the tip on its control, so a control near an edge pushes it out of the
//! window; this one is placed in window coordinates, shifted in from the
//! right and left edges, flipped above or below where it would cross the
//! bottom or top, and wrapped at [`WRAP`] when one line is wider than the window.
//! All in logical pixels: the window's size is [`super::window_size`], not
//! Freya's `root_size`, which is in physical pixels.

use freya::prelude::*;
use std::time::Duration;

/// How long the pointer rests before the tip shows (Freya's default).
const DELAY: Duration = Duration::from_millis(500);
/// Kept free between the tip and the window's edges, and the control.
const EDGE: f32 = 8.0;
const GAP: f32 = 5.0;
/// A tip wider than the window wraps at this width.
const WRAP: f32 = 320.0;

/// Where the tip goes: `tip` is its size, `at` the control's area, `window`
/// the window's size; the tip's top left in window coordinates. `above`
/// is the preferred side; the other one when it does not fit there.
pub fn place(tip: Size2D, at: Area, window: Size2D, above: bool) -> (f32, f32) {
    let centred = at.min_x() + (at.width() - tip.width) / 2.0;
    let x = centred.min(window.width - EDGE - tip.width).max(EDGE);
    let over = at.min_y() - GAP - tip.height;
    let under = at.max_y() + GAP;
    let fits_over = over >= EDGE;
    let fits_under = under + tip.height <= window.height - EDGE;
    let y = match (above, fits_over, fits_under) {
        (true, true, _) | (false, true, false) => over,
        _ => under,
    };
    (x, y)
}

#[derive(PartialEq)]
pub struct Tip {
    text: String,
    above: bool,
    children: Vec<Element>,
    layout: LayoutData,
    key: DiffKey,
}

impl KeyExt for Tip {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl ChildrenExt for Tip {
    fn children_mut(&mut self) -> &mut Vec<Element> {
        &mut self.children
    }
}

impl LayoutExt for Tip {
    fn layout_mut(&mut self) -> &mut LayoutData {
        &mut self.layout
    }
}

/// A tip can size its control's box: a list row's text, flexed.
impl ContainerExt for Tip {}

impl Tip {
    /// Below its control, unless [`Tip::top`].
    pub fn new(text: impl Into<String>) -> Tip {
        Tip {
            text: text.into(),
            above: false,
            children: vec![],
            layout: LayoutData::default(),
            key: DiffKey::None,
        }
    }

    /// Above its control where it fits (the composer's buttons).
    pub fn top(mut self) -> Tip {
        self.above = true;
        self
    }
}

impl Component for Tip {
    fn render(&self) -> impl IntoElement {
        let mut hovering = use_state(|| false);
        let mut delay = use_state::<Option<TaskHandle>>(|| None);
        let mut at = use_state(|| None::<Area>);
        let mut size = use_state(|| None::<Size2D>);
        let mut wraps = use_state(|| false);
        let window = super::window_size();
        let over = move |_| {
            if let Some(task) = delay.write().take() {
                task.cancel();
            }
            let task = spawn(async move {
                timer(DELAY).await;
                hovering.set_if_modified(true);
            });
            delay.set(Some(task));
        };
        let out = move |_| {
            if let Some(task) = delay.write().take() {
                task.cancel();
            }
            hovering.set_if_modified(false);
            size.set_if_modified(None);
            wraps.set_if_modified(false);
        };
        // a press anywhere hides it, as a menu or a list may open over it
        let pressed = move |_: Event<PointerEventData>| {
            if let Some(task) = delay.write().take() {
                task.cancel();
            }
            hovering.set_if_modified(false);
        };
        let measured = move |e: Event<SizedEventData>| {
            let s = e.area.size;
            // one line wider than the window: wrap, and keep it wrapped while shown
            if !*wraps.peek() && s.width > window.width - 2.0 * EDGE {
                wraps.set(true);
            }
            size.set_if_modified(Some(s));
        };
        // no text, no tip: a control may silence its own for a while
        let tip = (hovering() && !self.text.is_empty()).then(|| {
            let (x, y) = match (size(), at()) {
                (Some(s), Some(a)) => place(s, a, window, self.above),
                _ => (0.0, 0.0),
            };
            let text = label().text(self.text.clone());
            let text = if wraps() {
                text.width(Size::px(WRAP.min(window.width - 2.0 * EDGE - 20.0)))
            } else if self.text.contains('\n') {
                text
            } else {
                text.max_lines(1)
            };
            // the room to the window's right edge, not the control's width:
            // measured in that, the tip keeps its natural width
            rect()
                .position(Position::new_global().left(x).top(y))
                .width(Size::px((window.width - x).max(0.0)))
                .layer(Layer::Overlay)
                .opacity(if size().is_some() { 1.0 } else { 0.0 })
                .child(rect().on_sized(measured).child(Tooltip::new().child(text)))
        });
        rect()
            .layout(self.layout.clone())
            .a11y_focusable(false)
            .a11y_role(AccessibilityRole::Tooltip)
            .on_pointer_over(over)
            .on_pointer_out(out)
            .on_global_pointer_down(pressed)
            .on_sized(move |e: Event<SizedEventData>| at.set_if_modified(Some(e.area)))
            .children(self.children.clone())
            .maybe_child(tip)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(x: f32, y: f32, w: f32, h: f32) -> Area {
        Area::new((x, y).into(), (w, h).into())
    }

    #[test]
    fn a_tip_stays_inside_the_window() {
        let window = Size2D::new(420.0, 320.0);
        let tip = Size2D::new(260.0, 24.0);
        // the Send button at the right edge: shifted left, above as asked
        let (x, y) = place(tip, area(360.0, 280.0, 56.0, 26.0), window, true);
        assert_eq!(x, 420.0 - EDGE - 260.0);
        assert_eq!(y, 280.0 - GAP - 24.0);
        // the sidebar toggle at the left edge, below: not past the left edge
        let (x, y) = place(tip, area(2.0, 4.0, 26.0, 26.0), window, false);
        assert_eq!((x, y), (EDGE, 30.0 + GAP));
        // no room below: above instead
        let (_, y) = place(tip, area(100.0, 300.0, 26.0, 16.0), window, false);
        assert_eq!(y, 300.0 - GAP - 24.0);
    }
}
