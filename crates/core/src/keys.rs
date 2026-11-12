//! Hotkeys: a double-tap of a modifier, or a single F-key press. No chords:
//! neither needs the key swallowed, so the OS listener stays listen-only.

use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    fmt,
    time::{Duration, Instant},
};

/// A physical key a hotkey can name, as written in config.toml.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(try_from = "String")]
pub enum Key {
    LeftShift,
    RightShift,
    LeftCtrl,
    RightCtrl,
    LeftAlt,
    RightAlt,
    LeftMeta,
    RightMeta,
    Fn,
    /// F1–F24
    F(u8),
}

const MODIFIERS: [(Key, &str); 9] = [
    (Key::LeftShift, "LeftShift"),
    (Key::RightShift, "RightShift"),
    (Key::LeftCtrl, "LeftCtrl"),
    (Key::RightCtrl, "RightCtrl"),
    (Key::LeftAlt, "LeftAlt"),
    (Key::RightAlt, "RightAlt"),
    (Key::LeftMeta, "LeftMeta"),
    (Key::RightMeta, "RightMeta"),
    (Key::Fn, "Fn"),
];

impl TryFrom<String> for Key {
    type Error = String;

    fn try_from(name: String) -> Result<Self, String> {
        if let Some((key, _)) = MODIFIERS.iter().find(|(_, n)| *n == name) {
            return Ok(*key);
        }
        match name.strip_prefix('F').and_then(|n| n.parse::<u8>().ok()) {
            Some(n @ 1..=24) => Ok(Key::F(n)),
            _ => Err(format!(
                "unknown key {name:?}: use LeftShift, RightShift, LeftCtrl, RightCtrl, LeftAlt, \
                 RightAlt, LeftMeta, RightMeta, Fn or F1–F24"
            )),
        }
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Key::F(n) => write!(f, "F{n}"),
            key => {
                let name = MODIFIERS
                    .iter()
                    .find(|(k, _)| k == key)
                    .map_or("?", |(_, n)| n);
                f.write_str(name)
            }
        }
    }
}

/// What the OS listener reports. It never swallows, and skips our own events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEvent {
    Modifier {
        key: Key,
        down: bool,
    },
    /// An F-key went down (not repeated).
    FKey(u8),
    /// Any other key went down: the modifier was used in a chord.
    Other,
    /// A mouse button went down (macOS): only the revert offer cares.
    Click,
    /// The left button came up after a drag or a double or triple click
    /// (macOS): text may have just been selected (`[translate] on_select`).
    Selected,
    /// ⌘⇧3, ⌘⇧4 or ⌘⇧5 (macOS): the system's screenshot, not typing; the
    /// box by the text stays for it.
    Screenshot,
    /// The listener lost events (the OS disabled it for a while, a device
    /// went away): forget held keys and any half-done double-tap.
    Reset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    Live,
    Batch,
    Rewrite,
    Chat,
    /// The first [[actions]] button on the selection, without the panel.
    Quick,
    /// Translate the selection in place, without the panel.
    Translate,
    /// [[actions]] button `i` (its `hotkey`), as Quick runs the first.
    Button(usize),
    /// Take the last paste-back back.
    Undo,
}

/// Turns the raw key stream into hotkey actions.
pub struct Detector {
    bindings: HashMap<Key, Action>,
    window: Duration,
    down: HashSet<Key>,
    /// The first tap of a possible double-tap.
    armed: Option<(Action, Instant)>,
}

impl Detector {
    /// `bindings`: which key triggers which action (config.rs ensures a key
    /// has one action); `window`: max time between the two presses.
    pub fn new(bindings: HashMap<Key, Action>, window: Duration) -> Self {
        Self {
            bindings,
            window,
            down: HashSet::new(),
            armed: None,
        }
    }

    pub fn feed(&mut self, ev: KeyEvent, now: Instant) -> Option<Action> {
        match ev {
            KeyEvent::Other | KeyEvent::Screenshot => {
                self.armed = None; // the modifier was used for a chord, not tapped
                None
            }
            KeyEvent::Click | KeyEvent::Selected => None,
            KeyEvent::Reset => {
                // a release lost with the events would make the next press a "repeat"
                self.down.clear();
                self.armed = None;
                None
            }
            KeyEvent::FKey(n) => {
                self.armed = None;
                self.bindings.get(&Key::F(n)).copied()
            }
            KeyEvent::Modifier { key, down: false } => {
                self.down.remove(&key);
                None
            }
            KeyEvent::Modifier { key, down: true } => self.press(key, now),
        }
    }

    fn press(&mut self, key: Key, now: Instant) -> Option<Action> {
        if !self.down.insert(key) {
            return None; // already held: a repeat, not a new press
        }
        let Some(&action) = self.bindings.get(&key) else {
            self.armed = None; // another modifier: part of a chord
            return None;
        };
        match self.armed {
            Some((armed, at)) if armed == action && now.duration_since(at) < self.window => {
                self.armed = None;
                Some(action)
            }
            _ => {
                self.armed = Some((action, now));
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detector() -> Detector {
        let b = [
            (Key::RightAlt, Action::Live),
            (Key::RightCtrl, Action::Live),
            (Key::RightShift, Action::Rewrite),
        ];
        let mut map: HashMap<Key, Action> = b.into_iter().collect();
        map.insert(Key::F(13), Action::Chat);
        map.insert(Key::F(16), Action::Quick);
        map.insert(Key::LeftAlt, Action::Undo);
        Detector::new(map, Duration::from_millis(450))
    }

    fn tap(d: &mut Detector, key: Key, at: Instant) -> Option<Action> {
        let fired = d.feed(KeyEvent::Modifier { key, down: true }, at);
        assert_eq!(
            d.feed(KeyEvent::Modifier { key, down: false }, at + ms(30)),
            None
        );
        fired
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn double_tap_inside_the_window_fires() {
        let (mut d, t) = (detector(), Instant::now());
        assert_eq!(tap(&mut d, Key::RightAlt, t), None);
        assert_eq!(tap(&mut d, Key::RightAlt, t + ms(300)), Some(Action::Live));
        // the pair is consumed: a third tap starts over
        assert_eq!(tap(&mut d, Key::RightAlt, t + ms(500)), None);
    }

    #[test]
    fn double_tap_outside_the_window_does_not() {
        let (mut d, t) = (detector(), Instant::now());
        assert_eq!(tap(&mut d, Key::RightAlt, t), None);
        assert_eq!(tap(&mut d, Key::RightAlt, t + ms(450)), None);
        // …but the late tap arms a new pair
        assert_eq!(tap(&mut d, Key::RightAlt, t + ms(700)), Some(Action::Live));
    }

    #[test]
    fn two_keys_of_one_action_pair_up() {
        let (mut d, t) = (detector(), Instant::now());
        assert_eq!(tap(&mut d, Key::RightAlt, t), None);
        assert_eq!(tap(&mut d, Key::RightCtrl, t + ms(200)), Some(Action::Live));
    }

    #[test]
    fn a_key_down_between_taps_cancels() {
        let (mut d, t) = (detector(), Instant::now());
        tap(&mut d, Key::RightShift, t);
        assert_eq!(d.feed(KeyEvent::Other, t + ms(100)), None);
        assert_eq!(tap(&mut d, Key::RightShift, t + ms(200)), None);
    }

    #[test]
    fn typing_with_the_modifier_held_is_not_a_tap() {
        // Shift+h, then one quick Shift tap: the chord's press is not the first tap
        let (mut d, t) = (detector(), Instant::now());
        let (down, up) = (
            KeyEvent::Modifier {
                key: Key::RightShift,
                down: true,
            },
            KeyEvent::Modifier {
                key: Key::RightShift,
                down: false,
            },
        );
        assert_eq!(d.feed(down, t), None);
        assert_eq!(d.feed(KeyEvent::Other, t + ms(40)), None);
        assert_eq!(d.feed(up, t + ms(80)), None);
        assert_eq!(tap(&mut d, Key::RightShift, t + ms(200)), None);
    }

    #[test]
    fn a_chord_or_another_modifier_resets() {
        let (mut d, t) = (detector(), Instant::now());
        tap(&mut d, Key::RightShift, t);
        tap(&mut d, Key::LeftCtrl, t + ms(100));
        assert_eq!(tap(&mut d, Key::RightShift, t + ms(200)), None);
        // a tap of another action's key also replaces the armed one
        tap(&mut d, Key::RightShift, t + ms(1000));
        tap(&mut d, Key::RightAlt, t + ms(1100));
        assert_eq!(tap(&mut d, Key::RightShift, t + ms(1200)), None);
    }

    #[test]
    fn a_held_key_is_one_press() {
        let (mut d, t) = (detector(), Instant::now());
        let down = KeyEvent::Modifier {
            key: Key::RightAlt,
            down: true,
        };
        assert_eq!(d.feed(down, t), None);
        assert_eq!(d.feed(down, t + ms(100)), None);
        assert_eq!(d.feed(down, t + ms(200)), None);
    }

    #[test]
    fn reset_forgets_a_release_that_was_lost() {
        let (mut d, t) = (detector(), Instant::now());
        let down = KeyEvent::Modifier {
            key: Key::RightAlt,
            down: true,
        };
        assert_eq!(d.feed(down, t), None); // its release is never seen
        assert_eq!(d.feed(KeyEvent::Reset, t + ms(50)), None);
        assert_eq!(tap(&mut d, Key::RightAlt, t + ms(100)), None);
        assert_eq!(tap(&mut d, Key::RightAlt, t + ms(300)), Some(Action::Live));
    }

    #[test]
    fn f_key_fires_on_press() {
        let (mut d, t) = (detector(), Instant::now());
        assert_eq!(d.feed(KeyEvent::FKey(13), t), Some(Action::Chat));
        assert_eq!(d.feed(KeyEvent::FKey(14), t), None);
        assert_eq!(d.feed(KeyEvent::FKey(16), t), Some(Action::Quick));
    }

    #[test]
    fn quick_and_undo_fire_like_any_other_action() {
        let (mut d, t) = (detector(), Instant::now());
        assert_eq!(tap(&mut d, Key::LeftAlt, t), None);
        assert_eq!(tap(&mut d, Key::LeftAlt, t + ms(200)), Some(Action::Undo));
        assert_eq!(d.feed(KeyEvent::FKey(16), t + ms(300)), Some(Action::Quick));
    }

    #[test]
    fn key_names_round_trip() {
        for name in ["RightAlt", "LeftMeta", "Fn", "F1", "F24"] {
            let key = Key::try_from(name.to_string()).unwrap();
            assert_eq!(key.to_string(), name);
        }
        for bad in ["F0", "F25", "RightOption", "rightalt", ""] {
            assert!(Key::try_from(bad.to_string()).is_err(), "{bad}");
        }
    }
}
