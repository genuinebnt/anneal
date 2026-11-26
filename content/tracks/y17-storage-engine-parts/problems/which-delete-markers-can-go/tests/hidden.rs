use solution::*;

#[allow(dead_code)]
fn merged(mut runs: Vec<Run>, window: std::ops::Range<usize>) -> Vec<Run> {
    merge_window(&mut runs, window);
    runs
}

#[test]
fn only_the_newest_older_entry_matters() {
    // the old value is already deleted by run 1, so the marker in run 2 hides nothing
    let runs = vec![vec![(1, Some(10))], vec![(1, None)], vec![(1, None), (2, Some(2))]];
    check!(format!("{runs:?}, merge 2..3"), merged(runs.clone(), 2..3), vec![vec![(1, Some(10))], vec![(1, None)], vec![(2, Some(2))]]);
}

#[test]
fn a_value_deleted_and_set_again_below() {
    let runs = vec![vec![(1, None)], vec![(1, Some(3))], vec![(1, None)]];
    check!(format!("{runs:?}, merge 2..3"), merged(runs.clone(), 2..3), runs.clone());
}

#[test]
fn a_value_equal_to_an_older_one_stays() {
    let runs = vec![vec![(4, Some(4))], vec![(4, Some(4))]];
    check!(format!("{runs:?}, merge 1..2"), merged(runs.clone(), 1..2), runs.clone());
}

#[test]
fn a_window_from_the_start_drops_every_marker() {
    let runs = vec![vec![(1, None), (2, Some(1))], vec![(2, None), (3, None)], vec![(3, Some(3))]];
    check!(format!("{runs:?}, merge 0..2"), merged(runs.clone(), 0..2), vec![vec![(3, Some(3))]]);
}

#[test]
fn the_extreme_keys() {
    let runs = vec![vec![(0, Some(0)), (u32::MAX, Some(1))], vec![(0, None), (u32::MAX, None)], vec![(7, None)]];
    check!(format!("{runs:?}, merge 1..3"), merged(runs.clone(), 1..3), vec![vec![(0, Some(0)), (u32::MAX, Some(1))], vec![(0, None), (u32::MAX, None)]]);
}

#[test]
fn one_run_alone_is_cleaned() {
    check!("a single run of markers", merged(vec![vec![(1, None), (2, None)]], 0..1), Vec::<Run>::new());
}

#[test]
fn many_markers_over_large_old_runs() {
    let big: Run = (0..200_000).map(|k| (k * 2, Some(k))).collect();
    let markers: Run = (0..100_000).map(|k| (k, None)).collect();
    let mut runs = vec![big.clone(), vec![(1, Some(1))], markers];
    merge_window(&mut runs, 2..3);
    // the even keys under 100 000 and key 1 have older values to hide
    check!("markers kept", runs.last().map(Vec::len), Some(50_001));
}

#[test]
fn random_windows() {
    let mut rng = anneal_prelude::Rng::new(170801);
    for case in 0..3000 {
        let count = 1 + rng.below(5);
        let mut runs = Vec::new();
        for _ in 0..count {
            let mut run = std::collections::BTreeMap::new();
            let n = rng.below(5);
            for _ in 0..n {
                let key = rng.below(6) as u32;
                let value = if rng.bool() { Some(rng.below(3) as u32) } else { None };
                run.insert(key, value);
            }
            runs.push(run.into_iter().collect::<Run>());
        }
        let start = rng.below(count);
        let end = start + 1 + rng.below(count - start);
        let story = format!("case {case}: {runs:?}, merge {start}..{end}");
        let got = merged(runs.clone(), start..end);
        let produced = got.len() + (end - start) - runs.len();
        check!(format!("{story}: at most one run replaces the window, gave {got:?}"), produced <= 1, true);
        check!(format!("{story}: older runs untouched, gave {got:?}"), got[..start] == runs[..start], true);
        check!(format!("{story}: newer runs untouched, gave {got:?}"), got[start + produced..] == runs[end..], true);
        for key in 0..6 {
            check!(format!("{story}: lookup {key}, gave {got:?}"), lookup(&got, key), lookup(&runs, key));
        }
        if produced == 1 {
            let run = &got[start];
            check!(format!("{story}: sorted, once each, not empty: {run:?}"), !run.is_empty() && run.windows(2).all(|w| w[0].0 < w[1].0), true);
            for &(key, value) in run {
                if value.is_none() {
                    let mut without = got.clone();
                    without[start].retain(|e| e.0 != key);
                    check!(format!("{story}: the marker for {key} changes an answer, gave {got:?}"), lookup(&without[..=start], key) != lookup(&got[..=start], key), true);
                }
            }
        }
        // every key of the window that still has a value must still be written in the new run
        let newest_in_window = |key: u32| runs[start..end].iter().rev().find_map(|r| r.iter().find(|e| e.0 == key).map(|e| e.1));
        for key in 0..6 {
            if let Some(Some(v)) = newest_in_window(key) {
                check!(format!("{story}: the value {v} of key {key} stays in the new run, gave {got:?}"), produced == 1 && got[start].contains(&(key, Some(v))), true);
            }
        }
    }
}
