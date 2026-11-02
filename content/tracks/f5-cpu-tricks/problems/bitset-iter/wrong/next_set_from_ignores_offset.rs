/// A growable set of small integers stored one bit each. Iterating costs one `trailing_zeros` per element, not one test per bit.
#[derive(Clone, Debug, Default)]
pub struct BitSet {
    words: Vec<u64>,
}

pub struct Iter<'a> {
    words: &'a [u64],
    word: usize,
    current: u64, // the bits of words[word] not yet yielded
}

impl BitSet {
    pub fn new() -> Self {
        BitSet { words: Vec::new() }
    }

    /// Room for indices below `bits` without reallocating. The set stays empty.
    pub fn with_capacity(bits: usize) -> Self {
        BitSet { words: Vec::with_capacity(bits.div_ceil(64)) }
    }

    /// True if `i` was not in the set yet.
    pub fn insert(&mut self, i: usize) -> bool {
        let (w, b) = (i / 64, i % 64);
        if w >= self.words.len() {
            self.words.resize(w + 1, 0);
        }
        let had = self.words[w] >> b & 1 == 1;
        self.words[w] |= 1 << b;
        !had
    }

    /// True if `i` was in the set. Never grows the set.
    pub fn remove(&mut self, i: usize) -> bool {
        let (w, b) = (i / 64, i % 64);
        match self.words.get_mut(w) {
            Some(word) => {
                let had = *word >> b & 1 == 1;
                *word &= !(1 << b);
                had
            }
            None => false,
        }
    }

    pub fn contains(&self, i: usize) -> bool {
        self.words.get(i / 64).is_some_and(|w| w >> (i % 64) & 1 == 1)
    }

    pub fn len(&self) -> usize {
        self.words.iter().map(|w| w.count_ones() as usize).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.words.iter().all(|&w| w == 0)
    }

    /// Ascending.
    pub fn iter(&self) -> Iter<'_> {
        Iter { words: &self.words, word: 0, current: self.words.first().copied().unwrap_or(0) }
    }

    /// The smallest member that is `>= from`.
    pub fn next_set_from(&self, from: usize) -> Option<usize> {
        let mut w = from / 64;
        if w >= self.words.len() {
            return None;
        }
        let mut cur = self.words[w];
        loop {
            if cur != 0 {
                return Some(w * 64 + cur.trailing_zeros() as usize);
            }
            w += 1;
            cur = *self.words.get(w)?;
        }
    }

    pub fn union_with(&mut self, other: &BitSet) {
        if other.words.len() > self.words.len() {
            self.words.resize(other.words.len(), 0);
        }
        for (a, b) in self.words.iter_mut().zip(&other.words) {
            *a |= b;
        }
    }

    pub fn intersect_with(&mut self, other: &BitSet) {
        for (i, a) in self.words.iter_mut().enumerate() {
            *a &= other.words.get(i).copied().unwrap_or(0); // words beyond `other` intersect with nothing
        }
    }

    pub fn difference_with(&mut self, other: &BitSet) {
        for (a, b) in self.words.iter_mut().zip(&other.words) {
            *a &= !b;
        }
    }
}

impl Iterator for Iter<'_> {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        while self.current == 0 {
            self.word += 1;
            self.current = *self.words.get(self.word)?;
        }
        let bit = self.current.trailing_zeros() as usize;
        self.current &= self.current - 1; // clear the lowest set bit
        Some(self.word * 64 + bit)
    }
}

/// Two sets are equal when they hold the same members, however many empty words one of them carries.
impl PartialEq for BitSet {
    fn eq(&self, other: &Self) -> bool {
        let n = self.words.len().max(other.words.len());
        (0..n).all(|i| self.words.get(i).copied().unwrap_or(0) == other.words.get(i).copied().unwrap_or(0))
    }
}

impl Eq for BitSet {}

impl FromIterator<usize> for BitSet {
    fn from_iter<I: IntoIterator<Item = usize>>(iter: I) -> Self {
        let mut s = BitSet::new();
        for i in iter {
            s.insert(i);
        }
        s
    }
}
