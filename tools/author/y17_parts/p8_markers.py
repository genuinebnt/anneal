from y17_parts.common import broken

TYPES = r'''use std::ops::Range;

/// One batch of changes, sorted by key, each key at most once. `None` says the key was deleted.
pub type Run = Vec<(u32, Option<u32>)>;
'''

STARTER = TYPES + r'''
/// The value of `key`: the newest run that mentions it decides. `runs` is ordered oldest first.
pub fn lookup(runs: &[Run], key: u32) -> Option<u32> {
    todo!()
}

/// Replaces the neighbouring runs `runs[window]` by at most one run.
pub fn merge_window(runs: &mut Vec<Run>, window: Range<usize>) {
    todo!()
}
'''

SOLUTION = r'''use std::collections::BTreeMap;
''' + TYPES + r'''
/// What the newest run that mentions `key` says about it.
fn newest_entry(runs: &[Run], key: u32) -> Option<Option<u32>> {
    runs.iter().rev().find_map(|run| {
        let at = run.binary_search_by_key(&key, |e| e.0).ok()?;
        Some(run[at].1)
    })
}

/// The value of `key`: the newest run that mentions it decides. `runs` is ordered oldest first.
pub fn lookup(runs: &[Run], key: u32) -> Option<u32> {
    newest_entry(runs, key).flatten()
}

/// Replaces the neighbouring runs `runs[window]` by at most one run.
pub fn merge_window(runs: &mut Vec<Run>, window: Range<usize>) {
    // oldest first, so that a later insert is a newer version winning
    let mut merged = BTreeMap::new();
    for run in &runs[window.clone()] {
        merged.extend(run.iter().copied());
    }
    let older = &runs[..window.start];
    let result: Run = merged
        .into_iter()
        // a delete marker earns its place only if, without it, an older value would show
        .filter(|&(key, value)| value.is_some() || newest_entry(older, key).flatten().is_some())
        .collect();
    runs.splice(window, (!result.is_empty()).then_some(result));
}
'''

HARNESS = r'''
#[allow(dead_code)]
fn merged(mut runs: Vec<Run>, window: std::ops::Range<usize>) -> Vec<Run> {
    merge_window(&mut runs, window);
    runs
}
'''

VISIBLE = HARNESS + r'''
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
'''

HIDDEN = HARNESS + r'''
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
'''

PROBLEM = dict(
    slug="which-delete-markers-can-go", title="Which delete markers can go", level="medium", stage="many-layers",
    tags=["merging runs", "delete markers", "space reclaim", "invariants"],
    statement=r'''
A store keeps its changes in batches called **runs**, ordered oldest first. A newer run overrides older ones. A deleted key is written as a marker, `(key, None)`. The runs pile up, so reads get slow and deleted data takes space.

**`lookup(runs, key)`**: the newest run that mentions `key` decides. The result is `None` if it holds a marker, or if no run mentions the key.

**`merge_window(runs, window)`** replaces the neighbouring runs `runs[window]` with **at most one** run in their place. The window is not empty and lies inside `runs`. Runs outside it must not change.

1. `lookup` must give the same answer for every key as before.
2. The new run is sorted by key and holds each key at most once.
3. A value which is the newest entry for its key inside the window is kept, even if an older run says the same.
4. A marker is kept **only if removing it would change what the runs up to and including the new one answer**.
5. A run with no entries is not left behind.

Runs can hold hundreds of thousands of entries. The window may hold a great many markers while the older runs are large.
''',
    starter=STARTER, solution=SOLUTION, visible=[VISIBLE], hidden=[HIDDEN], use="use solution::*;",
    constraints=["Runs of up to 200 000 entries"],
    hints=[
        ("approach", "Take one marker in the window and ask which other runs can make it matter. Then ask which of their entries for that key is the one that counts."),
        ("rust", "Inserting runs into a `BTreeMap` oldest first leaves the newest version of each key. `Vec::splice` replaces a range, and an `Option` can serve as the replacement. `binary_search_by_key` searches a sorted run."),
        ("edge", "A window that does not start at the oldest run may still drop a marker, and a marker over an older marker hides nothing. Newer runs are not your concern."),
    ],
    notes=("A marker matters only when an older value would show through without it. That depends on the newest entry for the key below the window: a value means keep, a marker or nothing means drop. Runs are sorted, so each check is a binary search per older run.", "O(window x log + markers x older runs x log)", "O(window)"),
    follow_up="1) This merges any neighbouring window. Why would merging two runs that are not neighbours give wrong answers? 2) If you always merge everything into one run, what becomes of markers, and what do you pay for that simplicity? 3) How much do you rewrite in total if you merge every time there are four runs of about the same size?",
    teaches=["A delete marker can be dropped only when no older value is left for it to hide.", "Merging must keep what every read would have answered: check that as an invariant.", "Only neighbouring runs can be merged, because order is what says which version is newer."],
    wrong={
        "drops_every_marker": broken(SOLUTION, (".filter(|&(key, value)| value.is_some() || newest_entry(older, key).flatten().is_some())", ".filter(|&(_, value)| value.is_some())"), ("    let older = &runs[..window.start];\n", "")),
        "keeps_markers_unless_oldest": broken(SOLUTION, ("value.is_some() || newest_entry(older, key).flatten().is_some())", "value.is_some() || !older.is_empty() && key < u32::MAX)")),
        "any_older_value_counts": broken(SOLUTION, ("value.is_some() || newest_entry(older, key).flatten().is_some())", "value.is_some()\n            || older\n                .iter()\n                .any(|r| r.binary_search_by_key(&key, |e| e.0).is_ok_and(|i| r[i].1.is_some())))")),
        "older_versions_win": broken(SOLUTION, ("    for run in &runs[window.clone()] {", "    for run in runs[window.clone()].iter().rev() {")),
        "searches_older_runs_one_by_one": broken(SOLUTION, ("        let at = run.binary_search_by_key(&key, |e| e.0).ok()?;\n        Some(run[at].1)", "        run.iter().find(|e| e.0 == key).map(|e| e.1)")),
        "leaves_an_empty_run": broken(SOLUTION, ("(!result.is_empty()).then_some(result)", "Some(result)")),
    },
)
