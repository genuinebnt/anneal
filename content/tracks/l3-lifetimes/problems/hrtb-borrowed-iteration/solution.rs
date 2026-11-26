pub struct Stats<C> {
    data: C,
}

impl<C> Stats<C>
where
    for<'a> &'a C: IntoIterator<Item = &'a u32>,
{
    pub fn new(data: C) -> Self {
        Stats { data }
    }

    /// The mean, or None if empty.
    pub fn mean(&self) -> Option<f64> {
        let (sum, n) = self
            .data
            .into_iter()
            .fold((0u64, 0u64), |(s, n), &x| (s + u64::from(x), n + 1));
        (n > 0).then(|| sum as f64 / n as f64)
    }

    /// Largest minus smallest, or None if empty.
    pub fn spread(&self) -> Option<u32> {
        let max = self.data.into_iter().max()?;
        let min = self.data.into_iter().min()?;
        Some(max - min)
    }

    pub fn count_above(&self, threshold: u32) -> usize {
        self.data.into_iter().filter(|&&x| x > threshold).count()
    }
}
