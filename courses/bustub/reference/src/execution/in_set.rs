//! `x IN (constants)` through a hash set.

use std::collections::HashSet;

pub struct InSet {
    // @begin 3i-c1
    values: HashSet<i64>,
    has_null: bool,
    //~ _in_set: (),
    // @end
}

impl InSet {
    /// A set of the list's non-NULL values that remembers whether the list held a NULL.
    pub fn new(list: &[Option<i64>]) -> InSet {
        // @begin 3i-c1
        InSet { values: list.iter().flatten().copied().collect(), has_null: list.iter().any(|v| v.is_none()) }
        //~ todo!("3i-c1: remember the non-NULL values, and whether the list had a NULL")
        // @end
    }

    /// The number of distinct non-NULL values.
    pub fn len(&self) -> usize {
        // @begin 3i-c1
        self.values.len()
        //~ todo!("3i-c1: distinct non-NULL values")
        // @end
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0 && !self.has_null_entry()
    }

    /// Whether the list held a NULL.
    fn has_null_entry(&self) -> bool {
        // @begin 3i-c1
        self.has_null
        //~ todo!("3i-c1: did the list have a NULL")
        // @end
    }

    /// `x IN (list)`: `Some(true)`, `Some(false)` or `None` (NULL).
    pub fn contains(&self, x: Option<i64>) -> Option<bool> {
        // @begin 3i-c1
        if self.is_empty() {
            return Some(false);
        }
        match x {
            Some(v) if self.values.contains(&v) => Some(true),
            _ if x.is_none() || self.has_null => None,
            _ => Some(false),
        }
        //~ todo!("3i-c1: TRUE on a hit; NULL for a NULL x (list not empty) or a miss with a NULL in the list; FALSE otherwise")
        // @end
    }

    /// `x NOT IN (list)`.
    pub fn not_in(&self, x: Option<i64>) -> Option<bool> {
        // @begin 3i-c1
        self.contains(x).map(|b| !b)
        //~ todo!("3i-c1: the three-valued negation of contains")
        // @end
    }
}
