pub struct Chart {
    // TODO: the fields
}

impl Chart {
    pub fn new(half_life: u64) -> Chart {
        todo!()
    }

    /// `plays` plays of `song`, made at time `at`.
    pub fn report(&mut self, song: &str, plays: u32, at: u64) {
        todo!()
    }

    pub fn score(&self, song: &str, now: u64) -> f64 {
        todo!()
    }

    /// The names of at most `n` songs, hottest first.
    pub fn top(&self, n: usize) -> Vec<String> {
        todo!()
    }
}
