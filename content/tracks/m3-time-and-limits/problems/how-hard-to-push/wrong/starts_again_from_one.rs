pub struct Batcher {
    max: u32,
    size: u32,
}

impl Batcher {
    pub fn new(max: u32) -> Self {
        Batcher { max, size: 1 }
    }

    pub fn next_size(&self) -> u32 {
        self.size
    }

    pub fn report(&mut self, accepted: bool) {
        if accepted {
            // creep upwards: the limit may be just above
            self.size = self.size.saturating_add(1).min(self.max);
        } else {
            // back off fast: the limit may be far below
            self.size = 1;
        }
    }
}
