use super::*;

#[test]
fn chunks_never_split_a_surrogate_pair() {
    let s = "Grüße, Straße! 𝄞 done";
    let c = utf16_chunks(s);
    assert_eq!(c[0].len(), 15, "𝄞 starts at unit 15");
    assert_eq!(String::from_utf16(&c.concat()).unwrap(), s);
    assert!(
        c.iter()
            .all(|x| x.len() <= 16 && String::from_utf16(x).is_ok())
    );
}

#[test]
fn our_mark_round_trips() {
    let (ev, field) = (
        CGEvent::new_keyboard_event(None, 0, true).unwrap(),
        CGEventField::EventSourceUserData,
    );
    CGEvent::set_integer_value_field(Some(&ev), field, MARK);
    assert_eq!(CGEvent::integer_value_field(Some(&ev), field), MARK);
}
