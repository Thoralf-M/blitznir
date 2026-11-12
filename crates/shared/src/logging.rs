//! log:: lines as `[time LEVEL target] message` on stderr and, for the
//! daemon, in `blitznir.log` too. RUST_LOG=debug (a bare level) shows more,
//! and other crates' info lines too.

use std::{
    fs::{File, OpenOptions, Permissions},
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
    sync::Mutex,
};

/// The log file is emptied on start once it is bigger than this.
const MAX_LOG_BYTES: u64 = 5 << 20;

struct Logger {
    file: Option<Mutex<File>>,
    /// RUST_LOG is set: every crate's lines at its level, not only ours.
    all: bool,
}

/// Our crates' targets: `blitznir::…` (this crate), `blitznir_core::…`, `blitznir_app::…`.
fn ours(target: &str) -> bool {
    target.starts_with("blitznir")
}

impl log::Log for Logger {
    /// Other crates only warn and worse by default: on Linux the tree has
    /// `tracing`'s log bridge on, and Freya's renderer alone then writes
    /// ~80 info lines a second (seen on the VM), zbus and ort more.
    fn enabled(&self, m: &log::Metadata<'_>) -> bool {
        m.level() <= log::max_level()
            && (self.all || m.level() <= log::Level::Warn || ours(m.target()))
    }

    fn log(&self, r: &log::Record<'_>) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let line = format!(
            "[{} {:5} {}] {}\n",
            blitznir_core::now_stamp(),
            r.level(),
            r.target(),
            r.args()
        );
        let _ = std::io::stderr().write_all(line.as_bytes());
        if let Some(f) = &self.file {
            let _ = blitznir_core::lock(f).write_all(line.as_bytes());
        }
    }

    fn flush(&self) {}
}

/// Private (0600) like every file in the data dir, an existing one too: a
/// line can quote what the user dictated or the settings they saved.
fn open(path: &Path) -> std::io::Result<File> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let f = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)?;
    f.set_permissions(Permissions::from_mode(0o600))?;
    if f.metadata()?.len() > MAX_LOG_BYTES {
        f.set_len(0)?;
    }
    Ok(f)
}

/// Install the logger; `file` is where the daemon keeps its log.
pub fn init(file: Option<&Path>) {
    let (file, failed) = match file.map(open) {
        Some(Ok(f)) => (Some(Mutex::new(f)), None),
        Some(Err(e)) => (None, Some(e)),
        None => (None, None),
    };
    let set = std::env::var("RUST_LOG").ok().and_then(|l| l.parse().ok());
    let level = set.unwrap_or(log::LevelFilter::Info);
    let all = set.is_some();
    // one logger for the life of the process
    if log::set_logger(Box::leak(Box::new(Logger { file, all }))).is_ok() {
        log::set_max_level(level);
    }
    if let Some(e) = failed {
        log::warn!("no log file: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::Log;

    #[test]
    fn other_crates_only_warn_unless_rust_log_asks_for_more() {
        log::set_max_level(log::LevelFilter::Info);
        let meta = |level, target| {
            log::MetadataBuilder::new()
                .level(level)
                .target(target)
                .build()
        };
        let quiet = Logger {
            file: None,
            all: false,
        };
        assert!(quiet.enabled(&meta(log::Level::Info, "blitznir_freya::zap")));
        assert!(quiet.enabled(&meta(log::Level::Info, "blitznir::stt")));
        assert!(!quiet.enabled(&meta(log::Level::Info, "freya_winit::renderer")));
        assert!(quiet.enabled(&meta(log::Level::Warn, "freya_winit::drivers")));
        let all = Logger {
            file: None,
            all: true,
        };
        assert!(all.enabled(&meta(log::Level::Info, "zbus::connection")));
        assert!(
            !all.enabled(&meta(log::Level::Debug, "blitznir::stt")),
            "still the level"
        );
    }

    #[test]
    fn the_log_is_private_and_emptied_once_too_big() {
        let dir = std::env::temp_dir().join(format!("blitznir-log-{}", std::process::id()));
        let path = dir.join("blitznir.log");
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        drop(open(&path).unwrap());
        assert_eq!(mode(&path), 0o600);
        std::fs::set_permissions(&path, Permissions::from_mode(0o644)).unwrap();
        std::fs::write(&path, vec![b'x'; MAX_LOG_BYTES as usize + 1]).unwrap();
        let mut f = open(&path).unwrap();
        assert_eq!(mode(&path), 0o600, "a file from before is made private");
        f.write_all(b"line\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "line\n");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
