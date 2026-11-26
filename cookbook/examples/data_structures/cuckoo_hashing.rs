//! RECIPE 123 (hashing): cuckoo hashing: a two-table cuckoo hash map AND a cuckoo filter (approximate membership with delete).
//! LEVEL: advanced
//! Use in: worst-case O(1) lookups (two probes), and space-efficient set membership that supports deletion.   Run: cargo run --release --example cuckoo_hashing
#![allow(dead_code)]
//!
//! WHAT/WHY: a cuckoo TABLE gives every key exactly two possible homes (one per table), so a lookup is at most two probes, a hard bound that
//! chaining and linear probing cannot offer. Insert kicks the current occupant to ITS other home, possibly cascading; if the chain exceeds a bound
//! (a cycle, or too full) we rebuild with new hash seeds and a larger table. With two hash functions the table must stay under about 50% load
//! (we keep 40%), so it trades memory for predictable reads.
//! A cuckoo FILTER (Fan et al.) stores only a small FINGERPRINT per item in 4-slot buckets. Partial-key cuckoo hashing makes the second bucket
//! computable from the first bucket and the fingerprint alone: `i2 = i1 XOR hash(fp)`; XOR is an involution, so from either bucket we can find the
//! other without the original key. That lets us relocate fingerprints and, unlike Bloom filters, DELETE items (only items that were inserted!).
//! False positive rate is about 2*4/2^f (8 slots examined, each matching with probability 2^-f); a Bloom filter needs ~1.44*log2(1/p) bits/item.
//! Verified by a differential test of the table against std `HashMap`, and by property tests of the filter (no false negatives, FPR under a derived bound).
//!
//! COMMON USES: set-membership pre-checks that must support removal (cache admission, "have I seen this?" dedupe, avoiding disk lookups for absent
//! keys), and lookup-heavy tables where bounded worst-case probes matter (packet forwarding tables, in-memory indexes).
//! Read: https://www.cs.cmu.edu/~dga/papers/cuckoo-conext2014.pdf (Cuckoo Filter) and https://en.wikipedia.org/wiki/Cuckoo_hashing
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
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

// ---------------------------------------------------------------- cuckoo hash table

const MAX_KICKS: usize = 64;

struct CuckooMap<K, V> {
    tables: [Vec<Option<(K, V)>>; 2],
    cap: usize, // slots per table, power of two
    len: usize,
    seeds: [u64; 2],
    rng: Rng,
    rehashes: usize,
}

impl<K: Hash + Eq, V> CuckooMap<K, V> {
    fn new() -> Self {
        let mut m = CuckooMap {
            tables: [vec![], vec![]],
            cap: 0,
            len: 0,
            seeds: [0; 2],
            rng: Rng(0x1234_5678_9abc_def1),
            rehashes: 0,
        };
        m.rebuild(8, None);
        m.rehashes = 0;
        m
    }
    fn len(&self) -> usize {
        self.len
    }
    /// Home of `k` in table `t`: two independent positions come from two seeds mixed into one base hash.
    fn pos(&self, k: &K, t: usize) -> usize {
        let mut s = DefaultHasher::new();
        k.hash(&mut s);
        mix(s.finish() ^ self.seeds[t]) as usize & (self.cap - 1)
    }
    fn locate(&self, k: &K) -> Option<(usize, usize)> {
        (0..2)
            .map(|t| (t, self.pos(k, t)))
            .find(|&(t, p)| matches!(&self.tables[t][p], Some((kk, _)) if kk == k))
    }
    fn get(&self, k: &K) -> Option<&V> {
        let (t, p) = self.locate(k)?; // at most two probes, always
        self.tables[t][p].as_ref().map(|(_, v)| v)
    }
    fn remove(&mut self, k: &K) -> Option<V> {
        let (t, p) = self.locate(k)?;
        self.len -= 1;
        self.tables[t][p].take().map(|(_, v)| v)
    }
    fn insert(&mut self, k: K, v: V) -> Option<V> {
        if let Some((t, p)) = self.locate(&k) {
            return self.tables[t][p].replace((k, v)).map(|(_, old)| old);
        }
        if (self.len + 1) * 5 > self.cap * 4 {
            self.rebuild(self.cap * 2, None); // keep each table under 40% full (both together)
        }
        self.len += 1;
        if let Err(stuck) = self.try_place(k, v) {
            self.rebuild(self.cap, Some(stuck)); // eviction chain too long: probably a cycle -> new seeds
        }
        None
    }
    /// Kick-out chain: put `cur` in its table-0 home, evict whoever is there to its table-1 home, and so on.
    fn try_place(&mut self, k: K, v: V) -> Result<(), (K, V)> {
        let (mut cur, mut t) = ((k, v), 0);
        for _ in 0..MAX_KICKS {
            let p = self.pos(&cur.0, t);
            match self.tables[t][p].replace(cur) {
                None => return Ok(()),
                Some(evicted) => (cur, t) = (evicted, 1 - t),
            }
        }
        Err(cur)
    }
    fn drain_all(&mut self) -> Vec<(K, V)> {
        self.tables
            .iter_mut()
            .flat_map(|t| t.iter_mut().filter_map(Option::take))
            .collect()
    }
    /// Reinsert everything with fresh seeds (and possibly bigger tables); retry until a layout works.
    fn rebuild(&mut self, new_cap: usize, extra: Option<(K, V)>) {
        let mut items = self.drain_all();
        items.extend(extra);
        loop {
            self.rehashes += 1;
            self.cap = new_cap;
            self.seeds = [self.rng.next(), self.rng.next()];
            self.tables = [
                (0..new_cap).map(|_| None).collect(),
                (0..new_cap).map(|_| None).collect(),
            ];
            let mut pending = std::mem::take(&mut items);
            let mut failed = false;
            while let Some((k, v)) = pending.pop() {
                if let Err(stuck) = self.try_place(k, v) {
                    pending.push(stuck);
                    items = pending;
                    items.extend(self.drain_all());
                    failed = true;
                    break;
                }
            }
            if !failed {
                return;
            }
        }
    }
}

// ---------------------------------------------------------------- cuckoo filter

const SLOTS: usize = 4;

struct CuckooFilter {
    buckets: Vec<[u16; SLOTS]>, // 0 = empty, so fingerprints are forced nonzero
    mask: usize,
    fp_bits: u32,
    count: usize,
    rng: Rng,
}

impl CuckooFilter {
    fn new(buckets_pow2: usize, fp_bits: u32) -> Self {
        assert!(buckets_pow2.is_power_of_two() && (1..=16).contains(&fp_bits));
        CuckooFilter {
            buckets: vec![[0; SLOTS]; buckets_pow2],
            mask: buckets_pow2 - 1,
            fp_bits,
            count: 0,
            rng: Rng(42),
        }
    }
    fn capacity(&self) -> usize {
        self.buckets.len() * SLOTS
    }
    fn load(&self) -> f64 {
        self.count as f64 / self.capacity() as f64
    }
    fn index_and_fp(&self, item: u64) -> (usize, u16) {
        let h = mix(item);
        let fp = ((h >> 40) as u16 & ((1u32 << self.fp_bits) - 1) as u16).max(1);
        (h as usize & self.mask, fp)
    }
    /// The alternate bucket: XOR with a hash of the fingerprint. Applying it twice returns the original, so no key is needed to relocate.
    fn alt(&self, i: usize, fp: u16) -> usize {
        (i ^ mix(fp as u64 ^ 0xabcd_ef01) as usize) & self.mask
    }
    fn put_free(&mut self, i: usize, fp: u16) -> bool {
        match self.buckets[i].iter_mut().find(|s| **s == 0) {
            Some(s) => {
                *s = fp;
                true
            }
            None => false,
        }
    }
    fn insert(&mut self, item: u64) -> bool {
        let (i1, fp) = self.index_and_fp(item);
        let i2 = self.alt(i1, fp);
        if self.put_free(i1, fp) || self.put_free(i2, fp) {
            self.count += 1;
            return true;
        }
        let (mut i, mut fp, mut undo) =
            (if self.rng.next() & 1 == 0 { i1 } else { i2 }, fp, vec![]);
        for _ in 0..500 {
            let slot = (self.rng.next() % SLOTS as u64) as usize;
            undo.push((i, slot, fp));
            std::mem::swap(&mut self.buckets[i][slot], &mut fp); // evict a random resident
            i = self.alt(i, fp);
            if self.put_free(i, fp) {
                self.count += 1;
                return true;
            }
        }
        // Chain too long: roll the swaps back in reverse so no existing fingerprint is lost (that would create a false negative).
        for (i, slot, old) in undo.into_iter().rev() {
            self.buckets[i][slot] = old;
        }
        false
    }
    fn contains(&self, item: u64) -> bool {
        let (i1, fp) = self.index_and_fp(item);
        self.buckets[i1].contains(&fp) || self.buckets[self.alt(i1, fp)].contains(&fp)
    }
    /// Only call for items that were inserted; deleting a never-inserted item can remove another item's colliding fingerprint.
    fn delete(&mut self, item: u64) -> bool {
        let (i1, fp) = self.index_and_fp(item);
        for i in [i1, self.alt(i1, fp)] {
            if let Some(s) = self.buckets[i].iter_mut().find(|s| **s == fp) {
                *s = 0;
                self.count -= 1;
                return true;
            }
        }
        false
    }
}

fn table_differential() {
    let mut mine: CuckooMap<u64, u64> = CuckooMap::new();
    let mut oracle = HashMap::new();
    let mut rng = Rng(7);
    for _ in 0..20_000 {
        let (k, v) = (rng.next() % 4_000, rng.next());
        match rng.next() % 3 {
            0 => assert_eq!(mine.insert(k, v), oracle.insert(k, v)),
            1 => assert_eq!(mine.remove(&k), oracle.remove(&k)),
            _ => assert_eq!(mine.get(&k), oracle.get(&k)),
        }
        assert_eq!(mine.len(), oracle.len());
    }
    assert!((0..4_000).all(|k| mine.get(&k) == oracle.get(&k)));
    println!(
        "table: 20k random ops agree with HashMap ({} live keys, {} rehashes)",
        oracle.len(),
        mine.rehashes
    );

    let mut big: CuckooMap<u64, u64> = CuckooMap::new();
    (0..100_000u64).for_each(|k| assert!(big.insert(k, k * 2).is_none()));
    assert!((0..100_000u64).all(|k| big.get(&k) == Some(&(k * 2))));
    assert!(big.get(&100_000).is_none());
    println!(
        "table: 100k sequential keys, load {:.2} of 2x{} slots, {} rehashes/grows",
        big.len() as f64 / (2 * big.cap) as f64,
        big.cap,
        big.rehashes
    );
    assert!(big.len() as f64 / (2 * big.cap) as f64 <= 0.41);
}

/// Insert 0..n (as many as fit), then measure false positives on disjoint keys. Returns (load, fpr).
fn filter_run(fp_bits: u32) -> (f64, f64) {
    let mut f = CuckooFilter::new(1 << 14, fp_bits);
    let target = f.capacity() * 92 / 100;
    let inserted: Vec<u64> = (0..target as u64).filter(|&x| f.insert(x)).collect();
    assert!(
        inserted.len() as f64 > 0.9 * target as f64,
        "filter should reach 90%+ of the 92% target"
    );
    assert!(
        inserted.iter().all(|&x| f.contains(x)),
        "no false negatives"
    );
    let load = f.load();
    let probes = 400_000u64;
    let fps = (0..probes)
        .filter(|i| f.contains(1_000_000_000 + i))
        .count();
    let fpr = fps as f64 / probes as f64;
    // Derived bound: a probe inspects 2 buckets x 4 slots = 8 fingerprints, each equal with prob 2^-f => fpr <= 8 * 2^-f (we allow 2x slack + noise).
    let bound = 2.0 * (2 * SLOTS) as f64 / (1u64 << fp_bits) as f64 * load + 1e-4;
    assert!(fpr <= bound, "f={fp_bits}: fpr {fpr} > bound {bound}");
    // Delete the first half: the rest must still be present, and most deleted ones gone.
    let (gone, kept) = inserted.split_at(inserted.len() / 2);
    assert!(gone.iter().all(|&x| f.delete(x)));
    assert!(
        kept.iter().all(|&x| f.contains(x)),
        "delete must not cause false negatives"
    );
    let still = gone.iter().filter(|&&x| f.contains(x)).count() as f64 / gone.len() as f64;
    assert!(
        still < 0.5 + 8.0 / (1u64 << fp_bits) as f64 + 0.05 || fp_bits < 6,
        "deleted items mostly gone: {still}"
    );
    (load, fpr) // load measured before the deletes
}

fn main() {
    table_differential();

    println!(
        "\ncuckoo filter (4 slots/bucket, 92% target load) vs an optimally tuned Bloom filter at the SAME measured FPR"
    );
    println!(
        "{:>8}{:>10}{:>15}{:>18}{:>18}",
        "fp bits", "load", "measured FPR", "cuckoo bits/item", "bloom bits/item"
    );
    for f in [8u32, 12, 16] {
        let (load, fpr) = filter_run(f);
        let cuckoo_bits = f as f64 / load; // bits actually allocated per stored item
        let bloom_bits = (1.0 / fpr).log2() * std::f64::consts::LOG2_E; // m/n = ln(1/p)/ln(2)^2 = 1.44 log2(1/p)
        println!(
            "{f:>8}{load:>10.2}{fpr:>15.5}{cuckoo_bits:>18.1}{:>18.1}",
            bloom_bits
        );
        if f == 16 {
            assert!(
                cuckoo_bits < bloom_bits,
                "at low FPR the cuckoo filter is smaller than Bloom"
            );
        }
    }
    println!(
        "(cuckoo filters win below roughly 3% FPR and also support delete; Bloom filters need no delete support and have simpler code, see r24)"
    );
    println!("OK");
}
