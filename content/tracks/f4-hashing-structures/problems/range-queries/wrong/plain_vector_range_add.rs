pub struct PrefixSums {
    /// 1-based: `tree[i]` is the sum of the `i & -i` slots ending at slot `i - 1`.
    tree: Vec<i64>,
}

impl PrefixSums {
    pub fn new(values: &[i64]) -> Self {
        let mut sums = PrefixSums {
            tree: vec![0; values.len() + 1],
        };
        for (i, &v) in values.iter().enumerate() {
            sums.add(i, v);
        }
        sums
    }

    pub fn add(&mut self, i: usize, delta: i64) {
        let mut i = i + 1;
        while i < self.tree.len() {
            self.tree[i] += delta;
            i += i & i.wrapping_neg();
        }
    }

    pub fn prefix(&self, mut end: usize) -> i64 {
        let mut total = 0;
        while end > 0 {
            total += self.tree[end];
            end &= end - 1;
        }
        total
    }

    pub fn sum(&self, l: usize, r: usize) -> i64 {
        self.prefix(r) - self.prefix(l)
    }

    pub fn first_reaching(&self, mut target: i64) -> Option<usize> {
        let slots = self.tree.len() - 1;
        if slots == 0 {
            return None;
        }
        // `position` slots are known to add up to less than the original target.
        let mut position = 0;
        let mut step = slots.next_power_of_two();
        while step > 0 {
            if position + step <= slots && self.tree[position + step] < target {
                position += step;
                target -= self.tree[position];
            }
            step /= 2;
        }
        (position < slots).then_some(position)
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
    values: Vec<i64>,
}

impl RangeAdder {
    pub fn new(values: &[i64]) -> Self {
        RangeAdder {
            values: values.to_vec(),
        }
    }

    pub fn add_range(&mut self, l: usize, r: usize, delta: i64) {
        for x in &mut self.values[l..r] {
            *x += delta;
        }
    }

    pub fn sum(&mut self, l: usize, r: usize) -> i64 {
        self.values[l..r].iter().sum()
    }
}
