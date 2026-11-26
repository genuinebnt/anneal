pub struct PrefixSums {
    prefix: Vec<i64>,
}

impl PrefixSums {
    pub fn new(values: &[i64]) -> Self {
        let mut prefix = vec![0];
        for &v in values {
            prefix.push(prefix[prefix.len() - 1] + v);
        }
        PrefixSums { prefix }
    }

    pub fn add(&mut self, i: usize, delta: i64) {
        for total in &mut self.prefix[i + 1..] {
            *total += delta;
        }
    }

    pub fn prefix(&self, end: usize) -> i64 {
        self.prefix[end]
    }

    pub fn sum(&self, l: usize, r: usize) -> i64 {
        self.prefix(r) - self.prefix(l)
    }

    pub fn first_reaching(&self, target: i64) -> Option<usize> {
        let i = self.prefix[1..].partition_point(|&t| t < target);
        (i + 1 < self.prefix.len()).then_some(i)
    }
}

/// How values of a range are summarised. `join` is associative; `empty` changes nothing when joined.
pub trait Summary: Clone {
    fn empty() -> Self;
    fn join(left: &Self, right: &Self) -> Self;
}

pub struct Combine<T: Summary> {
    len: usize,
    /// Leaves at `len..2 * len`; node `i` joins `2i` and `2i + 1`.
    tree: Vec<T>,
}

impl<T: Summary> Combine<T> {
    pub fn new(values: &[T]) -> Self {
        let len = values.len();
        let mut tree = vec![T::empty(); len];
        tree.extend_from_slice(values);
        for i in (1..len).rev() {
            tree[i] = T::join(&tree[2 * i], &tree[2 * i + 1]);
        }
        Combine { len, tree }
    }

    pub fn set(&mut self, i: usize, value: T) {
        let mut i = i + self.len;
        self.tree[i] = value;
        while i > 1 {
            i /= 2;
            self.tree[i] = T::join(&self.tree[2 * i], &self.tree[2 * i + 1]);
        }
    }

    pub fn query(&self, l: usize, r: usize) -> T {
        let (mut l, mut r) = (l + self.len, r + self.len);
        let (mut left, mut right) = (T::empty(), T::empty());
        while l < r {
            if l % 2 == 1 {
                left = T::join(&left, &self.tree[l]);
                l += 1;
            }
            if r % 2 == 1 {
                r -= 1;
                right = T::join(&self.tree[r], &right);
            }
            l /= 2;
            r /= 2;
        }
        T::join(&left, &right)
    }
}

pub struct RangeAdder {
    len: usize,
    total: Vec<i64>,
    /// Added to every slot under a node, and not yet passed to its children.
    pending: Vec<i64>,
}

impl RangeAdder {
    pub fn new(values: &[i64]) -> Self {
        let len = values.len();
        let mut adder = RangeAdder {
            len,
            total: vec![0; 4 * len.max(1)],
            pending: vec![0; 4 * len.max(1)],
        };
        if len > 0 {
            adder.build(1, 0, len, values);
        }
        adder
    }

    fn build(&mut self, node: usize, lo: usize, hi: usize, values: &[i64]) {
        if hi - lo == 1 {
            self.total[node] = values[lo];
            return;
        }
        let mid = lo + (hi - lo) / 2;
        self.build(2 * node, lo, mid, values);
        self.build(2 * node + 1, mid, hi, values);
        self.total[node] = self.total[2 * node] + self.total[2 * node + 1];
    }

    fn apply(&mut self, node: usize, slots: usize, delta: i64) {
        self.total[node] += delta * slots as i64;
        self.pending[node] += delta;
    }

    fn push_down(&mut self, node: usize, lo: usize, mid: usize, hi: usize) {
        let delta = std::mem::take(&mut self.pending[node]);
        if delta != 0 {
            self.apply(2 * node, mid - lo, delta);
            self.apply(2 * node + 1, hi - mid, delta);
        }
    }

    pub fn add_range(&mut self, l: usize, r: usize, delta: i64) {
        if l < r {
            self.add_at(1, (0, self.len), (l, r), delta);
        }
    }

    fn add_at(
        &mut self,
        node: usize,
        (lo, hi): (usize, usize),
        (l, r): (usize, usize),
        delta: i64,
    ) {
        if r <= lo || hi <= l {
            return;
        }
        if l <= lo && hi <= r {
            self.apply(node, hi - lo, delta);
            return;
        }
        let mid = lo + (hi - lo) / 2;
        self.push_down(node, lo, mid, hi);
        self.add_at(2 * node, (lo, mid), (l, r), delta);
        self.add_at(2 * node + 1, (mid, hi), (l, r), delta);
        self.total[node] = self.total[2 * node] + self.total[2 * node + 1];
    }

    pub fn sum(&mut self, l: usize, r: usize) -> i64 {
        if l < r {
            self.sum_at(1, (0, self.len), (l, r))
        } else {
            0
        }
    }

    fn sum_at(&mut self, node: usize, (lo, hi): (usize, usize), (l, r): (usize, usize)) -> i64 {
        if r <= lo || hi <= l {
            return 0;
        }
        if l <= lo && hi <= r {
            return self.total[node];
        }
        let mid = lo + (hi - lo) / 2;
        self.push_down(node, lo, mid, hi);
        self.sum_at(2 * node, (lo, mid), (l, r)) + self.sum_at(2 * node + 1, (mid, hi), (l, r))
    }
}
