//! Blitznir's logic: everything that can be tested without a screen, a
//! microphone, a model or an OS. The binary (`crates/app`) only glues this to
//! the platform through the seams in [`platform`].
#![forbid(unsafe_code)]

pub mod beam;
pub mod beam_sound;
pub mod config;
pub mod dictation;
pub mod jobs;
pub mod keys;
pub mod llm;
pub mod markdown;
pub mod picture;
pub mod platform;
pub mod segmenter;
pub mod sky;
pub mod store;
pub mod tasks;
pub mod textproc;
pub mod vocab;
pub mod wake;
pub mod wave;
pub mod zoom;

/// Every error core hands out. The message is what the user sees.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Config(String),
    /// config.toml does not parse. The message is what the log gets: toml's
    /// own words (`detail`) can quote a value from the file, an endpoint's
    /// key; [`Error::detailed`] adds them for the user's own screen.
    #[error("config.toml does not parse at line {line}, column {column}")]
    ConfigSyntax {
        line: usize,
        column: usize,
        detail: String,
    },
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Http(#[from] reqwest::Error),
    #[error("{0}")]
    Llm(String),
    #[error("cancelled")]
    Cancelled,
    #[error("{0}")]
    Platform(String),
    #[error("{0}")]
    Stt(String),
}

impl Error {
    /// The message with what only the user's own screen may show.
    pub fn detailed(&self) -> String {
        match self {
            Error::ConfigSyntax { detail, .. } => format!("{self}: {detail}"),
            e => e.to_string(),
        }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Local time as `2026-09-25T12:30:00`, the timestamp format of every file.
pub fn now_stamp() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

/// Every mutex in the workspace is locked through this: a thread that
/// panicked while holding one leaves data that is still the best we have,
/// and one panic must not take every later lock down with it.
pub fn lock<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
