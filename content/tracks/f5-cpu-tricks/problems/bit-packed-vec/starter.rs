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

impl PackedVec {
    /// Panics unless `1 <= width <= 64`.
    pub fn new(width: u32) -> Self {
        todo!()
    }

    /// The fewest bits (at least 1) that hold every value.
    pub fn min_width(values: &[u64]) -> u32 {
        todo!()
    }

    pub fn from_slice(width: u32, values: &[u64]) -> Result<Self, PackError> {
        todo!()
    }

    pub fn width(&self) -> u32 {
        todo!()
    }

    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        todo!()
    }

    /// How many `u64` words the data occupies: `ceil(len * width / 64)`.
    pub fn words_used(&self) -> usize {
        todo!()
    }

    /// `Err(TooWide)` if the value does not fit in `width` bits.
    pub fn push(&mut self, value: u64) -> Result<(), PackError> {
        todo!()
    }

    pub fn get(&self, i: usize) -> Option<u64> {
        todo!()
    }

    /// `Err(OutOfRange)` past the end, `Err(TooWide)` if it does not fit. Must not disturb the neighbours.
    pub fn set(&mut self, i: usize, value: u64) -> Result<(), PackError> {
        todo!()
    }

    pub fn iter(&self) -> impl Iterator<Item = u64> + '_ {
        std::iter::empty() // TODO
    }
}
