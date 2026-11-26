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

#[test]
fn a_fixed_order_either_works_or_does_not() {
    let job = seq(vec![Add(3), Double, Add(-8)]);
    check!("1, then +3, double, -8", got(&job, 1, 10), (Some(vec![0, 1, 2]), 3));
    check!("a tank of 7 overflows at the double", got(&job, 1, 7), (None, 1));
}

#[test]
fn free_actions_go_in_written_order_when_they_can() {
    check!("both orders work", got(&free(vec![Add(1), Add(2)]), 0, 10), (Some(vec![0, 1]), 2));
    check!("only drawing last works", got(&free(vec![Add(-5), Add(5)]), 0, 10), (Some(vec![1, 0]), 2));
}

#[test]
fn a_later_action_can_force_another_order_inside_a_finished_group() {
    let job = seq(vec![free(vec![Double, Add(3)]), Add(-7)]);
    check!("only +3 then double leaves enough", got(&job, 1, 10), (Some(vec![1, 0, 2]), 3));
}

#[test]
fn members_never_mix() {
    // Taking +5, -5, +5, -5 would work, but it would split a group.
    let job = free(vec![seq(vec![Add(5), Add(5)]), seq(vec![Add(-5), Add(-5)])]);
    check!("a tank of 5", got(&job, 0, 5), (None, 1));
    check!("a tank of 10", got(&job, 0, 10), (Some(vec![0, 1, 2, 3]), 4));
}

#[test]
fn nothing_to_do() {
    check!("an empty list", got(&seq(vec![]), 4, 4), (Some(vec![]), 0));
    check!("empty groups inside", got(&free(vec![seq(vec![]), free(vec![]), Add(0)]), 0, 0), (Some(vec![0]), 1));
}
