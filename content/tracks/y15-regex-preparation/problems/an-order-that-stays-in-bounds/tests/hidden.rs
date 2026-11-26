use solution::*;
use Job::{Add, Double};

#[allow(dead_code)]
fn seq(list: Vec<Job>) -> Job {
    Job::InOrder(list)
}
#[allow(dead_code)]
fn free(list: Vec<Job>) -> Job {
    Job::AnyOrder(list)
}

#[allow(dead_code)]
fn got(job: &Job, start: i64, capacity: i64) -> (Option<Vec<usize>>, usize) {
    let outcome = plan(job, start, capacity);
    (outcome.order, outcome.furthest)
}

use anneal_prelude::Rng;

#[test]
fn numbers_follow_the_sheet_not_the_order_performed() {
    let job = seq(vec![free(vec![seq(vec![Add(-1), Add(-1)]), Add(2)]), Double]);
    check!("the last written member goes first", got(&job, 0, 4), (Some(vec![2, 0, 1, 3]), 4));
}

#[test]
fn furthest_counts_the_best_attempt_not_the_last() {
    let job = free(vec![seq(vec![Add(1), Add(1), Add(1), Add(-9)]), Add(-9)]);
    check!("three actions at best", got(&job, 0, 8), (None, 3));
    check!("nothing possible at all", got(&free(vec![Add(-1), Add(9)]), 0, 8), (None, 0));
}

#[test]
fn going_back_through_several_finished_groups() {
    let pair = || free(vec![Double, Add(1)]);
    let job = seq(vec![pair(), pair(), pair(), Add(-22)]);
    check!("every pair must add before it doubles", got(&job, 1, 22), (Some(vec![1, 0, 3, 2, 5, 4, 6]), 7));
}

#[test]
fn huge_levels_do_not_overflow() {
    let big = i64::MAX;
    check!("doubling past i64", got(&seq(vec![Double]), big / 2 + 1, big), (None, 0));
    check!("adding past i64", got(&free(vec![Add(1), Add(-1)]), big, big), (Some(vec![1, 0]), 2));
    check!("the most negative step", got(&seq(vec![Add(i64::MIN)]), 5, big), (None, 0));
}

#[test]
fn bounds_are_checked_after_every_action() {
    check!("a dip in the middle is not allowed", got(&seq(vec![Add(-3), Add(5)]), 1, 10), (None, 0));
    check!("touching both bounds is fine", got(&seq(vec![Add(10), Add(-10)]), 0, 10), (Some(vec![0, 1]), 2));
}

/// Every complete sequence of (number, job is Double, amount) that the sheet allows. For tiny sheets only.
#[allow(dead_code)]
fn orders(job: &Job, next: &mut usize) -> Vec<Vec<(usize, Option<i64>)>> {
    match job {
        Add(n) => {
            *next += 1;
            vec![vec![(*next - 1, Some(*n))]]
        }
        Double => {
            *next += 1;
            vec![vec![(*next - 1, None)]]
        }
        Job::InOrder(list) => {
            let parts: Vec<_> = list.iter().map(|j| orders(j, next)).collect();
            join(&parts, &(0..parts.len()).collect::<Vec<_>>())
        }
        Job::AnyOrder(list) => {
            let parts: Vec<_> = list.iter().map(|j| orders(j, next)).collect();
            let mut arrangements = vec![Vec::new()];
            for member in 0..parts.len() {
                arrangements = arrangements
                    .into_iter()
                    .flat_map(|a: Vec<usize>| {
                        (0..=a.len()).map(move |at| {
                            let mut longer = a.clone();
                            longer.insert(at, member);
                            longer
                        })
                    })
                    .collect();
            }
            arrangements.iter().flat_map(|a| join(&parts, a)).collect()
        }
    }
}

#[allow(dead_code)]
fn join(parts: &[Vec<Vec<(usize, Option<i64>)>>], arrangement: &[usize]) -> Vec<Vec<(usize, Option<i64>)>> {
    arrangement.iter().fold(vec![Vec::new()], |so_far, &member| {
        so_far
            .iter()
            .flat_map(|head| parts[member].iter().map(move |tail| [head.clone(), tail.clone()].concat()))
            .collect()
    })
}

#[allow(dead_code)]
fn random(rng: &mut Rng, depth: u32, left: &mut u32) -> Job {
    if depth == 0 || rng.below(3) == 0 {
        if *left == 0 {
            return seq(vec![]);
        }
        *left -= 1;
        return if rng.below(4) == 0 { Double } else { Add(rng.int(-5, 5)) };
    }
    let n = rng.below(4);
    let list = (0..n).map(|_| random(rng, depth - 1, left)).collect();
    if rng.bool() {
        seq(list)
    } else {
        free(list)
    }
}

#[test]
fn random_sheets_agree_with_listing_every_order() {
    let mut rng = Rng::new(1507);
    let (mut possible, mut impossible) = (0, 0);
    for _ in 0..1500 {
        let mut left = 7;
        let job = seq(vec![random(&mut rng, 3, &mut left), random(&mut rng, 3, &mut left)]);
        let capacity = rng.int(3, 14);
        let start = rng.int(0, capacity);
        let mut best: Option<Vec<usize>> = None;
        let mut furthest = 0;
        for order in orders(&job, &mut 0) {
            let mut level = start;
            let mut count = 0;
            for &(_, amount) in &order {
                level = amount.map_or(level * 2, |n| level + n);
                if !(0..=capacity).contains(&level) {
                    break;
                }
                count += 1;
            }
            furthest = furthest.max(count);
            if count == order.len() {
                let numbers: Vec<usize> = order.iter().map(|a| a.0).collect();
                if best.as_ref().map_or(true, |b| &numbers < b) {
                    best = Some(numbers);
                }
            }
        }
        if best.is_some() {
            possible += 1;
        } else {
            impossible += 1;
        }
        check!(format!("{job:?} from {start} in {capacity}"), got(&job, start, capacity), (best, furthest));
    }
    check!(format!("both kinds appear: {possible} and {impossible}"), possible > 200 && impossible > 200, true);
}

#[test]
fn many_free_actions_with_an_easy_answer() {
    let job = free((0..60).map(|_| Add(1)).collect());
    check!("60 free actions", got(&job, 0, 60), (Some((0..60).collect()), 60));
    let groups = free((0..30).map(|_| free(vec![Add(2), Add(-1)])).collect());
    check!("30 free pairs", got(&groups, 0, 1000), (Some((0..60).collect()), 60));
}

#[test]
fn a_long_sheet_in_fixed_order() {
    let job = seq((0..300).map(|i| if i % 2 == 0 { Add(1) } else { Add(-1) }).collect());
    check!("300 actions", got(&job, 0, 1), (Some((0..300).collect()), 300));
}
