//! The desktop's dark preference, org.freedesktop.appearance color-scheme
//! through the Settings portal (GNOME, KDE and COSMIC answer), kept by a
//! thread of its own: read once, then followed through SettingChanged, so
//! asking it costs the panel nothing.

use gtk::gio;
use gtk::glib::{self, ToVariant};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const SETTINGS: &str = "org.freedesktop.portal.Settings";
const APPEARANCE: &str = "org.freedesktop.appearance";
const KEY: &str = "color-scheme";

/// The portal's own numbers: 0 says nothing, 1 prefers dark, 2 light.
#[derive(Clone, Default)]
pub struct Scheme(Arc<AtomicU32>);

impl Scheme {
    /// Starts following the desktop's setting; until the first answer, None.
    pub fn follow() -> Scheme {
        let scheme = Scheme::default();
        let s = scheme.clone();
        let spawned = thread::Builder::new()
            .name("color-scheme".into())
            .spawn(move || {
                let ctx = glib::MainContext::new();
                match ctx.with_thread_default(|| s.watch(&ctx)) {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => log::info!("color-scheme: {e}; the window system's theme stays"),
                    Err(e) => log::warn!("color-scheme: {e}"),
                }
            });
        if let Err(e) = spawned {
            log::warn!("color-scheme: {e}");
        }
        scheme
    }

    pub fn dark(&self) -> Option<bool> {
        match self.0.load(Ordering::Relaxed) {
            1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }

    /// Subscribed before the read: a change in between is not lost. Runs
    /// until the session's bus goes.
    fn watch(&self, ctx: &glib::MainContext) -> Result<(), String> {
        let bus = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)
            .map_err(|e| e.to_string())?;
        let s = self.clone();
        let _sub = bus.signal_subscribe(
            Some(PORTAL),
            Some(SETTINGS),
            Some("SettingChanged"),
            Some(PATH),
            Some(APPEARANCE),
            gio::DBusSignalFlags::NONE,
            move |_, _, _, _, _, args| {
                // (ssv): namespace, key, value
                let (ns, key, value) = (
                    args.child_value(0),
                    args.child_value(1),
                    args.child_value(2),
                );
                let is = |v: &glib::Variant, want: &str| v.str() == Some(want);
                if is(&ns, APPEARANCE) && is(&key, KEY) {
                    s.store(&value);
                }
            },
        );
        let read = read(&bus, "ReadOne").or_else(|| read(&bus, "Read"));
        let value = read.ok_or("the Settings portal answers neither ReadOne nor Read")?;
        self.store(&value);
        glib::MainLoop::new(Some(ctx), false).run();
        Ok(())
    }

    fn store(&self, value: &glib::Variant) {
        if let Some(n) = number(value) {
            self.0.store(n, Ordering::Relaxed);
        }
    }
}

/// ReadOne (xdg-desktop-portal 1.15+) or the older Read, which wraps the
/// value in one more variant.
fn read(bus: &gio::DBusConnection, method: &str) -> Option<glib::Variant> {
    let reply = bus
        .call_sync(
            Some(PORTAL),
            PATH,
            SETTINGS,
            method,
            Some(&(APPEARANCE, KEY).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            2_000,
            gio::Cancellable::NONE,
        )
        .ok()?;
    Some(reply.child_value(0))
}

/// The u32 inside however many variants hold it.
fn number(v: &glib::Variant) -> Option<u32> {
    let mut v = v.clone();
    loop {
        if let Some(n) = v.get::<u32>() {
            return Some(n);
        }
        v = v.as_variant()?;
    }
}

#[cfg(test)]
mod tests {
    use super::number;
    use gtk::glib::ToVariant;

    #[test]
    fn the_number_is_found_however_wrapped() {
        let one = 1u32.to_variant();
        assert_eq!(number(&one), Some(1));
        let read_one = one.to_variant(); // v
        assert_eq!(number(&read_one), Some(1));
        let read = read_one.to_variant(); // v in v: the old Read
        assert_eq!(number(&read), Some(1));
        assert_eq!(number(&"dark".to_variant()), None);
    }
}
