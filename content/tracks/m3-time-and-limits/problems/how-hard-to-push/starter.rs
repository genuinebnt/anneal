pub struct Batcher {
    // your fields
}

impl Batcher {
    /// No batch may be larger than `max`, which is at least 1.
    pub fn new(max: u32) -> Self {
        todo!()
    }

    /// The size of the next batch to send, from 1 to `max`.
    pub fn next_size(&self) -> u32 {
        todo!()
    }

    /// What happened to a batch of `next_size()`: accepted whole, or refused whole.
    pub fn report(&mut self, accepted: bool) {
        todo!()
    }
}
