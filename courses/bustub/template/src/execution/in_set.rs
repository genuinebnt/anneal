//! `x IN (constants)` through a hash set.

use std::collections::HashSet;

pub struct InSet {
    _in_set: (),
}

impl InSet {
    /// A set of the list's non-NULL values that remembers whether the list held a NULL.
    pub fn new(list: &[Option<i64>]) -> InSet {
        todo!("3i-c1: remember the non-NULL values, and whether the list had a NULL")
    }

    /// The number of distinct non-NULL values.
    pub fn len(&self) -> usize {
        todo!("3i-c1: distinct non-NULL values")
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0 && !self.has_null_entry()
    }

    /// Whether the list held a NULL.
    fn has_null_entry(&self) -> bool {
        todo!("3i-c1: did the list have a NULL")
    }

    /// `x IN (list)`: `Some(true)`, `Some(false)` or `None` (NULL).
    pub fn contains(&self, x: Option<i64>) -> Option<bool> {
        todo!("3i-c1: TRUE on a hit; NULL for a NULL x (list not empty) or a miss with a NULL in the list; FALSE otherwise")
    }

    /// `x NOT IN (list)`.
    pub fn not_in(&self, x: Option<i64>) -> Option<bool> {
        todo!("3i-c1: the three-valued negation of contains")
    }
}
