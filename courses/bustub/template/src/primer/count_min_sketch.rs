//! A **count-min sketch** estimates how often each item occurred in a stream, in a fixed amount of memory. It is a `depth x width` matrix of
//! counters. Every row has its own hash function; adding an item adds one to the counter its hash selects in each row. An item's count is the
//! **minimum** of its counters: collisions can only add to a counter, never take away, so the minimum is the least inflated one, and the
//! estimate is never too low.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::common::exception::{Exception, ExceptionType, Result};

const SEED_BASE: u64 = 15445;

pub struct CountMinSketch<K> {
    _key: PhantomData<fn(&K)>,
    // TODO(0d-01): your fields: the dimensions and the counters (atomic counters let many threads add without a lock)
}

impl<K: Hash> CountMinSketch<K> {
    /// A sketch of `depth` rows of `width` counters, all zero. A zero width or depth is an error.
    pub fn new(width: u32, depth: u32) -> Result<CountMinSketch<K>> {
        todo!("0d-01: an error for a zero width or depth; otherwise width x depth counters, all zero")
    }

    /// The column of `item` in row `row`: a hash of the item seeded by the row, modulo the width. Every row must hash differently
    /// (the same item lands in different columns of different rows) and always the same way (the same row and item: the same column).
    pub fn column(&self, row: u32, item: &K) -> usize {
        todo!("0d-01: hash (SEED_BASE, row) and then the item with a DefaultHasher; modulo the width")
    }

    // TODO(0d-01): helpers of your own (the counter of an item in a row)

    /// Counts one more occurrence of `item`.
    pub fn insert(&self, item: &K) {
        todo!("0d-01: add one to the item's counter in every row")
    }

    /// The estimated number of occurrences of `item`: never lower than the true count.
    pub fn count(&self, item: &K) -> u32 {
        todo!("0d-01: the smallest of the item's counters over the rows")
    }

    /// Back to the empty sketch.
    pub fn clear(&self) {
        todo!("0d-01: every counter to zero")
    }

    /// Adds the counters of `other` to this sketch's, so that it counts both streams. The sketches must have the same dimensions
    /// (error otherwise): the same hash functions put an item in the same columns.
    pub fn merge(&self, other: &CountMinSketch<K>) -> Result<()> {
        todo!("0d-01: an error (Invalid) if the widths or depths differ; otherwise add every counter of `other` to the same counter here")
    }

    /// The `k` candidates with the highest estimated counts, as `(item, count)` pairs in descending order of count (candidates with equal counts keep
    /// their order). At most `candidates.len()` pairs. A sketch does not remember items, which is why the caller supplies the candidates.
    pub fn top_k(&self, k: u16, candidates: &[K]) -> Vec<(K, u32)>
    where
        K: Clone,
    {
        todo!("0d-01: the estimated count of every candidate; sort by count, highest first (stably); keep the first k")
    }
}
