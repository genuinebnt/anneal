/// A time series: `points[i]` was reported by `labels[i]`.
pub struct Series {
    pub points: Vec<i64>,
    pub labels: Vec<String>,
}

impl Series {
    pub fn new() -> Self {
        Series {
            points: Vec::new(),
            labels: Vec::new(),
        }
    }

    /// Records `x`, reported by `label`. Returns the label of the record holder after this point (the `String`
    /// stored in `labels`, not a copy) and how far `x` beat the old record: `None` when it didn't beat it or when
    /// it's the first point. The record holder is the first label that reported the maximum, so a tie doesn't
    /// take the record.
    pub fn record(&mut self, x: i64, label: String) -> (&str, Option<i64>) {
        let best = self.points.iter().copied().max();
        let holder = best.map(|b| self.points.iter().position(|&p| p == b).unwrap());
        self.points.push(x);
        self.labels.push(label);
        let newest = self.labels.len() - 1;
        match (best, holder) {
            (Some(b), Some(i)) if x <= b => (&self.labels[i], None),
            (Some(b), _) => (&self.labels[newest], Some(x - b)),
            _ => (&self.labels[newest], None),
        }
    }
}
