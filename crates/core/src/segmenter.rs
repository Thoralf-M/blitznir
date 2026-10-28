//! Streaming speech-to-text on a model without a streaming API.
//!
//! How it works: audio arrives in 50 ms blocks. Every tick (≥ 0.35 s of new
//! audio, so a slow decode throttles itself) the whole current *segment* is
//! re-decoded, and the text two consecutive decodes agree on, cut at a word
//! boundary, is what live mode may type. A segment ends at a speech pause
//! (≥ 0.5 s of silence, once it is ≥ 3 s long) or at the hard cap
//! (`segment_max_s`). Its text is then *committed* — never touched again —
//! and the last 2 s of audio are re-fed to the next segment, so words at the
//! boundary are heard with context on both sides. Tokens inside that re-fed
//! overlap duplicate committed text and are skipped by their end time; the
//! few the lenient margin lets through are removed textually by `smart_join`.
//!
//! A pause cut fires 0.5 s into the pause, too early to know whether the
//! model's "." is a sentence end or a hesitation: the dot is held back and
//! restored only if the pause lasted ≥ 1.2 s.
//!
//! A buffer without a single voiced 50 ms frame is never decoded: the int8
//! model invents "Thank you." on silence.

use crate::Result;
use crate::config::Dictation;
use crate::platform::{Recognizer, Token};
use crate::textproc::{smart_join, stable_prefix};

pub const SAMPLE_RATE: usize = 16_000;
/// RMS frame; also the capture block size.
const FRAME: usize = SAMPLE_RATE / 20;
const FRAME_MS: u32 = 50;
/// New audio per tick.
const TICK: usize = SAMPLE_RATE * 35 / 100;
/// Pause length that ends a segment.
const SEGMENT_SILENCE_MS: u32 = 500;
/// Don't end segments faster than this.
const SEGMENT_MIN: usize = 3 * SAMPLE_RATE;
/// Audio re-fed to the next segment so boundary words get context on both sides.
const SEGMENT_OVERLAP: usize = 2 * SAMPLE_RATE;
/// A trailing "." after a shorter pause than this is hesitation, not a sentence end.
const SENTENCE_PAUSE_MS: u32 = 1200;
/// TDT timestamps are coarse (80 ms grid): a word counts if it ends later than
/// `skip - 0.4 s`, so a real word right after the boundary is never dropped.
const SKIP_MARGIN_S: f32 = 0.4;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SegConfig {
    /// Below this RMS a 50 ms frame is silence.
    pub silence_rms: f32,
    /// Hard cap on one segment, seconds.
    pub max_s: f32,
}

impl From<&Dictation> for SegConfig {
    fn from(d: &Dictation) -> Self {
        SegConfig {
            silence_rms: d.silence_rms,
            max_s: d.segment_max_s,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    /// What live mode may have on screen now: committed text + the stable prefix.
    Target(String),
    /// A segment was committed; everything committed so far (for recording.txt).
    Committed(String),
}

/// Tokens merged into words: a token without a leading space continues the
/// previous word. A word ends where its last token (punctuation included) ends.
struct Word {
    text: String,
    end_s: f32,
}

fn words(tokens: Vec<Token>) -> Vec<Word> {
    let mut out: Vec<Word> = vec![];
    for t in tokens {
        match out.last_mut() {
            Some(w) if !t.text.starts_with(' ') => {
                w.text.push_str(&t.text);
                w.end_s = t.end_s;
            }
            _ => out.push(Word {
                text: t.text,
                end_s: t.end_s,
            }),
        }
    }
    out
}

fn secs(samples: usize) -> f32 {
    samples as f32 / SAMPLE_RATE as f32
}

fn rms(frame: &[f32]) -> f32 {
    (frame.iter().map(|x| x * x).sum::<f32>() / frame.len().max(1) as f32).sqrt()
}

pub struct Segmenter {
    cfg: SegConfig,
    /// Audio since the last tick.
    pending: Vec<f32>,
    /// Samples not yet in a whole RMS frame.
    frame: Vec<f32>,
    /// The current segment, starting with the re-fed overlap.
    seg: Vec<f32>,
    /// Words ending before this (seconds into `seg`) are overlap duplicates.
    skip_s: f32,
    silence_ms: u32,
    /// A sentence dot stripped at a pause cut, fate undecided.
    pending_dot: bool,
    committed: String,
    prev_hyp: String,
    /// The last decode of `seg`.
    words: Vec<Word>,
}

impl Segmenter {
    pub fn new(cfg: SegConfig) -> Self {
        Segmenter {
            cfg,
            pending: vec![],
            frame: vec![],
            seg: vec![],
            skip_s: 0.0,
            silence_ms: 0,
            pending_dot: false,
            committed: String::new(),
            prev_hyp: String::new(),
            words: vec![],
        }
    }

    /// Text finalized by earlier segments.
    pub fn committed(&self) -> &str {
        &self.committed
    }

    /// Feed audio; decodes once enough new audio for a tick has arrived.
    pub fn push(&mut self, rec: &mut dyn Recognizer, audio: &[f32]) -> Result<Vec<Out>> {
        self.pending.extend_from_slice(audio);
        if self.pending.len() < TICK {
            return Ok(vec![]);
        }
        let chunk = std::mem::take(&mut self.pending);
        self.absorb(&chunk);
        self.decode(rec)?;
        let hyp = self.text(self.skip_s - SKIP_MARGIN_S, f32::INFINITY);
        let target = smart_join(&self.committed, &stable_prefix(&self.prev_hyp, &hyp));
        self.prev_hyp = hyp;
        let mut out = vec![Out::Target(target)];
        out.extend(self.cut().map(Out::Committed));
        Ok(out)
    }

    /// The recording ended: take `tail` (audio after the last push), decode
    /// the last segment once, and return the whole text.
    pub fn finish(&mut self, rec: &mut dyn Recognizer, tail: &[f32]) -> Result<String> {
        let mut rest = std::mem::take(&mut self.pending);
        rest.extend_from_slice(tail);
        self.absorb(&rest);
        self.decode(rec)?;
        let last = self.text(self.skip_s - SKIP_MARGIN_S, f32::INFINITY);
        let joined = smart_join(&self.committed, &last);
        // nothing new after the pause (at most a re-heard "done." the join
        // dropped as overlap): the recording ended on that sentence
        let words = |s: &str| s.split_whitespace().count();
        if self.pending_dot && words(&joined) == words(&self.committed) {
            self.pending_dot = false;
            return Ok(format!("{}.", joined.trim_end()).trim().to_string());
        }
        Ok(joined.trim().to_string())
    }

    /// Append audio to the segment; track trailing silence per 50 ms frame.
    fn absorb(&mut self, audio: &[f32]) {
        self.seg.extend_from_slice(audio);
        self.frame.extend_from_slice(audio);
        let (frames, _) = self.frame.as_chunks::<FRAME>();
        let loud: Vec<bool> = frames
            .iter()
            .map(|f| rms(f) >= self.cfg.silence_rms)
            .collect();
        self.frame.drain(..loud.len() * FRAME);
        for loud in loud {
            if !loud {
                self.silence_ms += FRAME_MS;
                continue;
            }
            // speech resumed: a long pause was a sentence end after all
            if self.pending_dot && self.silence_ms >= SENTENCE_PAUSE_MS {
                self.committed = format!("{}. ", self.committed.trim_end());
            }
            self.pending_dot = false;
            self.silence_ms = 0;
        }
    }

    fn decode(&mut self, rec: &mut dyn Recognizer) -> Result<()> {
        let voiced = self
            .seg
            .chunks(FRAME)
            .any(|f| rms(f) >= self.cfg.silence_rms);
        self.words = if voiced {
            words(rec.transcribe(&self.seg)?)
        } else {
            vec![]
        };
        Ok(())
    }

    /// Words of the last decode ending in `(after, upto]` seconds.
    fn text(&self, after: f32, upto: f32) -> String {
        let text: String = self
            .words
            .iter()
            .filter(|w| w.end_s > after && w.end_s <= upto)
            .map(|w| w.text.as_str())
            .collect();
        text.trim().to_string()
    }

    /// End the segment at a pause or the cap: commit its text and restart
    /// from the overlap. Returns the committed text when it changed.
    fn cut(&mut self) -> Option<String> {
        let len = self.seg.len();
        let pause = self.silence_ms >= SEGMENT_SILENCE_MS && len >= SEGMENT_MIN;
        if !pause && secs(len) < self.cfg.max_s {
            return None;
        }
        // At a pause every word is finished: commit them all. At the cap
        // (mid-speech) hold back the overlap for the next segment to re-hear.
        let cut = if pause {
            len
        } else {
            len.saturating_sub(SEGMENT_OVERLAP)
        };
        let upto = if pause { f32::INFINITY } else { secs(cut) };
        // the same lenient lower bound as the live text: anything that could
        // have been typed MUST be committed, or the final pass would delete it
        let mut text = self.text(self.skip_s - SKIP_MARGIN_S, upto);
        if pause && text.ends_with('.') && !text.ends_with("..") {
            text.pop();
            text.truncate(text.trim_end().len());
            // a dot heard before any word ends no sentence
            self.pending_dot = !text.is_empty() || !self.committed.trim().is_empty();
        }
        let changed = !text.is_empty();
        if changed {
            self.committed = smart_join(&self.committed, &text) + " ";
        }
        let seed_start = len - len.min(SEGMENT_OVERLAP);
        self.seg.drain(..seed_start);
        self.skip_s = (secs(cut) - secs(seed_start)).max(0.0);
        self.prev_hyp.clear();
        self.words.clear();
        changed.then(|| self.committed.clone())
    }
}

#[cfg(test)]
mod tests;
