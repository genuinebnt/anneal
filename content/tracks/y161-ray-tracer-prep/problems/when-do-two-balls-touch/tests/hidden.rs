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
fn side_by_side() {
    expect(
        "the same velocity",
        ball(0.0, 0.0, 3.0, 4.0, 1.0),
        ball(5.0, 0.0, 3.0, 4.0, 1.0),
        None,
    );
    expect(
        "both still",
        ball(0.0, 0.0, 0.0, 0.0, 1.0),
        ball(5.0, 0.0, 0.0, 0.0, 1.0),
        None,
    );
}

#[test]
fn one_chasing_the_other() {
    expect(
        "catching up",
        ball(0.0, 0.0, 5.0, 0.0, 1.0),
        ball(8.0, 0.0, 2.0, 0.0, 1.0),
        Some(2.0),
    );
    expect(
        "falling behind",
        ball(0.0, 0.0, 2.0, 0.0, 1.0),
        ball(8.0, 0.0, 5.0, 0.0, 1.0),
        None,
    );
}

#[test]
fn points_without_size() {
    expect(
        "a point into a disc",
        ball(0.0, 0.0, 1.0, 0.0, 0.0),
        ball(10.0, 0.0, 0.0, 0.0, 4.0),
        Some(6.0),
    );
    expect(
        "two points meet",
        ball(0.0, 0.0, 1.0, 0.0, 0.0),
        ball(10.0, 0.0, -1.0, 0.0, 0.0),
        Some(5.0),
    );
}

#[test]
fn from_very_far() {
    for far in [1e8, 1e9, 3e9] {
        // The lanes are 1 apart, the radii add up to 2: they meet sqrt(3) before level.
        let want = far - 3f64.sqrt();
        expect(
            "a hit from far away",
            ball(0.0, 0.0, 1.0, 0.0, 1.0),
            ball(far, 1.0, 0.0, 0.0, 1.0),
            Some(want),
        );
        expect(
            "a near miss from far away",
            ball(0.0, 0.0, 1.0, 0.0, 1.0),
            ball(far, 2.01, 0.0, 0.0, 1.0),
            None,
        );
        expect(
            "a clear miss from far away",
            ball(0.0, 0.0, 1.0, 0.0, 1.0),
            ball(far, 3.0, 0.0, 0.0, 1.0),
            None,
        );
    }
}

#[test]
fn very_fast_and_very_slow() {
    expect(
        "fast",
        ball(0.0, 0.0, 1e12, 0.0, 1.0),
        ball(10.0, 0.0, 0.0, 0.0, 1.0),
        Some(8e-12),
    );
    expect(
        "slow",
        ball(0.0, 0.0, 1e-9, 0.0, 1.0),
        ball(10.0, 0.0, 0.0, 0.0, 1.0),
        Some(8e9),
    );
}

#[test]
fn a_hair_apart() {
    let got = first_touch(
        &ball(0.0, 0.0, 1.0, 0.0, 1.0),
        &ball(2.000001, 0.0, 0.0, 0.0, 1.0),
    );
    check!(
        format!("{got:?}"),
        got.is_some_and(|t| (t - 1e-6).abs() < 1e-12),
        true
    );
}

/// How close the two centres ever come at a time of 0 or more, and when.
#[allow(dead_code)]
fn nearest(a: &Ball, b: &Ball) -> (f64, f64) {
    let distance = |t: f64| (b.x - a.x + (b.vx - a.vx) * t).hypot(b.y - a.y + (b.vy - a.vy) * t);
    // The distance falls and then rises, so a search by thirds finds its lowest point.
    let (mut low, mut high) = (0.0, 1e6);
    for _ in 0..200 {
        let third = (high - low) / 3.0;
        if distance(low + third) < distance(high - third) {
            high -= third;
        } else {
            low += third;
        }
    }
    (distance(low), low)
}

#[test]
fn random_pairs() {
    let mut rng = Rng::new(1602);
    let (mut hits, mut misses) = (0, 0);
    for case in 0..4000 {
        let mut c = || rng.int(-500, 500) as f64 / 10.0;
        let (ax, ay, avx, avy) = (c(), c(), c(), c());
        let (bx, by) = (c(), c());
        // Aim `b` somewhere near where `a` will be, so that many pairs meet.
        let when = rng.int(1, 40) as f64 / 10.0;
        let miss = (
            rng.int(-150, 150) as f64 / 10.0,
            rng.int(-150, 150) as f64 / 10.0,
        );
        let bvx = (ax + avx * when + miss.0 - bx) / when;
        let bvy = (ay + avy * when + miss.1 - by) / when;
        let a = ball(ax, ay, avx, avy, rng.int(0, 60) as f64 / 10.0);
        let b = ball(bx, by, bvx, bvy, rng.int(1, 60) as f64 / 10.0);
        let reach = a.radius + b.radius;
        let (least, at) = nearest(&a, &b);
        if (least - reach).abs() < 1e-5 {
            continue;
        }
        let got = first_touch(&a, &b);
        let name = format!("case {case}: {a:?} {b:?} got {got:?}");
        check!(name.clone(), got.is_some(), least < reach);
        let Some(t) = got else {
            misses += 1;
            continue;
        };
        hits += 1;
        check!(name.clone(), t >= 0.0 && t <= at + 1e-6, true);
        let there = (bx - ax + (bvx - avx) * t).hypot(by - ay + (bvy - avy) * t);
        if t == 0.0 {
            check!(name, there <= reach + 1e-9, true);
        } else {
            check!(name, (there - reach).abs() < 1e-7, true);
        }
    }
    check!(
        format!("both kinds appear: {hits} and {misses}"),
        hits > 800 && misses > 800,
        true
    );
}

#[test]
fn the_order_does_not_matter() {
    let mut rng = Rng::new(1603);
    for case in 0..1000 {
        let mut c = || rng.int(-30, 30) as f64;
        let a = ball(c(), c(), c(), c(), 2.5);
        let b = ball(c(), c(), c(), c(), 3.25);
        let (one, two) = (first_touch(&a, &b), first_touch(&b, &a));
        let same = match (one, two) {
            (Some(x), Some(y)) => (x - y).abs() < 1e-9,
            (x, y) => x == y,
        };
        check!(format!("case {case}: {one:?} and {two:?}"), same, true);
    }
}
