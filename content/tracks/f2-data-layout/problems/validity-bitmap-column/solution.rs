/// A nullable `f64` column in Arrow's layout: a values buffer and a validity bitmap with one bit per row
/// (set = present). A null row's value slot holds 0.0, so a kernel like `sum` runs over `values` without
/// branching; anything that could see the placeholder (`count_gt`) masks with the bitmap.
pub struct Float64Column {
    values: Vec<f64>,
    validity: Vec<u64>,
    nulls: usize,
}

impl Float64Column {
    /// Room for `rows` rows, reserved up front: two allocations, 8 bytes and 1 bit per row.
    pub fn with_capacity(rows: usize) -> Float64Column {
        Float64Column {
            values: Vec::with_capacity(rows),
            validity: Vec::with_capacity(rows.div_ceil(64)),
            nulls: 0,
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
        let i = self.values.len();
        if i % 64 == 0 {
            self.validity.push(0);
        }
        match value {
            Some(v) => {
                self.values.push(v);
                self.validity[i / 64] |= 1 << (i % 64);
            }
            None => {
                self.values.push(0.0);
                self.nulls += 1;
            }
        }
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Row `i`; panics if `i >= len()`.
    pub fn get(&self, i: usize) -> Option<f64> {
        let v = self.values[i];
        (self.validity[i / 64] >> (i % 64) & 1 == 1).then_some(v)
    }

    pub fn null_count(&self) -> usize {
        self.nulls
    }

    /// The sum of the non-null values, in row order (0.0 when there are none). Null slots hold 0.0 and
    /// `x + 0.0 == x`, so this is one pass over `values` with no bitmap and no branches.
    pub fn sum(&self) -> f64 {
        self.values.iter().fold(0.0, |acc, v| acc + v)
    }

    /// The mean of the non-null values; `None` when there are none.
    pub fn mean(&self) -> Option<f64> {
        let n = self.len() - self.nulls;
        (n > 0).then(|| self.sum() / n as f64)
    }

    /// How many non-null values are greater than `t`: compare a word's worth of rows into a bitmask, AND
    /// it with the validity word, count the bits. A null's 0.0 would otherwise count whenever `t < 0`.
    pub fn count_gt(&self, t: f64) -> usize {
        self.values
            .chunks(64)
            .zip(&self.validity)
            .map(|(chunk, &valid)| {
                let mut hits: u64 = 0;
                for (j, &v) in chunk.iter().enumerate() {
                    hits |= ((v > t) as u64) << j;
                }
                (hits & valid).count_ones() as usize
            })
            .sum()
    }
}
