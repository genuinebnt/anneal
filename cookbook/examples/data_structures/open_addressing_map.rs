//! RECIPE 121 (hashing): an open-addressing hash map with linear probing, Robin Hood displacement and backward-shift deletion.
//! LEVEL: advanced
//! Use in: understanding how fast flat hash tables work; building specialised maps (interners, symbol tables).   Run: cargo run --release --example open_addressing_map
#![allow(dead_code)]
//!
//! WHAT: `OpenMap<K, V, S>`: one flat `Vec<Option<Slot>>`, no per-bucket allocation, no tombstones, generic over `K: Hash + Eq` and any `BuildHasher`.
//! WHY: chaining chases pointers; open addressing keeps probes in adjacent cache lines. Plain linear probing suffers from long unlucky probe runs.
//! HOW:
//!  - capacity is a power of two, so `hash & mask` replaces `%`. We resize (double) when load would exceed 7/8.
//!  - Robin Hood: each slot remembers its hash, hence its "distance from home". While inserting, if the resident is closer to home than the
//!    newcomer ("richer"), they swap and we keep inserting the evicted one. This equalises probe lengths (low variance), and lets lookups stop
//!    early: once we meet a resident whose distance is smaller than ours, the key cannot be further on.
//!  - Backward-shift deletion: after removing, pull following entries one slot back until an empty slot or an entry already at home. This keeps
//!    the Robin Hood invariant without tombstones, so lookups never degrade after many deletes.
//! Verified by a differential test against std's `HashMap` (20k seeded random operations) and by probe-length statistics. A weak hash shows why
//! the hasher matters far more for open addressing than for chaining.
//!
//! COMMON USES: Robin Hood hashing was used by older versions of Rust std HashMap (hashbrown, its replacement, instead
//! uses SIMD group probing); symbol tables, interners, in-memory indexes, game/engine entity maps where cache behaviour matters.
//! Read: https://en.wikipedia.org/wiki/Hash_table and https://github.com/rust-lang/hashbrown
use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::{BuildHasher, BuildHasherDefault, Hash, Hasher, RandomState};

struct Slot<K, V> {
    hash: u64,
    key: K,
    val: V,
}

struct OpenMap<K, V, S = RandomState> {
    slots: Vec<Option<Slot<K, V>>>,
    len: usize,
    build: S,
}

impl<K: Hash + Eq, V> OpenMap<K, V, RandomState> {
    fn new() -> Self {
        Self::with_hasher(RandomState::new())
    }
}

impl<K: Hash + Eq, V, S: BuildHasher> OpenMap<K, V, S> {
    fn with_hasher(build: S) -> Self {
        Self::with_capacity_and_hasher(8, build)
    }
    /// Capacity is rounded up to a power of two (at least 8).
    fn with_capacity_and_hasher(cap: usize, build: S) -> Self {
        let cap = cap.next_power_of_two().max(8);
        OpenMap {
            slots: (0..cap).map(|_| None).collect(),
            len: 0,
            build,
        }
    }
    fn len(&self) -> usize {
        self.len
    }
    fn capacity(&self) -> usize {
        self.slots.len()
    }
    fn mask(&self) -> usize {
        self.slots.len() - 1
    }
    /// How far slot `i` is from the home position of `hash` (wrapping).
    fn dist(&self, i: usize, hash: u64) -> usize {
        i.wrapping_sub(hash as usize & self.mask()) & self.mask()
    }

    fn find<Q: Hash + Eq + ?Sized>(&self, k: &Q) -> Option<usize>
    where
        K: Borrow<Q>,
    {
        let h = self.build.hash_one(k);
        let mut i = h as usize & self.mask();
        for d in 0.. {
            let s = self.slots[i].as_ref()?; // empty slot: absent
            if self.dist(i, s.hash) < d {
                return None; // Robin Hood early exit: a richer resident means the key would have displaced it
            }
            if s.hash == h && s.key.borrow() == k {
                return Some(i);
            }
            i = (i + 1) & self.mask();
        }
        None
    }

    fn get<Q: Hash + Eq + ?Sized>(&self, k: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
    {
        self.find(k).map(|i| &self.slots[i].as_ref().unwrap().val)
    }

    fn contains_key<Q: Hash + Eq + ?Sized>(&self, k: &Q) -> bool
    where
        K: Borrow<Q>,
    {
        self.find(k).is_some()
    }

    fn insert(&mut self, key: K, val: V) -> Option<V> {
        if let Some(i) = self.find(&key) {
            return Some(std::mem::replace(
                &mut self.slots[i].as_mut().unwrap().val,
                val,
            ));
        }
        self.insert_new(key, val);
        None
    }

    /// Insert a key known to be absent; returns the slot where the NEW entry landed.
    fn insert_new(&mut self, key: K, val: V) -> usize {
        if (self.len + 1) * 8 > self.capacity() * 7 {
            self.grow();
        }
        let hash = self.build.hash_one(&key);
        self.len += 1;
        self.place(Slot { hash, key, val })
    }

    fn place(&mut self, mut cur: Slot<K, V>) -> usize {
        let mask = self.mask();
        let mut i = cur.hash as usize & mask;
        let (mut d, mut landed) = (0, None);
        loop {
            match &mut self.slots[i] {
                slot @ None => {
                    *slot = Some(cur);
                    return landed.unwrap_or(i);
                }
                Some(res) => {
                    let rd = i.wrapping_sub(res.hash as usize & mask) & mask;
                    if rd < d {
                        std::mem::swap(res, &mut cur); // steal from the rich: resident moves on, we stay
                        landed.get_or_insert(i);
                        d = rd;
                    }
                }
            }
            i = (i + 1) & mask;
            d += 1;
        }
    }

    fn grow(&mut self) {
        let new_cap = self.capacity() * 2;
        let old = std::mem::replace(&mut self.slots, (0..new_cap).map(|_| None).collect());
        for s in old.into_iter().flatten() {
            self.place(s); // hash is stored, so no rehash of keys
        }
    }

    fn remove<Q: Hash + Eq + ?Sized>(&mut self, k: &Q) -> Option<V>
    where
        K: Borrow<Q>,
    {
        let mut j = self.find(k)?;
        let removed = self.slots[j].take().unwrap();
        loop {
            let next = (j + 1) & self.mask();
            match &self.slots[next] {
                Some(s) if self.dist(next, s.hash) > 0 => {
                    self.slots[j] = self.slots[next].take(); // shift back one slot, closer to home
                    j = next;
                }
                _ => break, // empty, or already at home: stop
            }
        }
        self.len -= 1;
        Some(removed.val)
    }

    /// Entry-like API: return the value for `key`, inserting `f()` first if absent.
    fn get_or_insert_with(&mut self, key: K, f: impl FnOnce() -> V) -> &mut V {
        let i = match self.find(&key) {
            Some(i) => i,
            None => self.insert_new(key, f()),
        };
        &mut self.slots[i].as_mut().unwrap().val
    }

    fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.slots.iter().flatten().map(|s| (&s.key, &s.val))
    }

    /// (average, maximum) probe distance over all stored entries.
    fn probe_stats(&self) -> (f64, usize) {
        let ds: Vec<usize> = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.as_ref().map(|s| self.dist(i, s.hash)))
            .collect();
        (
            ds.iter().sum::<usize>() as f64 / ds.len().max(1) as f64,
            ds.into_iter().max().unwrap_or(0),
        )
    }

    /// Robin Hood invariant: every entry is at most one farther from home than its predecessor, and findable.
    fn check_invariants(&self) {
        let mut count = 0;
        for i in 0..self.capacity() {
            if let Some(s) = &self.slots[i] {
                count += 1;
                let prev = (i + self.mask()) & self.mask();
                if let Some(p) = &self.slots[prev] {
                    assert!(
                        self.dist(i, s.hash) <= self.dist(prev, p.hash) + 1,
                        "robin hood invariant broken at {i}"
                    );
                } else {
                    assert_eq!(
                        self.dist(i, s.hash),
                        0,
                        "entry after a hole must be at home"
                    );
                }
                assert_eq!(self.find(&s.key), Some(i));
            }
        }
        assert_eq!(count, self.len);
    }
}

/// A decent hasher (word fold + murmur finalizer) for the "good hash" runs.
#[derive(Default)]
struct Good(u64);
impl Hasher for Good {
    fn write(&mut self, bytes: &[u8]) {
        for c in bytes.chunks(8) {
            let mut b = [0u8; 8];
            b[..c.len()].copy_from_slice(c);
            self.0 =
                (self.0.rotate_left(5) ^ u64::from_le_bytes(b)).wrapping_mul(0x517c_c1b7_2722_0a95);
        }
    }
    fn finish(&self) -> u64 {
        let mut k = self.0;
        k ^= k >> 33;
        k = k.wrapping_mul(0xff51_afd7_ed55_8ccd);
        k ^= k >> 33;
        k = k.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
        k ^ (k >> 33)
    }
}

/// FNV-1a truncated to 6 bits: only 64 distinct hash values, so clusters are inevitable.
struct WeakFnv(u64);
impl Default for WeakFnv {
    fn default() -> Self {
        WeakFnv(0xcbf2_9ce4_8422_2325)
    }
}
impl Hasher for WeakFnv {
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    fn finish(&self) -> u64 {
        self.0 >> 58 // keep 6 bits
    }
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
}

/// Run random operations on both maps; every return value and (periodically) the full contents must agree.
fn differential<S: BuildHasher>(
    mut mine: OpenMap<u64, u64, S>,
    ops: usize,
    key_space: u64,
    seed: u64,
) {
    let mut oracle: HashMap<u64, u64> = HashMap::new();
    let mut rng = Rng(seed);
    for step in 0..ops {
        let k = rng.next() % key_space;
        let v = rng.next();
        match rng.next() % 4 {
            0 => assert_eq!(mine.insert(k, v), oracle.insert(k, v)),
            1 => assert_eq!(mine.remove(&k), oracle.remove(&k)),
            2 => assert_eq!(mine.get(&k), oracle.get(&k)),
            _ => assert_eq!(
                *mine.get_or_insert_with(k, || v),
                *oracle.entry(k).or_insert(v)
            ),
        }
        assert_eq!(mine.len(), oracle.len());
        if step % 2_000 == 0 {
            mine.check_invariants();
            let mut a: Vec<_> = mine.iter().map(|(k, v)| (*k, *v)).collect();
            let mut b: Vec<_> = oracle.iter().map(|(k, v)| (*k, *v)).collect();
            a.sort();
            b.sort();
            assert_eq!(a, b);
        }
    }
    mine.check_invariants();
}

fn stats_at_load_factors<S: BuildHasher>(label: &str, build: S) -> Vec<(f64, f64, usize)> {
    let cap = 1 << 14;
    let mut m: OpenMap<u64, u64, S> = OpenMap::with_capacity_and_hasher(cap, build);
    let mut out = vec![];
    let mut next_key = 0u64;
    println!("{label}: load  avg probe  max probe");
    for target in [0.5, 0.7, 0.8, 0.87] {
        while (m.len() as f64) < target * cap as f64 {
            m.insert(next_key.wrapping_mul(0x9e37_79b9_97f4_a7c1) >> 3, next_key);
            next_key += 1;
        }
        assert_eq!(m.capacity(), cap, "no resize expected below 7/8");
        let (avg, max) = m.probe_stats();
        println!(
            "  {:>20.2}{avg:>11.2}{max:>11}",
            m.len() as f64 / cap as f64
        );
        out.push((target, avg, max));
    }
    out
}

fn main() {
    // Basic API, including borrowed lookup (`&str` for `String` keys) and the entry-like helper.
    let mut words: OpenMap<String, usize> = OpenMap::new();
    for w in "the quick brown fox jumps over the lazy dog the end".split(' ') {
        *words.get_or_insert_with(w.to_string(), || 0) += 1;
    }
    assert_eq!(words.get("the"), Some(&3));
    assert_eq!(words.len(), 9);
    assert_eq!(words.remove("the"), Some(3));
    assert!(!words.contains_key("the") && words.contains_key("fox"));
    println!(
        "word counts ok: {} distinct words, capacity {}",
        words.len(),
        words.capacity()
    );

    differential(OpenMap::new(), 20_000, 3_000, 1);
    differential(
        OpenMap::with_hasher(BuildHasherDefault::<Good>::default()),
        20_000,
        500,
        2,
    );
    differential(
        OpenMap::with_hasher(BuildHasherDefault::<WeakFnv>::default()),
        5_000,
        400,
        3,
    );
    println!("differential tests vs std HashMap: 20k ops x2 + 5k ops with the weak hash all agree");

    let good = stats_at_load_factors("good hash", BuildHasherDefault::<Good>::default());
    // Same key stream, but only 64 distinct hashes: smaller map so the quadratic cluster cost stays tiny.
    let mut weak: OpenMap<u64, u64, _> =
        OpenMap::with_capacity_and_hasher(4096, BuildHasherDefault::<WeakFnv>::default());
    for i in 0..2_000u64 {
        weak.insert(i, i);
    }
    let (wavg, wmax) = weak.probe_stats();
    println!(
        "weak 6-bit FNV, 2000 keys in 4096 slots (load 0.49): avg probe {wavg:.1}, max {wmax}"
    );

    // Robin Hood keeps the mean near the linear-probing theory (~(1/(1-a))/2) and the max small with a good hash.
    assert!(
        good[0].1 < 1.0 && good[3].1 < 8.0,
        "average probe too long: {good:?}"
    );
    assert!(good[3].2 < 100, "max probe too long: {good:?}");
    assert!(good[0].1 < good[3].1, "probe length grows with load");
    assert!(wavg > 5.0 * good[1].1.max(0.5), "weak hash must hurt");
    println!("OK");
}
