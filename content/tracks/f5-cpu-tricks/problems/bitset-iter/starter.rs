/// A growable set of small integers stored one bit each.
#[derive(Clone, Debug, Default)]
pub struct BitSet {
    words: Vec<u64>,
}

/// Ascending iterator. Make it cost one `trailing_zeros` per member, not one test per bit.
pub struct Iter<'a> {
    words: &'a [u64],
    word: usize,
    current: u64,
}

impl BitSet {
    pub fn new() -> Self {
        todo!()
    }

    /// Room for indices below `bits` without reallocating. The set stays empty.
    pub fn with_capacity(bits: usize) -> Self {
        todo!()
    }

    /// True if `i` was not in the set yet. Grows the set as needed.
    pub fn insert(&mut self, i: usize) -> bool {
        todo!()
    }

    /// True if `i` was in the set. Never grows the set, and `i` may be far beyond it.
    pub fn remove(&mut self, i: usize) -> bool {
        todo!()
    }

    pub fn contains(&self, i: usize) -> bool {
        todo!()
    }

    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        todo!()
    }

    pub fn iter(&self) -> Iter<'_> {
        todo!()
    }

    /// The smallest member that is `>= from`.
    pub fn next_set_from(&self, from: usize) -> Option<usize> {
        todo!()
    }

    pub fn union_with(&mut self, other: &BitSet) {
        todo!()
    }

    pub fn intersect_with(&mut self, other: &BitSet) {
        todo!()
    }

    pub fn difference_with(&mut self, other: &BitSet) {
        todo!()
    }
}

impl Iterator for Iter<'_> {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        todo!()
    }
}

// TODO: equal when they hold the same members, even if one carries trailing empty words.
impl PartialEq for BitSet {
    fn eq(&self, other: &Self) -> bool {
        todo!()
    }
}
impl Eq for BitSet {}

impl FromIterator<usize> for BitSet {
    fn from_iter<I: IntoIterator<Item = usize>>(iter: I) -> Self {
        todo!()
    }
}
