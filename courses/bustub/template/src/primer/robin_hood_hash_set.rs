//! A fixed-capacity hash set that resolves collisions by **linear probing with Robin Hood displacement**.
//!
//! Every key has a *home bucket* (`hash % capacity`) and is stored at the first bucket from there that suits it. The *probe distance* of a key is
//! how far it sits from its home. Robin Hood hashing takes from the rich and gives to the poor: when an incoming key is further from its home than
//! the key it meets, it takes that bucket and the displaced key probes on. That keeps probe distances even, and keeps the longest probe short. A
//! removed key leaves a **tombstone**.

use std::sync::RwLock;

use crate::common::exception::{Exception, ExceptionType, Result};

/// A deterministic hash of a key, the same on every run and machine, so that a caller can pick keys for a given home bucket.
pub trait RobinHoodHash {
    fn robin_hood_hash(&self) -> usize;
}

impl RobinHoodHash for i32 {
    fn robin_hood_hash(&self) -> usize {
        (*self as i64 as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) as usize
    }
}

impl RobinHoodHash for i64 {
    fn robin_hood_hash(&self) -> usize {
        (*self as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) as usize
    }
}

impl RobinHoodHash for String {
    /// FNV-1a over the bytes, then a final mixing step.
    fn robin_hood_hash(&self) -> usize {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in self.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        h ^= h >> 33;
        h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
        h ^= h >> 33;
        h = h.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
        (h ^ (h >> 33)) as usize
    }
}

pub struct RobinHoodHashSet<K> {
    _set: std::marker::PhantomData<K>,
    // TODO(0c-01): your fields: the capacity, the buckets (with a way to tell an empty one from a removed one from a live one), the number of live keys, the lock
}

// TODO(0c-01): the bucket type and the table of your own

impl<K: RobinHoodHash + Clone + PartialEq> RobinHoodHashSet<K> {
    /// A set with exactly `capacity` buckets, all empty. Capacity 0 is an error.
    pub fn new(capacity: usize) -> Result<RobinHoodHashSet<K>> {
        todo!("0c-01: an error for capacity 0; otherwise `capacity` empty slots and size 0")
    }

    /// The bucket a key starts probing at.
    pub fn home_bucket(&self, key: &K) -> usize {
        todo!("0c-01: the key's hash modulo the capacity")
    }

    /// How far `bucket` is from `home`, going forward and wrapping around the end of the table.
    pub fn probe_distance(&self, home: usize, bucket: usize) -> usize {
        todo!("0c-01: steps forward from home to bucket, modulo the capacity")
    }

    /// The capacity the set was made with.
    pub fn capacity(&self) -> usize {
        todo!("0c-01: the capacity the set was made with")
    }

    /// The number of buckets (the same as the capacity).
    pub fn bucket_count(&self) -> usize {
        todo!("0c-01: the number of buckets")
    }

    /// The number of live keys.
    pub fn size(&self) -> usize {
        todo!("0c-01: the number of live keys, under the read lock")
    }

    /// `size / capacity`.
    pub fn load_factor(&self) -> f64 {
        todo!("0c-01: size divided by capacity")
    }

    /// Adds `key`. Returns `true` if it was added or an equal key was already there (the key is replaced), `false` if the table is full.
    pub fn insert(&self, key: &K) -> bool {
        todo!("0c-02: write lock; if the key is already there replace it and say true; if size == capacity say false before touching anything; else probe from the home bucket with `carried` and its home: an empty bucket takes the carried key (size + 1, true); a tombstone is free too; a live resident whose probe distance is smaller than the carried key's is swapped out and becomes the carried key; go to the next bucket (wrapping)")
    }

    /// Is `key` in the set?
    pub fn contains(&self, key: &K) -> bool {
        todo!("0c-02: read lock; is there a bucket for the key (find)?")
    }

    /// The bucket that holds `key`, or `bucket_count()` if it is absent.
    pub fn get_bucket(&self, key: &K) -> usize {
        todo!("0c-02: the bucket found for the key, or bucket_count() if none")
    }

    /// The bucket of `key`: probe from its home until an empty bucket, past tombstones and past other keys.
    ///
    /// A textbook Robin Hood lookup also stops at a key that is closer to its home than we are to ours (our key would have displaced
    /// it). With tombstones that rule is **unsafe**: `remove` leaves a tombstone, a later insert may put a key with a short probe
    /// distance into it, and a key that was already further on is then unreachable if the lookup stops in front of that new key.
    // TODO(0c-02): a private lookup of your own: probe from the home bucket until an empty bucket; an equal live key is the answer; tombstones and other keys are probed past

    /// Removes `key`, leaving a tombstone. Returns whether it was there.
    pub fn remove(&self, key: &K) -> bool {
        todo!("0c-03: write lock; find the key; put a tombstone in its bucket; size - 1; true. Absent: false")
    }

    /// Forgets every key and tombstone; the capacity stays.
    pub fn clear(&self) {
        todo!("0c-03: write lock; every bucket empty; size 0")
    }

    /// The largest probe distance among live keys (0 for an empty table); tombstones do not count.
    pub fn max_probe_distance(&self) -> usize {
        todo!("0c-03: read lock; the largest probe_distance(home, bucket) over live buckets")
    }
}
