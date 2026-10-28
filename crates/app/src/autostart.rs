//! Start at login: a LaunchAgent on macOS, an XDG autostart entry on Linux.
//! One login item, named Blitznir, that starts this binary.

use blitznir_core::{Error, Result};
use std::path::PathBuf;

#[cfg(target_os = "macos")]
fn file() -> Option<PathBuf> {
    Some(dirs::home_dir()?.join("Library/LaunchAgents/Blitznir.plist"))
}

#[cfg(target_os = "linux")]
fn file() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("autostart/Blitznir.desktop"))
}

#[cfg(target_os = "macos")]
fn entry(exe: &str) -> String {
    let exe = exe.replace('&', "&amp;").replace('<', "&lt;");
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>Blitznir</string>
  <key>ProgramArguments</key><array><string>{exe}</string></array>
  <key>RunAtLoad</key><true/>
</dict>
</plist>
"#
    )
}

#[cfg(target_os = "linux")]
fn entry(exe: &str) -> String {
    let exe = exe.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        "[Desktop Entry]\nType=Application\nName=Blitznir\nExec=\"{exe}\"\n\
         StartupNotify=false\nTerminal=false\n"
    )
}

/// The entry for this very binary, and where it goes.
fn ours() -> Result<(PathBuf, String)> {
    let exe = std::env::current_exe()?.canonicalize()?;
    let file = file().ok_or_else(|| Error::Platform("no home directory".into()))?;
    Ok((file, entry(&exe.to_string_lossy())))
}

/// On only when the entry starts this binary: one for another build shows
/// unticked here, and ticking replaces it.
pub fn enabled() -> bool {
    ours().is_ok_and(|(file, entry)| std::fs::read_to_string(file).is_ok_and(|s| s == entry))
}

pub fn set(on: bool) -> Result<()> {
    let (file, entry) = ours()?;
    if !on {
        return match std::fs::remove_file(&file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        };
    }
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    Ok(std::fs::write(file, entry)?)
}
