use super::*;

fn rms(s: &[f32]) -> f32 {
    (s.iter().map(|v| v * v).sum::<f32>() / s.len().max(1) as f32).sqrt()
}

fn db(a: f32, b: f32) -> f32 {
    20.0 * (a / b).log10()
}

fn take(s: &mut Synth, secs_: f32) -> Vec<f32> {
    (0..secs(secs_)).map(|_| s.sample()).collect()
}

#[test]
fn it_opens_quietly_and_the_hum_fades_in() {
    let s = take(&mut Synth::new(5), 4.0);
    let opening = rms(&s[..secs(0.2)]);
    let working = rms(&s[secs(2.0)..secs(4.0)]);
    assert!(db(opening, working) < -6.0, "{}", db(opening, working));
    assert!(working > 0.005, "not silent: {working}");
    assert!(s.iter().all(|v| v.abs() < 0.95), "headroom");
}

#[test]
fn the_landing_comes_in_at_the_sample_without_a_jump_and_thins_out() {
    let (mut a, mut b) = (Synth::new(21), Synth::new(21));
    let working = take(&mut a, 3.0);
    take(&mut b, 3.0);
    b.land(3.0);
    let (on, landed) = (take(&mut a, 0.02), take(&mut b, 0.02));
    assert!((on[0] - landed[0]).abs() < 0.001, "no jump where it lands");
    let d: Vec<f32> = on.iter().zip(&landed).map(|(x, y)| x - y).collect();
    let apart = db(rms(&d), rms(&on));
    assert!(apart < -20.0, "{apart} dB in its first 20 ms");
    let rest = take(&mut b, LANDING_SECS);
    let before = rms(&working[secs(2.0)..]);
    let first = rms(&rest[..secs(0.3)]);
    assert!(db(first, before) < 1.0, "never louder than it was working");
    let later = rms(&rest[secs(1.0)..secs(1.5)]);
    assert!(
        db(later, first) < -6.0,
        "the dust thins out: {}",
        db(later, first)
    );
}

#[test]
fn the_landing_ends_faded_and_done() {
    let mut s = Synth::new(3);
    take(&mut s, 1.0);
    s.land(1.0);
    let rest = take(&mut s, LANDING_SECS);
    assert!(s.done());
    let n = rest.len();
    let (tail, before) = (
        rms(&rest[n - secs(0.001)..]),
        rms(&rest[n - secs(0.6)..n - secs(0.5)]),
    );
    assert!(tail < 0.05 * before + 1e-5, "{tail} {before}");
    s.land(1.0);
    assert!(s.done(), "a second landing changes nothing");
}

#[test]
fn a_shorter_job_lands_quieter() {
    assert!((landing_scale(0.3) - 0.45).abs() < 1e-6);
    // a 2 s job (to the paste) ran 1.9 s when it lands
    let mid = landing_scale(1.9);
    assert!((mid - 0.8).abs() < 0.06, "{mid}");
    assert!((landing_scale(3.0) - 1.0).abs() < 1e-6);
    assert!(
        (db(landing_scale(1.0), 1.0) + 6.0).abs() < 1.0,
        "about -6 dB"
    );
    let landed = |ran| {
        let mut s = Synth::new(1);
        take(&mut s, 3.0);
        s.land(ran);
        rms(&take(&mut s, 1.5))
    };
    let (short, mid, long) = (landed(1.0), landed(2.0), landed(3.0));
    assert!(short < mid && mid < long, "{short} {mid} {long}");
}

#[test]
fn one_seed_one_sound_another_seed_another() {
    let run = |seed| take(&mut Synth::new(seed), 1.0);
    assert_eq!(run(9), run(9));
    assert_ne!(run(9), run(10));
}
