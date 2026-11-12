//! Region screenshot through org.freedesktop.portal.Screenshot, interactive:
//! the desktop's own selection UI. GNOME 50 refuses org.gnome.Shell.Screenshot
//! to every other program, gnome-screenshot included (it falls back to an X11
//! picker that never shows), so on GNOME Wayland this is the only way.

use gtk::{
    gio,
    glib::{self, ToVariant},
    prelude::FileExt,
};
use std::{
    path::Path,
    sync::mpsc,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const PORTAL: &str = "org.freedesktop.portal.Desktop";
/// Long enough to pick a region at leisure; a forgotten one is given up.
const ANSWER_WAIT: Duration = Duration::from_secs(120);

fn err(e: impl std::fmt::Display) -> String {
    format!("screenshot portal: {e}")
}

/// Let the user pick a region and move the picture to `out`. false: cancelled.
/// Blocks until the user is done; runs its own main context, so any thread will do.
pub fn screenshot(out: &Path) -> Result<bool, String> {
    let ctx = glib::MainContext::new();
    ctx.with_thread_default(|| shoot(&ctx, out)).map_err(err)?
}

fn shoot(ctx: &glib::MainContext, out: &Path) -> Result<bool, String> {
    let bus = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).map_err(err)?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let token = format!("blitznir{}_{nanos}", std::process::id());
    let sender = bus.unique_name().ok_or_else(|| err("no bus name"))?;
    let sender = sender.trim_start_matches(':').replace('.', "_");
    // the reply comes as a signal on this path; subscribe before asking
    let request = format!("/org/freedesktop/portal/desktop/request/{sender}/{token}");
    let (tx, rx) = mpsc::channel();
    let sub = bus.signal_subscribe(
        Some(PORTAL),
        Some("org.freedesktop.portal.Request"),
        Some("Response"),
        Some(&request),
        None,
        gio::DBusSignalFlags::NONE,
        move |_, _, _, _, _, params| {
            let _ = tx.send(params.clone());
        },
    );
    let opts = glib::VariantDict::new(None);
    opts.insert("handle_token", &token);
    opts.insert("interactive", true);
    let asked = bus.call_sync(
        Some(PORTAL),
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Screenshot",
        "Screenshot",
        // (sa{sv}); a Rust tuple would box the dict as a `v`
        Some(&glib::Variant::tuple_from_iter([
            "".to_variant(),
            opts.end(),
        ])),
        None,
        gio::DBusCallFlags::NONE,
        -1,
        gio::Cancellable::NONE,
    );
    let answer = asked.map_err(err).and_then(|_| wait(ctx, &rx));
    bus.signal_unsubscribe(sub);
    let (code, results) = answer?
        .get::<(u32, glib::VariantDict)>()
        .ok_or_else(|| err("unexpected reply"))?;
    if code != 0 {
        return Ok(false); // 1 = cancelled, 2 = ended some other way
    }
    let uri: String = results
        .lookup("uri")
        .ok()
        .flatten()
        .ok_or_else(|| err("no uri in the reply"))?;
    let shot = gio::File::for_uri(&uri)
        .path()
        .ok_or_else(|| err(format!("{uri} is not a local file")))?;
    // GNOME also filed it under ~/Pictures/Screenshots: move it, as
    // screencapture on mac writes only our file
    std::fs::copy(&shot, out).map_err(err)?;
    let _ = std::fs::remove_file(&shot);
    Ok(true)
}

fn wait(
    ctx: &glib::MainContext,
    rx: &mpsc::Receiver<glib::Variant>,
) -> Result<glib::Variant, String> {
    let end = Instant::now() + ANSWER_WAIT;
    loop {
        while ctx.iteration(false) {}
        if let Ok(reply) = rx.try_recv() {
            return Ok(reply);
        }
        if Instant::now() > end {
            return Err(err("no answer in 2 minutes"));
        }
        thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn without_a_session_bus_it_is_an_error() {
        if std::path::Path::new("/.dockerenv").exists() {
            let out = std::env::temp_dir().join("portal-test.png");
            assert!(super::screenshot(&out).is_err());
        }
    }
}
