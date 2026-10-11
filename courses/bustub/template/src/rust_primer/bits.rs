//! A fixed-capacity set of small integers, one bit each.

pub struct BitSet {
    _bits: (),
}

impl BitSet {
    /// An empty set for the indexes `0..capacity`.
    pub fn new(capacity: usize) -> BitSet {
        todo!("r-c3: enough words for `capacity` bits, all clear")
    }

    /// How many indexes fit: `0..capacity`.
    pub fn capacity(&self) -> usize {
        todo!("r-c3: how many indexes fit")
    }

    /// Sets bit `i`; true if it was clear. Out of range: false, no change.
    pub fn set(&mut self, i: usize) -> bool {
        todo!("r-c3: set the bit and say whether it changed")
    }

    /// Clears bit `i`; true if it was set. Out of range: false, no change.
    pub fn clear(&mut self, i: usize) -> bool {
        todo!("r-c3: clear the bit and say whether it changed")
    }

    /// Whether bit `i` is set; `false` past the capacity.
    pub fn test(&self, i: usize) -> bool {
        todo!("r-c3: is the bit set (false past the capacity)")
    }

    /// The number of bits set.
    pub fn count(&self) -> usize {
        todo!("r-c3: how many bits are set")
    }

    /// The set indexes, in increasing order.
    pub fn iter(&self) -> Box<dyn Iterator<Item = usize> + '_> {
        todo!("r-c3: the set indexes in increasing order")
    }

    /// The smallest clear index below the capacity.
    pub fn first_clear(&self) -> Option<usize> {
        todo!("r-c3: the first clear bit, if any")
    }

    /// Sets every bit that is set in `other`. Bits of `other` past this set's capacity are ignored.
    pub fn union_with(&mut self, other: &BitSet) {
        todo!("r-c3: set every bit that is set in `other`")
    }
}
