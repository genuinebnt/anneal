from author import T, write_track


def sub(s, old, new):
    """str.replace that fails loudly when `old` isn't there (a wrong solution that silently equals the reference)."""
    assert old in s, f"not found: {old[:60]!r}"
    return s.replace(old, new)


P = []

# ---------------------------------------------------------------------------------------------------------------
BITSET = """
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
        let mut cur = self.words[w] & (!0u64 << (from % 64));
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
"""

BITSET_STARTER = """
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
"""

P.append(dict(
    slug="bitset-iter", title="A bitset with trailing_zeros iteration", level="easy", stage="first-bits",
    tags=["bitset", "u64 words", "trailing_zeros", "count_ones", "PartialEq", "set algebra"],
    teaches=["Index `i` lives in word `i / 64`, bit `i % 64`: the arithmetic behind `Vec<bool>` replacements, bloom filters and hashbrown's control bytes.",
             "Iterate members with `trailing_zeros` and `x & (x - 1)` (clear the lowest set bit): work proportional to the members, not the range.",
             "Set algebra on words, and the trap in `intersect_with` when the two sets have different word counts."],
    statement="""
        `BitSet` stores a set of `usize` in `Vec<u64>`, one bit per possible member. It grows when you insert a large
        index; `remove` and `contains` never grow it. `iter` yields members **ascending** and must find the next member with
        `trailing_zeros`, clearing it with `x & (x - 1)`, rather than testing every bit.

        `union_with`, `intersect_with` and `difference_with` modify `self` in place; the other set may be shorter or longer.
        Two sets are **equal when they have the same members**, even if one of them carries extra empty words (it was
        bigger once), so `==` cannot just compare the vectors.
    """,
    starter=BITSET_STARTER,
    solution=BITSET,
    visible=[
        T("insert_contains_len", "insert 3, 64, 3", "{ let mut s = BitSet::new(); let a = s.insert(3); let b = s.insert(64); let c = s.insert(3); (a, b, c, s.len(), s.contains(3), s.contains(4), s.contains(64)) }", "(true, true, false, 2, true, false, true)"),
        T("iter_ascending", "insert 130, 5, 64, 63, 0", "{ let s: BitSet = [130, 5, 64, 63, 0].into_iter().collect(); s.iter().collect::<Vec<_>>() }", "vec![0, 5, 63, 64, 130]"),
        T("remove", "remove a member and a stranger", "{ let mut s: BitSet = [1, 70].into_iter().collect(); (s.remove(70), s.remove(70), s.remove(5000), s.iter().collect::<Vec<_>>()) }", "(true, false, false, vec![1])"),
        T("next_set_from", "members 2, 64, 200", "{ let s: BitSet = [2, 64, 200].into_iter().collect(); (s.next_set_from(0), s.next_set_from(3), s.next_set_from(64), s.next_set_from(65), s.next_set_from(201)) }", "(Some(2), Some(64), Some(64), Some(200), None)"),
        T("set_algebra", "{1,2,70} with {2,3,70}", "{ let a: BitSet = [1, 2, 70].into_iter().collect(); let b: BitSet = [2, 3, 70].into_iter().collect(); let (mut u, mut i, mut d) = (a.clone(), a.clone(), a.clone()); u.union_with(&b); i.intersect_with(&b); d.difference_with(&b); (u.iter().collect::<Vec<_>>(), i.iter().collect::<Vec<_>>(), d.iter().collect::<Vec<_>>()) }", "(vec![1, 2, 3, 70], vec![2, 70], vec![1])"),
    ],
    hidden=[
        T("empty_set", "nothing inserted", "{ let s = BitSet::new(); (s.len(), s.is_empty(), s.iter().next(), s.contains(0), s.next_set_from(0)) }", "(0, true, None, false, None)"),
        T("equal_despite_trailing_words", "{1} that once held 500 equals {1}", "{ let mut a = BitSet::new(); a.insert(1); a.insert(500); a.remove(500); let mut b = BitSet::new(); b.insert(1); (a == b, b == a, a.len() == b.len()) }", "(true, true, true)"),
        T("unequal_sets", "different members and different empties", "{ let a: BitSet = [1].into_iter().collect(); let b: BitSet = [2].into_iter().collect(); let mut c = BitSet::new(); c.insert(700); c.remove(700); (a == b, a == c, BitSet::new() == c) }", "(false, false, true)"),
        T("intersect_with_shorter_other", "self has a member beyond other's words", "{ let mut a: BitSet = [1, 2, 300].into_iter().collect(); let b: BitSet = [2].into_iter().collect(); a.intersect_with(&b); a.iter().collect::<Vec<_>>() }", "vec![2]"),
        T("intersect_with_longer_other", "other is longer", "{ let mut a: BitSet = [1, 2].into_iter().collect(); let b: BitSet = [2, 300].into_iter().collect(); a.intersect_with(&b); a.iter().collect::<Vec<_>>() }", "vec![2]"),
        T("union_grows_self", "other is longer", "{ let mut a: BitSet = [1].into_iter().collect(); let b: BitSet = [500].into_iter().collect(); a.union_with(&b); a.iter().collect::<Vec<_>>() }", "vec![1, 500]"),
        T("difference_with_longer_other", "other is longer", "{ let mut a: BitSet = [1, 2].into_iter().collect(); let b: BitSet = [2, 900].into_iter().collect(); a.difference_with(&b); a.iter().collect::<Vec<_>>() }", "vec![1]"),
        T("word_boundaries", "members at 63, 64, 127, 128", "{ let s: BitSet = [63, 64, 127, 128].into_iter().collect(); (s.iter().collect::<Vec<_>>(), s.next_set_from(64), s.next_set_from(65), s.next_set_from(128), s.next_set_from(129)) }", "(vec![63, 64, 127, 128], Some(64), Some(127), Some(128), None)"),
        T("full_word", "all 64 bits of word 0 and a gap", "{ let s: BitSet = (0..64).chain(192..200).collect(); (s.len(), s.iter().count(), s.next_set_from(64), s.iter().last()) }", "(72, 72, Some(192), Some(199))"),
        T("with_capacity_is_empty", "preallocated, nothing in it", "{ let s = BitSet::with_capacity(10_000); (s.len(), s.is_empty(), s.contains(9_999), s.iter().count()) }", "(0, true, false, 0)"),
        """
        #[test]
        fn random_vs_btreeset() {
            use std::collections::BTreeSet;
            let mut rng = anneal_prelude::Rng::new(6101);
            for _ in 0..300 {
                let span = [10usize, 64, 130, 400][rng.below(4)];
                let mut s = BitSet::new();
                let mut model = BTreeSet::new();
                let mut log = Vec::new();
                for _ in 0..rng.below(40) {
                    let i = rng.below(span);
                    if rng.below(3) == 0 {
                        log.push(format!("remove {i}"));
                        check!(format!("{}", log.join(", ")), s.remove(i), model.remove(&i));
                    } else {
                        log.push(format!("insert {i}"));
                        check!(format!("{}", log.join(", ")), s.insert(i), model.insert(i));
                    }
                }
                let from = rng.below(span + 5);
                check!(format!("{}: iter, len, next_set_from({from})", log.join(", ")), (s.iter().collect::<Vec<_>>(), s.len(), s.next_set_from(from)), (model.iter().copied().collect::<Vec<_>>(), model.len(), model.range(from..).next().copied()));
                let other: BitSet = (0..rng.below(20)).map(|_| rng.below(span + 70)).collect();
                let omodel: BTreeSet<usize> = other.iter().collect();
                let (mut u, mut x, mut d) = (s.clone(), s.clone(), s.clone());
                u.union_with(&other);
                x.intersect_with(&other);
                d.difference_with(&other);
                check!(format!("{}: algebra with {omodel:?}", log.join(", ")),
                    (u.iter().collect::<Vec<_>>(), x.iter().collect::<Vec<_>>(), d.iter().collect::<Vec<_>>()),
                    (model.union(&omodel).copied().collect::<Vec<_>>(), model.intersection(&omodel).copied().collect::<Vec<_>>(), model.difference(&omodel).copied().collect::<Vec<_>>()));
            }
        }

        #[test]
        fn a_million_members() {
            let mut s = BitSet::with_capacity(4_000_000);
            for i in (0..4_000_000).step_by(4) {
                s.insert(i);
            }
            let sum: usize = s.iter().sum();
            check!("members 0, 4, 8 .. 3999996: len and sum", (s.len(), sum), (1_000_000, 1_999_998_000_000));
        }
        """,
    ],
    wrong=dict(
        eq_compares_vectors=sub(BITSET, "let n = self.words.len().max(other.words.len());\n        (0..n).all(|i| self.words.get(i).copied().unwrap_or(0) == other.words.get(i).copied().unwrap_or(0))", "self.words == other.words"),
        intersect_keeps_tail=sub(BITSET, "*a &= other.words.get(i).copied().unwrap_or(0); // words beyond `other` intersect with nothing", "if let Some(b) = other.words.get(i) {\n                *a &= b;\n            }"),
        len_counts_words=sub(BITSET, "self.words.iter().map(|w| w.count_ones() as usize).sum()", "self.words.iter().filter(|&&w| w != 0).count()"),
        next_set_from_ignores_offset=sub(BITSET, "let mut cur = self.words[w] & (!0u64 << (from % 64));", "let mut cur = self.words[w];"),
        remove_toggles=sub(BITSET, "*word &= !(1 << b);", "*word ^= 1 << b;"),
    ),
    hints=[("approach", "Member `i` is bit `i % 64` of `words[i / 64]`. Growing means `resize(w + 1, 0)`; reading a missing word is `get(w)` with 0 as the default."),
           ("rust", "Iteration state is one word's remaining bits: `bit = cur.trailing_zeros()`, yield `word * 64 + bit`, then `cur &= cur - 1`. When `cur == 0`, move to the next word. `from % 64` masks `!0u64 << (from % 64)` keep the bits at or above `from`."),
           ("edge", "`intersect_with`: self's words past the end of `other` must become 0, so iterate over self's words and default the missing ones to 0. `PartialEq`: compare word by word with missing words as 0.")],
    notes=("One bit per member makes membership O(1), `len` a sum of `count_ones` (one instruction per word on modern CPUs) and set algebra one AND/OR/ANDN per 64 elements. The `x & (x - 1)` idiom clears the lowest set bit, so iteration visits only members. It is the loop inside hashbrown's group matching, Roaring bitmaps, bloom filters and graph visited-sets.", "O(1) insert/contains, O(words) algebra, O(words + members) iterate", "n/8 bytes"),
    follow_up="1) When does a bitset beat a `HashSet<usize>` or `Vec<bool>`, and when does it lose (sparse sets with huge indices)? 2) Add `iter().rev()` with `DoubleEndedIterator` using `leading_zeros`. 3) How does `count_ones` compile on x86 with and without `-C target-cpu=native` (`popcnt`), and what does the fallback cost? 4) Make `intersect_with` shrink the vector: why might you not (reallocation, hysteresis)? 5) Where does Roaring bitmap improve on this (hybrid containers; the capstone of this track)?",
    related=["D13", "S4"],
))

# ---------------------------------------------------------------------------------------------------------------
PACKED = """
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
        self.words[w] = self.words[w] & !(m << off) | (value << off); // clear the old bits, then put the new ones
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
"""

PACKED_STARTER = """
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
"""

P.append(dict(
    slug="bit-packed-vec", title="A bit-packed integer vector", level="medium", stage="pack-and-scan",
    tags=["bit packing", "masks and shifts", "word boundaries", "columnar storage"],
    teaches=["Value `i` starts at bit `i * width`: word index, bit offset, and the case where it straddles two words.",
             "Masks that work for every width including 64 (`1 << 64` overflows), and set that clears before it ORs.",
             "Why columnar formats (Parquet, Arrow dictionary indices, FastLanes) store small integers this way."],
    statement="""
        `PackedVec::new(width)` stores unsigned integers that each fit in `width` bits (1 to 64) back to back in
        `Vec<u64>`, so a column of values below 1000 takes 10 bits each instead of 64. `push`, `get` (`None` past the end) and
        `set` work on any index, **including values that straddle a word boundary**. `push` and `set` refuse a value that does
        not fit (`TooWide`); `set` past the end is `OutOfRange`. `words_used()` reports the storage, which must be
        exactly `ceil(len * width / 64)`. `min_width` finds the smallest width that holds a slice of values.
    """,
    starter=PACKED_STARTER,
    solution=PACKED,
    visible=[
        T("small_width_roundtrip", "width 3: 0..8", "{ let v = PackedVec::from_slice(3, &[0, 1, 2, 3, 4, 5, 6, 7]).unwrap(); v.iter().collect::<Vec<_>>() }", "vec![0, 1, 2, 3, 4, 5, 6, 7]"),
        T("storage_is_dense", "21 values of 3 bits need 63 bits = 1 word, the 22nd needs a second", "{ let mut v = PackedVec::new(3); for _ in 0..21 { v.push(5).unwrap(); } let a = v.words_used(); v.push(5).unwrap(); (a, v.words_used()) }", "(1, 2)"),
        T("too_wide", "width 4 refuses 16", "{ let mut v = PackedVec::new(4); (v.push(15), v.push(16), v.len()) }", "(Ok(()), Err(PackError::TooWide), 1)"),
        T("get_out_of_range", "empty and past the end", "{ let mut v = PackedVec::new(7); v.push(100).unwrap(); (v.get(0), v.get(1), PackedVec::new(7).get(0)) }", "(Some(100), None, None)"),
        T("set_overwrites_only_its_slot", "width 5, set the middle one", "{ let mut v = PackedVec::from_slice(5, &[31, 31, 31]).unwrap(); v.set(1, 0).unwrap(); v.iter().collect::<Vec<_>>() }", "vec![31, 0, 31]"),
    ],
    hidden=[
        T("min_width", "width of 0, 1, 255, 256, u64::MAX", "(PackedVec::min_width(&[]), PackedVec::min_width(&[0]), PackedVec::min_width(&[1]), PackedVec::min_width(&[3, 255]), PackedVec::min_width(&[256]), PackedVec::min_width(&[u64::MAX]))", "(1, 1, 1, 8, 9, 64)"),
        T("width_64_holds_everything", "u64::MAX and 0 side by side", "{ let v = PackedVec::from_slice(64, &[u64::MAX, 0, 1 << 63, 12345]).unwrap(); (v.iter().collect::<Vec<_>>(), v.words_used()) }", "(vec![u64::MAX, 0, 1 << 63, 12345], 4)"),
        T("width_1_bits", "a bitmap of 70 flags", "{ let flags: Vec<u64> = (0..70).map(|i| (i % 3 == 0) as u64).collect(); let v = PackedVec::from_slice(1, &flags).unwrap(); (v.iter().collect::<Vec<_>>() == flags, v.words_used()) }", "(true, 2)"),
        T("straddling_values", "width 13: values cross word boundaries", "{ let vals: Vec<u64> = (0..40).map(|i| (i * 977 + 5) % 8192).collect(); let v = PackedVec::from_slice(13, &vals).unwrap(); (v.iter().collect::<Vec<_>>() == vals, v.words_used()) }", "(true, 9)"),
        T("set_on_a_straddling_slot", "width 13, slot 4 straddles words 0 and 1", "{ let mut v = PackedVec::from_slice(13, &[8191; 10]).unwrap(); v.set(4, 0).unwrap(); let a: Vec<u64> = v.iter().collect(); v.set(4, 4097).unwrap(); let b: Vec<u64> = v.iter().collect(); (a, b) }", "(vec![8191, 8191, 8191, 8191, 0, 8191, 8191, 8191, 8191, 8191], vec![8191, 8191, 8191, 8191, 4097, 8191, 8191, 8191, 8191, 8191])"),
        T("set_errors", "out of range and too wide, nothing changes", "{ let mut v = PackedVec::from_slice(4, &[1, 2]).unwrap(); let r = (v.set(2, 1), v.set(0, 16), v.set(1, 15)); (r, v.iter().collect::<Vec<_>>()) }", "((Err(PackError::OutOfRange), Err(PackError::TooWide), Ok(())), vec![1, 15])"),
        T("width_zero_panics", "width 0 and 65", "(std::panic::catch_unwind(|| PackedVec::new(0)).is_err(), std::panic::catch_unwind(|| PackedVec::new(65)).is_err())", "(true, true)"),
        T("from_slice_rejects_wide_values", "second value too wide", "PackedVec::from_slice(3, &[7, 8]).map(|v| v.len())", "Err(PackError::TooWide)"),
        """
        #[test]
        fn random_vs_vec_for_every_width() {
            let mut rng = anneal_prelude::Rng::new(6202);
            for width in 1..=64u32 {
                let max = if width == 64 { u64::MAX } else { (1u64 << width) - 1 };
                let mut v = PackedVec::new(width);
                let mut model: Vec<u64> = Vec::new();
                let n = rng.below(150);
                let pick = |rng: &mut anneal_prelude::Rng| -> u64 {
                    match rng.below(4) {
                        0 => 0,
                        1 => max,
                        _ => ((rng.below(1 << 30) as u64) << 34 ^ rng.below(1 << 30) as u64 * 0x9E37_79B9) & max,
                    }
                };
                for _ in 0..n {
                    let x = pick(&mut rng);
                    v.push(x).unwrap();
                    model.push(x);
                }
                for _ in 0..n {
                    let i = rng.below(n.max(1));
                    if i < n {
                        let x = pick(&mut rng);
                        v.set(i, x).unwrap();
                        model[i] = x;
                    }
                }
                check!(format!("width {width}, {n} values: contents"), v.iter().collect::<Vec<_>>(), model.clone());
                check!(format!("width {width}, {n} values: words used"), v.words_used(), (n * width as usize).div_ceil(64));
                check!(format!("width {width}: get(n)"), v.get(n), None);
            }
        }

        #[test]
        fn two_million_ten_bit_values() {
            let vals: Vec<u64> = (0..2_000_000u64).map(|i| (i * 7919) % 1000).collect();
            let v = PackedVec::from_slice(PackedVec::min_width(&vals), &vals).unwrap();
            let sum: u64 = v.iter().sum();
            check!("2,000,000 values below 1000: width, words, sum", (v.width(), v.words_used(), sum), (10, 312_500, vals.iter().sum::<u64>()));
        }
        """,
    ],
    wrong=dict(
        mask_overflows_at_64=sub(PACKED, "if width == 64 { u64::MAX } else { (1u64 << width) - 1 }", "(1u64 << width) - 1"),
        set_does_not_clear=sub(PACKED, "self.words[w] = self.words[w] & !(m << off) | (value << off); // clear the old bits, then put the new ones", "self.words[w] |= value << off;"),
        straddle_ignored_on_get=sub(PACKED, "if off + self.width > 64 {\n            v |= self.words[w + 1] << (64 - off); // the value continues in the next word\n        }\n        Some", "Some"),
        pushes_without_range_check=sub(PACKED, "if value > mask(self.width) {\n            return Err(PackError::TooWide);\n        }\n        let needed", "let needed"),
    ),
    hints=[("approach", "Value `i` occupies bits `i * width .. (i + 1) * width`. Its word is `bit / 64` and its offset inside the word is `bit % 64`. If `offset + width > 64` the top part of the value is in the next word."),
           ("rust", "Read: `(words[w] >> off) | (words[w + 1] << (64 - off))` (only when it straddles; `off > 0` then), then AND with the mask. Write: clear the slot's bits first (`word & !(mask << off)`), OR the new bits in, and do the same on the next word with the leftover bits."),
           ("edge", "`1u64 << 64` panics in debug builds: build the mask with an `if width == 64` branch (or `u64::MAX >> (64 - width)`). Keep the storage dense: `ceil(len * width / 64)` words, grown on push.")],
    notes=("Packing trades a few shifts and masks per access for an 6x or more smaller footprint on small values, which is what decides cache behaviour on large columns. The straddling case is the bug magnet: a value's bits can be split across two words, so reads combine two shifted halves and writes update both. Columnar formats go further (Parquet's bit-packing hybrid, FastLanes) by laying values out so SIMD can unpack many at once.", "O(1) get/set/push", "len * width / 8 bytes"),
    follow_up="1) How would you unpack 64 values of the same width in a tight loop without a branch per value (specialise on width with const generics)? 2) What does an Arrow dictionary-encoded column do with this (indices of ceil(log2(distinct)) bits)? 3) Frame-of-reference: store `value - min` so the width shrinks; what happens with outliers (patched FOR)? 4) Why do SIMD-friendly formats pack in 4-lane interleaved order instead of sequentially? 5) Make `get` return a value without bounds checks using `unsafe`: is the speedup worth losing the safety here?",
    related=["F2", "F5", "D13"],
))

# ---------------------------------------------------------------------------------------------------------------
VARINT = """
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum DecodeError {
    /// The input ended inside a number.
    Truncated,
    /// The number does not fit in 64 bits.
    Overflow,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct NotSorted {
    pub index: usize,
}

/// LEB128: 7 bits per byte, least significant group first, the high bit says "more follows".
pub fn encode_u64(mut v: u64, out: &mut Vec<u8>) {
    while v >= 0x80 {
        out.push((v as u8 & 0x7f) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

/// The value and the number of bytes it took. Non-minimal encodings (such as `[0x80, 0x00]`) are accepted, as protobuf does.
pub fn decode_u64(buf: &[u8]) -> Result<(u64, usize), DecodeError> {
    let mut v = 0u64;
    for (i, &b) in buf.iter().enumerate() {
        let shift = 7 * i as u32;
        if shift == 63 && b > 1 {
            return Err(DecodeError::Overflow); // the 10th byte may only contribute the top bit
        }
        if shift > 63 {
            return Err(DecodeError::Overflow);
        }
        v |= ((b & 0x7f) as u64) << shift;
        if b & 0x80 == 0 {
            return Ok((v, i + 1));
        }
    }
    Err(DecodeError::Truncated)
}

/// Maps small magnitudes to small codes: 0, -1, 1, -2, 2 become 0, 1, 2, 3, 4.
pub fn zigzag(v: i64) -> u64 {
    ((v << 1) ^ (v >> 63)) as u64
}

pub fn unzigzag(u: u64) -> i64 {
    ((u >> 1) as i64) ^ -((u & 1) as i64)
}

pub fn encode_i64(v: i64, out: &mut Vec<u8>) {
    encode_u64(zigzag(v), out)
}

pub fn decode_i64(buf: &[u8]) -> Result<(i64, usize), DecodeError> {
    let (u, n) = decode_u64(buf)?;
    Ok((unzigzag(u), n))
}

/// Layout: count, then the first value, then each difference to the previous one, all varints.
/// Equal neighbours are fine (difference 0); a decrease is `Err(NotSorted { index })` of the offending element.
pub fn encode_sorted(values: &[u64], out: &mut Vec<u8>) -> Result<(), NotSorted> {
    if let Some(i) = values.windows(2).position(|w| w[1] < w[0]) {
        return Err(NotSorted { index: i + 1 });
    }
    encode_u64(values.len() as u64, out);
    let mut prev = 0u64;
    for &v in values {
        encode_u64(v - prev, out);
        prev = v;
    }
    Ok(())
}

/// The inverse. The buffer must contain exactly one encoded list (trailing bytes are ignored). Must not trust the count:
/// a few bytes can claim 2^60 elements.
pub fn decode_sorted(buf: &[u8]) -> Result<Vec<u64>, DecodeError> {
    let (count, mut pos) = decode_u64(buf)?;
    let mut out = Vec::with_capacity((count as usize).min(buf.len()));
    let mut prev = 0u64;
    for _ in 0..count {
        let (d, n) = decode_u64(&buf[pos..])?;
        prev = prev.checked_add(d).ok_or(DecodeError::Overflow)?;
        out.push(prev);
        pos += n;
    }
    Ok(out)
}
"""

VARINT_STARTER = """
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum DecodeError {
    /// The input ended inside a number.
    Truncated,
    /// The number does not fit in 64 bits.
    Overflow,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct NotSorted {
    pub index: usize,
}

/// LEB128: 7 bits per byte, least significant group first, the high bit says "more follows".
pub fn encode_u64(v: u64, out: &mut Vec<u8>) {
    todo!()
}

/// The value and the number of bytes it took. Accepts non-minimal encodings (`[0x80, 0x00]` is 0, 2 bytes), rejects
/// anything that does not fit in 64 bits (`Overflow`) and input that ends mid-number (`Truncated`).
pub fn decode_u64(buf: &[u8]) -> Result<(u64, usize), DecodeError> {
    todo!()
}

/// Maps small magnitudes to small codes: 0, -1, 1, -2, 2 become 0, 1, 2, 3, 4 (works on `i64::MIN` too).
pub fn zigzag(v: i64) -> u64 {
    todo!()
}

pub fn unzigzag(u: u64) -> i64 {
    todo!()
}

pub fn encode_i64(v: i64, out: &mut Vec<u8>) {
    todo!()
}

pub fn decode_i64(buf: &[u8]) -> Result<(i64, usize), DecodeError> {
    todo!()
}

/// Layout: the count, then the first value, then each difference to the previous value, all as varints.
/// Equal neighbours are fine; a decrease is `Err(NotSorted { index })` with the index of the element that is too small.
/// On error nothing may be appended to `out`.
pub fn encode_sorted(values: &[u64], out: &mut Vec<u8>) -> Result<(), NotSorted> {
    todo!()
}

/// The inverse of `encode_sorted`; bytes after the list are ignored. Do not trust the count: a handful of bytes can
/// claim 2^60 elements, and that must be a `Truncated` error, not an allocation. A sum past `u64::MAX` is `Overflow`.
pub fn decode_sorted(buf: &[u8]) -> Result<Vec<u64>, DecodeError> {
    todo!()
}
"""

P.append(dict(
    slug="varint-codec", title="Varints, zigzag and delta-encoded lists", level="medium", stage="pack-and-scan",
    tags=["LEB128", "zigzag", "delta encoding", "wire format", "untrusted input"],
    teaches=["7-bit groups with a continuation bit: the integer encoding of protobuf, Thrift, DWARF, WebAssembly and SQLite4.",
             "Zigzag turns signed numbers into small unsigned ones, including `i64::MIN`, without a branch.",
             "Decoding untrusted bytes: truncation, 64-bit overflow in the 10th byte, and a length prefix that lies."],
    statement="""
        Write the integer codec every binary protocol has.

        - `encode_u64` / `decode_u64`: LEB128. `decode_u64` returns the value and how many bytes it consumed, accepts
          non-minimal encodings, and rejects a number that does not fit in 64 bits (`Overflow`) or an input that stops in
          the middle of one (`Truncated`).
        - `zigzag` / `unzigzag` and `encode_i64` / `decode_i64`: signed values, small magnitudes stay short.
        - `encode_sorted` / `decode_sorted`: a non-decreasing list as `count, first, delta, delta, ...`, the trick behind
          posting lists in search engines and sorted columns. Unsorted input is `Err(NotSorted { index })`.

        The decoder reads bytes from the network: it must never panic, loop forever or allocate by a length it has not
        verified.
    """,
    starter=VARINT_STARTER,
    solution=VARINT,
    visible=[
        T("small_values_take_one_byte", "0, 1, 127", "{ let mut b = Vec::new(); for v in [0u64, 1, 127] { encode_u64(v, &mut b); } b }", "vec![0, 1, 127]"),
        T("multi_byte", "128, 300", "{ let mut b = Vec::new(); encode_u64(128, &mut b); encode_u64(300, &mut b); b }", "vec![0x80, 0x01, 0xac, 0x02]"),
        T("decode_reports_length", "300 followed by junk", "decode_u64(&[0xac, 0x02, 0xff])", "Ok((300, 2))"),
        T("zigzag_order", "0, -1, 1, -2, 2", "[0i64, -1, 1, -2, 2].map(zigzag)", "[0, 1, 2, 3, 4]"),
        T("sorted_roundtrip", "3, 3, 10, 500", "{ let mut b = Vec::new(); encode_sorted(&[3, 3, 10, 500], &mut b).unwrap(); (b.clone(), decode_sorted(&b)) }", "(vec![4, 3, 0, 7, 0xea, 0x03], Ok(vec![3, 3, 10, 500]))"),
    ],
    hidden=[
        T("u64_max_is_ten_bytes", "u64::MAX", "{ let mut b = Vec::new(); encode_u64(u64::MAX, &mut b); (b.clone(), decode_u64(&b)) }", "(vec![0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01], Ok((u64::MAX, 10)))"),
        T("overflow_in_the_tenth_byte", "10th byte 0x02 would be bit 64", "decode_u64(&[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x02])", "Err(DecodeError::Overflow)"),
        T("eleven_bytes_overflow", "continuation after the 10th byte", "decode_u64(&[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x01])", "Err(DecodeError::Overflow)"),
        T("truncated", "continuation bit but no more bytes; empty input", "(decode_u64(&[0x80]), decode_u64(&[0xff, 0xff]), decode_u64(&[]))", "(Err(DecodeError::Truncated), Err(DecodeError::Truncated), Err(DecodeError::Truncated))"),
        T("non_minimal_is_accepted", "[0x80, 0x00] is 0 in two bytes", "(decode_u64(&[0x80, 0x00]), decode_u64(&[0xe5, 0x8e, 0x26]))", "(Ok((0, 2)), Ok((624485, 3)))"),
        T("zigzag_extremes", "i64::MIN and i64::MAX", "(zigzag(i64::MIN), zigzag(i64::MAX), unzigzag(u64::MAX), unzigzag(u64::MAX - 1), unzigzag(zigzag(i64::MIN)))", "(u64::MAX, u64::MAX - 1, i64::MIN, i64::MAX, i64::MIN)"),
        T("signed_roundtrip_sizes", "-1 is one byte, i64::MIN is ten", "{ let mut a = Vec::new(); encode_i64(-1, &mut a); let mut b = Vec::new(); encode_i64(i64::MIN, &mut b); (a.len(), b.len(), decode_i64(&a), decode_i64(&b)) }", "(1, 10, Ok((-1, 1)), Ok((i64::MIN, 10)))"),
        T("sorted_rejects_a_decrease", "5, 9, 8", "{ let mut b = vec![42]; let r = encode_sorted(&[5, 9, 8], &mut b); (r, b) }", "(Err(NotSorted { index: 2 }), vec![42])"),
        T("sorted_empty_and_single", "[] and [7]", "{ let mut a = Vec::new(); encode_sorted(&[], &mut a).unwrap(); let mut b = Vec::new(); encode_sorted(&[7], &mut b).unwrap(); (a.clone(), decode_sorted(&a), b.clone(), decode_sorted(&b)) }", "(vec![0], Ok(vec![]), vec![1, 7], Ok(vec![7]))"),
        T("sorted_decode_ignores_trailing_bytes", "list then junk", "decode_sorted(&[2, 1, 1, 0xde, 0xad])", "Ok(vec![1, 2])"),
        T("lying_count_does_not_allocate", "count 2^60 with three bytes of data", "{ let mut b = Vec::new(); encode_u64(1 << 60, &mut b); b.extend([1, 1, 1]); decode_sorted(&b) }", "Err(DecodeError::Truncated)"),
        T("delta_sum_overflow", "deltas pass u64::MAX", "{ let mut b = vec![2]; encode_u64(u64::MAX, &mut b); encode_u64(5, &mut b); decode_sorted(&b) }", "Err(DecodeError::Overflow)"),
        """
        #[test]
        fn random_roundtrips() {
            let mut rng = anneal_prelude::Rng::new(6303);
            for _ in 0..400 {
                let bits = rng.below(65) as u32;
                let v = if bits == 0 { 0 } else { (((rng.below(1 << 31) as u64) << 33) | rng.below(1 << 31) as u64 * 4) >> (64 - bits) };
                let mut b = Vec::new();
                encode_u64(v, &mut b);
                let expect_len = ((64 - v.leading_zeros()).max(1) as usize).div_ceil(7);
                check!(format!("u64 {v}"), (decode_u64(&b), b.len()), (Ok((v, expect_len)), expect_len));
                let s = v as i64;
                let mut c = Vec::new();
                encode_i64(s, &mut c);
                check!(format!("i64 {s}"), decode_i64(&c), Ok((s, c.len())));
            }
        }

        #[test]
        fn random_sorted_lists() {
            let mut rng = anneal_prelude::Rng::new(6304);
            for _ in 0..300 {
                let n = rng.below(20);
                let mut v: Vec<u64> = (0..n).map(|_| rng.below(1_000_000) as u64 * rng.below(1000) as u64).collect();
                v.sort();
                let mut b = Vec::new();
                encode_sorted(&v, &mut b).unwrap();
                check!(format!("sorted list {v:?}"), decode_sorted(&b), Ok(v.clone()));
                if b.len() > 1 {
                    let cut = rng.below(b.len() - 1) + 1;
                    let r = decode_sorted(&b[..cut]);
                    assert!(r.is_err() || r.as_ref().map(|x| x.len()).unwrap_or(0) < v.len() || cut == b.len(), "a truncated buffer decoded to {r:?}");
                }
            }
        }

        #[test]
        fn garbage_never_panics() {
            let mut rng = anneal_prelude::Rng::new(6305);
            for _ in 0..2000 {
                let n = rng.below(16);
                let bytes: Vec<u8> = (0..n).map(|_| rng.below(256) as u8).collect();
                let _ = decode_u64(&bytes);
                let _ = decode_i64(&bytes);
                let _ = decode_sorted(&bytes);
            }
        }

        #[test]
        fn a_hundred_thousand_ids() {
            let mut acc = 0u64;
            let ids: Vec<u64> = (0..100_000u64).map(|i| { acc += 1 + i % 7; acc }).collect();
            let mut b = Vec::new();
            encode_sorted(&ids, &mut b).unwrap();
            check!("100000 increasing ids: decoded equal, and compact (about 1 byte each)", (decode_sorted(&b).unwrap() == ids, b.len() < 110_000), (true, true));
        }
        """,
    ],
    wrong=dict(
        trusts_the_count=sub(VARINT, "Vec::with_capacity((count as usize).min(buf.len()))", "Vec::with_capacity(count as usize)"),
        accepts_overflow=sub(VARINT, "if shift == 63 && b > 1 {\n            return Err(DecodeError::Overflow); // the 10th byte may only contribute the top bit\n        }\n", ""),
        zigzag_arithmetic_shift_bug=sub(VARINT, "((v << 1) ^ (v >> 63)) as u64", "(v.unsigned_abs() << 1) | (v < 0) as u64"),
        unsorted_unchecked=sub(VARINT, "if let Some(i) = values.windows(2).position(|w| w[1] < w[0]) {\n        return Err(NotSorted { index: i + 1 });\n    }", "if let Some(i) = values.windows(2).position(|w| w[1] < w[0]) {\n        return Err(NotSorted { index: i });\n    }"),
        delta_wraps=sub(VARINT, "prev = prev.checked_add(d).ok_or(DecodeError::Overflow)?;", "prev = prev.wrapping_add(d);"),
    ),
    hints=[("approach", "Encode: while the value is `>= 0x80`, emit its low 7 bits with the high bit set and shift right by 7; emit the last group as is. Decode: OR each byte's low 7 bits shifted by `7 * index` until a byte without the high bit."),
           ("rust", "Zigzag: `(v << 1) ^ (v >> 63)` (the `>>` on `i64` is arithmetic and gives 0 or -1); inverse `(u >> 1) as i64 ^ -((u & 1) as i64)`. For overflow: the 10th byte (shift 63) may only be 0 or 1, and an 11th byte is an overflow."),
           ("edge", "Never `Vec::with_capacity(count)` for a count you read from the input: cap it by what the buffer could possibly hold (`buf.len()`). Use `checked_add` when summing deltas.")],
    notes=("LEB128 makes small numbers short at the price of a data-dependent loop. Zigzag keeps negative numbers short too (plain two's complement would make `-1` ten bytes). Delta encoding sorted ids turns big numbers into small gaps, which is how search-engine posting lists shrink by an order of magnitude. The security lesson is general for parsers: every length, count and offset in the input is attacker-controlled.", "O(bytes)", "O(output)"),
    follow_up="1) Why do protobuf varints cost a branch per byte, and how does Stream VByte / group varint decode with SIMD (`pshufb`)? 2) Add a canonical-only decoder that rejects `[0x80, 0x00]`: when do protocols require it (signatures, hashes of encoded data)? 3) Compare with fixed-width big-endian ids and with Elias-gamma / Golomb-Rice codes for the gaps. 4) How would you make `decode_sorted` streaming (an iterator that borrows the buffer, no allocation)? 5) What do you do about a hostile `count` of 2^40 with a valid short prefix (limits, incremental growth)?",
    related=["S9", "F7", "B4"],
))


# ---------------------------------------------------------------------------------------------------------------
SWAR = """
const LO: u64 = 0x0101_0101_0101_0101;
const HI: u64 = 0x8080_8080_8080_8080;

/// `b` copied into all eight bytes.
pub fn splat(b: u8) -> u64 {
    LO * b as u64
}

/// The high bit of each byte of `w` that is zero, nothing else. Exact: no byte's result depends on its neighbours.
pub fn zero_bytes(w: u64) -> u64 {
    !(((w & !HI) + !HI) | w | !HI)
}

fn word(c: &[u8]) -> u64 {
    u64::from_le_bytes(c.try_into().unwrap())
}

pub fn find_byte(hay: &[u8], needle: u8) -> Option<usize> {
    let pat = splat(needle);
    let chunks = hay.chunks_exact(8);
    let rem = chunks.remainder();
    let mut base = 0;
    for c in chunks {
        let m = zero_bytes(word(c) ^ pat);
        if m != 0 {
            return Some(base + (m.trailing_zeros() / 8) as usize);
        }
        base += 8;
    }
    rem.iter().position(|&b| b == needle).map(|i| base + i)
}

pub fn rfind_byte(hay: &[u8], needle: u8) -> Option<usize> {
    let chunks = hay.chunks_exact(8);
    let rem = chunks.remainder();
    let base = hay.len() - rem.len();
    if let Some(i) = rem.iter().rposition(|&b| b == needle) {
        return Some(base + i);
    }
    let pat = splat(needle);
    for (k, c) in chunks.enumerate().rev() {
        let m = zero_bytes(word(c) ^ pat);
        if m != 0 {
            return Some(k * 8 + 7 - (m.leading_zeros() / 8) as usize);
        }
    }
    None
}

pub fn count_byte(hay: &[u8], needle: u8) -> usize {
    let pat = splat(needle);
    let chunks = hay.chunks_exact(8);
    let rem = chunks.remainder();
    let mut n = 0;
    for c in chunks {
        n += zero_bytes(word(c) ^ pat).count_ones() as usize;
    }
    n + rem.iter().filter(|&&b| b == needle).count()
}
"""

SWAR_STARTER = """
/// `b` copied into all eight bytes of a u64.
pub fn splat(b: u8) -> u64 {
    todo!()
}

/// The high bit (0x80) of each byte of `w` that is zero, and no other bit set. Must be exact for every byte, with no
/// false positives from borrows between neighbouring bytes.
pub fn zero_bytes(w: u64) -> u64 {
    todo!()
}

/// Index of the first `needle`, scanning eight bytes per step.
pub fn find_byte(hay: &[u8], needle: u8) -> Option<usize> {
    todo!()
}

/// Index of the last `needle`.
pub fn rfind_byte(hay: &[u8], needle: u8) -> Option<usize> {
    todo!()
}

/// How many bytes equal `needle`.
pub fn count_byte(hay: &[u8], needle: u8) -> usize {
    todo!()
}
"""

P.append(dict(
    slug="swar-memchr", title="memchr without SIMD: eight bytes per step", level="hard", stage="roaring",
    tags=["SWAR", "memchr", "zero-byte trick", "endianness", "chunks_exact", "count_ones"],
    teaches=["SWAR (SIMD within a register): treat a `u64` as eight byte lanes and test all of them with a few integer operations.",
             "The zero-byte trick, and why the famous cheap form `(w - LO) & !w & HI` is only trustworthy for 'is there one' and its lane bits can lie above the first hit.",
             "Little-endian lane order: the first byte in memory is the low byte, so `trailing_zeros` finds the first match and `leading_zeros` the last."],
    statement="""
        Build the scalar core of `memchr`: find, find-last and count a byte in a slice while examining eight bytes at a time.

        `splat(b)` repeats a byte across a `u64`. `zero_bytes(w)` returns a mask with the high bit (`0x80`) set in exactly the
        lanes of `w` that are zero. It has to be **exact in every lane**: the classic shortcut `(w - 0x0101..) & !w & 0x8080..`
        can also flag a `0x01` lane that sits above a real zero lane (the borrow ripples up), which is harmless when you
        only want the first hit and wrong when you count. Use a formulation whose lanes never interact.

        `find_byte`, `rfind_byte` and `count_byte` XOR each little-endian word with `splat(needle)` (matching lanes become
        zero), use `zero_bytes`, and handle the tail shorter than eight bytes. Slices of every length and starting offset
        must work.
    """,
    starter=SWAR_STARTER,
    solution=SWAR,
    visible=[
        T("splat_repeats_the_byte", "0xAB", "splat(0xAB)", "0xABAB_ABAB_ABAB_ABABu64"),
        T("one_zero_lane", "only lane 4 is zero", "zero_bytes(0x1122_3300_5566_7788)", "0x0000_0080_0000_0000u64"),
        T("two_zero_lanes", "lanes 0 and 4 are zero", "zero_bytes(0x1122_3300_5566_7700)", "0x0000_0080_0000_0080u64"),
        T("find_first", "two matches", "(find_byte(b\"hello world, hello\", b'o'), find_byte(b\"hello\", b'z'), find_byte(b\"\", b'a'))", "(Some(4), None, None)"),
        T("find_last", "two matches", "(rfind_byte(b\"hello world, hello\", b'o'), rfind_byte(b\"hello\", b'z'), rfind_byte(b\"\", b'a'))", "(Some(17), None, None)"),
        T("count_matches", "three matches", "(count_byte(b\"banana\", b'a'), count_byte(b\"banana\", b'n'), count_byte(b\"\", b'a'))", "(3, 2, 0)"),
    ],
    hidden=[
        T("exact_above_a_zero", "lane 0 is zero, lane 1 is 0x01 (a borrow would flag it)", "zero_bytes(0x1111_1111_1111_0100)", "0x0000_0000_0000_0080u64"),
        T("match_in_last_byte_of_a_word", "index 7", "find_byte(b\"abcdefgXijklmnop\", b'X')", "Some(7)"),
        T("match_in_first_byte_of_a_word", "index 8", "find_byte(b\"abcdefghXjklmnop\", b'X')", "Some(8)"),
        T("match_in_the_tail", "index 18 of 19", "find_byte(b\"abcdefghijklmnopqrX\", b'X')", "Some(18)"),
        T("zero_needle", "NUL bytes", "(find_byte(&[1, 2, 3, 4, 5, 6, 7, 8, 0, 9], 0), rfind_byte(&[0, 2, 3, 4, 5, 6, 7, 8, 0, 9], 0), count_byte(&[0; 20], 0))", "(Some(8), Some(8), 20)"),
        T("high_bytes", "0xFF and 0x80", "(find_byte(&[0x80, 0xFF, 1, 2, 3, 4, 5, 6, 0xFF], 0xFF), rfind_byte(&[0x80, 0xFF, 1, 2, 3, 4, 5, 6, 0xFF], 0xFF), count_byte(&[0xFF; 17], 0xFF))", "(Some(1), Some(8), 17)"),
        T("count_is_exact_with_ones_above_zeros", "needle 0 over bytes 0,1,0,1...", "count_byte(&[0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1], 0)", "8"),
        T("rfind_in_a_full_word", "last of several in one word", "rfind_byte(b\"aXbXcXdXeeeeeeee\", b'X')", "Some(7)"),
        """
        #[test]
        fn random_vs_naive_every_offset() {
            let mut rng = anneal_prelude::Rng::new(6401);
            let buf: Vec<u8> = (0..600).map(|_| (rng.below(6) as u8) * 0x2B).collect(); // few distinct values, includes 0
            for start in 0..9 {
                for len in 0..70 {
                    let s = &buf[start..start + len];
                    for needle in [0u8, 0x2B, 0x56, 0xD7, 0x01] {
                        let f = s.iter().position(|&b| b == needle);
                        let r = s.iter().rposition(|&b| b == needle);
                        let c = s.iter().filter(|&&b| b == needle).count();
                        check!("find/rfind/count agree with a byte loop", (find_byte(s, needle), rfind_byte(s, needle), count_byte(s, needle)), (f, r, c));
                    }
                }
            }
        }

        #[test]
        fn zero_bytes_random_words() {
            let mut rng = anneal_prelude::Rng::new(6402);
            for _ in 0..20_000 {
                let mut w = 0u64;
                for lane in 0..8 {
                    let b = match rng.below(4) { 0 => 0, 1 => 1, 2 => 0x80, _ => rng.below(256) as u8 };
                    w |= (b as u64) << (8 * lane);
                }
                let mut want = 0u64;
                for lane in 0..8 {
                    if (w >> (8 * lane)) & 0xFF == 0 {
                        want |= 0x80 << (8 * lane);
                    }
                }
                check!("zero_bytes is exact in every lane", zero_bytes(w), want);
            }
        }
        """,
    ],
    wrong=dict(
        cheap_mask_counts_wrong=sub(SWAR, "pub fn zero_bytes(w: u64) -> u64 {\n    !(((w & !HI) + !HI) | w | !HI)\n}", "pub fn zero_bytes(w: u64) -> u64 {\n    w.wrapping_sub(LO) & !w & HI\n}"),
        rfind_uses_trailing_zeros=sub(SWAR, "k * 8 + 7 - (m.leading_zeros() / 8) as usize", "k * 8 + (m.trailing_zeros() / 8) as usize"),
        big_endian_words=sub(SWAR, "u64::from_le_bytes(c.try_into().unwrap())", "u64::from_be_bytes(c.try_into().unwrap())"),
        tail_ignored=sub(sub(SWAR, "    rem.iter().position(|&b| b == needle).map(|i| base + i)\n", "    let _ = rem;\n    None\n"), "n + rem.iter().filter(|&&b| b == needle).count()", "n"),
        tail_index_not_offset=sub(SWAR, "rem.iter().position(|&b| b == needle).map(|i| base + i)", "rem.iter().position(|&b| b == needle)"),
    ),
    hints=[("approach", "`x = word ^ splat(needle)` has a zero lane wherever the needle matched. `zero_bytes(x)` flags those lanes. The first match is `m.trailing_zeros() / 8` (little-endian: lane 0 is the lowest byte), the last is `7 - m.leading_zeros() / 8`, the count is `m.count_ones()` if the mask is exact."),
           ("rust", "`hay.chunks_exact(8)` gives full words and `.remainder()` the tail (take the remainder before consuming the iterator). `u64::from_le_bytes(chunk.try_into().unwrap())` is free after optimisation and avoids unaligned-pointer tricks."),
           ("edge", "Exact zero test: `!(((w & 0x7F..) + 0x7F..) | w | 0x7F..)`. Masking off the high bit before adding means a lane can never carry into its neighbour; the OR with `w` catches lanes whose high bit was set.")],
    notes=("The `memchr` crate does this (and AVX2/NEON beyond it); `std`'s `memchr` fallback and `str::find` for a single byte use the SWAR trick. Doing eight lanes per step turns a latency-bound byte loop into a throughput-bound one, and the same lane masks are the building block for SIMD movemask scans, UTF-8 validation and hash-table group probing (hashbrown's generic group is exactly this).", "O(n / 8) steps", "O(1) extra"),
    follow_up="1) Why is the cheap form fine for `find_byte` but not `count_byte`? Construct the smallest failing word. 2) Replace `from_le_bytes` with an unaligned read through `ptr::read_unaligned`: is it faster, and is it sound at the slice end? 3) Write the SSE2 version with `_mm_cmpeq_epi8` / `_mm_movemask_epi8` and compare the mask handling (16 bits, `trailing_zeros` directly). 4) Extend to two needles at once (`memchr2`). 5) Where does hashbrown use the same mask trick for its control bytes?",
    related=["F1", "F5", "S9"],
))

STAGES = [("first-bits", "First bits", "easy"), ("pack-and-scan", "Pack and scan", "medium"), ("roaring", "Scan eight bytes at a time", "hard")]

if __name__ == "__main__":
    n = write_track("f5-cpu-tricks", "F5", "CPU-level tricks", "F", "sde3", 5,
                    "Bit-level and word-level tricks that real systems lean on: bitsets, bit-packed vectors, varints and SWAR byte scans.",
                    STAGES, P)
    print("F5", n)
