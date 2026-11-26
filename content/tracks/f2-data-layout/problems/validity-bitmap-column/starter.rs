/// A nullable `f64` column: one row per value, `None` for NULL.
pub struct Float64Column {
    rows: Vec<Option<f64>>,
}

impl Float64Column {
    /// Room for `rows` rows, reserved up front.
    pub fn with_capacity(rows: usize) -> Float64Column {
        Float64Column {
            rows: Vec::with_capacity(rows),
        }
    }

    pub fn from_options(rows: &[Option<f64>]) -> Float64Column {
        let mut c = Float64Column::with_capacity(rows.len());
        for &r in rows {
            c.push(r);
        }
        c
    }

    pub fn push(&mut self, value: Option<f64>) {
        self.rows.push(value);
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Row `i`; panics if `i >= len()`.
    pub fn get(&self, i: usize) -> Option<f64> {
        self.rows[i]
    }

    pub fn null_count(&self) -> usize {
        self.rows.iter().filter(|r| r.is_none()).count()
    }

    /// The sum of the non-null values, in row order (0.0 when there are none).
    pub fn sum(&self) -> f64 {
        self.rows.iter().flatten().fold(0.0, |acc, v| acc + v)
    }

    /// The mean of the non-null values; `None` when there are none.
    pub fn mean(&self) -> Option<f64> {
        let n = self.len() - self.null_count();
        (n > 0).then(|| self.sum() / n as f64)
    }

    /// How many non-null values are greater than `t`.
    pub fn count_gt(&self, t: f64) -> usize {
        self.rows.iter().flatten().filter(|&&v| v > t).count()
    }
}
