use super::*;

fn toks(p: &str) -> String {
    tokens(p).expect("tokens").join(" ")
}

#[test]
fn phrases_become_the_models_tokens_as_sentencepiece_cuts_them() {
    // each as SentencePiece itself encoded it with the model's bpe.model
    assert_eq!(toks("hey blitz"), "▁HE Y ▁B LI T Z");
    assert_eq!(toks("Blitz, send!"), "▁B LI T Z ▁S END");
    assert_eq!(toks("all done"), "▁ALL ▁DON E");
    assert_eq!(toks("hallo blitz"), "▁HA LL O ▁B LI T Z");
    assert_eq!(toks("computer"), "▁COMP U TER");
    assert_eq!(toks("send it"), "▁S END ▁IT");
    assert_eq!(toks("hello world"), "▁HE LL O ▁WORLD");
    assert_eq!(toks("merry christmas"), "▁ME R RY ▁ CH R IST MA S");
    assert_eq!(toks("Grüße"), toks("gruesse"), "umlauts written out");
    assert_eq!(tokens("!?"), None, "no letters");
}

#[test]
fn the_keyword_list_has_each_phrase_and_alternative_with_its_label() {
    let w = Wake {
        stop: "blitz stop, all done".into(),
        ..Wake::default()
    };
    let list = keywords(&w).unwrap();
    let lines: Vec<&str> = list.lines().collect();
    // each phrase twice: as two words, and run together as said fast
    assert_eq!(lines.len(), 6, "{list}");
    assert!(lines[0].ends_with("@WAKE") && lines[0].starts_with("▁HE Y ▁B LI T Z :"));
    assert!(lines[1].ends_with("@WAKE") && lines[1].starts_with("▁HE Y B LI T Z :"));
    assert!(lines[2..].iter().all(|l| l.ends_with("@STOP")));
    assert!(
        !list.contains("@SEND"),
        "send is the transcript's, not the spotter's"
    );
    assert_eq!(Kind::of("SEND"), Some(Kind::Send));
    let bad = Wake {
        wake: "🙂".into(),
        ..Wake::default()
    };
    assert!(keywords(&bad).is_err());
}

#[test]
fn a_stop_phrase_at_the_end_is_taken_off_the_text() {
    let stop = "all done";
    assert_eq!(
        strip_tail("Hello Anna, see you at five. All done.", stop),
        "Hello Anna, see you at five."
    );
    assert_eq!(strip_tail("Hello Anna, all done", stop), "Hello Anna");
    assert_eq!(
        strip_tail("Hello Anna, all dun.", stop),
        "Hello Anna",
        "close enough"
    );
    assert_eq!(
        strip_tail("We are all done with it", stop),
        "We are all done with it",
        "only at the end"
    );
    assert_eq!(strip_tail("Hello", stop), "Hello");
    assert_eq!(strip_tail("All done", stop), "");
    assert_eq!(
        strip_tail("Fertig, done", "fertig, done"),
        "Fertig",
        "alternatives"
    );
}

fn wake() -> Wake {
    Wake {
        quiet_s: 3.0,
        send_within_s: 120.0,
        ..Wake::default()
    }
}

#[test]
fn the_wake_phrase_starts_and_the_stop_phrase_or_quiet_ends_a_dictation() {
    let t = Instant::now();
    let at = |ms: u64| t + Duration::from_millis(ms);
    let mut l = Listener::new(&wake());
    assert_eq!(l.hit(Kind::Stop, at(0)), None, "nothing to stop");
    assert_eq!(l.hit(Kind::Wake, at(0)), Some(Act::Start));
    l.started(true, at(0));
    assert_eq!(l.hit(Kind::Wake, at(100)), None, "already dictating");
    assert_eq!(l.block(true, at(500)), None);
    assert_eq!(l.block(false, at(1000)), None);
    assert_eq!(l.block(false, at(3900)), None, "under 3 s quiet");
    assert_eq!(
        l.block(false, at(4000)),
        Some(Act::Stop { said: false }),
        "3 s quiet"
    );
    // before the first word it waits twice as long
    let mut l = Listener::new(&wake());
    l.started(true, at(0));
    assert_eq!(l.block(false, at(0)), None);
    assert_eq!(l.block(false, at(5000)), None);
    assert_eq!(l.block(false, at(6000)), Some(Act::Stop { said: false }));
    // a hotkey dictation never ends on a pause, only the stop phrase
    let mut l = Listener::new(&wake());
    l.started(false, at(0));
    assert_eq!(l.block(false, at(0)), None);
    assert_eq!(l.block(false, at(60_000)), None);
    assert_eq!(
        l.hit(Kind::Stop, at(60_000)),
        Some(Act::Stop { said: true })
    );
}

#[test]
fn the_send_phrase_sends_once_shortly_after_a_dictation() {
    let t = Instant::now();
    let at = |ms: u64| t + Duration::from_millis(ms);
    let to = Focus(Some(7));
    let mut l = Listener::new(&wake());
    assert_eq!(l.hit(Kind::Send, at(0)), None, "no dictation yet");
    l.started(true, at(0));
    assert_eq!(l.hit(Kind::Send, at(100)), None, "while dictating");
    l.ended(to, true, at(0));
    assert_eq!(l.hit(Kind::Send, at(5000)), Some(Act::Send(to)));
    assert_eq!(l.hit(Kind::Send, at(6000)), None, "once");
    // too late after the dictation
    let mut l = Listener::new(&wake());
    l.ended(to, true, at(0));
    assert_eq!(l.block(false, at(121_000)), None);
    assert_eq!(l.hit(Kind::Send, at(121_000)), None);
    // Linux names no app: still armed
    let mut l = Listener::new(&wake());
    l.ended(Focus(None), true, at(0));
    assert_eq!(l.hit(Kind::Send, at(1000)), Some(Act::Send(Focus(None))));
}

#[test]
fn the_transcript_confirms_the_wake_phrase_or_not() {
    let w = "hey blitz";
    assert!(said("Hey Blitz, write a short note", w));
    assert!(said("okay. hey, blitz!", w));
    assert!(said("Heyblitz write", w), "joined into one word");
    assert!(said("hey blits", w), "close enough");
    // a sentence with its telling word passes the check: the spotter must take it first
    assert!(said("We played blitz and then I hit send", w));
    assert!(!said("Lift is a fast form of chess", w));
    assert!(!said("", w));
    assert!(said("blitz sent", "blitz send, blitz sent"));
}

#[test]
fn the_wake_phrases_end_is_taken_off_the_start() {
    let w = "hey blitz";
    assert_eq!(
        strip_head("Blitz, write a short note.", w),
        "Write a short note."
    );
    assert_eq!(
        strip_head("Hey Blitz, write a short note.", w),
        "Write a short note."
    );
    assert_eq!(strip_head("blitz write it", w), "Write it");
    assert_eq!(strip_head("Blitz.", w), "");
    assert_eq!(
        strip_head("Write a short note.", w),
        "Write a short note.",
        "nothing to take"
    );
    assert_eq!(strip_head("Bli", w), "Bli", "not yet a word of it");
}

#[test]
fn a_send_is_armed_only_when_asked_and_talk_never_keeps_a_dictation_forever() {
    let t = Instant::now();
    let at = |ms: u64| t + Duration::from_millis(ms);
    let mut l = Listener::new(&wake());
    l.started(false, at(0));
    l.ended(Focus(Some(7)), false, at(0));
    assert_eq!(
        l.hit(Kind::Send, at(1000)),
        None,
        "not armed: a hotkey dictation"
    );
    assert_eq!(l.block(false, at(2000)), None);
    // words keep coming (a TV): two minutes end it all the same
    l.started(true, at(0));
    assert_eq!(l.block(true, at(119_000)), None);
    assert_eq!(l.block(true, at(120_000)), Some(Act::Stop { said: false }));
    // a session that never started leaves the wake phrase working
    let mut l = Listener::new(&wake());
    l.started(true, at(0));
    assert_eq!(l.hit(Kind::Wake, at(10)), None);
    l.idle();
    assert_eq!(l.hit(Kind::Wake, at(20)), Some(Act::Start));
}

#[test]
fn an_accent_the_model_spells_otherwise_still_counts_by_sound() {
    let w = "hey blitz";
    assert!(
        said("Hej plut!", w),
        "as the speech model spelt a real speaker"
    );
    assert!(said("Hey, plitz.", w));
    // a sentence with its telling word passes the check: the spotter must take it first
    assert!(said("We played blitz and then I hit send", w));
    assert!(!said("Lift is a fast form of chess", w));
    assert!(!said("Help us", w));
}

#[test]
fn the_live_text_ends_with_the_stop_phrase_all_but_exactly() {
    let stop = "all done";
    assert!(said_last("Please bring the slides. All done.", stop));
    assert!(said_last("all done", stop));
    assert!(
        !said_last("Great work, well done.", stop),
        "two letters off"
    );
    assert!(!said_last("All done with the slides.", stop), "not last");
    assert!(!said_last("done", stop));
}

#[test]
fn cyrillic_transcripts_and_the_last_sentence() {
    assert!(
        said("Хей, плют.", "hey blitz"),
        "as the model wrote a German speaker"
    );
    assert_eq!(
        drop_last_sentence("Hello Anna. Meet on Tuesday."),
        Some("Hello Anna.".into())
    );
    assert_eq!(drop_last_sentence("Meet on Tuesday"), Some(String::new()));
}
