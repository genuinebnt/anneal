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
pub const OPTION_SIZES: [usize; 8] = [0; 8];

/// An index into `ChainMap`'s entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Idx(u32);

impl Idx {
    /// The largest index an `Idx` holds.
    pub const MAX: usize = u32::MAX as usize - 1;

    /// Panics if `i > Idx::MAX`.
    pub fn new(i: usize) -> Idx {
        assert!(i <= Idx::MAX, "index {i} doesn't fit an Idx");
        Idx(i as u32)
    }

    pub fn index(self) -> usize {
        self.0 as usize
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

    /// Sets `key` to `value` and returns the old value. A new key goes to the front of its bucket's chain.
    pub fn insert(&mut self, key: u32, value: u32) -> Option<u32> {
        todo!()
    }

    pub fn get(&self, key: u32) -> Option<u32> {
        todo!()
    }

    /// The keys in `key`'s bucket, front of the chain first.
    pub fn chain_of(&self, key: u32) -> Vec<u32> {
        todo!()
    }
}
