use solution::*;
use anneal_prelude::Rng;

#[allow(dead_code)]
fn ask(s: &Summary, x: i64) -> usize {
    Summary { kept: s.kept.clone(), total: s.total }.at_most(x)
}

/// Check every promise for one list of times.
#[allow(dead_code)]
fn judge(label: &str, times: Vec<i64>, limit: usize, rng: &mut Rng) {
    let mut sorted = times.clone();
    sorted.sort();
    let n = sorted.len();
    let s = summarise(times, limit);
    check!(format!("{label}: numbers kept ({}), limit {limit}", s.kept.len()), s.kept.len() <= limit, true);
    check!(format!("{label}: total"), s.total, n);
    let allowed = n.div_ceil(limit - 1);
    let mut questions = vec![i64::MIN, i64::MAX, 0];
    if n > 0 {
        for _ in 0..200 {
            let v = sorted[rng.below(n)];
            questions.extend([v, v.saturating_sub(1), v.saturating_add(1)]);
        }
        questions.extend([sorted[0], sorted[n - 1], sorted[0].saturating_sub(1)]);
        for _ in 0..100 {
            questions.push(rng.int(sorted[0], sorted[n - 1]));
        }
    }
    questions.sort();
    let mut previous = 0;
    for x in questions {
        let truth = sorted.partition_point(|&v| v <= x);
        let guess = ask(&s, x);
        let exact = n == 0 || x < sorted[0] || x >= sorted[n - 1];
        let limit_here = if exact { 0 } else { allowed };
        check!(format!("{label}: at_most({x}) guessed {guess}, truth {truth}, allowed error {limit_here}"), guess.abs_diff(truth) <= limit_here, true);
        check!(format!("{label}: a larger question never gets a smaller answer (at {x})"), guess >= previous, true);
        previous = guess;
    }
}

#[test]
fn times_that_crowd_together() {
    let mut rng = Rng::new(9111);
    // most deliveries take about an hour; a few take weeks
    let mut times: Vec<i64> = (0..5000).map(|_| rng.int(3500, 3700)).collect();
    times.extend((0..40).map(|_| rng.int(1_000_000, 2_000_000_000)));
    rng.shuffle(&mut times);
    judge("crowded", times, 50, &mut rng);
}

#[test]
fn one_value_most_of_the_time() {
    let mut rng = Rng::new(9112);
    let times: Vec<i64> = (0..3000).map(|i| if i % 10 < 8 { 600 } else { rng.int(0, 100_000) }).collect();
    judge("mostly 600", times, 21, &mut rng);
    judge("all the same", vec![5; 1000], 3, &mut rng);
}

#[test]
fn the_smallest_limit() {
    let mut rng = Rng::new(9113);
    judge("limit 2", (0..101).map(|i| i * i).collect(), 2, &mut rng);
}

#[test]
fn the_ends_of_the_number_line() {
    let mut rng = Rng::new(9114);
    let mut times = vec![i64::MIN, i64::MAX, i64::MAX, 0, -1, 1];
    times.extend((0..200).map(|_| rng.next_u64() as i64));
    judge("full range", times, 16, &mut rng);
}

#[test]
fn around_the_limit() {
    let mut rng = Rng::new(9115);
    for limit in [2, 3, 7, 8] {
        for n in 0..=limit + 3 {
            let times = (0..n).map(|_| rng.int(0, 20)).collect();
            judge(&format!("{n} times, limit {limit}"), times, limit, &mut rng);
        }
    }
}

#[test]
fn many_random_lists() {
    let mut rng = Rng::new(9116);
    for case in 0..300 {
        let n = rng.below(400);
        let span = [3, 100, 1_000_000_000_000][rng.below(3)];
        let times: Vec<i64> = (0..n)
            .map(|_| {
                let v = rng.int(0, span);
                if rng.below(4) == 0 { v / 1000 } else { v }
            })
            .collect();
        let limit = 2 + rng.below(40);
        judge(&format!("case {case} ({n} times up to {span}, limit {limit})"), times, limit, &mut rng);
    }
}

#[test]
fn a_busy_route() {
    let mut rng = Rng::new(9117);
    let times: Vec<i64> = (0..100_000).map(|_| rng.int(0, 1_000_000_000)).collect();
    judge("100 000 times", times, 1001, &mut rng);
}

#[test]
fn nothing_and_one_thing() {
    let mut rng = Rng::new(9118);
    judge("no times", vec![], 2, &mut rng);
    judge("one time", vec![-7], 2, &mut rng);
    judge("two times", vec![3, -3], 2, &mut rng);
}
