use super::*;

fn shot(id: u64) -> Shot {
    Shot {
        id,
        url: String::new(),
        png: Bytes::new(),
    }
}

fn row(model: &str, vision: bool) -> ModelRow {
    ModelRow {
        endpoint: "LM Studio".into(),
        model: model.into(),
        label: format!("{model} · LM Studio"),
        error: false,
        vision,
    }
}

fn text(ui: &Ui, now: Instant) -> Option<(String, Tone)> {
    strip_for(ui, now).map(|s| (s.text, s.tone))
}

#[test]
fn the_strip_says_only_what_is_going_on_and_takes_no_room_otherwise() {
    let mut ui = Ui {
        shown: true,
        selection: Some("Termin am Montag".into()),
        models: vec![row("seeing", true), row("blind", false)],
        picked: ("LM Studio".into(), "seeing".into()),
        ..Ui::default()
    };
    let now = Instant::now();
    assert_eq!(text(&ui, now), None, "idle: nothing");
    ui.images = vec![shot(1), shot(2)];
    assert_eq!(text(&ui, now), None, "the thumbnails say it");

    ui.picked.1 = "blind".into();
    let warn = Some(("The picked model may not accept images".into(), Tone::Warn));
    assert_eq!(text(&ui, now), warn);
    ui.settings = true;
    assert_eq!(text(&ui, now), None, "no composer, no images to warn about");
    ui.settings = false;

    ui.status.set("Copied");
    assert_eq!(
        text(&ui, Instant::now()),
        Some(("Copied".into(), Tone::Quiet))
    );
    let later = Instant::now() + NOTE_FOR;
    assert_eq!(text(&ui, later), warn, "a confirmation fades");
    assert!(ui.status.stale_in(Instant::now()).is_some());

    ui.busy = Some(Busy::Chat);
    ui.doing = "Thinking… · Esc cancels".into();
    let doing = Some(("Thinking… · Esc cancels".into(), Tone::Quiet));
    assert_eq!(text(&ui, later), doing, "a running job keeps the strip");

    ui.status.fail("Chat failed: connection refused");
    let failed = Some(("Chat failed: connection refused".into(), Tone::Error));
    assert_eq!(text(&ui, later), failed, "an error stands");
    assert_eq!(ui.status.stale_in(later), None, "and no timer takes it");
    ui.busy = None;
    assert_eq!(text(&ui, later + NOTE_FOR * 10), failed);
    ui.status.clear();
    assert_eq!(text(&ui, later), warn, "dismissed");

    ui.images.clear();
    ui.selection = None;
    let copying = Some(("Copying the selection…".into(), Tone::Quiet));
    assert_eq!(text(&ui, later), copying);
    ui.shown = false;
    assert_eq!(text(&ui, later), None, "a hidden panel copies nothing");
}

#[test]
fn a_recent_row_is_read_until_something_is_asked() {
    let mut ui = Ui {
        chatting: true,
        ..Ui::default()
    };
    ui.about = Some(HistoryItem {
        text: "Termin am Montag".into(),
        mode: "batch".into(),
        ts: "2020-01-02T09:30:00".into(),
        original: None,
        action: None,
        prompt: None,
        cancelled: false,
        failed: None,
    });
    assert!(viewing(&ui));
    assert_eq!(ui.about.as_ref().unwrap().what(), "Dictation");
    ui.messages.push(Msg {
        role: "user".into(),
        text: "wann?".into(),
        ts: String::new(),
        images: vec![],
        thinking: String::new(),
        unthought: blitznir_core::store::Unthought::No,
        unanswered: None,
    });
    assert!(!viewing(&ui), "asked: a chat like any other");
}
