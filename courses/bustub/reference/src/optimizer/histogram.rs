//! An equi-width histogram for selectivity estimates.

pub struct Histogram {
    // @begin 3h-c5
    min: i64,
    max: i64,
    counts: Vec<u64>,
    n: u64,
    //~ _hist: (),
    // @end
}

impl Histogram {
    /// `buckets >= 1`. An empty input gives a histogram that estimates 0 everywhere.
    pub fn build(values: &[i64], buckets: usize) -> Histogram {
        // @begin 3h-c5
        let buckets = buckets.max(1);
        let (Some(&min), Some(&max)) = (values.iter().min(), values.iter().max()) else {
            return Histogram { min: 0, max: 0, counts: vec![0; buckets], n: 0 };
        };
        let mut h = Histogram { min, max, counts: vec![0; buckets], n: values.len() as u64 };
        for &v in values {
            let b = h.bucket_of(v);
            h.counts[b] += 1;
        }
        h
        //~ todo!("3h-c5: min, max and a count per equal-width bucket")
        // @end
    }

    // @begin 3h-c5
    fn width(&self) -> f64 {
        (self.max - self.min + 1) as f64 / self.counts.len() as f64
    }

    fn bucket_of(&self, v: i64) -> usize {
        (((v - self.min) as f64 / self.width()) as usize).min(self.counts.len() - 1)
    }
    //~ // TODO(3h-c5): helpers of your own
    // @end

    /// The number of values the histogram was built from.
    pub fn count(&self) -> u64 {
        // @begin 3h-c5
        self.n
        //~ todo!("3h-c5: the number of values")
        // @end
    }

    /// The number of values in each bucket, lowest bucket first.
    pub fn bucket_counts(&self) -> &[u64] {
        // @begin 3h-c5
        &self.counts
        //~ todo!("3h-c5: the count of each bucket")
        // @end
    }

    /// Estimated number of values `<= x`.
    pub fn estimate_le(&self, x: i64) -> f64 {
        // @begin 3h-c5
        if self.n == 0 || x < self.min {
            return 0.0;
        }
        if x >= self.max {
            return self.n as f64;
        }
        let b = self.bucket_of(x);
        let below: u64 = self.counts[..b].iter().sum();
        let lo = self.min as f64 + b as f64 * self.width();
        let share = ((x as f64 + 1.0 - lo) / self.width()).clamp(0.0, 1.0);
        below as f64 + share * self.counts[b] as f64
        //~ todo!("3h-c5: the buckets below plus a linear share of the bucket x is in")
        // @end
    }

    /// Estimated number of values in `lo..=hi`.
    pub fn estimate_range(&self, lo: i64, hi: i64) -> f64 {
        if hi < lo {
            return 0.0;
        }
        self.estimate_le(hi) - self.estimate_le(lo - 1)
    }
}
