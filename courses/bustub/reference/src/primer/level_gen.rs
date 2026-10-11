//! The random height of a skip-list node.

/// A small seeded xorshift generator.
pub struct XorShift(u64);

impl XorShift {
    pub fn new(seed: u64) -> XorShift {
        XorShift(seed.max(1))
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

pub struct LevelGenerator {
    // @begin 0b-c1
    rng: XorShift,
    branching: u64,
    max_level: usize,
    //~ _gen: (),
    // @end
}

impl LevelGenerator {
    /// A generator seeded with `seed`: levels are drawn from `1..=max_level`, each one `branching` times rarer than the one below.
    pub fn new(seed: u64, branching: u64, max_level: usize) -> LevelGenerator {
        // @begin 0b-c1
        LevelGenerator { rng: XorShift::new(seed), branching: branching.max(2), max_level: max_level.max(1) }
        //~ todo!("0b-c1: remember the generator and the limits")
        // @end
    }

    /// A level in `1..=max_level`.
    pub fn next_level(&mut self) -> usize {
        // @begin 0b-c1
        let mut level = 1;
        while level < self.max_level && self.rng.next_u64() % self.branching == 0 {
            level += 1;
        }
        level
        //~ todo!("0b-c1: 1, plus one for every trial that succeeds, up to the maximum")
        // @end
    }
}
