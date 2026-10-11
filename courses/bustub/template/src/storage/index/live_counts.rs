//! Live-key counts per leaf, with fast prefix sums and a k-th-key search (a Fenwick tree).

pub struct LiveCounts {
    _counts: (),
}

impl LiveCounts {
    /// Counts for `counts.len()` leaves, starting from `counts`.
    pub fn new(counts: &[usize]) -> LiveCounts {
        todo!("2d-c3: build the structure from the counts")
    }

    /// The number of leaves.
    pub fn leaves(&self) -> usize {
        todo!("2d-c3: how many leaves")
    }

    /// The number of live keys of `leaf`.
    pub fn count(&self, leaf: usize) -> usize {
        todo!("2d-c3: the live keys of one leaf")
    }

    /// Changes the leaf's count by `delta`; a count never goes below 0.
    pub fn add(&mut self, leaf: usize, delta: i64) {
        todo!("2d-c3: update the count and whatever makes prefix sums fast")
    }

    /// Live keys in leaves `0..i`.
    pub fn prefix(&self, i: usize) -> usize {
        todo!("2d-c3: sum of the counts before leaf i, in O(log n)")
    }

    pub fn range(&self, l: usize, r: usize) -> usize {
        if r <= l {
            0
        } else {
            self.prefix(r) - self.prefix(l)
        }
    }

    pub fn total(&self) -> usize {
        self.prefix(self.leaves())
    }

    /// The leaf holding the `k`-th live key (from 0) and the key's offset among that leaf's live keys.
    pub fn find(&self, k: usize) -> Option<(usize, usize)> {
        todo!("2d-c3: descend the tree to the leaf where the running sum passes k")
    }
}
