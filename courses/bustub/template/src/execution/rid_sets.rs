//! Combining the row ids of two index scans.

/// The ids in `a` or `b`, in order, an id that is in both once. Both inputs are sorted.
pub fn union_sorted(a: &[u64], b: &[u64]) -> Vec<u64> {
    todo!("3e-c4: merge, emitting an id that is in both once")
}

/// The ids in both `a` and `b`, in order.
pub fn intersect_sorted(a: &[u64], b: &[u64]) -> Vec<u64> {
    todo!("3e-c4: advance the smaller side; emit when they are equal")
}

/// The ids of `a` that `b` does not have, in order.
pub fn difference_sorted(a: &[u64], b: &[u64]) -> Vec<u64> {
    todo!("3e-c4: the ids of `a` that `b` does not have")
}
