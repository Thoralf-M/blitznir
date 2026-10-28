use super::*;
use crate::platform::fake::{FakeRecognizer, Timeline};
use std::sync::{Arc, Mutex};

const CFG: SegConfig = SegConfig {
    silence_rms: 0.01,
    max_s: 15.0,
};

/// Consecutive words of `each` seconds from `start`.
fn words_from(text: &str, start: f64, each: f64) -> Vec<(String, f64, f64)> {
    text.split(' ')
        .enumerate()
        .map(|(i, w)| {
            (
                w.to_string(),
                start + i as f64 * each,
                start + (i + 1) as f64 * each,
            )
        })
        .collect()
}

fn timeline(parts: &[Vec<(String, f64, f64)>]) -> Timeline {
    let all: Vec<(&str, f64, f64)> = parts
        .iter()
        .flatten()
        .map(|(w, s, e)| (w.as_str(), *s, *e))
        .collect();
    Timeline::new(&all)
}

/// Feed `pcm` in 0.35 s ticks; returns (tick end time, output) pairs and the final text.
fn run(rec: &mut FakeRecognizer, pcm: &[f32]) -> (Vec<(f32, Out)>, String) {
    let mut seg = Segmenter::new(CFG);
    let mut outs = vec![];
    for (i, chunk) in pcm.chunks(TICK).enumerate() {
        let at = secs((i + 1) * TICK);
        outs.extend(seg.push(rec, chunk).unwrap().into_iter().map(|o| (at, o)));
    }
    let text = seg.finish(rec, &[]).unwrap();
    (outs, text)
}

fn commits(outs: &[(f32, Out)]) -> Vec<(f32, String)> {
    outs.iter()
        .filter_map(|(t, o)| {
            if let Out::Committed(c) = o {
                Some((*t, c.clone()))
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn silence_is_never_decoded() {
    let t = Timeline::new(&[]);
    let mut rec = t.recognizer();
    let (outs, text) = run(&mut rec, &t.audio(5.0));
    assert_eq!(rec.calls, 0);
    assert_eq!(text, "");
    assert!(commits(&outs).is_empty());
}

#[test]
fn a_pause_cuts_once_the_segment_is_3_s_long() {
    // a 0.8 s pause at 1.0 s is too early to cut; the one at 4.2 s cuts
    let t = timeline(&[
        words_from("a b", 0.0, 0.5),
        words_from("c d e f g h", 1.8, 0.4),
        words_from("i j", 5.0, 0.4),
    ]);
    let mut rec = t.recognizer();
    let (outs, text) = run(&mut rec, &t.audio(6.5));
    let c = commits(&outs);
    assert_eq!(c[0].1, "a b c d e f g h ");
    assert!(c[0].0 >= 4.7 && c[0].0 < 5.1, "first cut at {}", c[0].0);
    assert_eq!(text, "a b c d e f g h i j");
}

#[test]
fn the_hard_cap_cuts_mid_speech_and_re_feeds_2_s() {
    let text: Vec<String> = (0..34).map(|i| format!("w{i}")).collect();
    let t = timeline(&[words_from(&text.join(" "), 0.0, 0.5)]);
    let starts = Arc::new(Mutex::new(vec![]));
    let (t2, s2) = (t.clone(), starts.clone());
    let mut rec = FakeRecognizer::new(move |pcm| {
        s2.lock().unwrap().push(Timeline::offset(pcm));
        Ok(t2.tokens(pcm))
    });
    let (outs, final_text) = run(&mut rec, &t.audio(18.0));
    let c = commits(&outs);
    assert!((c[0].0 - 15.05).abs() < 1e-3, "cap cut at {}", c[0].0);
    assert_eq!(
        c[0].1,
        format!("{} ", text[..26].join(" ")),
        "words ending ≤ 13.05 s"
    );
    let starts = starts.lock().unwrap();
    assert!(
        starts.iter().any(|s| (s - 13.05).abs() < 1e-3),
        "the next segment starts 2 s back: {starts:?}"
    );
    assert_eq!(final_text, text.join(" "));
    for (_, o) in &outs {
        if let Out::Target(target) = o {
            assert!(
                final_text.starts_with(target.trim_end()),
                "{target:?} is not settled text"
            );
        }
    }
}

#[test]
fn overlap_duplicates_are_skipped_by_time_then_by_text() {
    // voice 0–3 s, then silence: the cut at 3.5 s re-feeds 1.5–3.5 s, skip = 2.0 s
    let t = Timeline::new(&[("x", 0.0, 3.0)]);
    let mut rec = FakeRecognizer::new(|pcm| {
        let (start, dur) = (Timeline::offset(pcm), secs(pcm.len()));
        let script: &[(&str, f32)] = if start == 0.0 {
            &[(" Hello", 1.0), (" big", 2.0), (" world", 2.9)]
        } else if (start - 1.5).abs() < 1e-3 {
            // "Hello" ends far before skip - 0.4: dropped by time. "world" ends
            // inside the margin: leaks, removed by the join. "again" is new.
            &[(" Hello", 0.3), (" world", 1.8), (" again", 2.05)]
        } else {
            &[]
        };
        Ok(script
            .iter()
            .filter(|(_, e)| *e <= dur)
            .map(|(w, e)| Token::new(w, *e))
            .collect())
    });
    let (outs, text) = run(&mut rec, &t.audio(6.0));
    assert_eq!(commits(&outs)[0], (3.5, "Hello big world ".to_string()));
    assert_eq!(text, "Hello big world again");
}

fn dotted(pause_s: f64) -> String {
    let first = words_from("Hello there this is done.", 0.0, 0.64);
    let second = words_from("Next words.", 3.2 + pause_s, 0.6);
    let end = 3.2 + pause_s + 1.2 + 1.0;
    let t = timeline(&[first, second]);
    run(&mut t.recognizer(), &t.audio(end)).1
}

#[test]
fn a_sentence_dot_survives_a_long_pause() {
    assert_eq!(dotted(1.6), "Hello there this is done. Next words.");
}

#[test]
fn a_hesitation_dot_is_dropped() {
    assert_eq!(dotted(0.7), "Hello there this is done next words.");
}

#[test]
fn a_lone_dot_before_any_words_ends_no_sentence() {
    // a breath heard as "." alone, a long pause, then the first words
    let t = timeline(&[
        words_from(".", 0.0, 3.2),
        words_from("Next words.", 4.8, 0.6),
    ]);
    let (outs, text) = run(&mut t.recognizer(), &t.audio(7.2));
    assert_eq!(text, "Next words.");
    for (_, o) in &outs {
        if let Out::Target(target) = o {
            assert!(!target.starts_with('.'), "typed {target:?}");
        }
    }
}

#[test]
fn tokens_are_grouped_into_words() {
    let w = words(vec![
        Token::new(" Sch", 0.16),
        Token::new("öne", 0.48),
        Token::new(" ", 1.2),
        Token::new("Ö", 1.44),
        Token::new(".", 3.0),
    ]);
    let got: Vec<(&str, f32)> = w.iter().map(|w| (w.text.as_str(), w.end_s)).collect();
    assert_eq!(got, [(" Schöne", 0.48), (" Ö.", 3.0)]);
}

#[test]
fn the_last_dot_survives_a_stop_right_after_the_pause_cut() {
    // voice 0–3.2 s, stop at 4.2 s: the pause cut at 3.85 s strips the dot
    // and re-feeds 1.85–3.85 s. The recognizer puts the dot 0.5 s after the
    // word, inside the lenient margin, so "done." leaks into the last decode
    // and the join drops it as an overlap duplicate: nothing new followed.
    let t = Timeline::new(&[("x", 0.0, 3.2)]);
    let mut rec = FakeRecognizer::new(|pcm| {
        let (start, dur) = (Timeline::offset(pcm), secs(pcm.len()));
        let script: &[(&str, f32)] = if start == 0.0 {
            &[(" Hello", 1.0), (" there", 2.0), (" done", 3.0), (".", 3.2)]
        } else if (start - 1.85).abs() < 1e-3 {
            &[(" done", 1.2), (".", 1.7)]
        } else {
            &[]
        };
        Ok(script
            .iter()
            .filter(|(_, e)| *e <= dur)
            .map(|(w, e)| Token::new(w, *e))
            .collect())
    });
    let (outs, text) = run(&mut rec, &t.audio(4.2));
    assert_eq!(commits(&outs)[0], (3.85, "Hello there done ".to_string()));
    assert_eq!(text, "Hello there done.");
}
