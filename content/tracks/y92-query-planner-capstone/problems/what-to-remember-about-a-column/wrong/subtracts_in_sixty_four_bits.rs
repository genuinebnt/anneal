// ---- column summaries ----

/// A summary cuts a column's sorted values into this many slices of (nearly) equal size.
pub const SLICES: usize = 32;
/// At most this many values are listed with their own counts.
pub const COMMON: usize = 8;

/// All the planner remembers about one column.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnStats {
    /// All rows, those with a missing value included.
    pub rows: usize,
    pub missing: usize,
    /// How many different values are present.
    pub distinct: usize,
    /// `SLICES + 1` values that bound the slices; empty if no value is present.
    pub cuts: Vec<i64>,
    /// Unusually frequent values with their exact counts, most frequent first.
    pub common: Vec<(i64, usize)>,
}

pub fn summarise(values: &[Option<i64>]) -> ColumnStats {
    let mut sorted: Vec<i64> = values.iter().flatten().copied().collect();
    sorted.sort_unstable();
    let present = sorted.len();
    let mut counts: Vec<(i64, usize)> = Vec::new();
    for &v in &sorted {
        match counts.last_mut() {
            Some((value, count)) if *value == v => *count += 1,
            _ => counts.push((v, 1)),
        }
    }
    let distinct = counts.len();
    let cuts = if present == 0 {
        Vec::new()
    } else {
        (0..=SLICES)
            .map(|i| sorted[i * (present - 1) / SLICES])
            .collect()
    };
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    counts.truncate(COMMON);
    // more than 1.25 times the average count, in whole numbers
    counts.retain(|&(_, count)| 4 * count * distinct > 5 * present);
    ColumnStats {
        rows: values.len(),
        missing: values.len() - present,
        distinct,
        cuts,
        common: counts,
    }
}

impl ColumnStats {
    pub fn present(&self) -> usize {
        self.rows - self.missing
    }

    /// The estimated number of rows whose value is `value`.
    pub fn equal_to(&self, value: i64) -> f64 {
        let (Some(&low), Some(&high)) = (self.cuts.first(), self.cuts.last()) else {
            return 0.0;
        };
        if value < low || value > high {
            return 0.0;
        }
        if let Some(&(_, count)) = self.common.iter().find(|c| c.0 == value) {
            return count as f64;
        }
        let listed: usize = self.common.iter().map(|c| c.1).sum();
        match self.distinct - self.common.len() {
            0 => 0.0,
            others => (self.present() - listed) as f64 / others as f64,
        }
    }

    /// The estimated number of present values that are at most `value`.
    fn at_most(&self, value: i64) -> f64 {
        let (Some(&low), Some(&high)) = (self.cuts.first(), self.cuts.last()) else {
            return 0.0;
        };
        if value < low {
            return 0.0;
        }
        if value >= high {
            return self.present() as f64;
        }
        let slice = self.cuts[..SLICES].partition_point(|&c| c <= value) - 1;
        let (a, b) = (self.cuts[slice], self.cuts[slice + 1]);
        let inside = (value - a + 1) as f64 / (b - a + 1) as f64;
        (slice as f64 + inside) / SLICES as f64 * self.present() as f64
    }

    /// The estimated number of rows whose value is in `low..=high`.
    pub fn between(&self, low: i64, high: i64) -> f64 {
        if low > high {
            return 0.0;
        }
        if low == high {
            return self.equal_to(low);
        }
        let below = low.checked_sub(1).map_or(0.0, |v| self.at_most(v));
        (self.at_most(high) - below).max(0.0)
    }
}
