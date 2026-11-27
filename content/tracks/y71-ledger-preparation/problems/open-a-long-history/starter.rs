use std::marker::PhantomData;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// Never given to another entry.
    pub id: u64,
    pub amount: i64,
}

/// A saved starting point.
#[derive(Debug, Clone)]
pub struct Bookmark<S> {
    // TODO: your fields (this one only makes the starter compile)
    state: PhantomData<S>,
}

pub fn mark<S: Clone>(log: &[Entry], rules: u32, state: &S) -> Bookmark<S> {
    todo!()
}

pub fn load<S: Clone + Default>(
    log: &[Entry],
    rules: u32,
    bookmark: Option<&Bookmark<S>>,
    apply: impl FnMut(S, &Entry) -> S,
) -> S {
    todo!()
}

pub fn due(before: usize, after: usize, every: usize) -> bool {
    todo!()
}
