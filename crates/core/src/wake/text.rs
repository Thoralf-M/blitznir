//! The dictation's text around the phrases: a stop phrase said last or the
//! wake phrase's end heard first taken off, and the speech model's check
//! of a wake phrase the spotter heard.

use super::{
    alternatives,
    sound::{distance, heard_as, letters, sound},
};

/// `text` without a stop phrase it ends with (said to end the dictation,
/// and typed with it), nor the commas or spaces before it. Close enough
/// counts: the dictation's model may spell it a little differently.
pub fn strip_tail(text: &str, stop: &str) -> String {
    let letters = |s: &str| -> String {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    };
    let spans: Vec<(usize, &str)> = text
        .split_whitespace()
        .map(|w| (w.as_ptr() as usize - text.as_ptr() as usize, w))
        .collect();
    for phrase in alternatives(stop) {
        let n = phrase.split_whitespace().count();
        if n == 0 || spans.len() < n {
            continue;
        }
        let tail = &spans[spans.len() - n..];
        let said: String = tail.iter().map(|(_, w)| letters(w)).collect();
        let want = letters(phrase);
        if distance(&said, &want) <= (want.chars().count() / 3).max(1) {
            let cut = &text[..tail[0].0];
            return cut
                .trim_end_matches(|c: char| c.is_whitespace() || ",;:-–".contains(c))
                .to_string();
        }
    }
    text.to_string()
}

/// `text` up to the last of its last four words that starts as `word` does
/// ("blitz"): a command the final pass wrote otherwise than the live text
/// ("Blitzdelete delete."); `text` as it was when none does.
pub fn strip_from(text: &str, word: &str) -> String {
    let head = letters(word);
    let words: Vec<&str> = text.split_whitespace().collect();
    let starts = |w: &&&str| {
        let w = letters(w);
        !head.is_empty() && w.get(..head.len()).is_some_and(|p| distance(p, &head) <= 1)
    };
    let Some(w) = words[words.len().saturating_sub(4)..]
        .iter()
        .rev()
        .find(starts)
    else {
        return text.to_string();
    };
    let at = w.as_ptr() as usize - text.as_ptr() as usize;
    text[..at]
        .trim_end_matches(|c: char| c.is_whitespace() || ",;:-–".contains(c))
        .to_string()
}

/// `text` without the end of the wake phrase it starts with: a dictation
/// the wake phrase started hears from just before the spotter fired, so
/// its first words can be "blitz," or "hey blitz,". Its first letter is
/// then upper case if the text's was.
/// The words `text` starts with that say `setting` (the fewest), as the
/// transcript wrote them: "Hey please," of "Hey please, write a note.", for
/// the dictation the wake phrase starts to take off as heard.
pub fn said_prefix(text: &str, setting: &str) -> Option<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let most = alternatives(setting)
        .map(|p| p.split_whitespace().count())
        .max()?;
    (1..=most.min(words.len())).find_map(|k| {
        let head = words[..k].join(" ");
        said(&head, setting).then(|| {
            words[..k]
                .iter()
                .map(|w| letters(w))
                .collect::<Vec<_>>()
                .join(" ")
        })
    })
}

pub fn strip_head(text: &str, wake: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let Some(k) = head_words(&words, wake) else {
        return text.to_string();
    };
    let rest = words[k..].join(" ");
    let rest = rest.trim_start_matches(|c: char| !c.is_alphanumeric());
    let upper = text
        .chars()
        .find(|c| c.is_alphabetic())
        .is_some_and(char::is_uppercase);
    let mut cs = rest.chars();
    match cs.next() {
        Some(c) if upper => c.to_uppercase().chain(cs).collect(),
        _ => rest.to_string(),
    }
}

/// How many of `words` the wake phrase is: the phrase's last k words, the
/// longest the text starts with, spelt all but exactly; else two words,
/// the second the phrase's last as written, a mark after it ("Okay
/// Blitz,"). Never by sound: that ate "Hey, let's" of "Hey, let's meet".
fn head_words(words: &[&str], wake: &str) -> Option<usize> {
    for phrase in alternatives(wake) {
        let said: Vec<&str> = phrase.split_whitespace().collect();
        for k in (1..=said.len().min(words.len())).rev() {
            // word by word: run together, "Der Blitz" passed for "hey blitz"
            let close = |(w, p): (&&str, &&str)| {
                let p = letters(p);
                distance(&letters(w), &p) <= (p.chars().count() / 3).max(1)
            };
            if words[..k].iter().zip(&said[said.len() - k..]).all(close) {
                return Some(k);
            }
        }
    }
    // the dictation's own spelling: any word, then the last one as written
    // and a mark after it ("Okay Blitz,"); "Der Blitz hat…" has none
    let last = alternatives(wake).next()?.split_whitespace().last()?;
    let w1 = words.get(1)?;
    let marked = w1.ends_with(['.', ',', '!', '?', ':', ';']);
    (marked && distance(&letters(w1), &letters(last)) <= 1).then_some(2)
}

/// Whether `text`, the dictation model's transcript of the moment the
/// spotter fired, says one of `setting`'s phrases close enough: the second
/// opinion (as Siri's bigger model checks its small one). The spotter took
/// "we played blitz" for "hey blitz" with music on; the transcript did not.
pub fn said(text: &str, setting: &str) -> bool {
    let words: Vec<String> = text
        .split_whitespace()
        .map(|w| {
            w.chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect()
        })
        .filter(|w: &String| !w.is_empty())
        .collect();
    alternatives(setting).any(|phrase| {
        let want: String = phrase
            .chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect();
        let n = phrase.split_whitespace().count();
        // a model may join or split a word: windows one shorter and longer too
        // or its most telling word, by sound: a model that took the speaker
        // for another language still wrote "Blitzend" for "hey blitz"
        let telling = phrase.split_whitespace().max_by_key(|w| w.len()).map(sound);
        let telling = telling
            .filter(|t| t.chars().count() >= 3)
            .is_some_and(|t| words.iter().any(|w| sound(w).contains(&t)));
        let spelt = telling
            || (n.saturating_sub(1).max(1)..=n + 1).any(|k| {
                words
                    .windows(k.min(words.len()).max(1))
                    .any(|w| distance(&w.concat(), &want) <= (want.chars().count() / 3).max(1))
            });
        // or said with an accent the model spelt as another language: by
        // sound, over the whole phrase ("Hej plut!" for "hey blitz")
        let key: String = phrase.split_whitespace().map(sound).collect();
        let sounds = n > 1
            && words.len() >= n
            && words.windows(n).any(|w| {
                let heard: String = w.iter().map(|w| sound(w)).collect();
                distance(&heard, &key) <= (key.chars().count() / 4).max(1)
            });
        spelt || sounds
    })
}

/// Whether `text`, a dictation's live transcript, ends with one of
/// `setting`'s phrases all but exactly (one letter off at most): said to
/// end it. The spotter missed a speaker's "all done" the transcript had
/// right; strict, so "well done" is not it.
pub fn said_last(text: &str, setting: &str) -> bool {
    let letters = |s: &str| -> String {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    };
    let words: Vec<&str> = text.split_whitespace().collect();
    alternatives(setting).any(|phrase| {
        let n = phrase.split_whitespace().count();
        words.len() >= n
            && distance(
                &letters(&words[words.len() - n..].concat()),
                &letters(phrase),
            ) <= 1
    })
}

/// `text` without its last sentence (from the last . ! ? or line break):
/// what the take-back phrase leaves. None when that is over [`LONGEST`]
/// characters: a field without punctuation would go whole.
pub fn drop_last_sentence(text: &str) -> Option<String> {
    let kept = text.trim_end_matches(|c: char| c.is_whitespace() || ",;:-–".contains(c));
    let kept = kept.trim_end_matches(['.', '!', '?', '。', '！', '？']);
    let cut = kept
        .rfind(['.', '!', '?', '。', '！', '？', '\n'])
        .map_or(0, |i| {
            i + kept[i..].chars().next().map_or(1, char::len_utf8)
        });
    (text[cut..].trim().chars().count() <= LONGEST).then(|| text[..cut].to_string())
}

/// The longest sentence the take-back phrase takes back (four lines or so).
pub const LONGEST: usize = 300;

/// Whether an utterance said on its own is a command of `setting`: its
/// words, all of them and no more ("Blitz, send.", "Blitz Ascend."); the
/// first within a letter of the phrase's, the rest `heard_as`. A command is
/// never searched for inside a sentence: Dragon too takes one only between
/// pauses, and every search for one ("scratch pad", "Blitz sent") cut text.
pub fn is_command(utterance: &str, setting: &str) -> bool {
    let words = words(utterance);
    alternatives(setting).any(|p| command(&words, p))
}

/// `text`'s last words when they are a command of `setting` (said after a
/// pause, the end of a dictation): as `strip_tail` takes them back.
pub fn command_tail(text: &str, setting: &str) -> Option<String> {
    let words = words(text);
    alternatives(setting).find_map(|p| {
        // its words, or fewer run together ("…a note. Blitzsend")
        (1..=p.split_whitespace().count()).rev().find_map(|n| {
            let start = words.len().checked_sub(n)?;
            // a sentence of its own: "…say blitz send." is talk about it
            // a comma is not: "I told him, blitz send."
            let own = start == 0 || words[start - 1].ends_with(['.', '!', '?', '…']);
            let tail = &words[start..];
            (own && command(tail, p)).then(|| {
                tail.iter()
                    .map(|w| letters(w))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
        })
    })
}

fn words(text: &str) -> Vec<&str> {
    let real = |w: &&str| w.chars().any(char::is_alphanumeric);
    text.split_whitespace().filter(real).collect()
}

fn command(words: &[&str], phrase: &str) -> bool {
    let want: Vec<&str> = phrase.split_whitespace().collect();
    // no sentence end inside ("Blitz. Ende."); a pause ("Blitz... send.") is none
    let ends = |w: &&str| w.ends_with(['.', '!', '?']) && !w.ends_with("..");
    if words.iter().rev().skip(1).any(ends) {
        return false;
    }
    // run together ("Blitzsend."): exactly; "Blitzend" is how "hey blitz" came out
    let all: String = words.iter().map(|w| letters(w)).collect();
    if words.len() < want.len() {
        return all == letters(phrase);
    }
    let Some(head) = want.first().filter(|_| words.len() == want.len()) else {
        return false;
    };
    let slack = (head.chars().count() / 4).max(1);
    let heard = |i: usize| {
        let (said, w) = (words[i], want[i]);
        // not "Blitz end." (an s gone into the z): one space from "Blitzend",
        // how "hey blitz" came out
        heard_as(said, w)
    };
    distance(&letters(words[0]), &letters(head)) <= slack && (1..want.len()).all(heard)
}

#[cfg(test)]
mod tests;
