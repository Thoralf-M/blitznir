use super::*;
use std::os::unix::fs::PermissionsExt;

pub(super) struct Tmp(pub(super) Store, pub(super) Paths);

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.1.dir);
    }
}

pub(super) fn tmp(history_max: usize) -> Tmp {
    let paths = Paths::new(
        std::env::temp_dir().join(format!("blitznir-store-{}", uuid::Uuid::new_v4().simple())),
    );
    Tmp(Store::new(paths.clone(), history_max).unwrap(), paths)
}

/// What made a row of `mode`, nothing more.
pub(super) fn made(mode: &str) -> Made {
    Made {
        mode: mode.into(),
        ..Made::default()
    }
}

pub(super) fn texts(s: &Store) -> Vec<String> {
    s.history().into_iter().map(|i| i.text).collect()
}

fn msg(role: &str, content: &str, images: Vec<String>) -> Message {
    Message {
        role: role.into(),
        content: content.into(),
        ts: now_stamp(),
        images,
        thinking: String::new(),
        unthought: Unthought::No,
        unanswered: None,
    }
}

fn chat(s: &Store, text: &str) -> Chat {
    let mut c = Chat::new();
    c.messages.push(msg("user", text, vec![]));
    s.save_chat(&mut c).unwrap();
    c
}

#[test]
fn an_unfinished_recording_is_recovered_once() {
    let t = tmp(4);
    t.0.write_recording("Hello from before the crash ").unwrap();
    assert_eq!(
        t.0.recover_recording().unwrap().as_deref(),
        Some("Hello from before the crash")
    );
    assert_eq!(t.0.history()[0].mode, "recovered");
    assert_eq!(std::fs::read_to_string(t.1.recording()).unwrap(), "");
    assert_eq!(t.0.recover_recording().unwrap(), None);
    assert_eq!(t.0.history().len(), 1);
}

#[test]
fn chats_are_newest_first_and_capped() {
    let t = tmp(4);
    let first = chat(&t.0, "first");
    let ids: Vec<String> = (0..CHATS_MAX + 2)
        .map(|i| chat(&t.0, &format!("c{i}")).id)
        .collect();
    let chats = t.0.chats();
    assert_eq!(chats.len(), CHATS_MAX);
    assert_eq!(chats[0].id, ids[ids.len() - 1]);
    assert!(t.0.chat(&first.id).is_none(), "the oldest went");
    // updating an old chat moves it to the front
    let mut old = t.0.chat(&ids[5]).unwrap();
    old.messages.push(msg("assistant", "hi", vec![]));
    t.0.save_chat(&mut old).unwrap();
    assert_eq!(t.0.chats()[0].id, ids[5]);
    assert_eq!(t.0.chats().len(), CHATS_MAX);
}

#[test]
fn an_empty_chat_is_not_stored_and_delete_removes_images() {
    let t = tmp(4);
    t.0.save_chat(&mut Chat::new()).unwrap();
    assert!(t.0.chats().is_empty());
    let name = Store::image_name();
    t.0.put_image(&name, "data:image/png;base64,AAAA").unwrap();
    assert_eq!(t.0.image(&name).unwrap(), "data:image/png;base64,AAAA");
    let mut c = Chat::new();
    c.messages.push(msg("user", "see", vec![name.clone()]));
    t.0.save_chat(&mut c).unwrap();
    t.0.delete_chat(&c.id).unwrap();
    assert!(t.0.chats().is_empty());
    assert!(!t.1.images().join(&name).exists());
}

fn roles(s: &Store, id: &str) -> Vec<String> {
    let chat = s.chat(id).unwrap_or_default();
    chat.messages.into_iter().map(|m| m.role).collect()
}

#[test]
fn a_resend_after_a_failed_turn_keeps_the_roles_alternating() {
    let t = tmp(4);
    let mut c = chat(&t.0, "first");
    c.messages.push(msg("assistant", "an answer", vec![]));
    let image = Store::image_name();
    t.0.put_image(&image, "data:image/png;base64,AA").unwrap();
    c.messages
        .push(msg("user", "this one fails", vec![image.clone()]));
    t.0.save_chat(&mut c).unwrap();
    t.0.unask(&mut c).unwrap();
    assert!(
        !t.1.images().join(&image).exists(),
        "its image went with it"
    );
    c.messages
        .push(msg("user", "this one fails, sent again", vec![]));
    t.0.save_chat(&mut c).unwrap();
    assert_eq!(roles(&t.0, &c.id), ["user", "assistant", "user"]);
    // an answered chat keeps its turns; a first turn takes the chat with it
    c.messages.push(msg("assistant", "answered", vec![]));
    t.0.save_chat(&mut c).unwrap();
    t.0.unask(&mut c).unwrap();
    assert_eq!(roles(&t.0, &c.id).len(), 4);
    let mut lone = chat(&t.0, "only this");
    t.0.unask(&mut lone).unwrap();
    assert!(t.0.chat(&lone.id).is_none());
}

#[test]
fn writes_are_atomic_and_private() {
    let t = tmp(4);
    let path = t.1.dir.join("x.json");
    std::fs::write(t.1.dir.join("x.json.tmp"), "stale half-written file").unwrap();
    write_atomic(&path, b"first").unwrap();
    write_atomic(&path, b"second").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "second");
    assert!(!t.1.dir.join("x.json.tmp").exists());
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn unreadable_files_read_as_empty_and_are_moved_aside() {
    let t = tmp(4);
    std::fs::write(t.1.history(), "{not json").unwrap();
    assert!(t.0.history().is_empty());
    t.0.add_history("after", "live").unwrap();
    assert_eq!(texts(&t.0), ["after"]);
    let bad = t.1.dir.join("history.json.bad");
    assert_eq!(std::fs::read_to_string(bad).unwrap(), "{not json");
    assert_eq!(t.0.state(), State::default());
    t.0.update_state(|s| s.folded = true).unwrap();
    t.0.update_state(|s| s.model = "gemma".into()).unwrap();
    t.0.update_state(|s| s.closed = vec!["dictations".into()])
        .unwrap();
    let s = State {
        endpoint: String::new(),
        model: "gemma".into(),
        folded: true, // one part changed, the others kept
        side_width: 0.0,
        zoom: 0.0,
        closed: vec!["dictations".into()],
        history_open: false, // folded unless asked for
    };
    assert_eq!(t.0.state(), s);
}

#[test]
fn zoom_steps_like_a_browser_and_stays_in_range() {
    assert_eq!(State::default().zoom(), 1.0);
    assert_eq!(State::zoom_step(1.0, true), 1.1);
    assert_eq!(State::zoom_step(1.0, false), 0.9);
    assert_eq!(
        State::zoom_step(1.05, true),
        1.1,
        "between steps: the next one"
    );
    assert_eq!(State::zoom_step(2.0, true), 2.0);
    assert_eq!(State::zoom_step(0.8, false), 0.8);
    let s = State {
        zoom: 9.0,
        ..State::default()
    };
    assert_eq!(s.zoom(), 2.0);
}

#[test]
fn the_sidebar_width_stays_in_range() {
    let w = |side_width| State {
        side_width,
        ..State::default()
    };
    assert_eq!(w(0.0).side_width(), State::SIDE_DEFAULT);
    assert_eq!(w(90.0).side_width(), State::SIDE_MIN);
    assert_eq!(w(300.0).side_width(), 300.0);
    assert_eq!(w(9000.0).side_width(), State::SIDE_MAX);
}

#[test]
fn data_urls_are_standard_base64() {
    let b64 = |s: &str| {
        data_url("text/plain", s.as_bytes())["data:text/plain;base64,".len()..].to_string()
    };
    assert_eq!(b64(""), "");
    assert_eq!(b64("M"), "TQ==");
    assert_eq!(b64("Ma"), "TWE=");
    assert_eq!(b64("Man"), "TWFu");
    assert_eq!(b64("hello world"), "aGVsbG8gd29ybGQ=");
    assert_eq!(
        data_url("image/png", &[0xff, 0xfe, 0x00]),
        "data:image/png;base64,//4A"
    );
}

#[test]
fn a_data_url_decodes_to_its_bytes() {
    for bytes in [
        &b""[..],
        b"M",
        b"Ma",
        b"Man",
        b"hello world",
        &[0xff, 0xfe, 0x00],
    ] {
        let url = data_url("image/png", bytes);
        assert_eq!(data_url_bytes(&url).as_deref(), Some(bytes), "{url}");
    }
    assert_eq!(data_url_bytes("data:image/png;base64,a!b="), None);
    assert_eq!(data_url_bytes("https://example.com/x.png"), None);
}

#[test]
fn a_message_brings_its_images_back_as_data_urls() {
    let t = tmp(4);
    let name = Store::image_name();
    t.0.put_image(&name, "data:image/png;base64,AA").unwrap();
    let m = msg("user", "see", vec![name, "gone.url".into()]);
    assert_eq!(t.0.message_images(&m), ["data:image/png;base64,AA"]);
}

#[test]
fn chat_titles_are_the_first_six_words() {
    assert_eq!(
        chat_title("  what does this   code do, and why is it slow?"),
        "what does this code do, and"
    );
    assert_eq!(chat_title(""), "");
}

#[test]
fn a_chat_about_a_recent_row_takes_its_text_as_context() {
    let t = tmp(4);
    t.0.add_history("Termin am Montag um zehn", "batch")
        .unwrap();
    let item = t.0.history().remove(0);
    let mut c = Chat::about(item.clone());
    assert_eq!(c.context("the selection"), "Termin am Montag um zehn");
    assert_eq!(Chat::new().context("the selection"), "the selection");
    // a row without an answer: what it was asked on
    let asked = Made::new("rewrite", "Rewrite", Some("Shorter:\n{text}".into()));
    t.0.add_unanswered("the long one", &asked, None).unwrap();
    let cancelled = Chat::about(t.0.history().remove(0));
    assert_eq!(cancelled.context("the selection"), "the long one");
    let body = crate::tasks::chat_body(&Default::default(), "m", c.context(""), &[]);
    let system = body["messages"][0]["content"].as_str().unwrap();
    assert!(
        system.ends_with("TEXT:\nTermin am Montag um zehn"),
        "{system}"
    );
    // unsaved until the first message; then it keeps what it is about
    t.0.save_chat(&mut c).unwrap();
    assert!(t.0.chats().is_empty());
    c.messages.push(msg("user", "when?", vec![]));
    t.0.save_chat(&mut c).unwrap();
    assert_eq!(t.0.chat(&c.id).unwrap().about, Some(item));
}

#[test]
fn all_chats_go_at_once_with_their_images() {
    let t = tmp(4);
    let name = Store::image_name();
    t.0.put_image(&name, "data:image/png;base64,AAAA").unwrap();
    let mut c = chat(&t.0, "one");
    c.messages[0].images.push(name.clone());
    t.0.save_chat(&mut c).unwrap();
    chat(&t.0, "two");
    assert_eq!(t.0.clear_chats().unwrap(), 2);
    assert!(t.0.chats().is_empty());
    assert!(!t.1.images().join(&name).exists());
    assert_eq!(t.0.clear_chats().unwrap(), 0);
}

#[test]
fn an_answer_keeps_its_thinking_and_old_chats_still_load() {
    let t = tmp(4);
    let mut c = chat(&t.0, "why?");
    let mut a = msg("assistant", "Because.", vec![]);
    (a.thinking, a.unthought) = ("Wait, I'll check…".into(), Unthought::Looped);
    c.messages.push(a.clone());
    t.0.save_chat(&mut c).unwrap();
    assert_eq!(t.0.chat(&c.id).unwrap().messages[1], a);
    let file = std::fs::read_to_string(t.1.chats()).unwrap();
    assert_eq!(
        file.matches("thinking").count(),
        1,
        "none on the user's turn"
    );
    let old = r#"[{"id":"a1","name":"Old","created":"x","updated":"x",
        "messages":[{"role":"assistant","content":"Hi.","ts":"x"}]}]"#;
    std::fs::write(t.1.chats(), old).unwrap();
    let m = &t.0.chat("a1").expect("an old chat loads").messages[0];
    assert_eq!(
        (m.content.as_str(), m.thinking.as_str(), m.unthought),
        ("Hi.", "", Unthought::No)
    );
}

#[test]
fn an_unanswered_turn_stays_marked_and_is_not_sent() {
    let t = tmp(4);
    let mut c = chat(&t.0, "first?");
    c.messages[0].unanswered = Some(Unanswered::Cancelled);
    c.messages.push(msg("user", "second?", vec![]));
    c.messages[1].unanswered = Some(Unanswered::Failed("the model crashed".into()));
    c.messages.push(msg("user", "third?", vec![]));
    t.0.save_chat(&mut c).unwrap();
    let back = t.0.chat(&c.id).expect("kept, its first turn unanswered");
    assert_eq!(back.messages, c.messages);
    let says: Vec<String> = back
        .messages
        .iter()
        .filter_map(|m| m.unanswered.as_ref())
        .map(Unanswered::says)
        .collect();
    assert_eq!(says, ["Cancelled", "Failed: the model crashed"]);
    let sent: Vec<&str> = back.sent().map(|m| m.content.as_str()).collect();
    assert_eq!(sent, ["third?"]);
    assert!(t.0.history().is_empty(), "no history row");
}
