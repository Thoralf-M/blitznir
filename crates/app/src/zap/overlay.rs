//! The overlay window and its one component: a canvas that draws the newest
//! strike while its flicker lasts, or the beam until it has collapsed (in
//! Skia), and esc for the job. When the bolts land on the selected text
//! itself, a faint light over it flashes with them; the beam never covers
//! it. White and grey only: the light is the glow.

use super::{
    Scene, Zap, beam,
    bolt::{Bolt, Pt, flicker},
};
use blitznir_core::{
    platform::{Bounds, Rect},
    sky::{self, Sky},
};
use freya::{
    engine::prelude::{
        BlurStyle, Canvas as SkCanvas, Color4f, Colors, Gradient, Interpolation, MaskFilter, Paint,
        PaintStyle, PathBuilder, RRect, SkPath, SkRect, TileMode, shaders,
    },
    prelude::*,
    winit::{
        event_loop::ActiveEventLoop,
        monitor::MonitorHandle,
        window::{WindowAttributes, WindowLevel},
    },
};
use std::{
    slice,
    sync::Arc,
    time::{Duration, Instant},
};

/// A flicker frame this often while it flashes, dips and strikes again
/// (its first fifth), half as often while it dies away; nothing is drawn
/// between strikes. A hung rewrite striking for 20 s on a 3840×1600 screen:
/// 60 fps all through, the app at 7 % of a core; this, 5.5 % (1.5 of it the
/// hammer's glow; a webview drawing the same, 5 %).
const FRAME: Duration = Duration::from_millis(16);
const FADE_FRAME: Duration = Duration::from_millis(33);
const FLASH_END: f32 = 0.2;

/// The beam moves all the time: every frame is [`FRAME`].
fn frame(s: &Scene) -> Duration {
    let t = s.strike.as_ref().map_or(1.0, |s| {
        s.at.elapsed().as_secs_f32() / s.lasts.as_secs_f32()
    });
    if t < FLASH_END || s.beam.is_some() {
        FRAME
    } else {
        FADE_FRAME
    }
}

/// Something is moving: a strike's flicker, or the beam before it is gone.
fn moving(s: &Scene) -> bool {
    let flickers = s.strike.as_ref().is_some_and(|s| s.at.elapsed() < s.lasts);
    flickers || s.beam.is_some_and(|b| !b.phase(Instant::now()).gone())
}

pub fn config(zap: Arc<Zap>, area: Rect) -> WindowConfig {
    WindowConfig::new(move || app(zap.clone()))
        .with_title("Blitznir lightning")
        .with_app_id(crate::APP_ID)
        .with_size(area.w, area.h)
        .with_decorations(false)
        .with_transparency(true)
        .with_background(Color::TRANSPARENT)
        // GNOME maximizes only a window that may change its size
        .with_resizable(cfg!(target_os = "linux"))
        .with_window_attributes(move |attrs, el| place(attrs, el, area))
        .with_window_handle(|w| {
            // clicks go to the app below
            if let Err(e) = w.set_cursor_hittest(false) {
                log::warn!("effect: the overlay takes clicks: {e}");
            }
        })
}

/// A monitor's scale and its rect in points: winit scales each by its own factor.
pub(crate) fn screen(m: &MonitorHandle) -> (f64, Rect) {
    let s = m.scale_factor();
    let (p, z) = (m.position(), m.size());
    let (w, h) = (f64::from(z.width) / s, f64::from(z.height) / s);
    let (x, y) = (f64::from(p.x) / s, f64::from(p.y) / s);
    (s, Rect { x, y, w, h })
}

/// Where the effect happens: the screen the target is on (Linux: the
/// primary one, where the compositor maximizes the overlay), the aim on it.
pub fn cover(el: &ActiveEventLoop, sel: Option<Bounds>, mouse: Option<(f64, f64)>) -> Option<Sky> {
    let primary = el.primary_monitor().map(|m| screen(&m));
    // the primary first: mirrored screens share one rect, and the status
    // item is on the primary's menu bar, in its scale (Sky::source)
    let mut screens: Vec<_> = el.available_monitors().map(|m| screen(&m)).collect();
    screens.sort_by_key(|s| Some(*s) != primary);
    let (_, main) = primary.or(screens.first().copied())?;
    // on no screen: a stale answer, for a window that has moved off since
    let sel = sel.filter(|b| screens.iter().any(|(_, a)| a.intersects(&b.rect)));
    let target = sky::aim(sel, mouse);
    let at = target.map(|(r, _)| r.centre()).or(mouse);
    let area = match at {
        Some(at) if cfg!(target_os = "macos") => sky::screen_at(&screens, at, main),
        _ => main,
    };
    Some(Sky::new(area, screens, target))
}

/// Over everything, on `area`: macOS places it there. Wayland lets no
/// window place itself, and GNOME puts an opaque black backdrop behind a
/// fullscreen one (seen on the VM), so there it is maximized: the work
/// area of the screen the compositor picks, transparent.
fn place(attrs: WindowAttributes, el: &ActiveEventLoop, area: Rect) -> WindowAttributes {
    let attrs = attrs.with_window_level(WindowLevel::AlwaysOnTop);
    #[cfg(target_os = "macos")]
    {
        use freya::winit::{dpi::LogicalPosition, platform::macos::WindowAttributesExtMacOS};
        let _ = el;
        attrs
            .with_position(LogicalPosition::new(area.x, area.y))
            .with_has_shadow(false)
    }
    #[cfg(target_os = "linux")]
    {
        let _ = (area, el);
        attrs.with_maximized(true)
    }
}

/// The zap's scene, from any thread, into this window; while something
/// moves, a frame every [`frame`] (the canvas draws only when asked), and
/// none between strikes or once the beam is gone. The scene it opens with
/// may already hold the beam: it animates before it waits.
fn use_scene(zap: &Arc<Zap>) -> State<Scene> {
    let mut scene = use_state(|| zap.scene());
    let zap = zap.clone();
    use_hook(move || {
        let (tx, rx) = async_channel::unbounded();
        zap.attach(tx);
        let window = Platform::get().current_window();
        spawn(async move {
            scene.set(zap.scene()); // a change before attach reached no one
            loop {
                while moving(&scene.peek()) {
                    window.request_redraw();
                    timer(frame(&scene.peek())).await;
                    if rx.try_recv().is_ok() {
                        while rx.try_recv().is_ok() {}
                        scene.set(zap.scene());
                    }
                }
                window.request_redraw(); // the end: nothing
                if rx.recv().await.is_err() {
                    break;
                }
                scene.set(zap.scene());
            }
        });
    });
    scene
}

fn app(zap: Arc<Zap>) -> impl IntoElement {
    let scene = use_scene(&zap);
    // every other key is swallowed: none lands in the selection about to go
    let on_key = move |e: Event<KeyboardEventData>| {
        if matches!(e.key, Key::Named(NamedKey::Escape)) {
            zap.escape();
        }
    };
    let mut cache = beam::Cache::default();
    let draw = RenderCallback::new(move |ctx| {
        let Some(c) = crate::ui::canvas(ctx) else {
            return;
        };
        let s = scene.peek();
        match &s.beam {
            Some(b) => beam::draw(c, b, Instant::now(), &mut cache),
            None => draw(c, &s),
        }
    });
    rect()
        .expanded()
        .on_global_key_down(on_key)
        .child(rect().background(draw).expanded())
}

fn white(alpha: f32) -> Color4f {
    Color4f::new(1.0, 1.0, 1.0, alpha)
}

/// The newest strike at its flicker's opacity now: the lit text under it,
/// the branches fading to their tips, the wide glow, the bright core, the
/// light spilling flat where it lands.
fn draw(c: &SkCanvas, s: &Scene) {
    let Some(strike) = &s.strike else {
        return;
    };
    let t = strike.at.elapsed().as_secs_f32() / strike.lasts.as_secs_f32();
    let o = flicker(t);
    if o <= 0.0 {
        return;
    }
    let b = &strike.bolt;
    if s.exact {
        lit(c, s.aim, o);
        chain(c, b, t);
    }
    // the last strike: brighter branches and glow, a wide halo
    let last = b.k > 1.0;
    let main = slice::from_ref(&b.main);
    tapered(c, &b.twigs, 0.7 * b.k, 0.55 * o, 6.0);
    tapered(c, &b.side, b.k, if last { 0.85 } else { 0.55 } * o, 8.0);
    if last {
        halo(c, main, 12.0 * b.k, 0.35 * o, 90.0);
    }
    stroke(c, main, 6.0 * b.k, if last { 0.42 } else { 0.28 } * o, 32.0);
    stroke(c, main, 1.8 * b.k, o, 6.0);
    if let Some(tip) = b.main.last() {
        surface(c, *tip, 55.0 * b.k, o);
    }
}

/// Where it lands: light spilling flat along the surface, far wider than
/// high, white at the contact and blue-violet out to its edge.
fn surface(c: &SkCanvas, (x, y): Pt, r: f32, o: f32) {
    let glow = |a: f32| Color4f::new(0.72, 0.8, 1.0, a);
    let colors = [white(0.85 * o), glow(0.4 * o), glow(0.0)];
    let stops = [0.0, 0.25, 1.0];
    let colors = Colors::new(&colors, Some(&stops), TileMode::Clamp, None);
    let gradient = Gradient::new(colors, Interpolation::default());
    let mut paint = Paint::default();
    paint.set_shader(shaders::radial_gradient(((0.0, 0.0), 1.0), &gradient, None));
    c.save();
    c.translate((x, y));
    c.scale((r * 2.6, r * 0.35));
    c.draw_rect(SkRect::new(-1.0, -1.0, 1.0, 1.0), &paint);
    c.restore();
}

/// Pieces of a branch toward its tip, each this much thinner and dimmer
/// than the one before: branches fade out as they do in photos.
const TAPER: [f32; 4] = [1.0, 0.78, 0.56, 0.34];

/// `lines` in [`TAPER`]'s pieces: each piece of every line in one path.
fn tapered(c: &SkCanvas, lines: &[Vec<Pt>], width: f32, alpha: f32, blur: f32) {
    let n = TAPER.len();
    for (i, k) in TAPER.iter().enumerate() {
        let piece = |pts: &Vec<Pt>| {
            let last = pts.len().saturating_sub(1);
            pts[i * last / n..=((i + 1) * last / n).min(last)].to_vec()
        };
        let pieces: Vec<Vec<Pt>> = lines.iter().filter(|p| !p.is_empty()).map(piece).collect();
        stroke(c, &pieces, width * k, alpha * k, blur);
    }
}

/// The faint chain along the line: hop after hop lights up over the first
/// third of the strike, both ways at once, and fades with the rest.
fn chain(c: &SkCanvas, b: &Bolt, t: f32) {
    let reach = (t / 0.35).min(1.0);
    let fade = if t < 0.6 { 1.0 } else { 1.0 - (t - 0.6) / 0.4 };
    for side in &b.chain {
        let shown = (side.len() as f32 * reach).ceil() as usize;
        stroke(
            c,
            &side[..shown.min(side.len())],
            1.1 * b.k,
            0.32 * fade,
            5.0,
        );
    }
}

/// `lines` in white, over their own glow: a canvas shadow of `blur`, which
/// is twice the Gaussian's sigma.
fn stroke(c: &SkCanvas, lines: &[Vec<Pt>], width: f32, alpha: f32, blur: f32) {
    let (path, paint) = pen(lines, width, alpha);
    c.draw_path(&path, &glowing(&paint, blur));
    c.draw_path(&path, &paint);
}

/// Only the glow of `lines`: a soft light around them, no line of its own.
fn halo(c: &SkCanvas, lines: &[Vec<Pt>], width: f32, alpha: f32, blur: f32) {
    let (path, paint) = pen(lines, width, alpha);
    c.draw_path(&path, &glowing(&paint, blur));
}

/// The glow of a white line: ionised air, a little blue-violet.
fn glowing(paint: &Paint, blur: f32) -> Paint {
    let mut glow = paint.clone();
    glow.set_color4f(Color4f::new(0.72, 0.8, 1.0, paint.alpha_f()), None);
    glow.set_mask_filter(MaskFilter::blur(BlurStyle::Normal, blur / 2.0, None));
    glow
}

/// `lines` as one path, and a white pen `width` wide.
fn pen(lines: &[Vec<Pt>], width: f32, alpha: f32) -> (SkPath, Paint) {
    let mut path = PathBuilder::new();
    for pts in lines {
        if let Some((first, rest)) = pts.split_first() {
            path.move_to(*first);
            for p in rest {
                path.line_to(*p);
            }
        }
    }
    let path = path.detach();
    let mut paint = Paint::default();
    paint
        .set_anti_alias(true)
        .set_style(PaintStyle::Stroke)
        .set_stroke_width(width)
        .set_color4f(white(alpha), None);
    (path, paint)
}

/// The selected text, lit a little wider than itself, with a soft rim.
fn lit(c: &SkCanvas, aim: Rect, o: f32) {
    let (x, y, w, h) = (aim.x as f32, aim.y as f32, aim.w as f32, aim.h as f32);
    let rr = RRect::new_rect_xy(
        SkRect::new(x - 3.0, y - 2.0, x + w + 3.0, y + h + 2.0),
        4.0,
        4.0,
    );
    let mut rim = Paint::default();
    rim.set_anti_alias(true)
        .set_color4f(white(0.8 * o), None)
        .set_mask_filter(MaskFilter::blur(BlurStyle::Outer, 5.0, None));
    c.draw_rrect(rr, &rim);
    let mut fill = Paint::default();
    fill.set_anti_alias(true).set_color4f(white(0.16 * o), None);
    c.draw_rrect(rr, &fill);
}
