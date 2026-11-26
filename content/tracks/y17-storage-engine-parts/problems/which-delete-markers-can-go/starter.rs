use std::ops::Range;

/// One batch of changes, sorted by key, each key at most once. `None` says the key was deleted.
pub type Run = Vec<(u32, Option<u32>)>;

/// The value of `key`: the newest run that mentions it decides. `runs` is ordered oldest first.
pub fn lookup(runs: &[Run], key: u32) -> Option<u32> {
    todo!()
}

/// Replaces the neighbouring runs `runs[window]` by at most one run.
pub fn merge_window(runs: &mut Vec<Run>, window: Range<usize>) {
    todo!()
}
