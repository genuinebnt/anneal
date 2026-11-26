use std::collections::BTreeMap;
use std::ops::Range;

/// One batch of changes, sorted by key, each key at most once. `None` says the key was deleted.
pub type Run = Vec<(u32, Option<u32>)>;

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
        .filter(|&(key, value)| value.is_some() || !older.is_empty() && key < u32::MAX)
        .collect();
    runs.splice(window, (!result.is_empty()).then_some(result));
}
