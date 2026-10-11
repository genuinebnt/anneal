//! Live-key counts per leaf, with fast prefix sums and a k-th-key search (a Fenwick tree).

pub struct LiveCounts {
    // @begin 2d-c3
    /// 1-based Fenwick tree over the leaf counts.
    tree: Vec<i64>,
    counts: Vec<i64>,
    //~ _counts: (),
    // @end
}

impl LiveCounts {
    /// Counts for `counts.len()` leaves, starting from `counts`.
    pub fn new(counts: &[usize]) -> LiveCounts {
        // @begin 2d-c3
        let mut c = LiveCounts { tree: vec![0; counts.len() + 1], counts: vec![0; counts.len()] };
        for (i, &n) in counts.iter().enumerate() {
            c.add(i, n as i64);
        }
        c
        //~ todo!("2d-c3: build the structure from the counts")
        // @end
    }

    /// The number of leaves.
    pub fn leaves(&self) -> usize {
        // @begin 2d-c3
        self.counts.len()
        //~ todo!("2d-c3: how many leaves")
        // @end
    }

    /// The number of live keys of `leaf`.
    pub fn count(&self, leaf: usize) -> usize {
        // @begin 2d-c3
        self.counts[leaf] as usize
        //~ todo!("2d-c3: the live keys of one leaf")
        // @end
    }

    /// Changes the leaf's count by `delta`; a count never goes below 0.
    pub fn add(&mut self, leaf: usize, delta: i64) {
        // @begin 2d-c3
        let delta = delta.max(-self.counts[leaf]);
        self.counts[leaf] += delta;
        let mut i = leaf + 1;
        while i < self.tree.len() {
            self.tree[i] += delta;
            i += i & i.wrapping_neg();
        }
        //~ todo!("2d-c3: update the count and whatever makes prefix sums fast")
        // @end
    }

    /// Live keys in leaves `0..i`.
    pub fn prefix(&self, i: usize) -> usize {
        // @begin 2d-c3
        let (mut i, mut sum) = (i.min(self.counts.len()), 0i64);
        while i > 0 {
            sum += self.tree[i];
            i -= i & i.wrapping_neg();
        }
        sum as usize
        //~ todo!("2d-c3: sum of the counts before leaf i, in O(log n)")
        // @end
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
        // @begin 2d-c3
        if k >= self.total() {
            return None;
        }
        let mut pos = 0usize;
        let mut rem = k as i64;
        let mut step = self.tree.len().next_power_of_two();
        while step > 0 {
            let next = pos + step;
            if next < self.tree.len() && self.tree[next] <= rem {
                pos = next;
                rem -= self.tree[next];
            }
            step >>= 1;
        }
        Some((pos, rem as usize))
        //~ todo!("2d-c3: descend the tree to the leaf where the running sum passes k")
        // @end
    }
}
