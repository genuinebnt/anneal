//! The quiz is reasoning only: `size_of`, `align_of`, `offset_of!` and `Layout` are off limits in this file.
use std::num::NonZeroU32;

pub type N1 = Option<NonZeroU32>;
pub type N2 = Option<Box<u64>>;
pub type N3 = Option<&'static str>;
pub type N4 = Option<f64>;
pub type N5 = Option<char>;
pub type N6 = Option<Option<bool>>;
pub type N7 = Option<(u8, u32)>;
pub type N8 = Option<Vec<u8>>;

/// `size_of` of `N1` to `N8`, in order.
pub const OPTION_SIZES: [usize; 8] = [4, 8, 16, 16, 4, 1, 12, 24];

/// An index into `ChainMap`'s entries, stored as its bitwise complement in a `NonZeroU32`: every index up to
/// `u32::MAX - 1` has a non-zero complement, and the zero bit pattern is left free for `None`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Idx(NonZeroU32);

impl Idx {
    /// The largest index an `Idx` holds.
    pub const MAX: usize = u32::MAX as usize - 1;

    /// Panics if `i > Idx::MAX`.
    pub fn new(i: usize) -> Idx {
        assert!(i <= Idx::MAX, "index {i} doesn't fit an Idx");
        Idx(NonZeroU32::new(!(i as u32)).expect("i <= MAX, so the stored value is non-zero"))
    }

    pub fn index(self) -> usize {
        !self.0.get() as usize
    }
}

/// One key/value pair and the next entry in its bucket's chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub key: u32,
    pub value: u32,
    pub next: Option<Idx>,
}

/// A hash map with chaining through indices: `heads[b]` is the first entry of bucket `b`, and each entry
/// links to the next. Entries are never removed.
pub struct ChainMap {
    heads: Vec<Option<Idx>>,
    entries: Vec<Entry>,
}

impl ChainMap {
    /// `buckets` is a power of two.
    pub fn with_buckets(buckets: usize) -> ChainMap {
        assert!(buckets.is_power_of_two());
        ChainMap {
            heads: vec![None; buckets],
            entries: Vec::new(),
        }
    }

    fn bucket(&self, key: u32) -> usize {
        key.wrapping_mul(0x9E37_79B9).rotate_left(16) as usize & (self.heads.len() - 1)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn find(&self, key: u32) -> Option<usize> {
        let mut cur = self.heads[self.bucket(key)];
        while let Some(i) = cur {
            let e = &self.entries[i.index()];
            if e.key == key {
                return Some(i.index());
            }
            cur = e.next;
        }
        None
    }

    /// Sets `key` to `value` and returns the old value. A new key goes to the front of its bucket's chain.
    pub fn insert(&mut self, key: u32, value: u32) -> Option<u32> {
        if let Some(i) = self.find(key) {
            return Some(std::mem::replace(&mut self.entries[i].value, value));
        }
        let b = self.bucket(key);
        let at = Idx::new(self.entries.len());
        self.entries.push(Entry {
            key,
            value,
            next: self.heads[b],
        });
        self.heads[b] = Some(at);
        None
    }

    pub fn get(&self, key: u32) -> Option<u32> {
        self.find(key).map(|i| self.entries[i].value)
    }

    /// The keys in `key`'s bucket, front of the chain first.
    pub fn chain_of(&self, key: u32) -> Vec<u32> {
        let mut out = Vec::new();
        let mut cur = self.heads[self.bucket(key)];
        while let Some(i) = cur {
            out.push(self.entries[i.index()].key);
            cur = self.entries[i.index()].next;
        }
        out
    }
}
