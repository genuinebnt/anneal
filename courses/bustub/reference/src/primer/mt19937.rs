//! The 32-bit Mersenne Twister (MT19937), seeded as the reference `mt19937(seed)` is, so a seed gives the standard sequence.
//!
//! A fixed seed gives the same sequence on every machine, so structures that draw from it, such as the heights of a skip list's nodes, have the
//! same shape on every machine.

const N: usize = 624;
const M: usize = 397;

pub struct Mt19937 {
    state: [u32; N],
    index: usize,
}

impl Mt19937 {
    /// Seeds the generator like `std::mt19937(seed)`.
    pub fn new(seed: u32) -> Mt19937 {
        let mut state = [0u32; N];
        state[0] = seed;
        for i in 1..N {
            state[i] = 1812433253u32.wrapping_mul(state[i - 1] ^ (state[i - 1] >> 30)).wrapping_add(i as u32);
        }
        Mt19937 { state, index: N }
    }

    fn twist(&mut self) {
        for i in 0..N {
            let y = (self.state[i] & 0x8000_0000) | (self.state[(i + 1) % N] & 0x7fff_ffff);
            let mut next = self.state[(i + M) % N] ^ (y >> 1);
            if y & 1 != 0 {
                next ^= 0x9908_b0df;
            }
            self.state[i] = next;
        }
        self.index = 0;
    }

    /// The next 32-bit number (`std::mt19937::operator()`).
    pub fn next_u32(&mut self) -> u32 {
        if self.index >= N {
            self.twist();
        }
        let mut y = self.state[self.index];
        self.index += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^= y >> 18;
        y
    }
}

#[cfg(test)]
mod tests {
    use super::Mt19937;

    #[test]
    fn matches_the_standard_10000th_value() {
        // the C++ standard requires the 10000th invocation of a default-seeded (5489) mt19937 to produce 4123659995
        let mut rng = Mt19937::new(5489);
        let mut last = 0;
        for _ in 0..10000 {
            last = rng.next_u32();
        }
        assert_eq!(last, 4123659995);
    }
}
