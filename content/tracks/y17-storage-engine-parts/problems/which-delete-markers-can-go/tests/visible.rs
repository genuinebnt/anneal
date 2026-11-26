use solution::*;

#[allow(dead_code)]
fn merged(mut runs: Vec<Run>, window: std::ops::Range<usize>) -> Vec<Run> {
    merge_window(&mut runs, window);
    runs
}

#[test]
fn the_newest_version_wins() {
    let runs = vec![vec![(1, Some(10)), (2, Some(20))], vec![(1, Some(11))]];
    check!(format!("{runs:?}"), (lookup(&runs, 1), lookup(&runs, 2), lookup(&runs, 3)), (Some(11), Some(20), None));
    check!("merge both", merged(runs.clone(), 0..2), vec![vec![(1, Some(11)), (2, Some(20))]]);
}

#[test]
fn a_marker_still_needed_is_kept() {
    let runs = vec![vec![(1, Some(10))], vec![(1, None)], vec![(2, Some(20))]];
    check!(format!("{runs:?}, merge 1..3"), merged(runs.clone(), 1..3), vec![vec![(1, Some(10))], vec![(1, None), (2, Some(20))]]);
}

#[test]
fn a_marker_with_nothing_to_hide_goes() {
    let runs = vec![vec![(5, Some(1))], vec![(7, None)], vec![(8, Some(2))]];
    check!(format!("{runs:?}, merge 1..3"), merged(runs.clone(), 1..3), vec![vec![(5, Some(1))], vec![(8, Some(2))]]);
}

#[test]
fn nothing_left_means_no_run() {
    let runs = vec![vec![(1, Some(1))], vec![(1, None)], vec![(9, Some(9))]];
    check!(format!("{runs:?}, merge 0..2"), merged(runs.clone(), 0..2), vec![vec![(9, Some(9))]]);
}

#[test]
fn newer_runs_are_left_alone() {
    let runs = vec![vec![(1, Some(1))], vec![(1, Some(2))], vec![(1, None)]];
    check!(format!("{runs:?}, merge 0..2"), merged(runs.clone(), 0..2), vec![vec![(1, Some(2))], vec![(1, None)]]);
}
