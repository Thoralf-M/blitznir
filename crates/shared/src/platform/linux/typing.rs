//! The uinput keyboard `blitznir-kbd`: types through the xkb reverse map,
//! below the compositor, so it works on X11 and every Wayland desktop.

use super::{
    keymap::{Stroke, layout_map},
    keys::OWN_DEVICE,
};
use blitznir_core::config::Linux as LinuxConfig;
use evdev::{AttributeSet, InputEvent, KeyCode, KeyEvent, uinput::VirtualDevice};
use std::{collections::HashMap, time::Duration};

/// Gap after every key frame; some apps drop keys sent faster.
const GAP: Duration = Duration::from_millis(2);

/// Modifiers + one key, e.g. `[linux] paste_keys = "ctrl+shift+v"`, `undo_keys`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chord {
    mods: Vec<KeyCode>,
    key: KeyCode,
}

/// Parse `ctrl+v`, `ctrl+shift+v`, `shift+insert`: modifiers ctrl, shift,
/// alt, super; the key is `insert` or one character, looked up on the layout
/// (apps match shortcuts by keysym, so "v" is wherever the layout puts it).
pub fn parse_chord(spec: &str, map: &HashMap<char, Stroke>) -> Result<Chord, String> {
    let bad = || format!("{spec:?}: want e.g. \"ctrl+v\" or \"shift+insert\"");
    let lower = spec.to_lowercase();
    let mut parts: Vec<&str> = lower.split('+').map(str::trim).collect();
    let last = parts.pop().ok_or_else(bad)?;
    let mods = parts
        .iter()
        .map(|m| match *m {
            "ctrl" | "control" => Ok(KeyCode::KEY_LEFTCTRL),
            "shift" => Ok(KeyCode::KEY_LEFTSHIFT),
            "alt" => Ok(KeyCode::KEY_LEFTALT),
            "super" | "meta" => Ok(KeyCode::KEY_LEFTMETA),
            _ => Err(bad()),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut chars = last.chars();
    let key = match (chars.next(), chars.next()) {
        _ if last == "insert" => KeyCode::KEY_INSERT,
        (Some(c), None) => KeyCode::new(map.get(&c).ok_or_else(bad)?.code),
        _ => return Err(bad()),
    };
    Ok(Chord { mods, key })
}

/// A piece of text to type: on the layout, or only through the clipboard.
#[derive(Debug, PartialEq, Eq)]
pub enum Run<'a> {
    Keys(&'a str),
    Paste(&'a str),
}

/// `s` cut into runs of on-layout and off-layout characters, in order.
pub fn runs<'a>(map: &HashMap<char, Stroke>, s: &'a str) -> Vec<Run<'a>> {
    let mut out = vec![];
    let mut start = 0;
    let mut on: Option<bool> = None;
    for (i, c) in s.char_indices() {
        let here = map.contains_key(&c);
        if on.is_some_and(|o| o != here) {
            out.push(run(on, &s[start..i]));
            start = i;
        }
        on = Some(here);
    }
    if start < s.len() {
        out.push(run(on, &s[start..]));
    }
    out
}

fn run(on: Option<bool>, s: &str) -> Run<'_> {
    if on == Some(true) {
        Run::Keys(s)
    } else {
        Run::Paste(s)
    }
}

pub struct Keyboard {
    dev: VirtualDevice,
    /// Replaced whole when `[linux]` changes, on this same device.
    pub keys: Keys,
}

/// `[linux] name = spec`, or ctrl+`fallback` when it does not parse.
fn chord_or(name: &str, spec: &str, map: &HashMap<char, Stroke>, fallback: KeyCode) -> Chord {
    parse_chord(spec, map).unwrap_or_else(|e| {
        log::error!("[linux] {name} = {e}; using ctrl+{fallback:?}");
        Chord {
            mods: vec![KeyCode::KEY_LEFTCTRL],
            key: fallback,
        }
    })
}

/// What `[linux] xkb_layout` ("" = the desktop's keymap) types each
/// character with, and the shortcuts on it.
pub struct Keys {
    map: HashMap<char, Stroke>,
    /// What the map comes from, for the log.
    pub layout: String,
    paste: Chord,
    undo: Chord,
    copy: Option<Chord>,
}

/// Can take a roundtrip to the compositor (its keymap): build it before
/// locking a keyboard that typing holds.
pub fn keys_of(cfg: &LinuxConfig) -> Result<Keys, String> {
    let (map, layout) = layout_map(&cfg.xkb_layout)?;
    let paste = chord_or("paste_keys", &cfg.paste_keys, &map, KeyCode::KEY_V);
    let undo = chord_or("undo_keys", &cfg.undo_keys, &map, KeyCode::KEY_Z);
    let copy = (!cfg.copy_keys.trim().is_empty())
        .then(|| chord_or("copy_keys", &cfg.copy_keys, &map, KeyCode::KEY_INSERT));
    Ok(Keys {
        map,
        layout,
        paste,
        undo,
        copy,
    })
}

impl Keyboard {
    /// Needs rw on /dev/uinput. The compositor takes ~200 ms to adopt a new
    /// device, so this runs once at startup, not per text.
    pub fn new(keys: Keys) -> Result<Keyboard, String> {
        let mut codes = AttributeSet::<KeyCode>::new();
        for code in 1..=248 {
            codes.insert(KeyCode::new(code));
        }
        let dev = VirtualDevice::builder()
            .and_then(|b| b.name(OWN_DEVICE).with_keys(&codes))
            .and_then(|b| b.build())
            .map_err(|e| {
                format!(
                    "cannot create a keyboard on /dev/uinput ({e}): run scripts/install-linux.sh, \
                     then log out and back in"
                )
            })?;
        Ok(Keyboard { dev, keys })
    }

    pub fn map(&self) -> &HashMap<char, Stroke> {
        &self.keys.map
    }

    fn key(&mut self, code: KeyCode, down: bool) -> std::io::Result<()> {
        let ev: InputEvent = *KeyEvent::new(code, i32::from(down));
        self.dev.emit(&[ev])?;
        std::thread::sleep(GAP);
        Ok(())
    }

    /// Press `mods`, tap `key`, release `mods` in reverse.
    fn chord(&mut self, mods: &[KeyCode], key: KeyCode) -> std::io::Result<()> {
        for &m in mods {
            self.key(m, true)?;
        }
        self.key(key, true)?;
        self.key(key, false)?;
        for &m in mods.iter().rev() {
            self.key(m, false)?;
        }
        Ok(())
    }

    /// Types the chars the layout has and skips the others (see [`runs`]).
    pub fn type_keys(&mut self, s: &str) -> std::io::Result<()> {
        for ch in s.chars() {
            let Some(&st) = self.keys.map.get(&ch) else {
                continue;
            };
            let mods: Vec<KeyCode> = [
                (st.shift, KeyCode::KEY_LEFTSHIFT),
                (st.altgr, KeyCode::KEY_RIGHTALT),
            ]
            .into_iter()
            .filter_map(|(on, k)| on.then_some(k))
            .collect();
            self.chord(&mods, KeyCode::new(st.code))?;
        }
        Ok(())
    }

    pub fn backspace(&mut self, n: usize) -> std::io::Result<()> {
        (0..n).try_for_each(|_| self.chord(&[], KeyCode::KEY_BACKSPACE))
    }

    /// `[linux] paste_keys`.
    pub fn paste(&mut self) -> std::io::Result<()> {
        let Chord { mods, key } = self.keys.paste.clone();
        self.chord(&mods, key)
    }

    /// `[linux] copy_keys`; false: "" (never).
    pub fn copy(&mut self) -> std::io::Result<bool> {
        let Some(Chord { mods, key }) = self.keys.copy.clone() else {
            return Ok(false);
        };
        self.chord(&mods, key).map(|()| true)
    }

    /// `[linux] undo_keys`.
    pub fn undo(&mut self) -> std::io::Result<()> {
        let Chord { mods, key } = self.keys.undo.clone();
        self.chord(&mods, key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::linux::keymap::reverse_map;

    fn us() -> HashMap<char, Stroke> {
        reverse_map("us").expect("us keymap")
    }

    #[test]
    fn paste_chords_parse() {
        let (us, ctrl, shift) = (us(), KeyCode::KEY_LEFTCTRL, KeyCode::KEY_LEFTSHIFT);
        let chord = |mods: &[KeyCode], key| Chord {
            mods: mods.to_vec(),
            key,
        };
        assert_eq!(
            parse_chord("ctrl+v", &us),
            Ok(chord(&[ctrl], KeyCode::KEY_V))
        );
        assert_eq!(
            parse_chord("Ctrl + Shift + V", &us),
            Ok(chord(&[ctrl, shift], KeyCode::KEY_V))
        );
        assert_eq!(
            parse_chord("shift+insert", &us),
            Ok(chord(&[shift], KeyCode::KEY_INSERT))
        );
        for bad in ["", "ctrl+", "hyper+v", "ctrl+vv", "ctrl+ü"] {
            assert!(parse_chord(bad, &us).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn the_v_of_ctrl_v_is_where_the_layout_has_it() {
        let de = reverse_map("de").expect("de keymap");
        let z = parse_chord("ctrl+z", &de).expect("z on de");
        assert_eq!(z.key, KeyCode::KEY_Y, "de swaps y and z");
        let bad = chord_or("undo_keys", "hyper+z", &de, KeyCode::KEY_Z);
        assert_eq!(
            (bad.mods, bad.key),
            (vec![KeyCode::KEY_LEFTCTRL], KeyCode::KEY_Z)
        );
    }

    #[test]
    fn text_is_cut_into_on_and_off_layout_runs() {
        let us = us();
        assert_eq!(
            runs(&us, "Grüße, Straße!"),
            [
                Run::Keys("Gr"),
                Run::Paste("üß"),
                Run::Keys("e, Stra"),
                Run::Paste("ß"),
                Run::Keys("e!"),
            ]
        );
        assert_eq!(runs(&us, "€"), [Run::Paste("€")]);
        assert_eq!(runs(&us, "ok"), [Run::Keys("ok")]);
        assert!(runs(&us, "").is_empty());
    }

    #[test]
    fn uinput_is_absent_in_the_container() {
        if std::path::Path::new("/.dockerenv").exists() {
            let us = LinuxConfig {
                xkb_layout: "us".into(),
                ..LinuxConfig::default()
            };
            let e = Keyboard::new(keys_of(&us).expect("us"))
                .err()
                .expect("no /dev/uinput");
            assert!(e.contains("install-linux.sh"), "{e}");
        }
    }
}
