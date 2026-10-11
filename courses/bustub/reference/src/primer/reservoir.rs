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
    // @begin 0d-c2
    k: usize,
    items: Vec<T>,
    seen: u64,
    rng: XorShift,
    //~ _res: std::marker::PhantomData<T>,
    // @end
}

impl<T> Reservoir<T> {
    /// An empty reservoir that keeps `k` items.
    pub fn new(k: usize, seed: u64) -> Reservoir<T> {
        // @begin 0d-c2
        Reservoir { k, items: Vec::with_capacity(k), seen: 0, rng: XorShift::new(seed) }
        //~ todo!("0d-c2: an empty reservoir for `k` items")
        // @end
    }

    /// Offers the next item of the stream. The first `k` are kept; item number `n` after that replaces a random held item with probability `k / n`,
    /// so every item seen so far is held with the same probability.
    pub fn offer(&mut self, item: T) {
        // @begin 0d-c2
        self.seen += 1;
        if self.items.len() < self.k {
            self.items.push(item);
        } else if self.k > 0 {
            let j = (self.rng.next_u64() % self.seen) as usize;
            if j < self.k {
                self.items[j] = item;
            }
        }
        //~ todo!("0d-c2: fill the reservoir, then replace a random slot with probability k / seen")
        // @end
    }

    /// The items held.
    pub fn sample(&self) -> &[T] {
        // @begin 0d-c2
        &self.items
        //~ todo!("0d-c2: the items held")
        // @end
    }

    /// How many items were offered.
    pub fn seen(&self) -> u64 {
        // @begin 0d-c2
        self.seen
        //~ todo!("0d-c2: how many items were offered")
        // @end
    }
}
