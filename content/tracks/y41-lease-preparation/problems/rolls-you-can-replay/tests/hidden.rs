use solution::*;
use std::collections::HashMap;

/// 30,000 draws sorted into three groups of the same size: each should get about 10,000. The bounds are wide
/// enough (more than seven standard deviations) that a fair source cannot fail them by bad luck.
#[allow(dead_code)]
fn three_even_groups(label: &str, seed: u64, hi: u64, group: impl Fn(u64) -> usize) {
    let mut dice = Dice::new(seed);
    let mut counts = [0u32; 3];
    for _ in 0..30_000 {
        let v = dice.between(0, hi);
        check!(format!("{v} is at most {hi}"), v <= hi, true);
        counts[group(v)] += 1;
    }
    check!(
        format!("{label}: counts {counts:?} are all near 10000"),
        counts.iter().all(|&n| (9_400..=10_600).contains(&n)),
        true
    );
}

/// A range of 3 * 2^62 values: a quarter of 2^64 does not fit into it evenly.
#[test]
fn the_three_thirds_of_a_huge_range_are_equally_likely() {
    let quarter = 1u64 << 62;
    three_even_groups("thirds of 0..3*2^62", 11, 3 * quarter - 1, |v| (v / quarter) as usize);
}

#[test]
fn the_three_remainders_of_a_huge_range_are_equally_likely() {
    let quarter = 1u64 << 62;
    three_even_groups("remainders mod 3", 12, 3 * quarter - 1, |v| (v % 3) as usize);
}

#[test]
fn a_small_range_is_even() {
    three_even_groups("0..=2", 13, 2, |v| v as usize);
}

#[test]
fn the_whole_of_u64() {
    let mut dice = Dice::new(3);
    let draws: Vec<u64> = (0..2000).map(|_| dice.between(0, u64::MAX)).collect();
    let high = draws.iter().filter(|&&v| v >= 1 << 63).count();
    check!(format!("{high} of 2000 in the upper half"), (850..=1150).contains(&high), true);
}

#[test]
fn ranges_that_end_at_the_top() {
    let mut dice = Dice::new(4);
    for _ in 0..500 {
        let v = dice.between(u64::MAX - 2, u64::MAX);
        check!("near the top", v >= u64::MAX - 2, true);
        let w = dice.between(1, u64::MAX);
        check!("never zero", w >= 1, true);
    }
}

#[test]
#[should_panic]
fn an_empty_range_panics() {
    Dice::new(1).between(5, 4);
}

#[test]
fn percent_has_the_right_rate() {
    let mut dice = Dice::new(21);
    for p in [1u32, 10, 50, 90, 99] {
        let hits = (0..20_000).filter(|_| dice.percent(p)).count() as i64;
        let want = 200 * i64::from(p);
        check!(format!("percent({p}): {hits} hits of 20000, about {want}"), (hits - want).abs() <= 600, true);
    }
}

#[test]
fn random_ranges_stay_inside_and_replay() {
    let mut rng = anneal_prelude::Rng::new(41_001);
    for _ in 0..500 {
        let (a, b) = (rng.next_u64() >> rng.below(64), rng.next_u64() >> rng.below(64));
        let (lo, hi) = (a.min(b), a.max(b));
        let seed = rng.next_u64();
        let (v, w) = (Dice::new(seed).between(lo, hi), Dice::new(seed).between(lo, hi));
        check!(format!("seed {seed}, between({lo}, {hi}) replays"), v, w);
        check!(format!("{v} lies in {lo}..={hi}"), (lo..=hi).contains(&v), true);
    }
}

#[test]
fn the_raw_rolls_of_other_seeds_follow_the_recipe() {
    let mut rng = anneal_prelude::Rng::new(41_002);
    for _ in 0..200 {
        // the test helper uses the same published recipe
        let seed = rng.next_u64();
        let (mut mine, mut model) = (Dice::new(seed), anneal_prelude::Rng::new(seed));
        for _ in 0..20 {
            check!(format!("seed {seed}"), mine.roll(), model.next_u64());
        }
    }
}

#[test]
fn no_two_small_streams_share_a_value() {
    let mut owner: HashMap<u64, (u64, u64)> = HashMap::new();
    for seed in 0..40 {
        for id in 0..40 {
            let mut dice = Dice::stream(seed, id);
            for _ in 0..100 {
                let value = dice.roll();
                if let Some(other) = owner.insert(value, (seed, id)) {
                    check!(format!("stream {other:?} and stream ({seed}, {id}) both roll {value}"), true, false);
                }
            }
        }
    }
}

#[test]
fn using_one_stream_does_not_move_another() {
    let expected: Vec<u64> = {
        let mut b = Dice::stream(7, 2);
        (0..10).map(|_| b.between(0, 999)).collect()
    };
    let mut a = Dice::stream(7, 1);
    let mut b = Dice::stream(7, 2);
    let mut got = Vec::new();
    for i in 0..10 {
        for _ in 0..i * 3 {
            a.roll();
        }
        got.push(b.between(0, 999));
    }
    check!("stream (7, 2) while (7, 1) is busy", got, expected);
}
