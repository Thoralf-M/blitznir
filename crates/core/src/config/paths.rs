//! Where everything personal lives.

use crate::{Error, Result};
use std::path::PathBuf;

/// The data directory's name under `dirs::data_dir()`.
pub const DATA_DIR_NAME: &str = "Blitznir";

/// `dirs::data_dir()/DATA_DIR_NAME` and the files in it.
#[derive(Debug, Clone)]
pub struct Paths {
    pub dir: PathBuf,
}

impl Paths {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// `$BLITZNIR_DATA_DIR`, else `~/Library/Application Support/Blitznir`
    /// or `~/.local/share/Blitznir`.
    pub fn user() -> Result<Paths> {
        Paths::from_env(std::env::var_os("BLITZNIR_DATA_DIR"))
    }

    /// [`Paths::user`] with the override passed in; empty counts as unset.
    pub fn from_env(dir: Option<std::ffi::OsString>) -> Result<Paths> {
        if let Some(dir) = dir.filter(|d| !d.is_empty()) {
            return Ok(Paths::new(dir));
        }
        let base = dirs::data_dir()
            .ok_or_else(|| Error::Config("no data directory for this user".into()))?;
        Ok(Paths::new(base.join(DATA_DIR_NAME)))
    }

    pub fn config(&self) -> PathBuf {
        self.dir.join("config.toml")
    }
    pub fn state(&self) -> PathBuf {
        self.dir.join("state.json")
    }
    pub fn history(&self) -> PathBuf {
        self.dir.join("history.json")
    }
    pub fn recording(&self) -> PathBuf {
        self.dir.join("recording.txt")
    }
    pub fn chats(&self) -> PathBuf {
        self.dir.join("chats.json")
    }
    pub fn images(&self) -> PathBuf {
        self.dir.join("images")
    }
    pub fn models(&self) -> PathBuf {
        self.dir.join("models")
    }
    pub fn log(&self) -> PathBuf {
        self.dir.join("blitznir.log")
    }
}
