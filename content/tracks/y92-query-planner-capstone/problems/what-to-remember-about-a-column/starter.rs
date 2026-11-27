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
    todo!()
}

impl ColumnStats {
    pub fn present(&self) -> usize {
        self.rows - self.missing
    }

    /// The estimated number of rows whose value is `value`.
    pub fn equal_to(&self, value: i64) -> f64 {
        todo!()
    }

    /// The estimated number of rows whose value is in `low..=high`.
    pub fn between(&self, low: i64, high: i64) -> f64 {
        todo!()
    }
}
