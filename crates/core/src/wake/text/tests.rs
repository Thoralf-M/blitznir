use super::{command_tail, drop_last_sentence, is_command};

const SEND: &str = "blitz send";
const SCRATCH: &str = "blitz scratch, blitz delete";

#[test]
fn a_command_is_a_whole_utterance_as_an_accent_leaves_it() {
    // as the speech model wrote a German speaker's takes
    for t in [
        "Blitz send.",
        "blitz send",
        "Blitz, send.",
        "Blitz... send.",
        "Blitz Ascend.",
        "Blitzsend.",
    ] {
        assert!(is_command(t, SEND), "{t}");
    }
    for t in [
        "Blitz Ascent.",
        "Blitz sind.",
        "Blitz, delete.",
        "Blitz scrutch.",
    ] {
        assert!(is_command(t, SEND) || is_command(t, SCRATCH), "{t}");
    }
    assert!(!is_command("anything", ""));
}

#[test]
fn talk_is_no_command() {
    // each pressed Return or cut text when commands were searched for in sentences
    for t in [
        "Blitz ist Quatsch.",
        "Hi. Blitz send.",
        "Please send.",
        "Bis zum Ende.",
        "Blitz am Ende.",
        "Dr. Blitz sent it.",
        "Blitz, sit.",
        "Split send.",
        "Blitzend.",
        "Blitz end.",
        "Blitz Ende.",
        "Das Ende kam wie ein Blitz. Ende.",
        "Blitz and.",
        "Blitzsender.",
    ] {
        assert!(!is_command(t, SEND), "{t}");
    }
    for t in [
        "It's on the scratch pad.",
        "Blitz scratched.",
        "Blitz scratch pad.",
        "Scratch that.",
    ] {
        assert!(!is_command(t, SCRATCH), "{t}");
    }
}

#[test]
fn a_command_ending_a_dictation_comes_off_as_said() {
    let live = "Meet on Tuesday. Blitz, scratch.";
    assert_eq!(
        command_tail(live, SCRATCH).as_deref(),
        Some("blitz scratch")
    );
    assert_eq!(
        command_tail("Meet on Tuesday. Blitz delete", SCRATCH).as_deref(),
        Some("blitz delete")
    );
    assert_eq!(
        command_tail("We rewrote Blitz from scratch.", SCRATCH),
        None
    );
    assert_eq!(command_tail("send", SEND), None);
    assert_eq!(
        drop_last_sentence("Hello Anna. Meet on Tuesday."),
        Some("Hello Anna.".into())
    );
}

#[test]
fn the_wake_phrase_comes_off_as_the_transcript_heard_it() {
    use super::strip_head;
    let head = super::said_prefix("Hey please, write a note.", "hey blitz").unwrap();
    assert_eq!(head, "hey please");
    let both = format!("hey blitz, {head}");
    assert_eq!(
        strip_head("Hey please, write a note.", &both),
        "Write a note."
    );
    assert_eq!(
        command_tail("Write a note. Blitzsend", SEND).as_deref(),
        Some("blitzsend")
    );
}

#[test]
fn a_command_the_final_pass_wrote_otherwise_still_comes_off() {
    use super::strip_from;
    let t = "A note. Normal sentence. Blitzdelete delete.";
    assert_eq!(strip_from(t, "blitz"), "A note. Normal sentence.");
    assert_eq!(strip_from("A note. Plitz Scratch.", "blitz"), "A note.");
    let far = "Blitz is fast. A long sentence goes on here.";
    assert_eq!(strip_from(far, "blitz"), far);
}

#[test]
fn only_a_command_said_as_its_own_sentence_ends_a_dictation() {
    for t in [
        "To send it, say blitz send.",
        "Then I typed blitz send",
        "I told him, blitz send.",
        "Ihre Augen waren blitzend.",
    ] {
        assert_eq!(command_tail(t, SEND), None, "{t}");
    }
    assert_eq!(command_tail("The file was blitz deleted.", SCRATCH), None);
    use super::strip_head;
    for t in [
        "Please find the file attached.",
        "Plus, we need milk.",
        "Built it yesterday.",
        "He played football.",
        "Help, let's go.",
    ] {
        assert_eq!(strip_head(t, "hey blitz"), t);
    }
}

#[test]
fn a_take_back_stops_at_a_line_and_never_takes_a_whole_field() {
    let mail = "Thanks for the gift.\n\nBest regards\nThomas";
    assert_eq!(
        drop_last_sentence(mail).as_deref(),
        Some("Thanks for the gift.\n\nBest regards\n")
    );
    assert_eq!(drop_last_sentence(&"word ".repeat(80)), None);
    assert_eq!(
        drop_last_sentence("一句。二句。").as_deref(),
        Some("一句。")
    );
}

#[test]
fn the_wake_phrase_comes_off_as_the_dictation_wrote_it() {
    use super::strip_head;
    let w = "hey blitz";
    assert_eq!(
        strip_head("Okay Blitz, let's test it.", w),
        "Let's test it."
    );
    for t in [
        "He played football.",
        "Okay, let's go.",
        "Hey, let's meet tomorrow.",
        "Hey Anna, call me.",
        "Der Blitz hat eingeschlagen.",
    ] {
        assert_eq!(strip_head(t, w), t);
    }
}

#[test]
fn what_follows_the_wake_phrase_starts_a_sentence() {
    use super::strip_head;
    let w = "hey blitz";
    assert_eq!(
        strip_head("blitz, give me three ideas.", w),
        "Give me three ideas."
    );
    assert_eq!(strip_head("Hey Blitz, iPhone notes.", w), "iPhone notes.");
    assert_eq!(
        strip_head("give me three ideas.", w),
        "Give me three ideas."
    );
}
