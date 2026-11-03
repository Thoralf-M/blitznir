//! A reasoning trace that repeats itself verbatim: caught while it streams,
//! and cut where it starts for the think-off retry's notes. Sage loops a
//! ~100-char block ("Wait, I'll check if I should mention…  Okay.  Let's
//! write.") until max_tokens.

/// A trace is looked at once per this many new chars: the check reads it all.
const EVERY: usize = 200;
/// The trace's last this many chars are the loop's sample. Sage repeats
/// 200-char stretches (code it re-drafts) even in traces that finish; on 15
/// of its traces 500 chars seen twice before caught 7 of 11 that ran to the
/// budget and cut none of the 4 that finished (200 chars: 8 of 11, but 3 of
/// 4 finishing ones cut too). Its loops repeat 100 to 3500 chars.
const WINDOW: usize = 500;
/// The sample seen this often before (overlaps count) is a loop: a passage
/// quoted twice is not.
const SEEN: usize = 2;

/// Watches a streaming trace for a loop.
#[derive(Default)]
pub(super) struct Watch {
    chars: usize,
    checked: usize,
}

impl Watch {
    /// `reasoning` now, `added` its newest part: true once it loops.
    pub fn looped(&mut self, reasoning: &str, added: &str) -> bool {
        self.chars += added.chars().count();
        if self.chars < self.checked + EVERY {
            return false;
        }
        self.checked = self.chars;
        repeats(reasoning)
    }

    /// The chars seen so far.
    pub fn chars(&self) -> usize {
        self.chars
    }
}

/// The last [`WINDOW`] chars of `r` occur [`SEEN`] times before them.
fn repeats(r: &str) -> bool {
    let Some((start, _)) = r.char_indices().rev().nth(WINDOW - 1) else {
        return false;
    };
    let (before, window) = r.split_at(start);
    let first = window.chars().next().map_or(1, char::len_utf8);
    let (mut from, mut seen) = (0, 0);
    while let Some(at) = before[from..].find(window) {
        seen += 1;
        if seen >= SEEN {
            return true;
        }
        from += at + first; // the same char starts every match
    }
    false
}

/// Notes longer than this keep their start: what a loop repeats is later.
const NOTES_MAX: usize = 16_000;

/// `reasoning` up to where it starts repeating itself: the first `i`, in
/// steps of 100 chars, whose next 200 chars already occur before it; at
/// most [`NOTES_MAX`] chars.
pub(super) fn unlooped(reasoning: &str) -> &str {
    const STEP: usize = 100;
    const SPAN: usize = 200;
    let at: Vec<usize> = reasoning.char_indices().map(|(b, _)| b).collect();
    let byte = |i: usize| at.get(i).copied().unwrap_or(reasoning.len());
    let mut end = at.len();
    let mut i = STEP;
    while i + SPAN <= at.len() {
        let (before, window) = (&reasoning[..byte(i)], &reasoning[byte(i)..byte(i + SPAN)]);
        if before.contains(window) {
            end = i;
            break;
        }
        i += STEP;
    }
    &reasoning[..byte(end.min(NOTES_MAX))]
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    /// Sage's loop, as it streamed.
    const SAGE: &str = "Wait, I'll check if I should mention that I can write others.\n    Yes.\n\n    Okay.\n\n    Let's write.\n\n    ";

    /// Text that never repeats, `n` chars.
    pub(crate) fn fresh(n: usize) -> String {
        let s: String = (0..)
            .map(|i| format!("step {i:04}. "))
            .take(n / 11 + 1)
            .collect();
        s[..n].to_string()
    }

    /// Where a watch over `trace`, fed 4 chars at a time, calls it a loop.
    fn caught(trace: &str) -> Option<usize> {
        let (mut w, mut so_far) = (Watch::default(), String::new());
        let chars: Vec<char> = trace.chars().collect();
        for piece in chars.chunks(4) {
            let piece: String = piece.iter().collect();
            so_far.push_str(&piece);
            if w.looped(&so_far, &piece) {
                return Some(w.chars());
            }
        }
        None
    }

    #[test]
    fn sages_loop_is_caught_within_a_few_rounds() {
        let head = fresh(1500);
        let trace = format!("{head}{}", SAGE.repeat(40));
        let at = caught(&trace).expect("caught");
        let rounds = (at - head.len()) as f32 / SAGE.chars().count() as f32;
        // the 500-char sample spans 5 rounds and must be seen twice before
        assert!(rounds <= 14.0, "after {rounds:.1} rounds");
        let umlauts = format!("{head}{}", "Prüfe, ob es grün ist. Ja. ".repeat(80));
        assert!(caught(&umlauts).is_some(), "on chars, not bytes");
    }

    #[test]
    fn a_sound_trace_that_says_things_again_is_no_loop() {
        // short phrases again and again, between new thoughts
        let mut trace = String::new();
        for i in 0..150 {
            let thought = fresh(140).replace("step", &format!("idea {i}"));
            trace += &format!("{thought}\n\nOkay.\n\nWait, let me check.\n\n    Yes.\n\n");
        }
        assert!(trace.chars().count() > 20_000);
        assert_eq!(caught(&trace), None);
        // a long passage quoted twice (the question, then again)
        let f = fresh(1200);
        let (quoted, between, after) = (&f[..400], &f[400..700], &f[700..]);
        let twice = format!("{quoted}{between}{quoted}{after}");
        assert_eq!(caught(&twice), None);
    }

    #[test]
    fn notes_stop_at_the_first_repeat_and_at_the_cap() {
        // the loop starts at 650; the first step whose window was seen is 700
        let twice = format!("{}{}", fresh(650), fresh(650));
        assert_eq!(unlooped(&twice), &twice[..700]);
        let umlauts = "ä".repeat(500); // cut on a char, not a byte
        assert_eq!(unlooped(&umlauts).chars().count(), 200);
        let long = fresh(20_000);
        assert_eq!(unlooped(&long).chars().count(), 16_000);
        assert_eq!(unlooped("short"), "short");
    }
}
