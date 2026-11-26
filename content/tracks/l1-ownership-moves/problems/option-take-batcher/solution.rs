pub struct Batcher {
    current: Option<Vec<u32>>,
    size: usize,
}

impl Batcher {
    pub fn new(size: usize) -> Self {
        assert!(size > 0, "batch size must be at least 1");
        Batcher {
            current: None,
            size,
        }
    }

    pub fn push(&mut self, value: u32) -> Option<Vec<u32>> {
        let batch = self
            .current
            .get_or_insert_with(|| Vec::with_capacity(self.size));
        batch.push(value);
        if batch.len() == self.size {
            self.current.take()
        } else {
            None
        }
    }

    pub fn flush(&mut self) -> Option<Vec<u32>> {
        self.current.take()
    }
}
