#[allow(unused_imports)]
use anneal_prelude::Rng;
use solution::*;

#[allow(dead_code)]
fn unit(rng: &mut Rng) -> f64 {
    (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64
}

/// What one run did: its estimate, the rooms it asked for, how many rolls it used.
#[allow(dead_code)]
fn run(rooms: &[(f64, f64)], rolls: &[f64]) -> (f64, Vec<usize>, usize) {
    let mut asked = Vec::new();
    let mut used = 0;
    let got = one_estimate(
        &mut |i| {
            asked.push(i);
            rooms.get(i).copied()
        },
        &mut || {
            let r = rolls
                .get(used)
                .copied()
                .expect("more rolls than the rules allow");
            used += 1;
            r
        },
    );
    (got, asked, used)
}

/// The true total, summed from the far end.
#[allow(dead_code)]
fn truth(rooms: &[(f64, f64)]) -> f64 {
    rooms
        .iter()
        .rev()
        .fold(0.0, |beyond, &(glow, passes)| glow + passes * beyond)
}

/// Adds up every outcome of the walk times its chance. The chance of going on at each roll is not
/// computed here: it is measured, by finding the roll at which your walk changes its mind.
#[allow(dead_code)]
fn average_over_all_outcomes(rooms: &[(f64, f64)]) -> f64 {
    // A roll of 0 always goes on, and a roll just below 1 always stops.
    let stop = 0.999_999_999;
    let rolls_in_all = rooms.len().saturating_sub(2);
    let mut sum = 0.0;
    let mut reach = 1.0;
    for k in 0..rolls_in_all {
        let mut rolls = vec![0.0; rolls_in_all];
        // Did the walk ask for room k + 3 when roll number k was `r`?
        let (mut goes, mut stops) = (0.0, stop);
        for _ in 0..60 {
            let r = (goes + stops) / 2.0;
            rolls[k] = r;
            if run(rooms, &rolls).1.len() > k + 3 {
                goes = r;
            } else {
                stops = r;
            }
        }
        let chance = stops;
        rolls[k] = stop;
        sum += reach * (1.0 - chance) * run(rooms, &rolls).0;
        reach *= chance;
    }
    sum + reach * run(rooms, &vec![0.0; rolls_in_all]).0
}

#[test]
fn random_rows_over_every_outcome() {
    let mut rng = Rng::new(1621);
    for case in 0..500 {
        let n = rng.int(1, 14) as usize;
        let rooms: Vec<(f64, f64)> = (0..n)
            .map(|_| {
                let passes = *rng.pick(&[0.0, 0.001, 0.04, 0.05, 0.3, 0.5, 0.9, 0.96, 1.0]);
                (rng.int(0, 50) as f64 / 10.0, passes)
            })
            .collect();
        let got = average_over_all_outcomes(&rooms);
        let want = truth(&rooms);
        check!(
            format!("case {case}: {rooms:?}: average {got}, truth {want}"),
            (got - want).abs() < 1e-9 * want.max(1.0),
            true
        );
    }
}

#[test]
fn one_roll_per_room_from_the_third_on() {
    let rooms = vec![(1.0, 0.99); 12];
    let (_, asked, used) = run(&rooms, &[0.0; 10]);
    check!("ran to the end", (asked, used), ((0..=12).collect(), 10));
}

#[test]
fn a_roll_equal_to_the_chance_stops() {
    let rooms = [(1.0, 1.0), (1.0, 1.0), (1.0, 0.25), (1.0, 1.0), (1.0, 1.0)];
    check!("stops", run(&rooms, &[0.25]).1, vec![0, 1, 2]);
    check!(
        "just below goes on",
        run(&rooms, &[0.2499999, 0.999]).1,
        vec![0, 1, 2, 3]
    );
}

#[test]
fn the_chance_stays_between_its_limits() {
    let rooms = [(1.0, 1.0), (1.0, 1.0), (1.0, 1.0), (1.0, 1.0), (1.0, 1.0)];
    check!(
        "a full weight can still stop",
        run(&rooms, &[0.96]).1,
        vec![0, 1, 2]
    );
    let dim = [
        (1.0, 0.001),
        (1.0, 0.001),
        (1.0, 0.001),
        (1.0, 1.0),
        (1.0, 1.0),
    ];
    check!(
        "a faint weight can still go on",
        run(&dim, &[0.049, 0.999]).1,
        vec![0, 1, 2, 3]
    );
}

#[test]
fn a_dead_end_still_follows_the_rules() {
    // Nothing passes room 2, yet the rules still roll after it.
    let rooms = [(1.0, 1.0), (1.0, 1.0), (1.0, 0.0), (9.0, 1.0), (9.0, 1.0)];
    let (got, asked, used) = run(&rooms, &[0.01, 0.99]);
    check!(
        "light from behind a dead end counts for nothing",
        (got, asked, used),
        (3.0, vec![0, 1, 2, 3], 2)
    );
}

#[test]
fn a_row_without_end() {
    // Every room glows 1 and passes a half: the total is 2.
    let mut rng = Rng::new(1622);
    let n = 200_000;
    let mut sum = 0.0;
    for _ in 0..n {
        sum += one_estimate(&mut |_| Some((1.0, 0.5)), &mut || unit(&mut rng));
    }
    let mean = sum / f64::from(n);
    check!(
        format!("{mean} within 0.01 of 2"),
        (mean - 2.0).abs() < 0.01,
        true
    );
}

#[test]
fn a_bright_row_without_end() {
    let mut rng = Rng::new(1623);
    let n = 100_000;
    let mut sum = 0.0;
    for _ in 0..n {
        sum += one_estimate(&mut |_| Some((1.0, 0.9)), &mut || unit(&mut rng));
    }
    let mean = sum / f64::from(n);
    check!(
        format!("{mean} within 0.15 of 10"),
        (mean - 10.0).abs() < 0.15,
        true
    );
}

#[test]
fn a_row_that_loses_nothing_still_ends() {
    let mut rng = Rng::new(1624);
    let mut rooms = 0u64;
    for _ in 0..2000 {
        one_estimate(
            &mut |i| {
                rooms += 1;
                assert!(i < 2000, "the walk does not end");
                Some((0.0, 1.0))
            },
            &mut || unit(&mut rng),
        );
    }
    check!(
        format!("about 22 rooms a walk, {rooms} in all"),
        (30_000..60_000).contains(&rooms),
        true
    );
}

#[test]
fn late_light_is_not_lost() {
    // All the light sits in room 8, behind dim rooms.
    let mut rooms = vec![(0.0, 0.5); 8];
    rooms.push((1000.0, 0.0));
    let got = average_over_all_outcomes(&rooms);
    check!(
        format!("{got}"),
        (got - 1000.0 * 0.5f64.powi(8)).abs() < 1e-9,
        true
    );
}
