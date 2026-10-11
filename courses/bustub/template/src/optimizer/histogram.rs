//! An equi-width histogram for selectivity estimates.

pub struct Histogram {
    _hist: (),
}

impl Histogram {
    /// `buckets >= 1`. An empty input gives a histogram that estimates 0 everywhere.
    pub fn build(values: &[i64], buckets: usize) -> Histogram {
        todo!("3h-c5: min, max and a count per equal-width bucket")
    }

    // TODO(3h-c5): helpers of your own

    /// The number of values the histogram was built from.
    pub fn count(&self) -> u64 {
        todo!("3h-c5: the number of values")
    }

    /// The number of values in each bucket, lowest bucket first.
    pub fn bucket_counts(&self) -> &[u64] {
        todo!("3h-c5: the count of each bucket")
    }

    /// Estimated number of values `<= x`.
    pub fn estimate_le(&self, x: i64) -> f64 {
        todo!("3h-c5: the buckets below plus a linear share of the bucket x is in")
    }

    /// Estimated number of values in `lo..=hi`.
    pub fn estimate_range(&self, lo: i64, hi: i64) -> f64 {
        if hi < lo {
            return 0.0;
        }
        self.estimate_le(hi) - self.estimate_le(lo - 1)
    }
}
