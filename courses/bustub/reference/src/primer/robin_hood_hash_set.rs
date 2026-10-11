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
    // @begin 0c-01
    capacity: usize,
    table: RwLock<Table<K>>,
    //~ _set: std::marker::PhantomData<K>,
    //~ // TODO(0c-01): your fields: the capacity, the buckets (with a way to tell an empty one from a removed one from a live one), the number of live keys, the lock
    // @end
}

// @begin 0c-01
#[derive(Clone, Debug, PartialEq)]
enum Slot<K> {
    Empty,
    /// A removed key. Lookups probe past it; an insert may reuse it.
    Tombstone,
    Live(K),
}

struct Table<K> {
    slots: Vec<Slot<K>>,
    /// The number of live keys.
    size: usize,
}
//~ // TODO(0c-01): the bucket type and the table of your own
// @end

impl<K: RobinHoodHash + Clone + PartialEq> RobinHoodHashSet<K> {
    /// A set with exactly `capacity` buckets, all empty. Capacity 0 is an error.
    pub fn new(capacity: usize) -> Result<RobinHoodHashSet<K>> {
        // @begin 0c-01
        if capacity == 0 {
            return Err(Exception::new(ExceptionType::Invalid, "capacity must be positive"));
        }
        Ok(RobinHoodHashSet { capacity, table: RwLock::new(Table { slots: vec![Slot::Empty; capacity], size: 0 }) })
        //~ todo!("0c-01: an error for capacity 0; otherwise `capacity` empty slots and size 0")
        // @end
    }

    /// The bucket a key starts probing at.
    pub fn home_bucket(&self, key: &K) -> usize {
        // @begin 0c-01
        key.robin_hood_hash() % self.capacity
        //~ todo!("0c-01: the key's hash modulo the capacity")
        // @end
    }

    /// How far `bucket` is from `home`, going forward and wrapping around the end of the table.
    pub fn probe_distance(&self, home: usize, bucket: usize) -> usize {
        // @begin 0c-01
        (bucket + self.capacity - home) % self.capacity
        //~ todo!("0c-01: steps forward from home to bucket, modulo the capacity")
        // @end
    }

    /// The capacity the set was made with.
    pub fn capacity(&self) -> usize {
        // @begin 0c-01
        self.capacity
        //~ todo!("0c-01: the capacity the set was made with")
        // @end
    }

    /// The number of buckets (the same as the capacity).
    pub fn bucket_count(&self) -> usize {
        // @begin 0c-01
        self.capacity
        //~ todo!("0c-01: the number of buckets")
        // @end
    }

    /// The number of live keys.
    pub fn size(&self) -> usize {
        // @begin 0c-01
        self.table.read().unwrap().size
        //~ todo!("0c-01: the number of live keys, under the read lock")
        // @end
    }

    /// `size / capacity`.
    pub fn load_factor(&self) -> f64 {
        // @begin 0c-01
        self.size() as f64 / self.capacity as f64
        //~ todo!("0c-01: size divided by capacity")
        // @end
    }

    /// Adds `key`. Returns `true` if it was added or an equal key was already there (the key is replaced), `false` if the table is full.
    pub fn insert(&self, key: &K) -> bool {
        // @begin 0c-02
        let mut table = self.table.write().unwrap();
        // an equal key already there: replace it
        if let Some(bucket) = self.find(&table, key) {
            table.slots[bucket] = Slot::Live(key.clone());
            return true;
        }
        // no empty bucket and no tombstone left: nothing may be touched (a displaced key would have nowhere to go)
        if table.size == self.capacity {
            return false;
        }
        let mut carried = key.clone();
        let mut home = self.home_bucket(&carried);
        let mut bucket = home;
        for _ in 0..self.capacity {
            match &table.slots[bucket] {
                Slot::Empty => {
                    table.slots[bucket] = Slot::Live(carried);
                    table.size += 1;
                    return true;
                }
                // a tombstone is free too (nothing makes one until `remove`, but the code must be ready)
                Slot::Tombstone => {
                    table.slots[bucket] = Slot::Live(carried);
                    table.size += 1;
                    return true;
                }
                Slot::Live(resident) => {
                    // take the bucket from a key that is closer to its home than the carried one is to its own
                    let resident_home = self.home_bucket(resident);
                    if self.probe_distance(resident_home, bucket) < self.probe_distance(home, bucket) {
                        let Slot::Live(displaced) = std::mem::replace(&mut table.slots[bucket], Slot::Live(carried)) else { unreachable!() };
                        carried = displaced;
                        home = resident_home;
                    }
                }
            }
            bucket = (bucket + 1) % self.capacity;
        }
        unreachable!("with fewer live keys than buckets there is a free bucket on the way")
        //~ todo!("0c-02: true if added or replaced, false when full (before touching anything); probing from the home bucket, a resident closer to its home is displaced and carries on, and a tombstone is free")
        // @end
    }

    /// Is `key` in the set?
    pub fn contains(&self, key: &K) -> bool {
        // @begin 0c-02
        let table = self.table.read().unwrap();
        self.find(&table, key).is_some()
        //~ todo!("0c-02: read lock; is there a bucket for the key (find)?")
        // @end
    }

    /// The bucket that holds `key`, or `bucket_count()` if it is absent.
    pub fn get_bucket(&self, key: &K) -> usize {
        // @begin 0c-02
        let table = self.table.read().unwrap();
        self.find(&table, key).unwrap_or(self.capacity)
        //~ todo!("0c-02: the bucket found for the key, or bucket_count() if none")
        // @end
    }

    /// The bucket of `key`: probe from its home until an empty bucket, past tombstones and past other keys.
    ///
    /// A textbook Robin Hood lookup also stops at a key that is closer to its home than we are to ours (our key would have displaced
    /// it). With tombstones that rule is **unsafe**: `remove` leaves a tombstone, a later insert may put a key with a short probe
    /// distance into it, and a key that was already further on is then unreachable if the lookup stops in front of that new key.
    // @begin 0c-02
    fn find(&self, table: &Table<K>, key: &K) -> Option<usize> {
        let home = self.home_bucket(key);
        for distance in 0..self.capacity {
            let bucket = (home + distance) % self.capacity;
            match &table.slots[bucket] {
                Slot::Empty => return None,
                Slot::Tombstone => {}
                Slot::Live(resident) => {
                    if resident == key {
                        return Some(bucket);
                    }
                }
            }
        }
        None
    }
    //~ // TODO(0c-02): a private lookup of your own: probe from the home bucket until an empty bucket; an equal live key is the answer; tombstones and other keys are probed past
    // @end

    /// Removes `key`, leaving a tombstone. Returns whether it was there.
    pub fn remove(&self, key: &K) -> bool {
        // @begin 0c-03
        let mut table = self.table.write().unwrap();
        let Some(bucket) = self.find(&table, key) else { return false };
        table.slots[bucket] = Slot::Tombstone;
        table.size -= 1;
        true
        //~ todo!("0c-03: write lock; find the key; put a tombstone in its bucket; size - 1; true. Absent: false")
        // @end
    }

    /// Forgets every key and tombstone; the capacity stays.
    pub fn clear(&self) {
        // @begin 0c-03
        let mut table = self.table.write().unwrap();
        table.slots.iter_mut().for_each(|s| *s = Slot::Empty);
        table.size = 0;
        //~ todo!("0c-03: write lock; every bucket empty; size 0")
        // @end
    }

    /// The largest probe distance among live keys (0 for an empty table); tombstones do not count.
    pub fn max_probe_distance(&self) -> usize {
        // @begin 0c-03
        let table = self.table.read().unwrap();
        (0..self.capacity)
            .filter_map(|bucket| match &table.slots[bucket] {
                Slot::Live(key) => Some(self.probe_distance(self.home_bucket(key), bucket)),
                _ => None,
            })
            .max()
            .unwrap_or(0)
        //~ todo!("0c-03: read lock; the largest probe_distance(home, bucket) over live buckets")
        // @end
    }
}
