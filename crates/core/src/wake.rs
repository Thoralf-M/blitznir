//! Hands-free dictation (`[wake]`): a wake phrase starts a live dictation,
//! a stop phrase or a pause ends it, and a send phrase presses Return in the
//! app it went to. A small keyword spotter (shared's wake.rs) listens for
//! the phrases; this is what it is told to look for, and what follows from
//! what it hears.

use crate::{config::Wake, platform::Focus};
use std::{
    collections::HashMap,
    fmt::Write,
    time::{Duration, Instant},
};

/// The spotter's SentencePiece pieces and their scores (piece, tab, score),
/// written out once from its bpe.model (a unigram model): words are cut by them.
const VOCAB: &str = include_str!("wake/bpe.vocab");

/// Which phrase the spotter heard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Wake,
    Stop,
    Send,
}

impl Kind {
    const ALL: [Kind; 3] = [Kind::Wake, Kind::Stop, Kind::Send];

    fn label(self) -> &'static str {
        match self {
            Kind::Wake => "WAKE",
            Kind::Stop => "STOP",
            Kind::Send => "SEND",
        }
    }

    /// The spotter's result: its keyword's label.
    pub fn of(label: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.label() == label)
    }

    /// Boost and trigger threshold. Phonon's phrases mixed with a
    /// GarageBand funk loop (bass, brass, a vocal synth), the phrases in a
    /// sentence: 1.5/0.25 heard them only with the music 10 dB under the
    /// voice, 3.0/0.10 even as loud as it; neither ever fired on the music
    /// alone, 26 s of talk or near misses ("blitz is a fast form of chess",
    /// "send me the file", "Blitzkrieg"). Through the air "blitz send" was
    /// missed at 1.5/0.25. The wake and send phrases, checked by the speech
    /// model before anything happens, can afford more: at 3.0/0.10 a
    /// speaker had to say "hey blitz" very clearly, and "blitz send" was
    /// heard once in seven. Recordings of that speaker: "hey blitz" 6 of 8
    /// at 4.0/0.05, 7 of 8 at 6.0/0.01, no ordinary sentence taken.
    fn tuning(self) -> (f32, f32) {
        match self {
            Kind::Wake => (6.0, 0.01),
            Kind::Send => (4.0, 0.05),
            Kind::Stop => (3.0, 0.10),
        }
    }
}

/// `phrase`'s words as the spotter's model spells them: upper case, umlauts
/// written out, anything but letters and apostrophes a word break.
fn words(phrase: &str) -> Vec<String> {
    let mut out = String::new();
    for c in phrase.chars().flat_map(char::to_uppercase) {
        match c {
            'Ä' => out.push_str("AE"),
            'Ö' => out.push_str("OE"),
            'Ü' => out.push_str("UE"),
            'ß' => out.push_str("SS"),
            c if c.is_ascii_uppercase() || c == '\'' => out.push(c),
            _ => out.push(' '),
        }
    }
    out.split_whitespace().map(str::to_string).collect()
}

/// The spotter's tokens for `phrase`: each word ▁-prefixed and cut as its
/// SentencePiece unigram model cuts it, into the pieces whose scores (log
/// probabilities) sum highest. None: no word, or a letter no piece has.
pub fn tokens(phrase: &str) -> Option<Vec<String>> {
    let scores: HashMap<&str, f32> = VOCAB
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .filter_map(|(p, s)| Some((p, s.parse().ok()?)))
        .collect();
    let mut out = vec![];
    for word in words(phrase) {
        let chars: Vec<char> = std::iter::once('▁').chain(word.chars()).collect();
        // best[i]: the best score of chars[..i], and where its last piece starts
        let mut best: Vec<Option<(f32, usize)>> = vec![None; chars.len() + 1];
        best[0] = Some((0.0, 0));
        for end in 1..=chars.len() {
            for start in 0..end {
                let piece: String = chars[start..end].iter().collect();
                if let (Some((before, _)), Some(s)) = (best[start], scores.get(piece.as_str()))
                    && best[end].is_none_or(|(b, _)| before + s > b)
                {
                    best[end] = Some((before + s, start));
                }
            }
        }
        let mut pieces = vec![];
        let mut end = chars.len();
        while end > 0 {
            let (_, start) = best[end]?;
            pieces.push(chars[start..end].iter().collect::<String>());
            end = start;
        }
        out.extend(pieces.into_iter().rev());
    }
    (!out.is_empty()).then_some(out)
}

/// A setting's phrases: comma-separated alternatives ("blitz send, blitz sent").
pub(crate) fn alternatives(setting: &str) -> impl Iterator<Item = &str> {
    setting.split(',').map(str::trim).filter(|p| !p.is_empty())
}

/// The spotter's keyword list for `w`, one line per phrase: its tokens, its
/// boost (`:`), threshold (`#`) and label (`@`). A phrase left empty is off.
pub fn keywords(w: &Wake) -> Result<String, String> {
    let mut out = String::new();
    // send and scratch are the transcript's, of an utterance on its own: a
    // spotted send pressed Return on talk ("Blitz ist Quatsch")
    for kind in [Kind::Wake, Kind::Stop] {
        let (boost, threshold) = kind.tuning();
        for phrase in alternatives(w.phrase(kind)) {
            let toks = tokens(phrase)
                .ok_or_else(|| format!("[wake] {phrase:?}: no letters the listener knows"))?;
            let label = kind.label();
            let _ = writeln!(out, "{} :{boost} #{threshold} @{label}", toks.join(" "));
            // said fast, its words run together: no word break between them
            let joined = tokens(&phrase.replace(char::is_whitespace, "")).filter(|j| *j != toks);
            if let Some(joined) = joined {
                let _ = writeln!(out, "{} :{boost} #{threshold} @{label}", joined.join(" "));
            }
        }
    }
    Ok(out)
}

impl Wake {
    /// The setting of `kind`'s phrase.
    pub fn phrase(&self, kind: Kind) -> &str {
        match kind {
            Kind::Wake => &self.wake,
            Kind::Stop => &self.stop,
            Kind::Send => &self.send,
        }
    }
}

pub use text::{
    command_tail, drop_last_sentence, is_command, said, said_last, said_prefix, strip_from,
    strip_head, strip_tail,
};

/// What the listener asks the app to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    /// Start a live dictation.
    Start,
    /// End the dictation running; `said`: by the stop phrase (it was typed
    /// with the text, and is taken off).
    Stop { said: bool },
    /// Press Return in this app (it is the one the dictation went to).
    Send(Focus),
}

/// A wake-started dictation ends after this long whatever it hears: talk in
/// the room (a TV, a colleague) keeps bringing words, which a pause needs.
const LONGEST: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    Idle,
    /// `wake`: started by the wake phrase (a pause ends only those), at
    /// `since`; `quiet`: since when nothing was loud; `heard`: something was.
    Dictating {
        wake: bool,
        since: Instant,
        quiet: Option<Instant>,
        heard: bool,
    },
    /// A dictation went into `to`; a send phrase until `until` presses Return.
    Armed {
        to: Focus,
        until: Instant,
    },
}

/// From what the spotter hears and whether words still come, to what the app
/// does. The app tells it when a dictation starts and ends.
pub struct Listener {
    state: State,
    /// `[wake] quiet_s`: a wake-started dictation ends after this much quiet.
    quiet: Duration,
    /// `[wake] send_within_s`.
    window: Duration,
}

impl Listener {
    pub fn new(w: &Wake) -> Listener {
        Listener {
            state: State::Idle,
            quiet: Duration::from_secs_f32(w.quiet_s.max(0.5)),
            window: Duration::from_secs_f32(w.send_within_s.max(0.0)),
        }
    }

    /// The spotter heard `kind`.
    pub fn hit(&mut self, kind: Kind, now: Instant) -> Option<Act> {
        match (kind, self.state) {
            (Kind::Wake, State::Dictating { .. }) => None,
            (Kind::Wake, _) => Some(Act::Start),
            (Kind::Stop, State::Dictating { .. }) => Some(Act::Stop { said: true }),
            // the app checked it was the last thing said (its transcript)
            (Kind::Send, State::Armed { to, until }) if now < until => {
                self.state = State::Idle;
                Some(Act::Send(to))
            }
            _ => None,
        }
    }

    /// 50 ms passed; `loud`: someone is still dictating (words still arrive).
    pub fn block(&mut self, loud: bool, now: Instant) -> Option<Act> {
        let quiet_for = |since: Option<Instant>| {
            since.map_or(Duration::ZERO, |t| now.saturating_duration_since(t))
        };
        let (next, act) = match self.state {
            State::Dictating {
                wake,
                since,
                quiet,
                heard,
            } => {
                let (quiet, heard) = if loud {
                    (None, true)
                } else {
                    (quiet.or(Some(now)), heard)
                };
                // before the first word, twice as long: a breath after the wake phrase
                let wait = if heard { self.quiet } else { self.quiet * 2 };
                let ends = wake
                    && ((!loud && quiet_for(quiet) >= wait)
                        || now.saturating_duration_since(since) >= LONGEST);
                let state = State::Dictating {
                    wake,
                    since,
                    quiet,
                    heard,
                };
                (state, ends.then_some(Act::Stop { said: false }))
            }
            State::Armed { until, .. } if now >= until => (State::Idle, None),
            state => (state, None),
        };
        self.state = next;
        act
    }

    /// A dictation started at `now`; `wake`: by the wake phrase.
    pub fn started(&mut self, wake: bool, now: Instant) {
        self.state = State::Dictating {
            wake,
            since: now,
            quiet: None,
            heard: false,
        };
    }

    /// The dictation ended, its text typed into `to` (Linux: Focus(None),
    /// no compositor says which app; the app checks it where it can).
    /// `arm`: the send phrase may follow (a wake-started dictation that
    /// typed something).
    pub fn ended(&mut self, to: Focus, arm: bool, now: Instant) {
        self.state = match arm && !self.window.is_zero() {
            false => State::Idle,
            true => State::Armed {
                to,
                until: now + self.window,
            },
        };
    }

    /// A send phrase would press Return now.
    pub fn armed(&self, now: Instant) -> bool {
        matches!(self.state, State::Armed { until, .. } if now < until)
    }

    /// No dictation runs any more: one that failed to start ends without a
    /// text, and must not leave the wake phrase ignored.
    pub fn idle(&mut self) {
        if matches!(self.state, State::Dictating { .. }) {
            self.state = State::Idle;
        }
    }
}

mod sound;
#[cfg(test)]
mod tests;
mod text;
