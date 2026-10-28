use super::*;

fn run(f: &mut Follow, target: (f32, f32), secs: f32, fps: f32) -> f32 {
    let mut peak = f32::MIN;
    for _ in 0..(secs * fps).round() as u32 {
        f.step(target, 1.0 / fps);
        peak = peak.max(f.x);
    }
    peak
}

#[test]
fn the_tip_glides_onto_a_new_word_in_about_half_a_second_and_never_past_it() {
    for fps in [10.0, 30.0, 60.0] {
        let mut f = Follow::at((0.0, 0.0));
        let peak = run(&mut f, (100.0, 0.0), 0.2, fps);
        assert!(
            f.x > 20.0 && f.x < 80.0,
            "{fps} fps: on its way at 0.2 s: {}",
            f.x
        );
        let peak = peak.max(run(&mut f, (100.0, 0.0), 0.3, fps));
        assert!(f.x > 94.0, "{fps} fps: nearly there at 0.5 s: {}", f.x);
        let peak = peak.max(run(&mut f, (100.0, 0.0), 1.0, fps));
        assert!(peak <= 100.0 + 1e-3, "{fps} fps: no overshoot: {peak}");
        assert!(f.settled((100.0, 0.0)), "{fps} fps: settled: {f:?}");
    }
}

#[test]
fn the_tip_drops_to_a_new_line_before_it_glides_back() {
    let mut f = Follow::at((300.0, 0.0));
    run(&mut f, (20.0, 20.0), 0.2, 30.0);
    assert!(f.y > 15.0, "down first: {}", f.y);
    assert!(f.x > 150.0, "still gliding back: {}", f.x);
    f.drop_to(40.0);
    assert_eq!(f.y, 40.0);
}

#[test]
fn a_stall_is_a_quarter_second_step_not_a_jump() {
    let mut f = Follow::at((0.0, 0.0));
    f.step((100.0, 0.0), 5.0);
    assert!(f.x < 100.0 && f.x > 30.0, "{}", f.x);
}

#[test]
fn the_heads_speed_is_its_recent_travel_along_the_lines() {
    let mut h = Heads::default();
    assert_eq!(h.speed(1.0), 0.0);
    for i in 0..=10 {
        let t = i as f32 * 0.1;
        h.push(t, (i as f32 * 10.0, 5.0), i as f32 * 10.0);
    }
    assert!((h.speed(1.0) - 100.0).abs() < 1e-3, "{}", h.speed(1.0));
    // a wrap: back to the left, but on and on along the lines
    h.push(1.1, (0.0, 25.0), 110.0);
    assert!(h.speed(1.1) > 90.0);
    assert_eq!(h.speed(3.0), 0.0, "stopped");
    assert_eq!(h.moved(), Some(1.1));
}

#[test]
fn the_trail_glides_along_the_fresh_words() {
    let mut h = Heads::default();
    assert_eq!(h.at(1.0), None);
    h.push(0.0, (10.0, 5.0), 10.0);
    h.push(0.2, (50.0, 5.0), 50.0);
    assert_eq!(h.at(0.1), Some((30.0, 5.0)));
    assert_eq!(h.at(0.5), Some((50.0, 5.0)));
    h.push(0.4, (0.0, 25.0), 120.0);
    assert_eq!(
        h.at(0.3),
        Some((0.0, 25.0)),
        "a wrap: no glide across the lines"
    );
    assert_eq!(h.at(-1.0), None);
}

#[test]
fn it_fades_in_and_out_and_runs_slower_when_idle() {
    assert_eq!(strength(0.0, 0.35, -1.0, CARET_OUT_SECS), 0.0);
    assert_eq!(strength(1.0, 0.35, -1.0, CARET_OUT_SECS), 1.0);
    assert!(strength(1.0, 0.35, 0.15, CARET_OUT_SECS) > 0.3);
    assert_eq!(strength(1.0, 0.35, CARET_OUT_SECS, CARET_OUT_SECS), 0.0);
    assert_eq!(frame(Duration::from_millis(300)), MOVING_FRAME);
    assert_eq!(frame(Duration::from_secs(2)), IDLE_FRAME);
}

#[test]
fn the_dictations_wave_sits_under_the_caret_line_in_a_small_window() {
    let line = Rect {
        x: 400.0,
        y: 300.0,
        w: 230.0,
        h: 18.0,
    };
    let s = spot(&line);
    assert_eq!(s.caret, 630.0);
    assert!(
        (s.y - (300.0 + 18.0 * 0.8 + 5.0)).abs() < 1e-9,
        "5 under the baseline"
    );
    assert!(s.y > s.text_bottom, "never over the text");
    let f = s.frame;
    assert!(f.x <= s.left - 6.0 && f.x + f.w >= s.caret + 100.0, "{f:?}");
    assert!(
        f.y < s.y && f.y + f.h > s.y + 6.0 && f.h <= 40.0,
        "line-sized: {f:?}"
    );
    assert!(f.w < 1200.0, "never the screen");
    // the next word: the same window
    let next = spot(&Rect { w: 262.0, ..line });
    assert_eq!(next.frame, f);
    // a long line: the tail is cut, the window stays small
    let long = spot(&Rect {
        x: 0.0,
        w: 3000.0,
        ..line
    });
    assert!(long.left > 2300.0 && long.frame.w < 1000.0, "{long:?}");
    // the line's start unknown: a guessed tail
    let blind = spot(&Rect { w: 0.0, ..line });
    assert_eq!(blind.caret, 400.0);
    assert!(blind.left < 400.0 - 200.0);
}
