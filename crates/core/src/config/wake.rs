//! `[wake]`: hands-free dictation (core's wake.rs): the phrases and timings.

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Wake {
    pub enabled: bool,
    pub wake: String,
    pub stop: String,
    pub send: String,
    pub scratch: String,
    pub quiet_s: f32,
    pub send_within_s: f32,
}

impl Default for Wake {
    fn default() -> Self {
        Self {
            enabled: false,
            wake: "hey blitz".into(),
            // "all done" reached the speech model from a German speaker as
            // "Und dann": the "blitz" the wake phrase trains carries better
            stop: "blitz stop".into(),
            send: "blitz send".into(),
            // "scratch that" alone reached the speech model as "scratch set
            // it", "Sketch that", "Scratch thread"; after "blitz" it stands out
            scratch: "blitz scratch, blitz delete".into(),
            quiet_s: 5.0,
            send_within_s: 120.0,
        }
    }
}

impl Wake {
    pub(super) fn validate(&self) -> Result<(), String> {
        if !(1.0..=30.0).contains(&self.quiet_s) {
            return Err("wake.quiet_s must be between 1 and 30".into());
        }
        if !(0.0..=3600.0).contains(&self.send_within_s) {
            return Err("wake.send_within_s must be between 0 and 3600".into());
        }
        if !self.enabled {
            return Ok(()); // a phrase it cannot use waits until it is on
        }
        if self.wake.trim().is_empty() {
            return Err("wake.wake: the phrase that starts a dictation".into());
        }
        crate::wake::keywords(self).map(drop)
    }
}
