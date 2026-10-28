//! Where the caret is, for the dictation's wave (`Platform::caret`): its
//! line from the line's start to the caret, by Accessibility. Asked a few
//! times a second while a dictation records, so it never logs.

use super::focus::{ax_app, ax_attr, ax_param, focused};
use blitznir_core::platform::Rect;
use objc2_application_services::{AXUIElement, AXValue, AXValueType};
use objc2_core_foundation::{CFNumber, CFRange, CFRetained, CFType, CGRect};
use std::ptr::NonNull;

/// A caret is a few points wide at most and a line box lower than this; a
/// bigger answer is the whole field (Chrome's character range for web text).
const CARET_W: f64 = 40.0;
const LINE_H: f64 = 120.0;
/// A field's text starts a little inside its frame.
const FIELD_INSET: f64 = 4.0;

/// The struct `T` in `v`, an AXValue of `kind`.
///
/// # Safety
/// `kind` must be the AXValueType whose struct is `T`.
unsafe fn value<T>(v: &CFType, kind: AXValueType, mut out: T) -> Option<T> {
    let v = v.downcast_ref::<AXValue>()?;
    // SAFETY: `out` is the `T` the caller says `kind` names; AX checks the kind.
    unsafe { v.value(kind, NonNull::from(&mut out).cast()) }.then_some(out)
}

fn range(v: &CFType) -> Option<CFRange> {
    let empty = CFRange {
        location: 0,
        length: 0,
    };
    // SAFETY: CFRange is the struct of the CFRange kind.
    unsafe { value(v, AXValueType::CFRange, empty) }
}

/// A line box or a caret's: on a screen, not the whole field.
fn line_box(v: &CFType) -> Option<Rect> {
    // SAFETY: CGRect is the struct of the CGRect kind.
    let r = unsafe { value(v, AXValueType::CGRect, CGRect::default()) }?;
    let r = Rect {
        x: r.origin.x,
        y: r.origin.y,
        w: r.size.width,
        h: r.size.height,
    };
    let sane = r.h > 2.0 && r.h < LINE_H && r.x > -20_000.0 && r.y > -20_000.0;
    sane.then_some(r)
}

/// The bounds of characters `location..location + length` of `el`.
fn bounds(el: &AXUIElement, location: isize, length: isize) -> Option<Rect> {
    let r = CFRange {
        location: location.max(0),
        length,
    };
    // SAFETY: a CFRange for the CFRange kind; AX copies it.
    let v: CFRetained<AXValue> =
        unsafe { AXValue::new(AXValueType::CFRange, NonNull::from(&r).cast()) }?;
    let b = ax_param(el, "AXBoundsForRange", &v).ok()?;
    line_box(&b)
}

/// The first character of the line holding character `at`.
fn line_start(el: &AXUIElement, at: isize) -> Option<Rect> {
    let line = ax_param(el, "AXLineForIndex", &CFNumber::new_isize(at)).ok()?;
    let line = line.downcast::<CFNumber>().ok()?.as_isize()?;
    let r = ax_param(el, "AXRangeForLine", &CFNumber::new_isize(line)).ok()?;
    let r = range(&r)?;
    bounds(el, r.location, r.length.min(1))
}

/// By character range (Cocoa, most apps): a zero-length range's bounds at
/// the caret, else the right edge of the character before it.
fn by_range(el: &AXUIElement) -> Option<Rect> {
    let sel = ax_attr(el, "AXSelectedTextRange").ok()?;
    let sel = range(&sel)?;
    let at = sel.location + sel.length;
    let narrow = |c: &Rect| c.w < CARET_W;
    let caret = (bounds(el, at, 0).filter(narrow).map(|c| (c.x, c))).or_else(|| {
        let c = bounds(el, at - 1, 1).filter(narrow)?;
        (at > 0).then_some((c.x + c.w, c))
    });
    let (x, c) = caret?;
    let same_line = |s: &Rect| (s.y - c.y).abs() < c.h / 2.0 && s.x <= x;
    let left = line_start(el, at).filter(same_line).map_or(x, |s| s.x);
    Some(Rect {
        x: left,
        y: c.y,
        w: x - left,
        h: c.h,
    })
}

/// The field's own frame: wider than a caret, else none.
fn field(el: &AXUIElement) -> Option<Rect> {
    let f = ax_attr(el, "AXFrame").ok()?;
    // SAFETY: CGRect is the struct of the CGRect kind.
    let f = unsafe { value(&f, AXValueType::CGRect, CGRect::default()) }?;
    (f.size.width > CARET_W).then_some(Rect {
        x: f.origin.x,
        y: f.origin.y,
        w: f.size.width,
        h: f.size.height,
    })
}

/// By text marker (Chrome and Electron's web text): the caret, no line
/// start; the field's left edge stands in for it (else the wave's guessed
/// tail ran out of the field on a new line), none without a frame.
fn by_marker(el: &AXUIElement) -> Option<Rect> {
    let sel = ax_attr(el, "AXSelectedTextMarkerRange").ok()?;
    let b = ax_param(el, "AXBoundsForTextMarkerRange", &sel).ok()?;
    let c = line_box(&b)?;
    let caret = c.x + c.w;
    let left = field(el).map(|f| f.x + FIELD_INSET).filter(|&l| l <= caret);
    (c.w < CARET_W).then_some(Rect {
        x: left.unwrap_or(caret),
        w: left.map_or(0.0, |l| caret - l),
        ..c
    })
}

pub fn line(pid: i32) -> Option<Rect> {
    let el = focused(&ax_app(pid)).ok()?;
    by_range(&el).or_else(|| by_marker(&el))
}
