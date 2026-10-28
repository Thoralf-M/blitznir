//! `[linux]`: how typing and the clipboard reach other apps there.

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Linux {
    pub xkb_layout: String,
    pub paste_keys: String,
    pub undo_keys: String,
    pub copy_keys: String,
}

impl Default for Linux {
    fn default() -> Self {
        Self {
            xkb_layout: String::new(),
            paste_keys: "ctrl+v".into(),
            undo_keys: "ctrl+z".into(),
            copy_keys: "ctrl+insert".into(),
        }
    }
}
