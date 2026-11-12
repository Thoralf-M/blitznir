//! The panel window's AppKit side: what winit does not tell or set.

use blitznir_core::platform::Rect;
use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
use objc2_core_foundation::CGRect;
use objc2_foundation::NSString;
use std::{ffi::c_void, ptr::NonNull};

/// A double click on the title bar does nothing in this app: the Freya
/// panel draws its header under a see-through title bar, and macOS takes two
/// quick presses on its buttons for one on the title bar and zooms. The
/// header's empty parts zoom by themselves (freya's window_drag). Written
/// to the app's own defaults, which win over the system-wide setting.
pub fn no_title_bar_zoom() {
    let defaults = objc2_foundation::NSUserDefaults::standardUserDefaults();
    let none = NSString::from_str("None");
    // SAFETY: an NSString value for a string key, both alive for the call.
    unsafe {
        defaults.setObject_forKey(Some(&none), &NSString::from_str("AppleActionOnDoubleClick"));
    }
}

/// The free area of the screen the window is on (the menu bar and the Dock
/// left out), as winit places windows: points from the top left of the
/// main screen. What the panel's zoom fills. `ns_view` is the window's
/// content view; None while it has no window or screen.
pub fn visible_frame(ns_view: NonNull<c_void>) -> Option<Rect> {
    // SAFETY: the content view of one of this app's windows, on the main
    // thread, alive for the call; every message is NSView's, NSWindow's,
    // NSScreen's or NSArray's own, with the types they take and return.
    unsafe {
        let view = ns_view.cast::<AnyObject>().as_ref();
        let window: Option<Retained<AnyObject>> = msg_send![view, window];
        let screen: Option<Retained<AnyObject>> = msg_send![&*window?, screen];
        let free: CGRect = msg_send![&*screen?, visibleFrame];
        let screens: Option<Retained<AnyObject>> = msg_send![class!(NSScreen), screens];
        let main: Option<Retained<AnyObject>> = msg_send![&*screens?, firstObject];
        let whole: CGRect = msg_send![&*main?, frame];
        // AppKit counts up from the main screen's bottom, winit down from its top
        let top = whole.size.height - (free.origin.y + free.size.height);
        Some(Rect {
            x: free.origin.x,
            y: top,
            w: free.size.width,
            h: free.size.height,
        })
    }
}

/// Move and size the window in one step (`frame`: outer, points, from the
/// main screen's top left as winit counts): winit moves, then sizes, and a
/// zoom's frame between the two would show. `ns_view` as for
/// [`visible_frame`].
pub fn set_frame(ns_view: NonNull<c_void>, frame: Rect) {
    // SAFETY: as in visible_frame; setFrame:display: is NSWindow's own.
    unsafe {
        let view = ns_view.cast::<AnyObject>().as_ref();
        let window: Option<Retained<AnyObject>> = msg_send![view, window];
        let screens: Option<Retained<AnyObject>> = msg_send![class!(NSScreen), screens];
        let (Some(window), Some(screens)) = (window, screens) else {
            return;
        };
        let main: Option<Retained<AnyObject>> = msg_send![&*screens, firstObject];
        let Some(main) = main else { return };
        let whole: CGRect = msg_send![&*main, frame];
        let bottom = whole.size.height - (frame.y + frame.h);
        let to = CGRect::new(
            objc2_core_foundation::CGPoint::new(frame.x, bottom),
            objc2_core_foundation::CGSize::new(frame.w, frame.h),
        );
        let _: () = msg_send![&*window, setFrame: to, display: true];
    }
}
