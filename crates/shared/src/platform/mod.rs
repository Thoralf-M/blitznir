//! The OS behind core's `Platform` seam. The only place for OS code and unsafe.

use blitznir_core::{
    Result,
    platform::{Platform, shell_quote},
};
use std::{path::Path, process::Command, sync::Arc, thread};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "linux")]
pub use linux::{PERMISSIONS, permission_help};
#[cfg(target_os = "macos")]
pub use macos::{PERMISSIONS, no_title_bar_zoom, permission_help, set_frame, visible_frame};

/// First thing in `main`, before anything starts GTK (Freya's tray does):
/// GTK's init sets the process locale from the environment, and with it
/// LC_NUMERIC, so Skia's SVG parser, through strtod, read "9.78" in an icon's
/// path as 9 in a German one (a decimal comma): every icon drawn with paths
/// lost them. The text is ours, in English, so no locale is needed.
pub fn before_gtk() {
    #[cfg(target_os = "linux")]
    gtk::disable_setlocale();
}

pub fn current() -> Arc<dyn Platform> {
    #[cfg(target_os = "macos")]
    return Arc::new(macos::Mac);
    #[cfg(target_os = "linux")]
    return Arc::new(linux::Linux::new());
}

/// `Platform::capture` for a command: `cmd` through `sh -c` with `{path}`
/// replaced by `out`, shell-quoted. The file is the answer, not the exit
/// code: `screencapture` exits 0 when the user cancels.
fn run_capture(cmd: &str, out: &Path) -> Result<bool> {
    let _ = std::fs::remove_file(out);
    let line = cmd.replace("{path}", &shell_quote(&out.to_string_lossy()));
    let status = Command::new("/bin/sh").arg("-c").arg(&line).status()?;
    let shot = std::fs::metadata(out).is_ok_and(|m| m.len() > 0);
    if !shot {
        log::info!("capture: no picture ({line} exited with {status})");
    }
    Ok(shot)
}

/// `program arg` in the background, waited for on a thread of its own so it
/// leaves no zombie behind in a daemon that runs for weeks.
fn spawn_reaped(program: &str, arg: &str) -> Result<()> {
    let mut child = Command::new(program).arg(arg).spawn()?;
    thread::spawn(move || child.wait());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_quotes_the_path_and_trusts_only_the_file() {
        let dir = std::env::temp_dir().join(format!("blitz nir's {}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("shot.png");
        assert!(run_capture("printf png > {path}", &out).unwrap());
        assert_eq!(std::fs::read_to_string(&out).unwrap(), "png");
        // a cancel: exit 0, no file (and the old file is gone)
        assert!(!run_capture("true {path}", &out).unwrap());
        assert!(!out.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
