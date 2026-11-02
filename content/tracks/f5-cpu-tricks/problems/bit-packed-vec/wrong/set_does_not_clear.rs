#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum PackError {
    /// The value needs more bits than the vector's width.
    TooWide,
    OutOfRange,
}

/// A vector of unsigned integers that each take `width` bits (1 to 64) instead of 64, packed back to back.
#[derive(Debug, Clone)]
pub struct PackedVec {
    width: u32,
    len: usize,
    words: Vec<u64>,
}

fn mask(width: u32) -> u64 {
    if width == 64 { u64::MAX } else { (1u64 << width) - 1 }
}

impl PackedVec {
    /// Panics unless `1 <= width <= 64`.
    pub fn new(width: u32) -> Self {
        assert!((1..=64).contains(&width), "width must be 1..=64");
        PackedVec { width, len: 0, words: Vec::new() }
    }

    /// The fewest bits (at least 1) that hold every value.
    pub fn min_width(values: &[u64]) -> u32 {
        let max = values.iter().copied().max().unwrap_or(0);
        (64 - max.leading_zeros()).max(1)
    }

    pub fn from_slice(width: u32, values: &[u64]) -> Result<Self, PackError> {
        let mut v = PackedVec::new(width);
        for &x in values {
            v.push(x)?;
        }
        Ok(v)
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// How many `u64` words the data occupies: `ceil(len * width / 64)`.
    pub fn words_used(&self) -> usize {
        self.words.len()
    }

    pub fn push(&mut self, value: u64) -> Result<(), PackError> {
        if value > mask(self.width) {
            return Err(PackError::TooWide);
        }
        let needed = ((self.len + 1) * self.width as usize).div_ceil(64);
        if needed > self.words.len() {
            self.words.resize(needed, 0);
        }
        self.len += 1;
        self.write(self.len - 1, value);
        Ok(())
    }

    pub fn get(&self, i: usize) -> Option<u64> {
        if i >= self.len {
            return None;
        }
        let bit = i * self.width as usize;
        let (w, off) = (bit / 64, (bit % 64) as u32);
        let mut v = self.words[w] >> off;
        if off + self.width > 64 {
            v |= self.words[w + 1] << (64 - off); // the value continues in the next word
        }
        Some(v & mask(self.width))
    }

    pub fn set(&mut self, i: usize, value: u64) -> Result<(), PackError> {
        if i >= self.len {
            return Err(PackError::OutOfRange);
        }
        if value > mask(self.width) {
            return Err(PackError::TooWide);
        }
        self.write(i, value);
        Ok(())
    }

    fn write(&mut self, i: usize, value: u64) {
        let bit = i * self.width as usize;
        let (w, off) = (bit / 64, (bit % 64) as u32);
        let m = mask(self.width);
        self.words[w] |= value << off;
        if off + self.width > 64 {
            let spill = off + self.width - 64; // bits that landed in the next word
            let hi_mask = mask(spill);
            self.words[w + 1] = self.words[w + 1] & !hi_mask | (value >> (64 - off));
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = u64> + '_ {
        (0..self.len).map(|i| self.get(i).unwrap())
    }
}
