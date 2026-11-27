//! Set operations on sorted streams, by merging.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Union,
    Intersect,
    Except,
}

/// `left op [ALL] right` for two ascending iterators; the result is ascending and lazy.
pub fn merge_set_op<'a>(
    left: impl Iterator<Item = i64> + 'a,
    right: impl Iterator<Item = i64> + 'a,
    op: Op,
    all: bool,
) -> Box<dyn Iterator<Item = i64> + 'a> {
    let _ = (left, right, op, all);
    todo!("3j-c4: peek both heads, take the smaller value, count its run on each side, emit as many copies as the operation says")
}
