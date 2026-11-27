#[allow(unused_imports)]
use anneal_prelude::Rng;
use solution::*;

#[allow(dead_code)]
fn ball(x: f64, y: f64, vx: f64, vy: f64, radius: f64) -> Ball {
    Ball {
        x,
        y,
        vx,
        vy,
        radius,
    }
}

#[allow(dead_code)]
fn expect(name: &str, a: Ball, b: Ball, want: Option<f64>) {
    for (a, b) in [(a, b), (b, a)] {
        let got = first_touch(&a, &b);
        let ok = match (got, want) {
            (Some(g), Some(w)) => (g - w).abs() <= 1e-9 * w.max(1.0),
            (g, w) => g == w,
        };
        check!(
            format!("{name}: {a:?} {b:?} gave {got:?}, want {want:?}"),
            ok,
            true
        );
    }
}

#[test]
fn head_on() {
    // 10 apart, radii 1 and 2, closing at 7 per second: they touch after 1 second.
    expect(
        "head on",
        ball(0.0, 0.0, 4.0, 0.0, 1.0),
        ball(10.0, 0.0, -3.0, 0.0, 2.0),
        Some(1.0),
    );
}

#[test]
fn passing_by() {
    expect(
        "too far to the side",
        ball(0.0, 0.0, 1.0, 0.0, 1.0),
        ball(10.0, 3.0, -1.0, 0.0, 1.0),
        None,
    );
}

#[test]
fn grazing() {
    expect(
        "just touching on the way past",
        ball(0.0, 0.0, 1.0, 0.0, 1.0),
        ball(10.0, 2.0, 0.0, 0.0, 1.0),
        Some(10.0),
    );
}

#[test]
fn already_together() {
    expect(
        "overlapping",
        ball(0.0, 0.0, 1.0, 0.0, 1.0),
        ball(1.0, 0.0, -1.0, 0.0, 1.0),
        Some(0.0),
    );
    expect(
        "overlapping and leaving",
        ball(0.0, 0.0, -1.0, 0.0, 1.0),
        ball(1.0, 0.0, 1.0, 0.0, 1.0),
        Some(0.0),
    );
    expect(
        "touching and still",
        ball(0.0, 0.0, 0.0, 0.0, 1.0),
        ball(2.0, 0.0, 0.0, 0.0, 1.0),
        Some(0.0),
    );
}

#[test]
fn moving_apart() {
    expect(
        "away",
        ball(0.0, 0.0, -1.0, 0.0, 1.0),
        ball(10.0, 0.0, 1.0, 0.0, 1.0),
        None,
    );
}
