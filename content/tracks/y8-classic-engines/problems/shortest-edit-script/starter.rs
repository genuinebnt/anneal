/// One step of an edit script turning `a` into `b`. Indexes are 0-based positions in `a` and `b`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// `a[i] == b[j]`: the line is in both.
    Keep(usize, usize),
    /// `a[i]` is not in `b`.
    Delete(usize),
    /// `b[j]` is not in `a`.
    Insert(usize),
}

/// A shortest edit script from `a` to `b`: the fewest `Delete`s plus `Insert`s, with the `Keep`s in between.
/// The ops walk both sequences from the start, in order. Where deleting and inserting are equally good, delete first.
pub fn diff(a: &[&str], b: &[&str]) -> Vec<Op> {
    todo!()
}
