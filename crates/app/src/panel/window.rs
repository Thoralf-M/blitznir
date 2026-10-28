//! The panel's window: none at start, made with `launch_window` on the first
//! open. macOS keeps it and hides it, so the second open is instant; Wayland
//! can neither hide nor focus a window, so Linux destroys it on every hide
//! and makes it anew. Every change runs on the event loop's thread,
//! posted there with `LaunchProxy::post_callback`.

use super::Panel;
use crate::ui;
use blitznir_core::lock;
use blitznir_core::platform::Rect;
use freya::prelude::{CloseDecision, RendererContext, WindowConfig};
use freya::winit::dpi::{LogicalPosition, LogicalSize};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

/// The panel window, in points.
const SIZE: (f64, f64) = (760.0, 540.0);
const MIN_SIZE: (f64, f64) = (420.0, 320.0);
/// Activation is asynchronous: focus is asked for again once it landed.
const REFOCUS: Duration = Duration::from_millis(250);
/// A caller waiting for the event loop gives up after this.
const LOOP_WAIT: Duration = Duration::from_secs(1);

impl Panel {
    /// Run `f` on the event loop's thread and wait for its answer. Never on
    /// that thread itself: it would wait for itself.
    pub(super) fn on_loop<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut RendererContext) -> T + Send + 'static,
    ) -> Option<T> {
        let (tx, rx) = mpsc::sync_channel(1);
        drop(self.proxy.post_callback(move |ctx| {
            let _ = tx.send(f(ctx));
        }));
        rx.recv_timeout(LOOP_WAIT).ok()
    }

    /// Show the panel and give it key focus, making the window first if
    /// there is none.
    pub(super) fn show(self: &Arc<Self>) {
        let me = self.clone();
        drop(self.proxy.post_callback(move |ctx| me.show_in(ctx)));
        let me = self.clone();
        thread::spawn(move || {
            thread::sleep(REFOCUS);
            // an Esc or a job's lightning hid it meanwhile: it stays hidden
            if !lock(&me.view).ui.shown {
                return;
            }
            let proxy = me.proxy.clone();
            drop(proxy.post_callback(move |ctx| me.focus_in(ctx)));
        });
        self.set_shown(true);
    }

    fn show_in(self: &Arc<Self>, ctx: &mut RendererContext) {
        let mut id = *lock(&self.window);
        // Wayland raises no window: one that lost the focus is made anew
        if let Some(old) = id
            && cfg!(target_os = "linux")
            && ctx
                .windows()
                .get(&old)
                .is_some_and(|w| !w.window().has_focus())
        {
            ctx.windows_mut().remove(&old);
            id = None;
        }
        if id.is_none_or(|id| !ctx.windows().contains_key(&id)) {
            let id = ctx.launch_window(self.window_config());
            *lock(&self.window) = Some(id);
            log::info!("panel window made");
        }
        self.focus_in(ctx);
    }

    fn focus_in(&self, ctx: &mut RendererContext) {
        let Some(id) = *lock(&self.window) else {
            return;
        };
        if let Some(w) = ctx.windows_mut().get_mut(&id) {
            w.window_mut().set_visible(true);
            w.window_mut().focus_window();
        }
    }

    /// Hide the panel (Linux: destroy it); on the event loop, later.
    pub(super) fn hide(&self) {
        self.set_shown(false);
        let window = *lock(&self.window);
        if let Some(id) = window {
            let me = self.me.clone();
            drop(self.proxy.post_callback(move |ctx| {
                if let Some(me) = me.upgrade() {
                    me.hide_in(ctx, id);
                }
            }));
        }
    }

    /// [`Panel::hide`], returning once it happened: before a paste or a
    /// screenshot, which must not see the panel.
    pub(super) fn hide_and_wait(&self) {
        self.set_shown(false);
        let me = self.me.clone();
        let hidden = self.on_loop(move |ctx| {
            let me = me.upgrade()?;
            let id = (*lock(&me.window))?;
            me.hide_in(ctx, id);
            Some(())
        });
        if hidden.is_none() {
            log::warn!("the panel did not hide in time");
        }
    }

    fn hide_in(&self, ctx: &mut RendererContext, id: freya::winit::window::WindowId) {
        if cfg!(target_os = "linux") {
            ctx.windows_mut().remove(&id);
            *lock(&self.window) = None;
        } else if let Some(w) = ctx.windows_mut().get_mut(&id) {
            w.window_mut().set_visible(false);
        }
    }

    /// The composer reports where it is, in the window's points.
    pub fn composer_at(&self, r: Rect) {
        lock(&self.view).composer = Some(r);
    }

    /// `r`, in the window's points, on the screen; None without a window.
    pub(super) fn on_screen(&self, r: Rect) -> Option<Rect> {
        let id = (*lock(&self.window))?;
        let at = self.on_loop(move |ctx| {
            let w = ctx.windows().get(&id)?.window();
            let at: LogicalPosition<f64> = w.inner_position().ok()?.to_logical(w.scale_factor());
            Some((at.x, at.y))
        })??;
        Some(Rect {
            x: r.x + at.0,
            y: r.y + at.1,
            ..r
        })
    }

    /// Our window is in front with key focus: a hotkey then only shows it.
    pub(super) fn window_focused(&self) -> bool {
        let Some(id) = *lock(&self.window) else {
            return false;
        };
        let focused = self.on_loop(move |ctx| {
            ctx.windows()
                .get(&id)
                .is_some_and(|w| w.window().is_visible() != Some(false) && w.window().has_focus())
        });
        focused.unwrap_or(false)
    }

    fn window_config(self: &Arc<Self>) -> WindowConfig {
        let me = self.clone();
        let closing = self.clone();
        let config = WindowConfig::new(move || ui::app(me.clone()))
            .with_title("Blitznir")
            .with_app_id(crate::APP_ID)
            .with_size(SIZE.0, SIZE.1)
            .with_min_size(MIN_SIZE.0, MIN_SIZE.1)
            .with_custom_scale_factor(f64::from(self.store.state().zoom()))
            .with_on_close(move |_, _| {
                // the close button hides, never quits
                closing.close();
                CloseDecision::KeepOpen
            });
        config.with_window_attributes(|attrs, event_loop| {
            let Some(screen) = event_loop.primary_monitor() else {
                return attrs;
            };
            let size: LogicalSize<f64> = screen.size().to_logical(screen.scale_factor());
            let at: LogicalPosition<f64> = screen.position().to_logical(screen.scale_factor());
            let x = at.x + (size.width - SIZE.0).max(0.0) / 2.0;
            let y = at.y + (size.height - SIZE.1).max(0.0) / 2.0;
            let attrs = attrs.with_position(LogicalPosition::new(x, y));
            // one bar, not two: the header runs under a see-through title
            // bar, beside the traffic lights (ui/header.rs makes room)
            #[cfg(target_os = "macos")]
            let attrs = {
                use freya::winit::platform::macos::WindowAttributesExtMacOS;
                attrs
                    .with_titlebar_transparent(true)
                    .with_title_hidden(true)
                    .with_fullsize_content_view(true)
            };
            attrs
        })
    }
}
