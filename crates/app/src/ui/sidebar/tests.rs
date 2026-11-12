use super::room::{CHATS_MIN, FIXED, RECENT_SHARE, caps, chrome, shared};

#[test]
fn the_chats_keep_their_room_and_the_recent_lists_scroll() {
    let tall = caps(1000.0, 2, true, &[]);
    assert_eq!(tall, 1000.0 * RECENT_SHARE, "a tall window: their share");
    assert!(
        1000.0 - FIXED - 2.0 * tall >= CHATS_MIN,
        "and the chats keep room"
    );
    assert_eq!(caps(250.0, 2, true, &[]), 0.0, "too short: headings only");
    assert!(
        caps(1000.0, 1, false, &[]) > tall,
        "chats folded: theirs to share"
    );
    assert!(
        2.0 * caps(300.0, 2, false, &[]) + FIXED <= 300.0,
        "folded: all fits"
    );
}

#[test]
fn a_short_list_leaves_its_room_to_the_other() {
    assert_eq!(shared(600.0, 2, &[]), 300.0, "half each");
    assert_eq!(
        shared(600.0, 2, &[40.0]),
        560.0,
        "one row of 40 pt: the rest for the other"
    );
    assert_eq!(
        shared(600.0, 2, &[400.0]),
        300.0,
        "taller than its half: not short after all"
    );
    assert_eq!(shared(600.0, 1, &[40.0]), 600.0, "alone: all of it");
}

#[test]
fn a_list_folded_since_is_not_counted_as_headings() {
    // last frame: 60 pt of headings, Dictations 200 and Replacements 300
    let laid = [200.0, 300.0];
    // both showed then, whatever shows now (Dictations folded since)
    assert_eq!(chrome(560.0, laid, [true, true]), 60.0);
    assert_eq!(
        chrome(360.0, laid, [false, true]),
        60.0,
        "it did not show then"
    );
}
