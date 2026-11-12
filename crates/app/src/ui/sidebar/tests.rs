use super::room::{CHATS_DRAGGED, CHATS_MIN, FIXED, RECENT_SHARE, cap_for, caps};

#[test]
fn the_chats_keep_their_room_and_the_recent_lists_scroll() {
    let tall = caps(1000.0, 2, Some(10), None, &[]);
    assert_eq!(tall, 1000.0 * RECENT_SHARE, "a tall window: their share");
    assert!(
        1000.0 - FIXED - 2.0 * tall >= CHATS_MIN,
        "and the chats keep room"
    );
    assert_eq!(
        caps(250.0, 2, Some(10), None, &[]),
        0.0,
        "too short: headings only"
    );
    assert!(
        caps(1000.0, 1, None, None, &[]) > tall,
        "chats folded: theirs to share"
    );
    assert!(
        2.0 * caps(300.0, 2, None, None, &[]) + FIXED <= 300.0,
        "folded: all fits"
    );
}

#[test]
fn a_dragged_edge_sets_the_share_and_leaves_the_chats_two_rows() {
    assert_eq!(
        caps(1000.0, 1, Some(10), Some(0.1), &[]),
        100.0,
        "as dragged"
    );
    assert_eq!(
        caps(1000.0, 1, Some(10), Some(0.001), &[]),
        1.0,
        "down to the headings"
    );
    let most = caps(1000.0, 1, Some(10), Some(1.0), &[]);
    assert_eq!(
        most,
        1000.0 - FIXED - CHATS_DRAGGED,
        "up to the chats' two rows"
    );
    assert_eq!(
        caps(1000.0, 2, Some(10), Some(1.0), &[]),
        most / 2.0,
        "shared by two"
    );
}

#[test]
fn folded_chats_give_history_all_the_room_even_after_a_drag() {
    assert_eq!(
        caps(1000.0, 1, None, Some(0.22), &[]),
        caps(1000.0, 1, None, None, &[])
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

#[test]
fn a_short_list_leaves_its_room_to_the_other() {
    // 1000 pt, chats dragged to their two rows
    let room = 1000.0 - FIXED - CHATS_DRAGGED;
    let even = caps(1000.0, 2, Some(10), Some(1.0), &[]);
    assert_eq!(even, room / 2.0, "half each");
    let uneven = caps(1000.0, 2, Some(10), Some(1.0), &[40.0]);
    assert_eq!(
        uneven,
        room - 40.0,
        "one row of 40 pt: the rest for the other"
    );
    let both = caps(1000.0, 2, Some(10), Some(1.0), &[500.0]);
    assert_eq!(both, even, "taller than its half: not short after all");
    assert_eq!(
        caps(1000.0, 2, Some(10), Some(0.2), &[40.0]),
        200.0,
        "as dragged"
    );
}

#[test]
fn one_chat_keeps_one_row_under_a_drag() {
    let two = caps(1000.0, 1, Some(10), Some(1.0), &[]);
    let one = caps(1000.0, 1, Some(1), Some(1.0), &[]);
    assert_eq!(one - two, CHATS_DRAGGED / 2.0, "a row more for History");
}
