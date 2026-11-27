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
fn the_ends_of_the_range() {
    check!("0", unit_from(0), 0.0);
    check!(
        "the largest raw value stays below 1",
        unit_from(u64::MAX),
        1.0 - STEP
    );
}

#[test]
fn halves_and_quarters() {
    check!("half", unit_from(1 << 63), 0.5);
    check!("a quarter", unit_from(1 << 62), 0.25);
    check!(
        "the smallest step takes 2048 raw values",
        (unit_from(2047), unit_from(2048)),
        (0.0, STEP)
    );
}

#[test]
fn everything_and_nothing() {
    let mut rng = Rng::new(1);
    check!(
        "all",
        covered_share(7, &mut |_, _| true, &mut || rng.next_u64()),
        Some(1.0)
    );
    check!(
        "none",
        covered_share(7, &mut |_, _| false, &mut || rng.next_u64()),
        Some(0.0)
    );
}

#[test]
fn exactly_that_many_pokes() {
    let (got, pokes) = record(10, 2);
    check!("pokes", pokes.len(), 100);
    let share = pokes.iter().filter(|p| p.0 + p.1 < 1.0).count() as f64 / 100.0;
    check!("the answer is the share that said yes", got, Some(share));
}

#[test]
fn sides_that_are_refused() {
    let mut calls = 0;
    for side in [0, MAX_SIDE + 1, usize::MAX, 1 << 32] {
        check!(
            format!("side {side}"),
            covered_share(
                side,
                &mut |_, _| {
                    calls += 1;
                    true
                },
                &mut || 0
            ),
            None
        );
    }
    check!("nothing was poked", calls, 0);
}
