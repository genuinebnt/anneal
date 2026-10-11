//! A sorted set of numbers that answers "how many are smaller?" and "which is the i-th?" quickly: the order statistics a B+ tree can offer
//! when every inner entry also remembers how many keys lie below it.

/// A set of `u64`s kept in sorted order.
pub struct RankedSet {
    _ranked: (),
}

impl RankedSet {
    /// An empty set.
    pub fn new() -> RankedSet {
        todo!("2a-c1: an empty set")
    }

    /// Adds `x`; false if it was already there.
    pub fn insert(&mut self, x: u64) -> bool {
        todo!("2a-c1: keep the numbers in order")
    }

    /// Removes `x`; false if it was not there.
    pub fn remove(&mut self, x: u64) -> bool {
        todo!("2a-c1: remove it, keeping the order")
    }

    /// The number of numbers held.
    pub fn len(&self) -> usize {
        todo!("2a-c1: how many numbers")
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// How many numbers are strictly smaller than `x` (`x` need not be in the set). Must not look at every number.
    pub fn rank(&self, x: u64) -> usize {
        todo!("2a-c1: binary search for the count of smaller numbers")
    }

    /// The `i`-th smallest number (0 is the smallest), or `None` if there are `i` or fewer.
    pub fn select(&self, i: usize) -> Option<u64> {
        todo!("2a-c1: the number with i smaller ones")
    }

    /// How many numbers lie in `lo..hi` (`lo` included, `hi` not). `0` when `hi <= lo`.
    pub fn count_range(&self, lo: u64, hi: u64) -> usize {
        todo!("2a-c1: how many numbers are in the half-open range")
    }
}

impl Default for RankedSet {
    fn default() -> Self {
        RankedSet::new()
    }
}
