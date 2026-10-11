//! A fixed-capacity set of small integers, one bit each.

pub struct BitSet {
    // @begin r-c3
    words: Vec<u64>,
    capacity: usize,
    //~ _bits: (),
    // @end
}

impl BitSet {
    /// An empty set for the indexes `0..capacity`.
    pub fn new(capacity: usize) -> BitSet {
        // @begin r-c3
        BitSet { words: vec![0; capacity.div_ceil(64)], capacity }
        //~ todo!("r-c3: enough words for `capacity` bits, all clear")
        // @end
    }

    /// How many indexes fit: `0..capacity`.
    pub fn capacity(&self) -> usize {
        // @begin r-c3
        self.capacity
        //~ todo!("r-c3: how many indexes fit")
        // @end
    }

    /// Sets bit `i`; true if it was clear. Out of range: false, no change.
    pub fn set(&mut self, i: usize) -> bool {
        // @begin r-c3
        if i >= self.capacity {
            return false;
        }
        let (w, b) = (i / 64, i % 64);
        let was = self.words[w] >> b & 1 == 1;
        self.words[w] |= 1 << b;
        !was
        //~ todo!("r-c3: set the bit and say whether it changed")
        // @end
    }

    /// Clears bit `i`; true if it was set. Out of range: false, no change.
    pub fn clear(&mut self, i: usize) -> bool {
        // @begin r-c3
        if i >= self.capacity {
            return false;
        }
        let (w, b) = (i / 64, i % 64);
        let was = self.words[w] >> b & 1 == 1;
        self.words[w] &= !(1 << b);
        was
        //~ todo!("r-c3: clear the bit and say whether it changed")
        // @end
    }

    /// Whether bit `i` is set; `false` past the capacity.
    pub fn test(&self, i: usize) -> bool {
        // @begin r-c3
        i < self.capacity && self.words[i / 64] >> (i % 64) & 1 == 1
        //~ todo!("r-c3: is the bit set (false past the capacity)")
        // @end
    }

    /// The number of bits set.
    pub fn count(&self) -> usize {
        // @begin r-c3
        self.words.iter().map(|w| w.count_ones() as usize).sum()
        //~ todo!("r-c3: how many bits are set")
        // @end
    }

    /// The set indexes, in increasing order.
    pub fn iter(&self) -> Box<dyn Iterator<Item = usize> + '_> {
        // @begin r-c3
        Box::new(self.words.iter().enumerate().flat_map(|(w, &word)| {
            (0..64).filter(move |b| word >> b & 1 == 1).map(move |b| w * 64 + b)
        }))
        //~ todo!("r-c3: the set indexes in increasing order")
        // @end
    }

    /// The smallest clear index below the capacity.
    pub fn first_clear(&self) -> Option<usize> {
        // @begin r-c3
        (0..self.capacity).find(|&i| !self.test(i))
        //~ todo!("r-c3: the first clear bit, if any")
        // @end
    }

    /// Sets every bit that is set in `other`. Bits of `other` past this set's capacity are ignored.
    pub fn union_with(&mut self, other: &BitSet) {
        // @begin r-c3
        for i in other.iter() {
            self.set(i);
        }
        //~ todo!("r-c3: set every bit that is set in `other`")
        // @end
    }
}
