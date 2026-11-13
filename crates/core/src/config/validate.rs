//! What `Config::parse` refuses beyond TOML's own errors: values out of
//! range, a key bound twice, a server named twice, patterns that do not
//! compile.

use super::{Config, actions};
use std::collections::HashSet;

impl Config {
    pub(super) fn validate(&self) -> std::result::Result<(), String> {
        let mut seen = HashSet::new();
        if let Some((k, _)) = self.hotkeys().find(|(k, _)| !seen.insert(*k)) {
            return Err(format!(
                "hotkey {k} is listed twice; a key triggers one action"
            ));
        }
        if !(100..=2000).contains(&self.hotkeys.double_tap_ms) {
            return Err("hotkeys.double_tap_ms must be between 100 and 2000".into());
        }
        let d = &self.dictation;
        if d.history_max == 0 {
            return Err("dictation.history_max must be at least 1".into());
        }
        if !(d.silence_rms > 0.0 && d.silence_rms < 1.0) {
            return Err("dictation.silence_rms must be between 0 and 1".into());
        }
        // the cap keeps 2 s of overlap, so less than 4 s would commit nothing
        if !(4.0..=60.0).contains(&d.segment_max_s) {
            return Err("dictation.segment_max_s must be between 4 and 60".into());
        }
        if self.translate.home.trim().is_empty() || self.translate.other.trim().is_empty() {
            return Err("translate.home and translate.other name a language each".into());
        }
        if self.panel.effect_volume > 100 {
            return Err("panel.effect_volume is a volume from 0 to 100".into());
        }
        if !(1024..=262_144).contains(&self.llm.chat_max_tokens) {
            return Err("llm.chat_max_tokens must be between 1024 and 262144".into());
        }
        crate::tasks::hide_regex(&self.llm.hide_models)
            .map_err(|e| format!("llm.hide_models: {e}"))?;
        crate::tasks::hide_regex(&self.llm.vision_models)
            .map_err(|e| format!("llm.vision_models: {e}"))?;
        actions::validate(&self.actions)?;
        self.wake.validate()?;
        crate::vocab::Vocab::new(&self.vocabulary)?;
        let mut names = HashSet::new();
        for e in &self.endpoints {
            // the picker and state.json tell servers by name
            if !names.insert(e.name.as_str()) {
                return Err(format!(
                    "endpoint {:?} is listed twice; each server needs a name of its own",
                    e.name
                ));
            }
            if !(e.url.starts_with("http://") || e.url.starts_with("https://")) {
                return Err(format!(
                    "endpoint {:?}: url must start with http:// or https://",
                    e.name
                ));
            }
        }
        let portal = cfg!(target_os = "linux") && self.capture.cmd.trim().is_empty(); // linux/portal.rs
        if !portal && !self.capture.cmd.contains("{path}") {
            return Err("capture.cmd must contain {path}".into());
        }
        Ok(())
    }
}
