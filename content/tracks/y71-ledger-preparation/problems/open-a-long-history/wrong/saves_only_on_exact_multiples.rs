#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// Never given to another entry.
    pub id: u64,
    pub amount: i64,
}

/// A saved starting point.
#[derive(Debug, Clone)]
pub struct Bookmark<S> {
    rules: u32,
    covers: usize,
    last_id: Option<u64>,
    state: S,
}

fn last_id(log: &[Entry], covers: usize) -> Option<u64> {
    covers.checked_sub(1).map(|i| log[i].id)
}

pub fn mark<S: Clone>(log: &[Entry], rules: u32, state: &S) -> Bookmark<S> {
    Bookmark {
        rules,
        covers: log.len(),
        last_id: last_id(log, log.len()),
        state: state.clone(),
    }
}

pub fn load<S: Clone + Default>(
    log: &[Entry],
    rules: u32,
    bookmark: Option<&Bookmark<S>>,
    apply: impl FnMut(S, &Entry) -> S,
) -> S {
    let good = bookmark.filter(|b| {
        b.rules == rules && b.covers <= log.len() && last_id(log, b.covers) == b.last_id
    });
    let (start, tail) = match good {
        Some(b) => (b.state.clone(), &log[b.covers..]),
        None => (S::default(), log),
    };
    tail.iter().fold(start, apply)
}

pub fn due(before: usize, after: usize, every: usize) -> bool {
    every != 0 && after > before && after % every == 0
}
