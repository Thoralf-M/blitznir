//! xkbcommon reverse map: which evdev key + modifiers type a char on a layout,
//! named or the compositor's own keymap. Verified for the us and de maps.

use std::collections::HashMap;
use xkbcommon::xkb;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stroke {
    /// evdev keycode (xkb keycode - 8)
    pub code: u16,
    pub shift: bool,
    /// ISO_Level3_Shift; sent as KEY_RIGHTALT, which is AltGr on de/fr/… layouts
    pub altgr: bool,
}

/// The main block of a pc105 keyboard. Without this filter xkb offers
/// KEY_EURO (435) for € and KEY_KPLEFTPAREN (179) for '(' via inet(evdev):
/// keys no keyboard has, which the compositor's keymap may not map. The keypad
/// (55, 71..=83) is out too: terminals in keypad-application mode turn it into
/// escape sequences.
fn main_block(code: u16) -> bool {
    code <= 86 && code != 55 && !(71..=83).contains(&code)
}

fn bit(km: &xkb::Keymap, name: &str) -> u32 {
    1u32.checked_shl(km.mod_get_index(name)).unwrap_or(0)
}

/// Without the desktop's keymap (an X11 session): the layout a pc105 keyboard
/// most often has; `[linux] xkb_layout` names another.
const FALLBACK: &str = "us";

/// `layout`'s map and what it is, for the log; "" = the desktop's keymap
/// (Wayland), else [`FALLBACK`]. Err: a named layout that does not compile.
pub fn layout_map(layout: &str) -> Result<(HashMap<char, Stroke>, String), String> {
    let named = |layout: &str| {
        reverse_map(layout)
            .map(|m| (m, layout.to_string()))
            .ok_or_else(|| format!("[linux] xkb_layout = {layout:?} is not an xkb layout"))
    };
    if !layout.trim().is_empty() {
        return named(layout.trim());
    }
    match super::seat::keymap().and_then(|text| {
        desktop_map(&text).ok_or_else(|| "the desktop's keymap does not compile".into())
    }) {
        Ok(found) => Ok(found),
        Err(e) => {
            log::warn!("{e}: typing with {FALLBACK:?}; [linux] xkb_layout names another");
            named(FALLBACK)
        }
    }
}

/// The compositor's keymap (XKB text): its map, and its first layout's name.
pub fn desktop_map(text: &str) -> Option<(HashMap<char, Stroke>, String)> {
    let ctx = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
    let km = xkb::Keymap::new_from_string(
        &ctx,
        text.to_string(),
        xkb::KEYMAP_FORMAT_TEXT_V1,
        xkb::KEYMAP_COMPILE_NO_FLAGS,
    )?;
    let name = km.layout_get_name(0).to_string();
    Some((map_of(&km), format!("the desktop's keymap: {name}")))
}

/// A named layout's map. None: the layout does not compile.
pub fn reverse_map(layout: &str) -> Option<HashMap<char, Stroke>> {
    let ctx = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
    let km = xkb::Keymap::new_from_names(
        &ctx,
        "evdev",
        "pc105",
        layout,
        "",
        None,
        xkb::KEYMAP_COMPILE_NO_FLAGS,
    )?;
    Some(map_of(&km))
}

/// char → cheapest Stroke (fewest modifiers, then lowest keycode) on the main
/// block, of the keymap's first layout. Levels that need any modifier but
/// Shift or AltGr (NumLock, Ctrl, …) are ignored; dead keys produce no char.
fn map_of(km: &xkb::Keymap) -> HashMap<char, Stroke> {
    let shift = bit(km, xkb::MOD_NAME_SHIFT);
    let altgr = bit(km, xkb::MOD_NAME_ISO_LEVEL3_SHIFT) | bit(km, "LevelThree");
    let mut best: HashMap<char, (u32, Stroke)> = HashMap::new();
    for raw in km.min_keycode().raw()..=km.max_keycode().raw() {
        let kc = xkb::Keycode::new(raw);
        let Some(code) = u16::try_from(raw.saturating_sub(8))
            .ok()
            .filter(|&c| main_block(c))
        else {
            continue;
        };
        for level in 0..km.num_levels_for_key(kc, 0) {
            let mut masks = [0u32; 16];
            let n = km.key_get_mods_for_level(kc, 0, level, &mut masks);
            let Some(mask) = masks[..n]
                .iter()
                .copied()
                .filter(|m| m & !(shift | altgr) == 0)
                .min_by_key(|m| m.count_ones())
            else {
                continue;
            };
            let stroke = Stroke {
                code,
                shift: mask & shift != 0,
                altgr: mask & altgr != 0,
            };
            let cost = u32::from(stroke.shift) + u32::from(stroke.altgr);
            for &sym in km.key_get_syms_by_level(kc, 0, level) {
                let Some(ch) = char::from_u32(xkb::keysym_to_utf32(sym)).filter(|c| *c != '\0')
                else {
                    continue;
                };
                let slot = best.entry(ch).or_insert((cost, stroke));
                if cost < slot.0 {
                    *slot = (cost, stroke);
                }
            }
        }
    }
    let mut map: HashMap<char, Stroke> = best.into_iter().map(|(c, (_, s))| (c, s)).collect();
    if let Some(&enter) = map.get(&'\r') {
        map.insert('\n', enter);
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use evdev::KeyCode;

    const fn s(code: KeyCode, shift: bool, altgr: bool) -> Stroke {
        Stroke {
            code: code.code(),
            shift,
            altgr,
        }
    }

    const PHRASE: &str = "Grüße, Straße! @€";

    #[test]
    fn us_layout() {
        let us = reverse_map("us").expect("us keymap (xkb-data installed?)");
        assert_eq!(us[&'a'], s(KeyCode::KEY_A, false, false));
        assert_eq!(us[&'A'], s(KeyCode::KEY_A, true, false));
        assert_eq!(us[&'@'], s(KeyCode::KEY_2, true, false));
        assert_eq!(us[&'1'], s(KeyCode::KEY_1, false, false), "not keypad");
        assert_eq!(us[&'*'], s(KeyCode::KEY_8, true, false), "not KP_Multiply");
        assert_eq!(us[&'('], s(KeyCode::KEY_9, true, false), "not KPLEFTPAREN");
        assert_eq!(us[&'\n'], s(KeyCode::KEY_ENTER, false, false));
        let off: String = PHRASE.chars().filter(|c| !us.contains_key(c)).collect();
        assert_eq!(off, "üßß€", "these go through the clipboard on us");
    }

    #[test]
    fn de_layout() {
        let de = reverse_map("de").expect("de keymap");
        assert_eq!(de[&'z'], s(KeyCode::KEY_Y, false, false));
        assert_eq!(de[&'ü'], s(KeyCode::KEY_LEFTBRACE, false, false));
        assert_eq!(de[&'ß'], s(KeyCode::KEY_MINUS, false, false));
        assert_eq!(de[&'@'], s(KeyCode::KEY_Q, false, true));
        assert_eq!(de[&'€'], s(KeyCode::KEY_E, false, true));
        assert!(PHRASE.chars().all(|c| de.contains_key(&c)));
        assert!(!de.contains_key(&'^'), "a dead key: clipboard, never a key");
    }

    #[test]
    fn an_unknown_layout_is_none() {
        assert!(reverse_map("no-such-layout").is_none());
        assert!(layout_map("no-such-layout").is_err());
    }

    #[test]
    fn a_compositors_keymap_text_types_as_its_layout() {
        // what a compositor sends: a whole keymap, here de's as xkb writes it
        let ctx = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let flags = xkb::KEYMAP_COMPILE_NO_FLAGS;
        let km = xkb::Keymap::new_from_names(&ctx, "evdev", "pc105", "de", "", None, flags)
            .expect("de keymap");
        let text = km.get_as_string(xkb::KEYMAP_FORMAT_TEXT_V1);
        let (map, what) = desktop_map(&text).expect("it compiles back");
        assert_eq!(map, reverse_map("de").expect("de"));
        assert!(what.contains("German"), "{what}");
        assert!(desktop_map("not a keymap").is_none());
    }
}
