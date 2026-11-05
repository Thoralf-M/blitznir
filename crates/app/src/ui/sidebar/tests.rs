use super::room::{CHATS_DRAGGED, CHATS_MIN, FIXED, RECENT_SHARE, cap_for, caps};

#[test]
fn the_chats_keep_their_room_and_the_recent_lists_scroll() {
    let tall = caps(1000.0, 2, true, None);
    assert_eq!(tall, 1000.0 * RECENT_SHARE, "a tall window: their share");
    assert!(
        1000.0 - FIXED - 2.0 * tall >= CHATS_MIN,
        "and the chats keep room"
    );
    assert_eq!(caps(300.0, 2, true, None), 0.0, "too short: headings only");
    assert!(
        caps(1000.0, 1, false, None) > tall,
        "chats folded: theirs to share"
    );
    assert!(
        2.0 * caps(300.0, 2, false, None) + FIXED <= 300.0,
        "folded: all fits"
    );
}

#[test]
fn a_dragged_edge_sets_the_share_and_leaves_the_chats_two_rows() {
    assert_eq!(caps(1000.0, 1, true, Some(0.1)), 100.0, "as dragged");
    assert_eq!(
        caps(1000.0, 1, true, Some(0.001)),
        1.0,
        "down to the headings"
    );
    let most = caps(1000.0, 1, true, Some(1.0));
    assert_eq!(
        most,
        1000.0 - FIXED - CHATS_DRAGGED,
        "up to the chats' two rows"
    );
    assert_eq!(
        caps(1000.0, 2, true, Some(1.0)),
        most / 2.0,
        "shared by two"
    );
}

#[test]
fn folded_chats_give_history_all_the_room_even_after_a_drag() {
    assert_eq!(
        caps(1000.0, 1, false, Some(0.22)),
        caps(1000.0, 1, false, None)
    );
}

#[test]
fn the_edge_follows_the_pointer_past_a_short_list() {
    let close = |a: f32, b: f32| (a - b).abs() < 0.01;
    // two lists at a 200 cap: each moves half the way
    assert!(close(cap_for(&[200.0, 200.0], 200.0, 60.0), 230.0));
    // one short list (50): only the full one moves, until the cap reaches it
    assert!(close(cap_for(&[200.0, 50.0], 200.0, 60.0), 260.0));
    assert!(close(cap_for(&[200.0, 50.0], 200.0, -100.0), 100.0));
    // 140 down: the full one to 60, both lists then 60 + 50 = 250 - 140
    assert!(close(cap_for(&[200.0, 50.0], 200.0, -140.0), 60.0));
    // a short list alone: up starts from the cap, not its height
    assert!(close(cap_for(&[84.0], 200.0, 10.0), 210.0));
}
