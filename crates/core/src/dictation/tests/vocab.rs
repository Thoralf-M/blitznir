//! The vocabulary's written forms while typing, the rewrite's hold, and a
//! spoken stop phrase taken off.

use super::*;

#[test]
fn live_typing_already_shows_the_written_forms() {
    let e = env();
    let chunks = Arc::new(Mutex::new(vec![]));
    let c = chunks.clone();
    e.platform.on_type(move |typed| {
        c.lock().unwrap().push(typed.to_string());
        Ok(())
    });
    let snapshots = Arc::new(Mutex::new(vec![]));
    let (path, t) = (e.paths.recording(), speech().0);
    let s = snapshots.clone();
    let mut rec = FakeRecognizer::new(move |pcm| {
        s.lock()
            .unwrap()
            .push(std::fs::read_to_string(&path).unwrap_or_default());
        Ok(t.tokens(pcm))
    });
    let o = Opts {
        vocab: vocab(&[("my name", "MYNAME"), ("berlin", "BERLIN")]),
        ..opts(true)
    };
    let text = session(&e, &mut rec, speech().1, &o);
    let want = "Hello MYNAME is Anna. I live in BERLIN.";
    assert_eq!(text, want);
    assert_eq!(e.platform.screen(), want);
    assert_eq!(e.store.history()[0].text, want);
    let chunks = chunks.lock().unwrap();
    let at = |w: &str| chunks.iter().position(|c| c.contains(w)).unwrap();
    assert!(at("MYNAME") < at("BERLIN"), "typed live: {chunks:?}");
    let on_disk = snapshots.lock().unwrap();
    assert!(
        on_disk.iter().any(|s| s == "Hello MYNAME is Anna "),
        "{on_disk:?}"
    );
}

#[test]
fn a_replacement_that_joins_the_last_words_is_still_typed() {
    // the recording stops right after "Berlin": never stable, so the screen
    // holds "…I live in" until the end, one word more than the written text
    let e = env();
    let t = speech().0;
    let pcm = t.audio(7.3);
    let o = Opts {
        vocab: vocab(&[("i live in berlin", "ILIB")]),
        ..opts(true)
    };
    let text = session(&e, &mut t.recognizer(), pcm, &o);
    assert_eq!(text, "Hello my name is Anna. ILIB.");
    assert_eq!(e.platform.screen(), text);
    assert_eq!(e.store.history()[0].text, text);
}

#[test]
fn a_rewrite_prompt_holds_batch_text_back_and_never_live() {
    let mut cfg = crate::config::Config::default();
    assert!(!Opts::new(false, &cfg).hold, "no prompt: batch types");
    cfg.dictation.rewrite = "Tidy up: {text}".into();
    cfg.dictation.rewrite_auto = true;
    assert!(Opts::new(false, &cfg).hold, "batch: the rewrite types");
    assert!(!Opts::new(true, &cfg).hold, "live types as it goes");
    cfg.dictation.rewrite = "  ".into();
    assert!(!Opts::new(false, &cfg).hold, "blank is off");
}

#[test]
fn a_stop_phrase_said_last_is_taken_off_the_screen_and_the_history() {
    let e = env();
    let o = Opts {
        stop: Arc::new(Mutex::new(("in berlin".into(), false))),
        ..opts(true)
    };
    let text = session(&e, &mut speech().0.recognizer(), speech().1, &o);
    // "Hello my name is Anna. I live in Berlin." ended by saying "in Berlin"
    assert_eq!(text, "Hello my name is Anna. I live");
    assert_eq!(e.platform.screen(), text, "typed live, then taken back");
    assert_eq!(e.store.history()[0].text, text);
}
