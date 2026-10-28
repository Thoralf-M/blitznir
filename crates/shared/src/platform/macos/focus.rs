//! Who has focus, whether it takes text, where its selection is, and handing
//! focus back: Accessibility and NSRunningApplication.

use blitznir_core::platform::{Bounds, Rect};
use objc2::runtime::NSObjectProtocol;
use objc2::sel;
use objc2_app_kit::{
    NSApplication, NSApplicationActivationOptions, NSRunningApplication, NSWorkspace,
};
use objc2_application_services::{AXError, AXUIElement, AXValue, AXValueType};
use objc2_core_foundation::{
    CFArray, CFBoolean, CFRetained, CFString, CFType, CGPoint, CGRect, CGSize,
};
use std::collections::VecDeque;
use std::ptr::NonNull;
use std::thread;
use std::time::{Duration, Instant};

type Ax<T> = std::result::Result<T, AXError>;

/// A hung app must not stall us.
const AX_TIMEOUT_S: f32 = 0.5;

/// One AX Copy call on a fresh out-pointer: a non-null result is +1
/// retained (the Copy rule), and ours.
fn copy(call: impl FnOnce(NonNull<*const CFType>) -> AXError) -> Ax<CFRetained<CFType>> {
    let mut out: *const CFType = std::ptr::null();
    let err = call(NonNull::from(&mut out));
    if err != AXError::Success {
        return Err(err);
    }
    NonNull::new(out.cast_mut())
        // SAFETY: the Copy rule hands us ownership of one retain.
        .map(|p| unsafe { CFRetained::from_raw(p) })
        .ok_or(AXError::NoValue)
}

pub(super) fn ax_attr(el: &AXUIElement, name: &'static str) -> Ax<CFRetained<CFType>> {
    let name = CFString::from_static_str(name);
    // SAFETY: `out` is the valid out-pointer `copy` made.
    copy(|out| unsafe { el.copy_attribute_value(&name, out) })
}

/// A parameterized attribute: `param` is a value AX handed out for it.
pub(super) fn ax_param(
    el: &AXUIElement,
    name: &'static str,
    param: &CFType,
) -> Ax<CFRetained<CFType>> {
    let name = CFString::from_static_str(name);
    // SAFETY: `out` is the valid out-pointer `copy` made.
    copy(|out| unsafe { el.copy_parameterized_attribute_value(&name, param, out) })
}

pub(super) fn ax_app(pid: i32) -> CFRetained<AXUIElement> {
    // SAFETY: plain C constructors and setters on a fresh element.
    unsafe {
        let app = AXUIElement::new_application(pid);
        app.set_messaging_timeout(AX_TIMEOUT_S);
        // Electron builds its AX tree only when asked; an error is fine
        let _ = app.set_attribute_value(
            &CFString::from_static_str("AXManualAccessibility"),
            CFBoolean::new(true),
        );
        app
    }
}

/// The element `app`'s attribute `name` points to (its focused element or window).
fn ax_element(app: &AXUIElement, name: &'static str) -> Ax<CFRetained<AXUIElement>> {
    let el =
        (ax_attr(app, name)?.downcast::<AXUIElement>()).map_err(|_| AXError::IllegalArgument)?;
    // SAFETY: a setter on an element we own.
    unsafe { el.set_messaging_timeout(AX_TIMEOUT_S) };
    Ok(el)
}

/// `app`'s focused element. Chrome refuses AXManualAccessibility and has no
/// focused element until a client sets AXEnhancedUserInterface (what
/// VoiceOver sets); its web tree is up about 2.3 s later (measured on a
/// fresh Chrome profile) and stays up, so the next ask finds the text.
pub(super) fn focused(app: &AXUIElement) -> Ax<CFRetained<AXUIElement>> {
    let el = ax_element(app, "AXFocusedUIElement");
    if el.as_ref().is_err_and(|e| *e == AXError::NoValue) {
        // SAFETY: a plain setter on an element we own; an error is fine
        let _ = unsafe {
            app.set_attribute_value(
                &CFString::from_static_str("AXEnhancedUserInterface"),
                CFBoolean::new(true),
            )
        };
    }
    el
}

const TEXT_ROLES: [&str; 4] = ["AXTextField", "AXTextArea", "AXComboBox", "AXSearchField"];

/// Some(true) editable text, Some(false) something else, None: the app won't say.
pub fn editable(pid: i32) -> Option<bool> {
    let el = focused(&ax_app(pid)).ok()?;
    let role = ax_attr(&el, "AXRole").ok()?.downcast::<CFString>().ok()?;
    if !TEXT_ROLES.contains(&role.to_string().as_str()) {
        return Some(false);
    }
    // a read-only text view (a mail being read, a log) is an AXTextArea too
    let mut settable: u8 = 0;
    // SAFETY: valid out-pointer.
    let err = unsafe {
        el.is_attribute_settable(
            &CFString::from_static_str("AXValue"),
            NonNull::from(&mut settable),
        )
    };
    Some(err != AXError::Success || settable != 0)
}

/// The focused element is a text field or text area that says its selected
/// text is empty. Only then: a web page's text (Chrome's AXWebArea, its
/// selection in text markers) or an app that won't say counts as maybe.
pub fn nothing_selected(pid: i32) -> bool {
    let Ok(el) = focused(&ax_app(pid)) else {
        return false;
    };
    let role = ax_attr(&el, "AXRole")
        .ok()
        .and_then(|r| r.downcast::<CFString>().ok());
    if !role.is_some_and(|r| TEXT_ROLES.contains(&r.to_string().as_str())) {
        return false;
    }
    let text = ax_attr(&el, "AXSelectedText")
        .ok()
        .and_then(|t| t.downcast::<CFString>().ok());
    text.is_some_and(|t| t.to_string().is_empty())
}

/// The CG struct `T` in `v`, when `v` is an AXValue of `kind`.
///
/// # Safety
/// `kind` must be the AXValueType whose struct is `T`.
unsafe fn ax_struct<T: Default>(v: &CFType, kind: AXValueType) -> Option<T> {
    let v = v.downcast_ref::<AXValue>()?;
    let mut out = T::default();
    // SAFETY: `out` is the `T` the caller says `kind` names; AX checks that
    // `v` holds that kind before it writes.
    unsafe { v.value(kind, NonNull::from(&mut out).cast()) }.then_some(out)
}

/// The selected text's rect by one pair of AX names: the selection, and the
/// parameterized attribute that turns it into bounds.
fn text_bounds(el: &AXUIElement, selection: &'static str, bounds: &'static str) -> Option<Rect> {
    let range = ax_attr(el, selection).ok()?;
    let v = ax_param(el, bounds, &range).ok()?;
    // SAFETY: CGRect is the struct of the CGRect kind.
    let r = unsafe { ax_struct::<CGRect>(&v, AXValueType::CGRect) }?;
    Some(Rect {
        x: r.origin.x,
        y: r.origin.y,
        w: r.size.width,
        h: r.size.height,
    })
}

fn frame(el: &AXUIElement) -> Option<Rect> {
    let (p, s) = (ax_attr(el, "AXPosition").ok()?, ax_attr(el, "AXSize").ok()?);
    // SAFETY: CGPoint and CGSize are the structs of their kinds.
    let p = unsafe { ax_struct::<CGPoint>(&p, AXValueType::CGPoint) }?;
    // SAFETY: as above.
    let s = unsafe { ax_struct::<CGSize>(&s, AXValueType::CGSize) }?;
    Some(Rect {
        x: p.x,
        y: p.y,
        w: s.width,
        h: s.height,
    })
}

/// Chrome and Electron answer for web text in text markers, where the
/// character range gets only the whole field: markers first. Both answer
/// with the selected text itself.
const TEXT_QUERIES: [(&str, &str, &str); 2] = [
    (
        "AXSelectedTextMarkerRange",
        "AXBoundsForTextMarkerRange",
        "text marker range",
    ),
    ("AXSelectedTextRange", "AXBoundsForRange", "character range"),
];

/// Where [`web_selection`] found it, for the log and for [`Bounds::text`].
const WEB: &str = "web page's selection";
/// [`web_selection`] looks at this many elements at most: an editor's web
/// view was measured with its page about 100 deep in the window, 40 ms to
/// walk there.
const WEB_WALK: usize = 600;

/// Read-only web text's selection: Chromium and Electron keep it on the
/// page (an AXWebArea) and name no focused element then. Breadth first
/// through the window to the first page with selected text.
fn web_selection(win: &CFRetained<AXUIElement>) -> Option<Rect> {
    let mut queue = VecDeque::from([win.clone()]);
    for _ in 0..WEB_WALK {
        let el = queue.pop_front()?;
        let role = ax_attr(&el, "AXRole")
            .ok()
            .and_then(|r| r.downcast::<CFString>().ok());
        if role.is_some_and(|r| r.to_string() == "AXWebArea") && has_selected_text(&el) {
            return text_bounds(
                &el,
                "AXSelectedTextMarkerRange",
                "AXBoundsForTextMarkerRange",
            );
        }
        let kids = ax_attr(&el, "AXChildren")
            .ok()
            .and_then(|k| k.downcast::<CFArray>().ok());
        if let Some(kids) = kids {
            // SAFETY: AXChildren is documented as an array of AXUIElements.
            let kids = unsafe { CFRetained::cast_unchecked::<CFArray<AXUIElement>>(kids) };
            queue.extend(kids.iter());
        }
    }
    None
}

/// A page's selection holds text, not only a caret.
fn has_selected_text(page: &AXUIElement) -> bool {
    let Ok(range) = ax_attr(page, "AXSelectedTextMarkerRange") else {
        return false;
    };
    let text = ax_param(page, "AXStringForTextMarkerRange", &range).ok();
    text.and_then(|t| t.downcast::<CFString>().ok())
        .is_some_and(|t| !t.to_string().trim().is_empty())
}

/// `Platform::selection_bounds`, and which question the app answered.
fn ask_bounds(pid: i32) -> Option<(Rect, &'static str)> {
    let app = ax_app(pid);
    let answer = |r: Option<Rect>, how| r.filter(Rect::plausible).map(|r| (r, how));
    if let Ok(el) = focused(&app) {
        for (selection, bounds, how) in TEXT_QUERIES {
            if let Some(a) = answer(text_bounds(&el, selection, bounds), how) {
                return Some(a);
            }
        }
        if let Some(a) = answer(frame(&el), "focused element") {
            return Some(a);
        }
    }
    let win = ax_element(&app, "AXFocusedWindow").ok()?;
    if let Some(a) = answer(web_selection(&win), WEB) {
        return Some(a);
    }
    answer(frame(&win), "focused window")
}

pub fn selection_bounds(pid: i32) -> Option<Bounds> {
    let t = Instant::now();
    let found = ask_bounds(pid);
    let ms = t.elapsed().as_millis();
    match found {
        Some((r, how)) => log::info!("selection bounds: {r:?}, from the {how}, in {ms} ms"),
        None => log::info!("selection bounds: pid {pid} will not say ({ms} ms)"),
    }
    let text = |how: &str| how == WEB || TEXT_QUERIES.iter().any(|(_, _, q)| *q == how);
    found.map(|(rect, how)| Bounds {
        rect,
        text: text(how),
    })
}

/// The app in front, never this one: as a paste target, our own pid would
/// get ⌘V in our own window.
pub fn frontmost() -> Option<i32> {
    NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .map(|a| a.processIdentifier())
        .filter(|pid| u32::try_from(*pid).ok() != Some(std::process::id()))
}

/// An Accessory app that stays active keeps winning the activation race:
/// give it up to `pid` on the main thread and hide our windows.
pub fn yield_to(pid: i32) {
    if !NSRunningApplication::currentApplication().isActive() {
        return; // not in front (the CLI, or the panel already went)
    }
    dispatch2::run_on_main(move |mtm| {
        let app = NSApplication::sharedApplication(mtm);
        if let Some(prev) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
            && app.respondsToSelector(sel!(yieldActivationToApplication:))
        {
            app.yieldActivationToApplication(&prev);
        }
        app.hide(None);
    });
}

/// Undo [`yield_to`]'s hide without becoming the active app: the app in
/// front keeps the keyboard.
pub fn show_quietly() {
    dispatch2::run_on_main(|mtm| NSApplication::sharedApplication(mtm).unhideWithoutActivation());
}

fn activate(pid: i32) {
    let Some(prev) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid) else {
        return;
    };
    #[allow(deprecated)] // a no-op on 14+, still needed by the macOS 13 fallback
    let opts = NSApplicationActivationOptions::ActivateIgnoringOtherApps;
    if prev.respondsToSelector(sel!(activateFromApplication:options:)) {
        prev.activateFromApplication_options(&NSRunningApplication::currentApplication(), opts);
    } else {
        #[allow(deprecated)]
        prev.activateWithOptions(opts);
    }
}

/// Up to 3 activation attempts, each polling the frontmost app for 1 s.
pub fn bring_front(pid: i32) -> bool {
    for _ in 0..3 {
        activate(pid);
        let end = Instant::now() + Duration::from_secs(1);
        while Instant::now() < end {
            if frontmost() == Some(pid) {
                return true;
            }
            thread::sleep(Duration::from_millis(30));
        }
    }
    false
}
