//! evdev hotkeys: one reader thread per keyboard (a device that can type an
//! "a"), /dev/input rescanned for keyboards plugged in later. Listen-only: no
//! grab, so nothing is ever swallowed.

use blitznir_core::{
    lock,
    platform::{Key, KeyEvent},
};
use evdev::{Device, EventSummary, KeyCode};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc::Sender},
    thread,
    time::Duration,
};

/// Our uinput device; the listener skips it or it would hear our own typing.
pub const OWN_DEVICE: &str = "blitznir-kbd";
/// How often /dev/input is looked at for new keyboards.
const RESCAN: Duration = Duration::from_secs(5);

/// Every key a hotkey can name. Most laptops handle Fn in firmware and never
/// send KEY_FN; it is here for the keyboards that do.
const TABLE: [(KeyCode, Key); 33] = [
    (KeyCode::KEY_LEFTSHIFT, Key::LeftShift),
    (KeyCode::KEY_RIGHTSHIFT, Key::RightShift),
    (KeyCode::KEY_LEFTCTRL, Key::LeftCtrl),
    (KeyCode::KEY_RIGHTCTRL, Key::RightCtrl),
    (KeyCode::KEY_LEFTALT, Key::LeftAlt),
    (KeyCode::KEY_RIGHTALT, Key::RightAlt),
    (KeyCode::KEY_LEFTMETA, Key::LeftMeta),
    (KeyCode::KEY_RIGHTMETA, Key::RightMeta),
    (KeyCode::KEY_FN, Key::Fn),
    (KeyCode::KEY_F1, Key::F(1)),
    (KeyCode::KEY_F2, Key::F(2)),
    (KeyCode::KEY_F3, Key::F(3)),
    (KeyCode::KEY_F4, Key::F(4)),
    (KeyCode::KEY_F5, Key::F(5)),
    (KeyCode::KEY_F6, Key::F(6)),
    (KeyCode::KEY_F7, Key::F(7)),
    (KeyCode::KEY_F8, Key::F(8)),
    (KeyCode::KEY_F9, Key::F(9)),
    (KeyCode::KEY_F10, Key::F(10)),
    (KeyCode::KEY_F11, Key::F(11)),
    (KeyCode::KEY_F12, Key::F(12)),
    (KeyCode::KEY_F13, Key::F(13)),
    (KeyCode::KEY_F14, Key::F(14)),
    (KeyCode::KEY_F15, Key::F(15)),
    (KeyCode::KEY_F16, Key::F(16)),
    (KeyCode::KEY_F17, Key::F(17)),
    (KeyCode::KEY_F18, Key::F(18)),
    (KeyCode::KEY_F19, Key::F(19)),
    (KeyCode::KEY_F20, Key::F(20)),
    (KeyCode::KEY_F21, Key::F(21)),
    (KeyCode::KEY_F22, Key::F(22)),
    (KeyCode::KEY_F23, Key::F(23)),
    (KeyCode::KEY_F24, Key::F(24)),
];

fn key_of(code: KeyCode) -> Option<Key> {
    TABLE.iter().find(|(c, _)| *c == code).map(|(_, k)| *k)
}

/// One raw key event (value 0 up, 1 down, 2 autorepeat) as core's event.
/// Any other key going down is `Other`: it cancels a half-done double-tap.
fn translate(code: KeyCode, value: i32) -> Option<KeyEvent> {
    match (key_of(code), value) {
        (_, 2) => None,
        (Some(Key::F(n)), 1) => Some(KeyEvent::FKey(n)),
        (Some(Key::F(_)), _) => None,
        (Some(key), v) => Some(KeyEvent::Modifier { key, down: v == 1 }),
        (None, 1) => Some(KeyEvent::Other),
        (None, _) => None,
    }
}

/// The modifiers held right now, per keyboard, from the listener's stream.
#[derive(Clone, Default)]
pub struct Held(Arc<Mutex<HashMap<PathBuf, HashSet<Key>>>>);

impl Held {
    fn set(&self, dev: &Path, key: Key, down: bool) {
        let mut all = lock(&self.0);
        let keys = all.entry(dev.to_path_buf()).or_default();
        if down {
            keys.insert(key);
        } else {
            keys.remove(&key);
        }
    }

    fn forget(&self, dev: &Path) {
        lock(&self.0).remove(dev);
    }

    /// Shift, Ctrl, Alt or Meta is down on some keyboard (Fn changes no text).
    pub fn any(&self) -> bool {
        lock(&self.0).values().flatten().any(|k| *k != Key::Fn)
    }
}

/// /dev/input/event* nodes that exist but do not open for reading.
fn denied_nodes() -> usize {
    let Ok(dir) = std::fs::read_dir("/dev/input") else {
        return 0;
    };
    dir.filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("event"))
        .filter(|e| std::fs::File::open(e.path()).is_err())
        .count()
}

/// Keyboards we can read, except our own. evdev::enumerate silently skips
/// the nodes it cannot open.
fn keyboards() -> Vec<(PathBuf, Device)> {
    evdev::enumerate()
        .filter(|(_, d)| {
            d.supported_keys()
                .is_some_and(|k| k.contains(KeyCode::KEY_A))
        })
        .filter(|(_, d)| d.name() != Some(OWN_DEVICE))
        .collect()
}

/// No keyboard reads, and some node refused us: the user is not in `input`.
pub fn input_denied() -> bool {
    denied_nodes() > 0 && keyboards().is_empty()
}

/// Start the scanner thread; it starts a reader per keyboard it finds.
pub fn spawn(tx: Sender<KeyEvent>, held: Held) -> std::io::Result<()> {
    let open: Arc<Mutex<HashSet<PathBuf>>> = Arc::default();
    thread::Builder::new()
        .name("keyboards".into())
        .spawn(move || {
            loop {
                for (path, dev) in keyboards() {
                    if !lock(&open).insert(path.clone()) {
                        continue;
                    }
                    log::info!(
                        "hotkeys: reading {} ({})",
                        path.display(),
                        dev.name().unwrap_or("?")
                    );
                    let (tx, held, open) = (tx.clone(), held.clone(), open.clone());
                    let spawned = thread::Builder::new()
                        .name("keyboard".into())
                        .spawn(move || read(&path, dev, &tx, &held, &open));
                    if let Err(e) = spawned {
                        log::error!("hotkeys: no reader thread: {e}");
                    }
                }
                thread::sleep(RESCAN);
            }
        })?;
    Ok(())
}

/// Blocks on the device until it goes away (unplugged, or a read error).
fn read(
    path: &Path,
    mut dev: Device,
    tx: &Sender<KeyEvent>,
    held: &Held,
    open: &Mutex<HashSet<PathBuf>>,
) {
    while let Ok(events) = dev.fetch_events() {
        for e in events {
            let EventSummary::Key(_, code, value) = e.destructure() else {
                continue;
            };
            let Some(ev) = translate(code, value) else {
                continue;
            };
            if let KeyEvent::Modifier { key, down } = ev {
                held.set(path, key, down);
            }
            if tx.send(ev).is_err() {
                return;
            }
        }
    }
    log::info!("hotkeys: {} went away", path.display());
    held.forget(path);
    lock(open).remove(path);
    // its releases are lost with it
    let _ = tx.send(KeyEvent::Reset);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code_of(key: Key) -> KeyCode {
        TABLE.iter().find(|(_, k)| *k == key).expect("in TABLE").0
    }

    #[test]
    fn the_table_has_the_kernel_codes() {
        for (code, key) in TABLE {
            assert_eq!(key_of(code), Some(key));
            assert_eq!(code_of(key), code);
        }
        // input-event-codes.h: F1..F10 = 59..68, F11 = 87, F12 = 88, F13..F24 = 183..194
        let f: Vec<u16> = (1..=24).map(|n| code_of(Key::F(n)).code()).collect();
        let want: Vec<u16> = (59..=68).chain([87, 88]).chain(183..=194).collect();
        assert_eq!(f, want);
        assert_eq!(code_of(Key::RightCtrl).code(), 97);
        assert_eq!(code_of(Key::RightAlt).code(), 100);
        assert_eq!(code_of(Key::RightMeta).code(), 126);
        assert_eq!(key_of(KeyCode::KEY_A), None);
    }

    #[test]
    fn translate_drops_repeats_and_f_key_releases() {
        assert_eq!(translate(KeyCode::KEY_RIGHTCTRL, 2), None);
        assert_eq!(translate(KeyCode::KEY_F13, 0), None);
        assert_eq!(translate(KeyCode::KEY_F13, 1), Some(KeyEvent::FKey(13)));
        assert_eq!(translate(KeyCode::KEY_A, 1), Some(KeyEvent::Other));
        assert_eq!(translate(KeyCode::KEY_A, 0), None);
        let up = KeyEvent::Modifier {
            key: Key::RightAlt,
            down: false,
        };
        assert_eq!(translate(KeyCode::KEY_RIGHTALT, 0), Some(up));
    }

    #[test]
    fn held_modifiers_are_per_keyboard_and_ignore_fn() {
        let (h, a, b) = (Held::default(), Path::new("/a"), Path::new("/b"));
        h.set(a, Key::Fn, true);
        assert!(!h.any());
        h.set(a, Key::LeftShift, true);
        h.set(b, Key::LeftShift, true);
        h.set(a, Key::LeftShift, false);
        assert!(h.any(), "still down on b");
        h.forget(b);
        assert!(!h.any());
    }

    #[test]
    fn the_container_has_no_input_devices() {
        if Path::new("/.dockerenv").exists() {
            assert_eq!((denied_nodes(), keyboards().len()), (0, 0));
            assert!(!input_denied(), "nothing refused, so no permission problem");
        }
    }
}
