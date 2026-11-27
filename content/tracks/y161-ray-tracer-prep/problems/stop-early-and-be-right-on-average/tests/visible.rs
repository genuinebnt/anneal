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
fn short_rows_need_no_luck() {
    check!("no rooms", run(&[], &[]), (0.0, vec![0], 0));
    check!("one room", run(&[(5.0, 0.5)], &[]), (5.0, vec![0, 1], 0));
    check!(
        "two rooms",
        run(&[(5.0, 0.5), (4.0, 0.5)], &[]),
        (7.0, vec![0, 1, 2], 0)
    );
}

#[test]
fn rooms_are_asked_for_in_order() {
    let rooms = [(1.0, 0.9); 5];
    check!(
        "never stopping",
        run(&rooms, &[0.0; 3]).1,
        vec![0, 1, 2, 3, 4, 5]
    );
}

#[test]
fn the_third_room_has_a_roll() {
    let rooms = [(1.0, 1.0), (1.0, 1.0), (1.0, 0.5), (8.0, 0.0)];
    // After room 2 the weight is 0.5: a roll of 0.7 stops there.
    let (got, asked, used) = run(&rooms, &[0.7]);
    check!(
        "stopped: the first three glows",
        (got, asked, used),
        (3.0, vec![0, 1, 2], 1)
    );
}

#[test]
fn a_roll_below_the_chance_goes_on() {
    let rooms = [(1.0, 1.0), (1.0, 1.0), (1.0, 0.5), (8.0, 0.0)];
    let (got, asked, used) = run(&rooms, &[0.2, 0.99]);
    check!(
        "asked for room 3 and stopped there",
        (asked, used),
        (vec![0, 1, 2, 3], 2)
    );
    // What room 3 must add is for you to work out. It is more than 4.
    check!(format!("{got}"), got > 7.0, true);
}

#[test]
fn right_on_average_over_every_outcome() {
    let rooms = [
        (1.0, 0.9),
        (2.0, 0.8),
        (0.5, 0.7),
        (4.0, 0.6),
        (3.0, 0.5),
        (10.0, 0.4),
    ];
    let got = average_over_all_outcomes(&rooms);
    check!(
        format!("{got} against {}", truth(&rooms)),
        (got - truth(&rooms)).abs() < 1e-9,
        true
    );
}
