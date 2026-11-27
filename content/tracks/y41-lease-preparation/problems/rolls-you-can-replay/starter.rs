/// A source of random numbers that can be replayed.
#[derive(Debug, Clone)]
pub struct Dice {
    // TODO: the fields
}

impl Dice {
    /// The same seed gives the same rolls.
    pub fn new(seed: u64) -> Dice {
        todo!()
    }

    /// Stream number `id` of `seed`.
    pub fn stream(seed: u64, id: u64) -> Dice {
        todo!()
    }

    /// The next raw value.
    pub fn roll(&mut self) -> u64 {
        todo!()
    }

    /// A value from `lo` to `hi`, both included.
    pub fn between(&mut self, lo: u64, hi: u64) -> u64 {
        todo!()
    }

    /// `true` about `percent` times in a hundred.
    pub fn percent(&mut self, percent: u32) -> bool {
        todo!()
    }
}
