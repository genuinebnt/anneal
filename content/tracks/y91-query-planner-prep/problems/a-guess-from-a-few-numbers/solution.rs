/// Everything that may be kept about one route.
pub struct Summary {
    /// At most `limit` numbers. What they are is up to you.
    pub kept: Vec<i64>,
    /// How many delivery times there were.
    pub total: usize,
}

/// The position, in the sorted list, of the `i`-th of `k` kept values.
fn rank(i: usize, k: usize, total: usize) -> usize {
    i * (total - 1) / (k - 1)
}

pub fn summarise(mut times: Vec<i64>, limit: usize) -> Summary {
    times.sort_unstable();
    let total = times.len();
    let kept = if total <= limit {
        times
    } else {
        (0..limit).map(|i| times[rank(i, limit, total)]).collect()
    };
    Summary { kept, total }
}

impl Summary {
    /// A guess at how many delivery times were `x` or less.
    pub fn at_most(&self, x: i64) -> usize {
        let k = self.kept.len();
        let up_to = self.kept.partition_point(|&v| v <= x);
        if k == self.total || up_to == 0 {
            return up_to;
        }
        if up_to == k {
            return self.total;
        }
        // The truth is more than the rank of the last value passed, and at most the rank of the next one.
        let low = rank(up_to - 1, k, self.total) + 1;
        let high = rank(up_to, k, self.total);
        low + (high - low) / 2
    }
}
