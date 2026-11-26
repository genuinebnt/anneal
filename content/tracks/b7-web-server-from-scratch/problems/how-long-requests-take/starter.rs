pub struct Timings {
    // TODO
}

impl Timings {
    /// Panics unless `bounds` strictly increase.
    pub fn new(bounds: &[u64]) -> Timings {
        todo!()
    }

    pub fn record(&self, v: u64) {
        todo!()
    }

    pub fn count(&self) -> u64 {
        todo!()
    }

    pub fn sum(&self) -> u64 {
        todo!()
    }

    pub fn max(&self) -> u64 {
        todo!()
    }

    pub fn quantile(&self, q: f64) -> Option<u64> {
        todo!()
    }

    pub fn render(&self, name: &str) -> String {
        todo!()
    }
}
