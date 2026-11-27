#[allow(unused_imports)]
use anneal_prelude::Rng;
use solution::*;

/// Runs one estimate and keeps every place that was poked.
#[allow(dead_code)]
fn record(side: usize, seed: u64) -> (Option<f64>, Vec<(f64, f64)>) {
    let mut rng = Rng::new(seed);
    let mut pokes = Vec::new();
    let got = covered_share(
        side,
        &mut |x, y| {
            pokes.push((x, y));
            x + y < 1.0
        },
        &mut || rng.next_u64(),
    );
    (got, pokes)
}

#[allow(dead_code)]
const STEP: f64 = 1.0 / (1u64 << 53) as f64;

#[test]
fn bigger_raw_never_gives_less() {
    let mut rng = Rng::new(1607);
    for _ in 0..20_000 {
        let (a, b) = (rng.next_u64(), rng.next_u64());
        let (a, b) = (a.min(b), a.max(b));
        let (x, y) = (unit_from(a), unit_from(b));
        check!(format!("{a} and {b}"), x <= y && y < 1.0 && x >= 0.0, true);
        check!(
            format!("{a}: a whole number of steps"),
            (x / STEP).fract(),
            0.0
        );
        // Each result stands for 2048 raw values: the block that starts at a multiple of 2048.
        let first = a - a % 2048;
        check!(
            format!("the block of {a}"),
            (unit_from(first), unit_from(first + 2047)),
            (x, x)
        );
        if first > 0 {
            check!(
                format!("just below the block of {a}"),
                unit_from(first - 1),
                x - STEP
            );
        }
    }
}

#[test]
fn one_poke_in_every_cell() {
    for side in [1usize, 2, 3, 8, 31, 64] {
        for seed in 0..6 {
            let (_, pokes) = record(side, seed);
            let mut seen = vec![0; side * side];
            for &(x, y) in &pokes {
                check!(
                    format!("side {side}: ({x}, {y}) in the square"),
                    (0.0..1.0).contains(&x) && (0.0..1.0).contains(&y),
                    true
                );
                seen[(y * side as f64) as usize * side + (x * side as f64) as usize] += 1;
            }
            check!(
                format!("side {side}, seed {seed}: one each"),
                seen.iter().all(|&n| n == 1),
                true
            );
        }
    }
}

#[test]
fn the_place_in_the_cell_is_random() {
    let (_, pokes) = record(64, 1608);
    let inside: Vec<(f64, f64)> = pokes
        .iter()
        .map(|&(x, y)| ((x * 64.0).fract(), (y * 64.0).fract()))
        .collect();
    let counts = [
        ("left half", inside.iter().filter(|p| p.0 < 0.5).count()),
        ("upper half", inside.iter().filter(|p| p.1 < 0.5).count()),
        (
            "one side of the diagonal",
            inside.iter().filter(|p| p.0 < p.1).count(),
        ),
        (
            "one side of the other diagonal",
            inside.iter().filter(|p| p.0 + p.1 < 1.0).count(),
        ),
        (
            "outer quarter",
            inside.iter().filter(|p| p.0 < 0.125 || p.0 > 0.875).count() * 2,
        ),
    ];
    // 4096 pokes: at least five standard deviations either side of half. The outer quarter is counted twice,
    // so it spreads more.
    for (name, n) in counts {
        let room: usize = if name == "outer quarter" { 288 } else { 160 };
        check!(
            format!("{name}: {n} of 4096"),
            n.abs_diff(2048) <= room,
            true
        );
    }
}

#[test]
fn the_rolls_are_really_used() {
    let (_, a) = record(8, 1);
    let (_, b) = record(8, 2);
    let (_, again) = record(8, 1);
    check!("the same rolls, the same pokes", a == again, true);
    check!(
        "other rolls, other pokes",
        a.iter().filter(|p| b.contains(p)).count(),
        0
    );
}

#[test]
fn the_largest_rolls() {
    let mut pokes = Vec::new();
    covered_share(
        MAX_SIDE,
        &mut |x, y| {
            if pokes.len() < 3 || x > 0.999 && y > 0.999 {
                pokes.push((x, y));
            }
            true
        },
        &mut || u64::MAX,
    );
    check!(
        "never reaches the far side of the square",
        pokes.iter().all(|p| p.0 <= 1.0 && p.1 <= 1.0),
        true
    );
}

#[test]
fn an_error_that_is_guaranteed() {
    // A disc and a half plane. With one poke per cell, only cells that the border crosses can go wrong.
    for seed in 0..40 {
        let side = 64;
        let mut rng = Rng::new(seed);
        let disc = covered_share(
            side,
            &mut |x, y| (x - 0.5).hypot(y - 0.5) < 0.4,
            &mut || rng.next_u64(),
        )
        .unwrap();
        let want = std::f64::consts::PI * 0.16;
        check!(
            format!("disc, seed {seed}: {disc}"),
            (disc - want).abs() <= 4.0 / side as f64,
            true
        );
        let slope =
            covered_share(side, &mut |x, y| y < 0.3 + 0.2 * x, &mut || rng.next_u64()).unwrap();
        check!(
            format!("slope, seed {seed}: {slope}"),
            (slope - 0.4).abs() <= 2.0 / side as f64,
            true
        );
    }
}

#[test]
fn a_thin_stripe_cannot_hide() {
    // A stripe exactly one cell wide, lined up with the cells: every poke in it must be counted.
    for column in 0..32 {
        let mut rng = Rng::new(column);
        let got = covered_share(32, &mut |x, _| (x * 32.0) as u64 == column, &mut || {
            rng.next_u64()
        });
        check!(format!("stripe {column}"), got, Some(1.0 / 32.0));
    }
}

#[test]
fn right_on_average() {
    // A shape that no grid lines up with.
    let mut total = 0.0;
    for seed in 0..400 {
        let mut rng = Rng::new(5000 + seed);
        total += covered_share(4, &mut |x, y| x * x + y * y < 0.8, &mut || rng.next_u64()).unwrap();
    }
    let mean = total / 400.0;
    let want = 0.8 * std::f64::consts::FRAC_PI_4;
    check!(
        format!("{mean} against {want}"),
        (mean - want).abs() < 0.015,
        true
    );
}
