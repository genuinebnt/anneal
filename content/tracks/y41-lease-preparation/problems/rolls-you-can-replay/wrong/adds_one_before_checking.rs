/// The step between two states of the generator.
const STEP: u64 = 0x9E37_79B9_7F4A_7C15;

/// Scrambles a number: every bit of the input affects every bit of the output.
fn scramble(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A source of random numbers that can be replayed.
#[derive(Debug, Clone)]
pub struct Dice {
    state: u64,
}

impl Dice {
    /// The same seed gives the same rolls.
    pub fn new(seed: u64) -> Dice {
        Dice { state: seed }
    }

    /// Stream number `id` of `seed`.
    pub fn stream(seed: u64, id: u64) -> Dice {
        // Two rounds of scrambling, with the id added in between: nearby pairs start far apart, and swapping
        // seed and id does not give the same start.
        Dice::new(scramble(scramble(seed.wrapping_add(STEP)).wrapping_add(id)))
    }

    /// The next raw value.
    pub fn roll(&mut self) -> u64 {
        self.state = self.state.wrapping_add(STEP);
        scramble(self.state)
    }

    /// A value from `lo` to `hi`, both included.
    pub fn between(&mut self, lo: u64, hi: u64) -> u64 {
        assert!(lo <= hi, "between({lo}, {hi}): the range is empty");
        let Some(count) = Some(hi - lo + 1) else {
            return self.roll(); // the whole of u64
        };
        // 2^64 is rarely a multiple of `count`. The first `2^64 % count` raw values are thrown away, so that
        // what is left splits into `count` groups of exactly the same size.
        let skip = count.wrapping_neg() % count;
        loop {
            let raw = self.roll();
            if raw >= skip {
                return lo + raw % count;
            }
        }
    }

    /// `true` about `percent` times in a hundred.
    pub fn percent(&mut self, percent: u32) -> bool {
        self.between(0, 99) < u64::from(percent)
    }
}
