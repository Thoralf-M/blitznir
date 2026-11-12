//! The clipboard without focus, through the compositor's data-control
//! protocol (zwlr_data_control, what wl-copy uses; wlroots, KDE, COSMIC and
//! others offer it, GNOME does not). Our panel has hidden by the paste, and
//! a Wayland client without focus may not set the clipboard otherwise; the
//! X11 clipboard reaches Wayland apps only where the compositor copies it
//! across. The text is ours until something else is copied: a thread of its
//! own answers every paste with it.
//!
//! It also hears every app's copy and selection: how often the clipboard
//! changed (a copy shows as one more, as macOS's changeCount does), and when
//! PRIMARY last did, which says whether PRIMARY is a fresh selection.

use super::pipe;
use blitznir_core::lock;
use std::{
    fs::File,
    io::Write,
    os::fd::{AsFd, OwnedFd},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle, event_created_child,
    protocol::{wl_registry, wl_seat},
};
use wayland_protocols_wlr::data_control::v1::client::{
    zwlr_data_control_device_v1 as device, zwlr_data_control_manager_v1 as manager,
    zwlr_data_control_offer_v1 as offer, zwlr_data_control_source_v1 as source,
};

/// What apps ask a text for.
const TEXT_TYPES: [&str; 4] = [
    "text/plain;charset=utf-8",
    "text/plain",
    "UTF8_STRING",
    "STRING",
];

/// The longest an app gets to hand over its copied text.
const READ_WAIT: Duration = Duration::from_secs(1);

/// The compositor's clipboard, held for as long as we live.
pub struct Control {
    conn: Connection,
    qh: QueueHandle<Owner>,
    manager: manager::ZwlrDataControlManagerV1,
    device: device::ZwlrDataControlDeviceV1,
    seen: Arc<Mutex<Seen>>,
}

/// What the device has said about the selections.
#[derive(Default)]
struct Seen {
    /// How often the clipboard changed.
    changes: u64,
    /// What is on it now.
    clipboard: Option<offer::ZwlrDataControlOfferV1>,
    /// When something was last selected (PRIMARY set, not cleared); None:
    /// nothing since the start.
    primary_at: Option<Instant>,
    /// PRIMARY's first event came: the state at the start, no new selection.
    primed: bool,
    /// The compositor took the device back: setting the clipboard fails.
    finished: bool,
}

/// The types an offer says it has, as it says them.
#[derive(Default)]
struct Types(Mutex<Vec<String>>);

#[derive(Default)]
struct Owner {
    seat: Option<wl_seat::WlSeat>,
    manager: Option<manager::ZwlrDataControlManagerV1>,
}

impl Control {
    /// None: no Wayland, or a compositor without the protocol (GNOME).
    pub fn new() -> Option<Control> {
        let conn = Connection::connect_to_env().ok()?;
        let mut queue: EventQueue<Owner> = conn.new_event_queue();
        let qh = queue.handle();
        conn.display().get_registry(&qh, ());
        let mut owner = Owner::default();
        queue.roundtrip(&mut owner).ok()?;
        let (seat, manager) = (owner.seat.take()?, owner.manager.take()?);
        let seen = Arc::<Mutex<Seen>>::default();
        let device = manager.get_data_device(&seat, &qh, seen.clone());
        let spawned = thread::Builder::new()
            .name("clipboard".into())
            .spawn(move || {
                // answers the pastes until the connection goes
                while queue.blocking_dispatch(&mut owner).is_ok() {}
                log::warn!("clipboard: the compositor's connection is gone");
            });
        spawned.ok()?;
        log::info!(
            "clipboard: the compositor's data control, v{}",
            manager.version()
        );
        Some(Control {
            conn,
            qh,
            manager,
            device,
            seen,
        })
    }

    /// The compositor tells when PRIMARY changes (data control v2).
    pub fn tells_primary(&self) -> bool {
        self.manager.version() >= 2
    }

    /// How often the clipboard has changed so far.
    pub fn changes(&self) -> u64 {
        lock(&self.seen).changes
    }

    /// PRIMARY changed within `within`: a selection just made.
    pub fn primary_fresh(&self, within: Duration) -> bool {
        lock(&self.seen)
            .primary_at
            .is_some_and(|t| t.elapsed() < within)
    }

    /// The clipboard's text, from whichever app holds it. None: none, no
    /// text, an empty one, or no answer within [`READ_WAIT`]. Read here,
    /// not on the dispatch thread: our own source answers there.
    pub fn clipboard_text(&self) -> Option<String> {
        let offer = lock(&self.seen).clipboard.clone()?;
        let types = offer
            .data::<Types>()
            .map(|t| lock(&t.0).clone())
            .unwrap_or_default();
        let mime = TEXT_TYPES.iter().find(|t| types.iter().any(|x| x == *t))?;
        let (mut from, to) = std::io::pipe().ok()?;
        offer.receive((*mime).into(), to.as_fd());
        self.conn.flush().ok()?;
        drop(to); // the app's copy is the only writer now: its close ends the read
        let bytes = pipe::read_within(&mut from, READ_WAIT)?;
        let text = if *mime == "STRING" {
            bytes.iter().map(|&b| char::from(b)).collect() // Latin-1
        } else {
            String::from_utf8_lossy(&bytes).into_owned()
        };
        (!text.is_empty()).then_some(text)
    }

    /// `text` is the clipboard now; its source answers the pastes.
    pub fn set(&self, text: &str) -> Result<(), String> {
        if lock(&self.seen).finished {
            return Err("clipboard: the compositor took data control back".into());
        }
        let source = self
            .manager
            .create_data_source(&self.qh, Arc::new(text.to_string()));
        for t in TEXT_TYPES {
            source.offer(t.into());
        }
        self.device.set_selection(Some(&source));
        self.conn
            .flush()
            .map_err(|e| format!("clipboard: the compositor: {e}"))
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for Owner {
    fn event(
        owner: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        else {
            return;
        };
        match interface.as_str() {
            "wl_seat" if owner.seat.is_none() => {
                owner.seat = Some(registry.bind(name, version.min(5), qh, ()));
            }
            "zwlr_data_control_manager_v1" => {
                owner.manager = Some(registry.bind(name, version.min(2), qh, ()));
            }
            _ => {}
        }
    }
}

/// A paste: the text into the asking app's pipe.
impl Dispatch<source::ZwlrDataControlSourceV1, Arc<String>> for Owner {
    fn event(
        _: &mut Self,
        source: &source::ZwlrDataControlSourceV1,
        event: source::Event,
        text: &Arc<String>,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            // a thread of its own: an app slow to read must not stop the dispatch
            source::Event::Send { fd, .. } => {
                let text = text.clone();
                let spawned = thread::Builder::new()
                    .name("clipboard-paste".into())
                    .spawn(move || write(fd, &text));
                if let Err(e) = spawned {
                    log::warn!("clipboard: a paste: {e}");
                }
            }
            // something else was copied: this text is no longer the clipboard
            source::Event::Cancelled => source.destroy(),
            _ => {}
        }
    }
}

fn write(fd: OwnedFd, text: &str) {
    if let Err(e) = File::from(fd).write_all(text.as_bytes()) {
        log::warn!("clipboard: a paste: {e}");
    }
}

/// The device announces every app's copy and selection as an offer: the
/// clipboard's is kept (to read it), PRIMARY's only noted.
impl Dispatch<device::ZwlrDataControlDeviceV1, Arc<Mutex<Seen>>> for Owner {
    fn event(
        _: &mut Self,
        _: &device::ZwlrDataControlDeviceV1,
        event: device::Event,
        seen: &Arc<Mutex<Seen>>,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let mut seen = lock(seen);
        match event {
            device::Event::Selection { id } => {
                if let Some(old) = std::mem::replace(&mut seen.clipboard, id) {
                    old.destroy();
                }
                seen.changes += 1;
            }
            device::Event::PrimarySelection { id } => {
                let new = std::mem::replace(&mut seen.primed, true);
                if let Some(o) = id {
                    o.destroy();
                    if new {
                        seen.primary_at = Some(Instant::now());
                    }
                }
            }
            device::Event::Finished => seen.finished = true,
            _ => {}
        }
    }

    event_created_child!(Owner, device::ZwlrDataControlDeviceV1, [
        device::EVT_DATA_OFFER_OPCODE => (offer::ZwlrDataControlOfferV1, Types::default()),
    ]);
}

/// An offer names its types first, one event each.
impl Dispatch<offer::ZwlrDataControlOfferV1, Types> for Owner {
    fn event(
        _: &mut Self,
        _: &offer::ZwlrDataControlOfferV1,
        event: offer::Event,
        types: &Types,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let offer::Event::Offer { mime_type } = event {
            lock(&types.0).push(mime_type);
        }
    }
}

impl Dispatch<manager::ZwlrDataControlManagerV1, ()> for Owner {
    fn event(
        _: &mut Self,
        _: &manager::ZwlrDataControlManagerV1,
        _: manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for Owner {
    fn event(
        _: &mut Self,
        _: &wl_seat::WlSeat,
        _: wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

#[cfg(test)]
mod tests {
    use std::{
        process::{Command, Stdio},
        time::Duration,
    };

    /// Only where a compositor offers data control and wl-paste is there
    /// (a headless sway in the container will do); `BLITZNIR_WAYLAND_TEST`
    /// set says they are, so a skip is a failure.
    #[test]
    fn a_set_text_is_what_another_app_pastes() {
        let required = std::env::var_os("BLITZNIR_WAYLAND_TEST").is_some();
        let paste = || Command::new("wl-paste").arg("--no-newline").output();
        let Some(c) = super::Control::new().filter(|_| paste().is_ok()) else {
            assert!(!required, "no data control or no wl-paste");
            return;
        };
        c.set("Grüße, zwei").expect("set");
        std::thread::sleep(Duration::from_millis(200));
        let out = paste().expect("wl-paste");
        assert_eq!(String::from_utf8_lossy(&out.stdout), "Grüße, zwei");
        c.set("next").expect("set again");
        std::thread::sleep(Duration::from_millis(200));
        let out = paste().expect("wl-paste");
        assert_eq!(String::from_utf8_lossy(&out.stdout), "next");
        // another app's copy: one more change, and its text reads back
        let before = c.changes();
        // wl-copy stays behind to serve its copy: it must not hold our output open
        let copy = |args: &[&str]| {
            let mut c = Command::new("wl-copy");
            c.args(args).stdout(Stdio::null()).stderr(Stdio::null());
            c.status().is_ok_and(|s| s.success())
        };
        let copied = copy(&["from another app"]);
        assert!(copied);
        std::thread::sleep(Duration::from_millis(300));
        assert!(c.changes() > before, "the copy was heard");
        assert_eq!(c.clipboard_text().as_deref(), Some("from another app"));
        // a selection: PRIMARY is fresh
        assert!(copy(&["--primary", "sel"]));
        std::thread::sleep(Duration::from_millis(300));
        assert!(c.primary_fresh(Duration::from_secs(5)));
    }
}
