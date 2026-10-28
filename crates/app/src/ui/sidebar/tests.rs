use super::room::{CHATS_MIN, FIXED, RECENT_SHARE};
use super::*;

#[test]
fn the_chats_keep_their_room_and_the_recent_lists_scroll() {
    let tall = caps(1000.0, 2, true);
    assert_eq!(tall, 1000.0 * RECENT_SHARE, "a tall window: their share");
    assert!(
        1000.0 - FIXED - 2.0 * tall >= CHATS_MIN,
        "and the chats keep room"
    );
    assert_eq!(caps(300.0, 2, true), 0.0, "too short: headings only");
    assert!(
        caps(1000.0, 1, false) > tall,
        "chats folded: theirs to share"
    );
    assert!(
        2.0 * caps(300.0, 2, false) + FIXED <= 300.0,
        "folded: all fits"
    );
}
