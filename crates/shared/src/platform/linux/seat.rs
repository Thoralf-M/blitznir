//! The compositor's keymap, asked once over Wayland: every compositor sends
//! a client the XKB keymap it reads keys with as soon as the client takes
//! the seat's keyboard, focused or not. What the uinput keyboard sends is
//! read with exactly that keymap, so typing maps characters with it too.
//! Several layouts: the first. Which one is active is said only to the
//! focused client, never us while we type into another app.

use std::{
    fs::File,
    os::{fd::OwnedFd, unix::fs::FileExt},
};
use wayland_client::{
    Connection, Dispatch, QueueHandle, WEnum,
    protocol::{wl_keyboard, wl_registry, wl_seat},
};

#[derive(Default)]
struct Ask {
    seat: Option<wl_seat::WlSeat>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    keymap: Option<(OwnedFd, u32)>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for Ask {
    fn event(
        ask: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
            && interface == "wl_seat"
            && ask.seat.is_none()
        {
            ask.seat = Some(registry.bind(name, version.min(5), qh, ()));
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for Ask {
    fn event(
        ask: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(caps),
        } = event
            && caps.contains(wl_seat::Capability::Keyboard)
            && ask.keyboard.is_none()
        {
            ask.keyboard = Some(seat.get_keyboard(qh, ()));
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for Ask {
    fn event(
        ask: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_keyboard::Event::Keymap {
            format: WEnum::Value(wl_keyboard::KeymapFormat::XkbV1),
            fd,
            size,
        } = event
        {
            ask.keymap = Some((fd, size));
        }
    }
}

fn err(e: impl std::fmt::Display) -> String {
    format!("the desktop's keymap: {e}")
}

/// The keymap as XKB text. Err: no Wayland session (X11), or no keyboard.
pub fn keymap() -> Result<String, String> {
    let conn = Connection::connect_to_env().map_err(err)?;
    let mut queue = conn.new_event_queue();
    conn.display().get_registry(&queue.handle(), ());
    let mut ask = Ask::default();
    // the registry, then the seat's capabilities, then the keymap
    for _ in 0..3 {
        queue.roundtrip(&mut ask).map_err(err)?;
        if ask.keymap.is_some() {
            break;
        }
    }
    let (fd, size) = ask.keymap.ok_or_else(|| err("the compositor sent none"))?;
    // at offset 0, not read(): the compositor may hand every client the
    // same file, whose offset a read would move for all of them
    let mut bytes = vec![0; size as usize];
    File::from(fd).read_exact_at(&mut bytes, 0).map_err(err)?;
    let text = String::from_utf8(bytes).map_err(err)?;
    Ok(text.trim_end_matches('\0').to_string())
}

#[cfg(test)]
mod tests {
    /// Only in a Wayland session with a keyboard (weston's x11 backend in the
    /// container will do); `BLITZNIR_KEYMAP_TEST` set says it has one, so a
    /// skip is a failure.
    #[test]
    fn the_compositor_sends_its_keymap() {
        let required = std::env::var_os("BLITZNIR_KEYMAP_TEST").is_some();
        let Ok(text) = super::keymap() else {
            assert!(!required, "no keymap from the compositor");
            return;
        };
        assert!(
            text.contains("xkb_keymap"),
            "{}",
            &text[..text.len().min(80)]
        );
        let (map, what) = super::super::keymap::desktop_map(&text).expect("it compiles");
        assert!(map.contains_key(&'a'), "{what}");
    }
}
