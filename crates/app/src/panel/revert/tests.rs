use super::*;

#[test]
fn the_box_sits_above_the_text_unless_the_menu_bar_is_there() {
    let line = Rect {
        x: 100.0,
        y: 400.0,
        w: 200.0,
        h: 18.0,
    };
    let f = frame(SMALL, Some(line), None, &[]).unwrap();
    assert_eq!((f.x, f.y + f.h + GAP), (100.0, 400.0), "just above");
    let first = Rect { y: 60.0, ..line };
    assert_eq!(
        frame(SMALL, Some(first), None, &[]).unwrap().y,
        60.0 + 18.0 + GAP,
        "below"
    );
    assert!(
        frame(SMALL, None, Some((5.0, 500.0)), &[]).unwrap().x >= 8.0,
        "on screen"
    );
    assert!(frame(SMALL, None, None, &[]).is_none());
    assert!(f.contains((110.0, f.y + 1.0)) && !f.contains((90.0, f.y + 1.0)));
}

#[test]
fn the_box_stays_on_the_screen_of_its_text() {
    let main = Rect {
        x: 0.0,
        y: 0.0,
        w: 1512.0,
        h: 982.0,
    };
    let left = Rect {
        x: -1920.0,
        y: -200.0,
        w: 1920.0,
        h: 1080.0,
    };
    let screens = [main, left];
    let line = Rect {
        x: -1800.0,
        y: 300.0,
        w: 200.0,
        h: 18.0,
    };
    let f = frame(SMALL, Some(line), None, &screens).unwrap();
    assert_eq!(
        (f.x, f.y + f.h + GAP),
        (-1800.0, 300.0),
        "on the left screen"
    );
    let high = Rect { y: -150.0, ..line };
    let f = frame(SMALL, Some(high), None, &screens).unwrap();
    assert_eq!(
        f.y,
        -150.0 + 18.0 + GAP,
        "below: the left screen's top is near"
    );
    let f = frame(SMALL, None, Some((-1910.0, -150.0)), &screens).unwrap();
    assert_eq!((f.x, f.y), (-1912.0, -170.0), "the mouse's screen");
    let edge = Rect { x: 1450.0, ..line };
    let f = frame(SMALL, Some(edge), None, &screens).unwrap();
    assert!(f.x + f.w <= main.w, "not past the right edge: {f:?}");
}

#[test]
fn a_sentence_gets_the_small_box_more_a_wide_one_as_tall_as_it_needs() {
    assert_eq!(size("Hello there, how are you today?"), SMALL);
    let para = "word ".repeat(60); // 300 characters: three wide lines
    let (w, h) = size(&para);
    assert_eq!(w, WIDE);
    assert!(h > SMALL.1 && h < TALL, "{h}");
    assert_eq!(size(&"word ".repeat(2000)), (WIDE, TALL), "then it scrolls");
    assert_eq!(size("one\ntwo").0, WIDE, "lines of their own");
    let laptop = Rect {
        x: 0.0,
        y: 0.0,
        w: 600.0,
        h: 400.0,
    };
    let f = frame((WIDE, TALL), None, Some((300.0, 380.0)), &[laptop]).unwrap();
    assert!(
        f.w <= 584.0 && f.h <= 354.0,
        "kept on a small screen: {f:?}"
    );
}

#[test]
fn a_tall_selection_never_pushes_the_box_off_the_bottom() {
    let screen = Rect {
        x: 0.0,
        y: 0.0,
        w: 1920.0,
        h: 1080.0,
    };
    // from near the top to near the bottom: room neither above nor below
    let text = Rect {
        x: 100.0,
        y: 40.0,
        w: 1700.0,
        h: 1000.0,
    };
    let f = frame((WIDE, TALL), Some(text), None, &[screen]).unwrap();
    assert!(f.y >= TOP && f.y + f.h <= screen.h - BOTTOM, "{f:?}");
    assert_eq!(f.h, TALL, "full size, over the text's end");
}

#[test]
fn a_whole_field_puts_the_box_by_the_mouse_not_above_the_field() {
    use blitznir_core::platform::Bounds;
    use blitznir_core::sky;
    // no rect for the text, only the focused element: a whole view
    let field = Bounds {
        rect: Rect {
            x: 224.0,
            y: 143.0,
            w: 1283.0,
            h: 806.0,
        },
        text: false,
    };
    let mouse = (700.0, 900.0); // on the words it just selected
    let at = sky::aim(Some(field), Some(mouse)).map(|(r, _)| r);
    let f = frame(SMALL, at, Some(mouse), &[]).unwrap();
    assert!(f.y + f.h < mouse.1 && f.y + f.h > mouse.1 - 40.0, "{f:?}");
}
