//! Reservoir sampling (Algorithm R).

/// A small seeded xorshift generator.
pub struct XorShift(u64);

impl XorShift {
    pub fn new(seed: u64) -> XorShift {
        XorShift(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).max(1))
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

pub struct Reservoir<T> {
    _res: std::marker::PhantomData<T>,
}

impl<T> Reservoir<T> {
    /// An empty reservoir that keeps `k` items.
    pub fn new(k: usize, seed: u64) -> Reservoir<T> {
        todo!("0d-c2: an empty reservoir for `k` items")
    }

    /// Offers the next item of the stream. The first `k` are kept; item number `n` after that replaces a random held item with probability `k / n`,
    /// so every item seen so far is held with the same probability.
    pub fn offer(&mut self, item: T) {
        todo!("0d-c2: fill the reservoir, then replace a random slot with probability k / seen")
    }

    /// The items held.
    pub fn sample(&self) -> &[T] {
        todo!("0d-c2: the items held")
    }

    /// How many items were offered.
    pub fn seen(&self) -> u64 {
        todo!("0d-c2: how many items were offered")
    }
}
