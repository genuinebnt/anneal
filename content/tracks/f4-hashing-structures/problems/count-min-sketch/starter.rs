use std::hash::Hash;

/// A count-min sketch: approximate frequencies in fixed memory. `estimate` never undercounts, and overcounts by at
/// most `eps * total` with probability at least `1 - delta` when built with `with_error(eps, delta)`.
pub struct CountMin {
    // TODO: depth rows of width counters, and the running total
}

impl CountMin {
    pub fn new(width: usize, depth: usize) -> Self {
        todo!()
    }

    /// width = ceil(e / eps), depth = ceil(ln(1 / delta)).
    pub fn with_error(eps: f64, delta: f64) -> Self {
        todo!()
    }

    pub fn width(&self) -> usize {
        todo!()
    }

    pub fn depth(&self) -> usize {
        todo!()
    }

    /// Adds `count` occurrences of `item`: one counter per row, each row choosing its column with a differently seeded hash.
    pub fn add<T: Hash + ?Sized>(&mut self, item: &T, count: u64) {
        todo!()
    }

    /// Never less than the true count.
    pub fn estimate<T: Hash + ?Sized>(&self, item: &T) -> u64 {
        todo!()
    }

    /// The sum of everything added.
    pub fn total(&self) -> u64 {
        todo!()
    }

    /// Adds another sketch of the same shape; `Err(())` otherwise.
    pub fn merge(&mut self, other: &CountMin) -> Result<(), ()> {
        todo!()
    }
}
