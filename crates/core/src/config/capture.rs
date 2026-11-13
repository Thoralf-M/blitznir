//! `[capture]`: the chat's screenshot command.

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Capture {
    pub cmd: String,
}

impl Default for Capture {
    fn default() -> Self {
        let cmd = if cfg!(target_os = "macos") {
            "screencapture -i -o -x {path}"
        } else {
            "" // the desktop's screenshot portal: GNOME 50 refuses gnome-screenshot
        };
        Self { cmd: cmd.into() }
    }
}
