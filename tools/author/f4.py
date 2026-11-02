from author import T, write_track


def sub(s, old, new):
    """str.replace that fails loudly when `old` isn't there (a wrong solution that silently equals the reference)."""
    assert old in s, f"not found: {old[:60]!r}"
    return s.replace(old, new)


P = []

# ---------------------------------------------------------------------------------------------------------------
BLOOM = """
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// A Bloom filter: "definitely not present" or "probably present", in a fixed number of bits.
pub struct Bloom {
    bits: Vec<u64>,
    m: usize, // number of bits
    k: u32,   // number of hash functions
    inserted: usize,
}

impl Bloom {
    /// Sized for `expected` items at false-positive rate `fp`:
    /// m = ceil(-n ln p / (ln 2)^2) bits, k = round(m / n * ln 2) hashes (at least 1).
    pub fn new(expected: usize, fp: f64) -> Self {
        let n = expected.max(1) as f64;
        let m = (-(n * fp.ln()) / std::f64::consts::LN_2.powi(2)).ceil().max(1.0) as usize;
        let k = ((m as f64 / n) * std::f64::consts::LN_2).round().max(1.0) as u32;
        Bloom { bits: vec![0; m.div_ceil(64)], m, k, inserted: 0 }
    }

    /// Two independent-enough 64-bit hashes from one `Hash` run; the k indices are `h1 + i * h2` (double hashing).
    fn hashes<T: Hash + ?Sized>(item: &T) -> (u64, u64) {
        let mut h = DefaultHasher::new();
        item.hash(&mut h);
        let a = h.finish();
        let mut z = a.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (a, z | 1) // odd, so the stride visits many distinct positions
    }

    fn index(&self, a: u64, b: u64, i: u32) -> usize {
        (a.wrapping_add((i as u64).wrapping_mul(b)) % self.m as u64) as usize
    }

    /// True if the item was not definitely present before (at least one bit changed).
    pub fn insert<T: Hash + ?Sized>(&mut self, item: &T) -> bool {
        let (a, b) = Self::hashes(item);
        let mut changed = false;
        for i in 0..self.k {
            let idx = self.index(a, b, i);
            let (w, bit) = (idx / 64, idx % 64);
            if self.bits[w] >> bit & 1 == 0 {
                changed = true;
                self.bits[w] |= 1 << bit;
            }
        }
        self.inserted += 1;
        changed
    }

    pub fn contains<T: Hash + ?Sized>(&self, item: &T) -> bool {
        let (a, b) = Self::hashes(item);
        (0..self.k).all(|i| {
            let idx = self.index(a, b, i);
            self.bits[idx / 64] >> (idx % 64) & 1 == 1
        })
    }

    pub fn bit_count(&self) -> usize {
        self.m
    }

    pub fn hash_count(&self) -> u32 {
        self.k
    }

    /// How many `insert` calls were made (duplicates count).
    pub fn len(&self) -> usize {
        self.inserted
    }

    pub fn is_empty(&self) -> bool {
        self.inserted == 0
    }

    /// Estimated current false-positive probability: (fraction of bits set)^k.
    pub fn estimated_fp_rate(&self) -> f64 {
        let ones: u32 = self.bits.iter().map(|w| w.count_ones()).sum();
        (ones as f64 / self.m as f64).powi(self.k as i32)
    }

    /// Merges another filter built with the same parameters; `Err(())` if the shapes differ.
    pub fn union_with(&mut self, other: &Bloom) -> Result<(), ()> {
        if self.m != other.m || self.k != other.k {
            return Err(());
        }
        for (a, b) in self.bits.iter_mut().zip(&other.bits) {
            *a |= b;
        }
        self.inserted += other.inserted;
        Ok(())
    }
}
"""

BLOOM_STARTER = """
use std::hash::Hash;

/// A Bloom filter: "definitely not present" or "probably present", in a fixed number of bits.
pub struct Bloom {
    // TODO: a bit array (Vec<u64>), its size in bits, the number of hash functions, an insert counter
}

impl Bloom {
    /// Sized for `expected` items at false-positive rate `fp`:
    /// m = ceil(-n ln p / (ln 2)^2) bits, k = round(m / n * ln 2) hashes (at least 1).
    pub fn new(expected: usize, fp: f64) -> Self {
        todo!()
    }

    /// True if the item was not definitely present before (at least one bit changed).
    /// Derive the k bit positions from two hashes by double hashing: position_i = (h1 + i * h2) mod m.
    pub fn insert<T: Hash + ?Sized>(&mut self, item: &T) -> bool {
        todo!()
    }

    pub fn contains<T: Hash + ?Sized>(&self, item: &T) -> bool {
        todo!()
    }

    pub fn bit_count(&self) -> usize {
        todo!()
    }

    pub fn hash_count(&self) -> u32 {
        todo!()
    }

    /// How many `insert` calls were made (duplicates count).
    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        todo!()
    }

    /// Estimated current false-positive probability: (fraction of bits set)^k.
    pub fn estimated_fp_rate(&self) -> f64 {
        todo!()
    }

    /// Merges another filter built with the same parameters; `Err(())` if the shapes differ.
    pub fn union_with(&mut self, other: &Bloom) -> Result<(), ()> {
        todo!()
    }
}
"""

P.append(dict(
    slug="bloom-filter", title="A Bloom filter sized from a target false-positive rate", level="medium", stage="sketches-and-rings",
    tags=["Bloom filter", "probabilistic", "double hashing", "bit array", "false positives", "LSM", "union"],
    teaches=["A set that answers 'definitely not' or 'probably yes' in about 10 bits per item at 1% error: why LSM trees and caches put one in front of every disk read.",
             "Sizing from first principles: `m = -n ln p / (ln 2)^2` bits and `k = (m / n) ln 2` hashes, and checking the result against measured error.",
             "Double hashing: k probe positions from two hashes, `h1 + i * h2`, instead of k independent hash computations."],
    statement="""
        Implement `Bloom::new(expected, fp)` sized for `expected` items at false-positive rate `fp`:
        `m = ceil(-n ln p / (ln 2)^2)` bits and `k = round(m / n * ln 2)` hash functions (at least 1). For 1000 items at 1% that is
        9586 bits and 7 hashes.

        `insert` sets the k bits chosen by **double hashing** (`position_i = (h1 + i * h2) mod m`, with `h2` forced odd) and returns whether any bit
        changed; `contains` is true only if all k bits are set: **no false negatives, ever**. Also provide the counters, the load-based
        `estimated_fp_rate` (`(set bits / m)^k`) and `union_with` for two filters of identical shape (`Err(())` otherwise).

        Generic over `T: Hash + ?Sized`, so both `&str` and `String` keys work (`insert("a")`, `contains(&String::from("a"))`).
    """,
    starter=BLOOM_STARTER,
    solution=BLOOM,
    visible=[
        T("sizing_for_1000_at_one_percent", "m and k", "{ let b = Bloom::new(1000, 0.01); (b.bit_count(), b.hash_count()) }", "(9586, 7)"),
        T("sizing_for_100_at_a_tenth", "smaller", "{ let b = Bloom::new(100, 0.1); (b.bit_count(), b.hash_count()) }", "(480, 3)"),
        T("inserted_items_are_found", "no false negatives on a few", "{ let mut b = Bloom::new(100, 0.01); for w in [\"a\", \"b\", \"c\"] { b.insert(w); } (b.contains(\"a\"), b.contains(\"b\"), b.contains(\"c\"), b.len()) }", "(true, true, true, 3)"),
        T("empty_filter_contains_nothing", "fresh", "{ let b = Bloom::new(100, 0.01); (b.contains(\"x\"), b.contains(&42u32), b.is_empty(), b.estimated_fp_rate()) }", "(false, false, true, 0.0)"),
        T("insert_reports_novelty", "second insert changes no bit", "{ let mut b = Bloom::new(100, 0.01); let first = b.insert(\"dup\"); let again = b.insert(\"dup\"); (first, again, b.len()) }", "(true, false, 2)"),
    ],
    hidden=[
        T("sizing_is_ceil_and_round", "n = 1, tiny", "{ let b = Bloom::new(1, 0.5); (b.bit_count(), b.hash_count()) }", "(2, 1)"),
        T("zero_expected_does_not_panic", "n = 0 is treated as 1", "{ let mut b = Bloom::new(0, 0.01); b.insert(&1u8); b.contains(&1u8) }", "true"),
        T("str_and_string_agree", "same hash for both", "{ let mut b = Bloom::new(100, 0.01); b.insert(\"key\"); b.contains(&String::from(\"key\")) }", "true"),
        T("union_combines", "membership of both", "{ let mut a = Bloom::new(100, 0.01); let mut b = Bloom::new(100, 0.01); a.insert(\"x\"); b.insert(\"y\"); let r = a.union_with(&b); (r, a.contains(\"x\"), a.contains(\"y\"), a.len()) }", "(Ok(()), true, true, 2)"),
        T("union_needs_the_same_shape", "different sizes", "{ let mut a = Bloom::new(100, 0.01); let b = Bloom::new(500, 0.01); a.union_with(&b) }", "Err(())"),
        T("fp_estimate_grows_with_load", "more items, more error", "{ let mut b = Bloom::new(1000, 0.01); for i in 0..100u32 { b.insert(&i); } let lo = b.estimated_fp_rate(); for i in 100..1000u32 { b.insert(&i); } (lo < b.estimated_fp_rate(), b.estimated_fp_rate() < 0.02) }", "(true, true)"),
        """
        #[test]
        fn no_false_negatives_ever() {
            let mut b = Bloom::new(5000, 0.01);
            for i in 0..5000u32 {
                b.insert(&format!("member-{i}"));
            }
            let missing = (0..5000u32).filter(|i| !b.contains(&format!("member-{i}"))).count();
            check!("every inserted key is reported present", missing, 0);
        }

        #[test]
        fn measured_false_positive_rate_is_near_the_target() {
            let mut b = Bloom::new(1000, 0.01);
            for i in 0..1000u32 {
                b.insert(&format!("in-{i}"));
            }
            let trials = 50_000u32;
            let fps = (0..trials).filter(|i| b.contains(&format!("out-{i}"))).count();
            let rate = fps as f64 / trials as f64;
            assert!(rate < 0.02, "false positive rate {rate} is more than twice the 1% target");
            assert!(rate > 0.002, "false positive rate {rate} is suspiciously low: is contains checking all k bits?");
        }

        #[test]
        fn union_keeps_every_member_of_both() {
            let mut a = Bloom::new(2000, 0.01);
            let mut b = Bloom::new(2000, 0.01);
            for i in 0..1000u32 {
                a.insert(&i);
                b.insert(&(i + 1000));
            }
            a.union_with(&b).unwrap();
            let missing = (0..2000u32).filter(|i| !a.contains(i)).count();
            check!("the union contains all 2000 keys", missing, 0);
        }
        """,
    ],
    wrong=dict(
        formula_uses_ln_not_ln_squared=sub(BLOOM, "std::f64::consts::LN_2.powi(2)).ceil()", "std::f64::consts::LN_2).ceil()"),
        single_hash=sub(BLOOM, "let k = ((m as f64 / n) * std::f64::consts::LN_2).round().max(1.0) as u32;", "let k = 1u32;"),
        insert_skips_the_last_probe=sub(BLOOM, "        let mut changed = false;\n        for i in 0..self.k {", "        let mut changed = false;\n        for i in 0..self.k.saturating_sub(1) {"),
        contains_any_bit=sub(BLOOM, "(0..self.k).all(|i| {", "(0..self.k).any(|i| {"),
        union_intersects=sub(BLOOM, "*a |= b;", "*a &= b;"),
    ),
    hints=[("approach", "A `Vec<u64>` of `m` bits. Hash the item once, derive `h1` and an odd `h2`, and probe bit `(h1 + i * h2) % m` for `i in 0..k`. `insert` sets them all, `contains` requires all of them."),
           ("rust", "`std::hash::DefaultHasher::new()` is deterministic. Take `T: Hash + ?Sized` so `&str` works; `\"a\".hash(&mut hasher)` and `String::from(\"a\").hash(&mut hasher)` produce the same value. Get a second hash by running the first through a mixer (splitmix64) and OR-ing 1."),
           ("edge", "Use `wrapping_add` / `wrapping_mul` for the probe arithmetic: `i * h2` overflows `u64`. Round `m` up with `ceil` and `k` with `round`, then clamp both to at least 1.")],
    notes=("RocksDB/LevelDB keep a Bloom filter per SSTable so a lookup for an absent key skips the disk read; Cassandra, Bigtable, Chrome's old Safe Browsing list and Bitcoin SPV filters all use one. 10 bits per key and 7 hashes give about 1% false positives, whatever the key size. The trade-offs to know: no deletes (use a counting or cuckoo filter), and the filter must be sized for the final item count or the error rises.", "O(k) per operation", "O(m) bits"),
    follow_up="1) Prove no false negatives and derive the optimal `k` yourself (minimise `(1 - e^(-kn/m))^k`). 2) Build a counting Bloom filter with 4-bit counters to support `remove`: what breaks on counter overflow? 3) Make it a blocked Bloom filter (all k bits inside one 512-bit cache line) and measure the speed-up with the F1 benchmarking track. 4) Compare with a cuckoo filter or xor filter: space at 1% error? 5) Where would you put a Bloom filter in the kvlite LSM project, and what happens to read amplification without it?",
    related=["F4", "F5", "S4"],
))

# ---------------------------------------------------------------------------------------------------------------
CMS = """
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// A count-min sketch: approximate frequencies in fixed memory. `estimate` never undercounts, and overcounts by at
/// most `eps * total` with probability at least `1 - delta` when built with `with_error(eps, delta)`.
pub struct CountMin {
    width: usize,
    depth: usize,
    table: Vec<u64>, // depth rows of width counters
    total: u64,
}

impl CountMin {
    pub fn new(width: usize, depth: usize) -> Self {
        let (width, depth) = (width.max(1), depth.max(1));
        CountMin { width, depth, table: vec![0; width * depth], total: 0 }
    }

    /// width = ceil(e / eps), depth = ceil(ln(1 / delta)).
    pub fn with_error(eps: f64, delta: f64) -> Self {
        CountMin::new((std::f64::consts::E / eps).ceil() as usize, (1.0 / delta).ln().ceil() as usize)
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn depth(&self) -> usize {
        self.depth
    }

    /// Column of `item` in `row`: each row hashes with its own seed, so collisions in one row are independent of the others.
    fn column<T: Hash + ?Sized>(&self, row: usize, item: &T) -> usize {
        let mut h = DefaultHasher::new();
        (row as u64).hash(&mut h);
        item.hash(&mut h);
        (h.finish() % self.width as u64) as usize
    }

    pub fn add<T: Hash + ?Sized>(&mut self, item: &T, count: u64) {
        for row in 0..self.depth {
            let col = self.column(row, item);
            let cell = &mut self.table[row * self.width + col];
            *cell = cell.saturating_add(count);
        }
        self.total = self.total.saturating_add(count);
    }

    /// The smallest counter among the item's cells: every counter is an overestimate, the least-collided one is closest.
    pub fn estimate<T: Hash + ?Sized>(&self, item: &T) -> u64 {
        (0..self.depth).map(|row| self.table[row * self.width + self.column(row, item)]).min().unwrap()
    }

    /// The sum of everything added.
    pub fn total(&self) -> u64 {
        self.total
    }

    /// Adds another sketch of the same shape (sketches of two streams merge into the sketch of their concatenation).
    pub fn merge(&mut self, other: &CountMin) -> Result<(), ()> {
        if self.width != other.width || self.depth != other.depth {
            return Err(());
        }
        for (a, b) in self.table.iter_mut().zip(&other.table) {
            *a = a.saturating_add(*b);
        }
        self.total = self.total.saturating_add(other.total);
        Ok(())
    }
}
"""

CMS_STARTER = """
use std::hash::Hash;

/// A count-min sketch: approximate frequencies in fixed memory. `estimate` never undercounts, and overcounts by at
/// most `eps * total` with probability at least `1 - delta` when built with `with_error(eps, delta)`.
pub struct CountMin {
    // TODO: depth rows of width counters, and the running total
}

impl CountMin {
    pub fn new(width: usize, depth: usize) -> Self {
        todo!()
    }

    /// width = ceil(e / eps), depth = ceil(ln(1 / delta)).
    pub fn with_error(eps: f64, delta: f64) -> Self {
        todo!()
    }

    pub fn width(&self) -> usize {
        todo!()
    }

    pub fn depth(&self) -> usize {
        todo!()
    }

    /// Adds `count` occurrences of `item`: one counter per row, each row choosing its column with a differently seeded hash.
    pub fn add<T: Hash + ?Sized>(&mut self, item: &T, count: u64) {
        todo!()
    }

    /// Never less than the true count.
    pub fn estimate<T: Hash + ?Sized>(&self, item: &T) -> u64 {
        todo!()
    }

    /// The sum of everything added.
    pub fn total(&self) -> u64 {
        todo!()
    }

    /// Adds another sketch of the same shape; `Err(())` otherwise.
    pub fn merge(&mut self, other: &CountMin) -> Result<(), ()> {
        todo!()
    }
}
"""

P.append(dict(
    slug="count-min-sketch", title="A count-min sketch for heavy hitters", level="medium", stage="sketches-and-rings",
    tags=["count-min sketch", "streaming", "heavy hitters", "seeded hashing", "mergeable", "approximate counting"],
    teaches=["Counting millions of distinct keys in a few KB: each key owns one counter per row, and the minimum over rows is the least-collided estimate.",
             "The guarantee in one line: never an undercount, and an overcount of at most `eps * total` with probability `1 - delta`; width `e / eps`, depth `ln(1 / delta)`.",
             "Why rows must hash independently (different seeds), and why the sketch is mergeable (sum the tables): the basis of distributed telemetry."],
    statement="""
        Implement `CountMin`: `depth` rows of `width` counters. `add(item, count)` adds `count` to one counter in every row (the column
        is `hash(row_seed, item) mod width`, so each row collides differently). `estimate(item)` is the **minimum** of the item's counters.

        `with_error(eps, delta)` picks `width = ceil(e / eps)` and `depth = ceil(ln(1 / delta))`. `total()` is the sum of all adds.
        `merge` adds another sketch of the same shape (`Err(())` otherwise). Counters saturate instead of overflowing.

        Guarantees the tests check: an estimate is **never below** the true count, and for a skewed stream with a few heavy keys, at least 99% of light
        keys are within `eps * total` of the truth.
    """,
    starter=CMS_STARTER,
    solution=CMS,
    visible=[
        T("sizing", "eps 0.01, delta 0.01", "{ let c = CountMin::with_error(0.01, 0.01); (c.width(), c.depth()) }", "(272, 5)"),
        T("exact_when_there_is_room", "few keys, wide sketch", "{ let mut c = CountMin::new(1000, 4); c.add(\"a\", 3); c.add(\"b\", 5); c.add(\"a\", 2); (c.estimate(\"a\"), c.estimate(\"b\"), c.estimate(\"never\"), c.total()) }", "(5, 5, 0, 10)"),
        T("never_undercounts_in_a_tiny_sketch", "width 3 forces collisions", "{ let mut c = CountMin::new(3, 2); for i in 0..50u32 { c.add(&i, 1 + (i % 3) as u64); } (0..50u32).all(|i| c.estimate(&i) >= 1 + (i % 3) as u64) }", "true"),
        T("merge_adds", "two streams", "{ let mut a = CountMin::new(200, 4); let mut b = CountMin::new(200, 4); a.add(\"x\", 4); b.add(\"x\", 6); b.add(\"y\", 1); a.merge(&b).unwrap(); (a.estimate(\"x\"), a.estimate(\"y\"), a.total()) }", "(10, 1, 11)"),
        T("merge_needs_equal_shape", "different width", "{ let mut a = CountMin::new(10, 3); let b = CountMin::new(11, 3); a.merge(&b) }", "Err(())"),
    ],
    hidden=[
        T("fresh_sketch_is_empty", "nothing seen", "{ let c = CountMin::new(50, 3); (c.estimate(\"x\"), c.total()) }", "(0, 0)"),
        T("zero_count_adds_nothing", "add(_, 0)", "{ let mut c = CountMin::new(50, 3); c.add(\"a\", 0); (c.estimate(\"a\"), c.total()) }", "(0, 0)"),
        T("saturates_instead_of_overflowing", "u64::MAX twice", "{ let mut c = CountMin::new(10, 2); c.add(\"a\", u64::MAX); c.add(\"a\", 5); (c.estimate(\"a\"), c.total()) }", "(u64::MAX, u64::MAX)"),
        T("str_and_string_agree", "same hashing", "{ let mut c = CountMin::new(100, 3); c.add(\"k\", 2); c.add(&String::from(\"k\"), 3); c.estimate(\"k\") }", "5"),
        T("degenerate_shapes_are_clamped", "zero width and depth", "{ let mut c = CountMin::new(0, 0); c.add(\"a\", 1); (c.width(), c.depth(), c.estimate(\"a\")) }", "(1, 1, 1)"),
        T("count_is_added_not_one", "weighted adds", "{ let mut c = CountMin::new(500, 4); c.add(&7u32, 1000); c.estimate(&7u32) }", "1000"),
        """
        #[test]
        fn heavy_hitters_do_not_spoil_light_keys() {
            // 20 heavy keys (3000 each) and 20,000 light keys (2 each): total 100,000, so eps * total = 1000.
            let mut c = CountMin::with_error(0.01, 0.01);
            for h in 0..20u32 {
                c.add(&format!("heavy-{h}"), 3000);
            }
            for l in 0..20_000u32 {
                c.add(&format!("light-{l}"), 2);
            }
            let total = c.total();
            check!("total", total, 100_000);
            for h in 0..20u32 {
                assert!(c.estimate(&format!("heavy-{h}")) >= 3000, "undercounted a heavy key");
            }
            let bound = (0.01 * total as f64) as u64;
            let within = (0..20_000u32).filter(|l| c.estimate(&format!("light-{l}")) <= 2 + bound).count();
            let frac = within as f64 / 20_000.0;
            assert!(frac >= 0.99, "only {frac} of light keys were within eps * total of the truth: rows are not independent, or the estimate is not the minimum");
            let top = (0..20u32).map(|h| c.estimate(&format!("heavy-{h}"))).min().unwrap();
            let loudest_light = (0..20_000u32).map(|l| c.estimate(&format!("light-{l}"))).filter(|&e| e < 3000).max().unwrap();
            assert!(top >= 3000 && loudest_light < top + 1, "heavy hitters should stand out");
        }

        #[test]
        fn random_stream_never_undercounts() {
            let mut rng = anneal_prelude::Rng::new(6901);
            let mut c = CountMin::new(64, 4);
            let mut truth = std::collections::HashMap::new();
            for _ in 0..30_000 {
                let key = rng.below(500) as u32;
                let n = 1 + rng.below(5) as u64;
                c.add(&key, n);
                *truth.entry(key).or_insert(0u64) += n;
            }
            for (k, v) in &truth {
                assert!(c.estimate(k) >= *v, "key {k}: estimate {} below the true {v}", c.estimate(k));
            }
            check!("total matches", c.total(), truth.values().sum::<u64>());
        }
        """,
    ],
    wrong=dict(
        takes_the_max=sub(CMS, ".min().unwrap()", ".max().unwrap()"),
        every_row_uses_the_same_hash=sub(CMS, "        (row as u64).hash(&mut h);\n", "        let _ = row;\n"),
        ignores_the_count=sub(CMS, "*cell = cell.saturating_add(count);", "*cell = cell.saturating_add(count.min(1));"),
        merge_replaces_instead_of_adding=sub(CMS, "*a = a.saturating_add(*b);", "*a = *b;"),
    ),
    hints=[("approach", "A flat `Vec<u64>` of `depth * width` counters. For row `r`, the column is `hash(r, item) % width`. `add` bumps one cell per row; `estimate` takes the minimum over the item's cells."),
           ("rust", "Seed each row by hashing the row number into the `DefaultHasher` first, then the item. Use `saturating_add` so a huge count cannot wrap. `with_error`: `(E / eps).ceil()` and `(1.0 / delta).ln().ceil()`."),
           ("edge", "The minimum is right because every counter only ever over-counts (collisions add, nothing subtracts). If all rows used the same column function, the min would equal a single row and the `1 - delta` guarantee would be gone.")],
    notes=("Count-min sketches power top-K and heavy-hitter detection in network monitoring, ad systems and databases (Redis's `CMS.*` commands, Apache DataSketches, Twitter's Algebird). Because merge is element-wise addition, each node keeps a local sketch and a coordinator sums them: the same shape as a CRDT counter. They are the right tool when exact counting needs memory proportional to the number of distinct keys.", "O(depth) per operation", "O(width * depth)"),
    follow_up="1) Add conservative update (only raise the cells that equal the current minimum) and measure the error reduction. 2) Pair the sketch with a small heap to maintain the approximate top-K as the stream goes by. 3) Why can a count-min sketch not support deletions with the minimum rule, and what is the count-mean-min variant for? 4) Derive the width from Markov's inequality: where does `e / eps` come from? 5) Implement HyperLogLog next (distinct counting) and compare what each sketch can answer.",
    related=["F4", "F5", "S4"],
))

# ---------------------------------------------------------------------------------------------------------------
RING = """
use std::collections::{BTreeMap, BTreeSet};

/// FNV-1a over the bytes, then a mixing finaliser so nearby inputs land far apart on the ring.
pub fn hash64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
    h ^= h >> 33;
    h = h.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    h ^ (h >> 33)
}

/// A consistent-hash ring with virtual nodes: each node owns `vnodes` points; a key belongs to the first point at or after its hash.
pub struct Ring {
    vnodes: usize,
    points: BTreeMap<u64, String>,
    nodes: BTreeSet<String>,
}

impl Ring {
    pub fn new(vnodes: usize) -> Self {
        Ring { vnodes: vnodes.max(1), points: BTreeMap::new(), nodes: BTreeSet::new() }
    }

    /// Adds a node (no effect if already present).
    pub fn add_node(&mut self, name: &str) {
        if !self.nodes.insert(name.to_string()) {
            return;
        }
        for i in 0..self.vnodes {
            self.points.insert(hash64(format!("{name}#{i}").as_bytes()), name.to_string());
        }
    }

    /// Removes a node and all its points; false if it was not there.
    pub fn remove_node(&mut self, name: &str) -> bool {
        if !self.nodes.remove(name) {
            return false;
        }
        self.points.retain(|_, n| n != name);
        true
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// The node responsible for `key`: the owner of the first point clockwise from the key's hash (wrapping past the top).
    pub fn get(&self, key: &str) -> Option<&str> {
        let h = hash64(key.as_bytes());
        self.points.range(h..).next().or_else(|| self.points.iter().next()).map(|(_, n)| n.as_str())
    }

    /// Up to `n` distinct nodes walking clockwise from the key: the primary first, then its replicas.
    pub fn get_n(&self, key: &str, n: usize) -> Vec<&str> {
        let h = hash64(key.as_bytes());
        let mut out: Vec<&str> = Vec::new();
        for (_, name) in self.points.range(h..).chain(self.points.range(..h)) {
            if out.len() == n {
                break;
            }
            if !out.contains(&name.as_str()) {
                out.push(name);
            }
        }
        out
    }
}
"""

RING_STARTER = """
/// FNV-1a over the bytes, then a mixing finaliser so nearby inputs land far apart on the ring. (Given: use it for keys and for points.)
pub fn hash64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
    h ^= h >> 33;
    h = h.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    h ^ (h >> 33)
}

/// A consistent-hash ring with virtual nodes: each node owns `vnodes` points (hash of `"{name}#{i}"`);
/// a key belongs to the first point at or after its hash, wrapping around past the top.
pub struct Ring {
    // TODO: an ordered map from point to node name, and the set of nodes
}

impl Ring {
    pub fn new(vnodes: usize) -> Self {
        todo!()
    }

    /// Adds a node (no effect if already present).
    pub fn add_node(&mut self, name: &str) {
        todo!()
    }

    /// Removes a node and all its points; false if it was not there.
    pub fn remove_node(&mut self, name: &str) -> bool {
        todo!()
    }

    pub fn node_count(&self) -> usize {
        todo!()
    }

    /// The node responsible for `key`.
    pub fn get(&self, key: &str) -> Option<&str> {
        todo!()
    }

    /// Up to `n` distinct nodes walking clockwise from the key: the primary first, then its replicas.
    pub fn get_n(&self, key: &str, n: usize) -> Vec<&str> {
        todo!()
    }
}
"""

P.append(dict(
    slug="consistent-hash-ring", title="A consistent-hash ring with virtual nodes and replicas", level="medium", stage="sketches-and-rings",
    tags=["consistent hashing", "BTreeMap range", "virtual nodes", "sharding", "replication", "Dynamo", "load balancing"],
    teaches=["Why `hash(key) % n` is a disaster for a cache or shard map (adding a node moves almost every key) and how a ring moves only about `1/(n+1)` of them.",
             "Virtual nodes as the fix for lumpy load: many points per node average out the arc lengths.",
             "`BTreeMap::range` for 'first point at or after this hash, wrapping around' and the replica walk used by Dynamo, Cassandra and memcached clients."],
    statement="""
        Implement a consistent-hash ring. Each node owns `vnodes` points on a 64-bit circle: the point for virtual node `i` of node `name` is
        `hash64("{name}#{i}")` (`hash64` is provided). A key belongs to the node owning the **first point at or after `hash64(key)`**, wrapping from the top
        of the circle to the lowest point.

        `add_node` / `remove_node` change membership; `get(key)` returns the owner (`None` for an empty ring); `get_n(key, n)` returns up to `n`
        **distinct** nodes walking clockwise from the key: the primary and then its replicas.

        Properties the tests check: adding a node moves only keys that now belong to the new node, about `1/(n+1)` of them; removing a node moves only that node's keys;
        with 200 virtual nodes the load across 10 nodes stays within a factor of about 1.5 of fair.
    """,
    starter=RING_STARTER,
    solution=RING,
    visible=[
        T("empty_ring", "no nodes", "{ let r = Ring::new(10); let n = r.get_n(\"k\", 3).len(); (r.get(\"k\").is_none(), n, r.node_count()) }", "(true, 0, 0)"),
        T("single_node_owns_everything", "one node", "{ let mut r = Ring::new(10); r.add_node(\"a\"); (0..20).all(|i| r.get(&format!(\"k{i}\")) == Some(\"a\")) }", "true"),
        T("lookups_are_deterministic", "same ring, same answer", "{ let mut r = Ring::new(50); for n in [\"a\", \"b\", \"c\"] { r.add_node(n); } let x: Vec<_> = (0..30).map(|i| r.get(&format!(\"k{i}\"))).collect(); let y: Vec<_> = (0..30).map(|i| r.get(&format!(\"k{i}\"))).collect(); x == y }", "true"),
        T("remove_node", "its keys go elsewhere", "{ let mut r = Ring::new(50); for n in [\"a\", \"b\"] { r.add_node(n); } let removed = r.remove_node(\"a\"); let again = r.remove_node(\"a\"); (removed, again, r.node_count(), (0..50).all(|i| r.get(&format!(\"k{i}\")) == Some(\"b\"))) }", "(true, false, 1, true)"),
        T("get_n_gives_distinct_nodes", "three replicas of three nodes", "{ let mut r = Ring::new(20); for n in [\"a\", \"b\", \"c\"] { r.add_node(n); } let mut v: Vec<String> = r.get_n(\"some-key\", 3).into_iter().map(String::from).collect(); let first = v[0].clone(); v.sort(); let same = first == r.get(\"some-key\").unwrap(); (v, same) }", "(vec![\"a\".to_string(), \"b\".to_string(), \"c\".to_string()], true)"),
    ],
    hidden=[
        T("get_n_is_capped_by_membership", "ask for more than exist", "{ let mut r = Ring::new(20); r.add_node(\"a\"); r.add_node(\"b\"); (r.get_n(\"k\", 5).len(), r.get_n(\"k\", 0).len()) }", "(2, 0)"),
        T("adding_twice_is_a_no_op", "idempotent", "{ let mut r = Ring::new(20); r.add_node(\"a\"); let before: Vec<_> = (0..20).map(|i| r.get(&format!(\"k{i}\")).map(String::from)).collect(); r.add_node(\"a\"); let after: Vec<_> = (0..20).map(|i| r.get(&format!(\"k{i}\")).map(String::from)).collect(); (before == after, r.node_count()) }", "(true, 1)"),
        T("wraps_around_the_top", "every key has an owner even past the last point", "{ let mut r = Ring::new(1); r.add_node(\"only\"); (0..2000).all(|i| r.get(&format!(\"wrap-{i}\")).is_some()) }", "true"),
        T("removing_an_unknown_node", "no change", "{ let mut r = Ring::new(10); r.add_node(\"a\"); (r.remove_node(\"zzz\"), r.node_count()) }", "(false, 1)"),
        """
        fn assignment(r: &Ring, keys: usize) -> Vec<String> {
            (0..keys).map(|i| r.get(&format!("key-{i}")).unwrap().to_string()).collect()
        }

        #[test]
        fn load_is_balanced_with_virtual_nodes() {
            let mut r = Ring::new(200);
            for i in 0..10 {
                r.add_node(&format!("node-{i}"));
            }
            let keys = 20_000;
            let mut counts = std::collections::HashMap::new();
            for owner in assignment(&r, keys) {
                *counts.entry(owner).or_insert(0usize) += 1;
            }
            check!("all ten nodes own keys", counts.len(), 10);
            let fair = keys / 10;
            for (n, c) in &counts {
                assert!(*c > fair * 6 / 10 && *c < fair * 14 / 10, "{n} owns {c} keys; fair share is {fair}");
            }
        }

        #[test]
        fn adding_a_node_moves_only_a_fair_share_to_the_new_node() {
            let mut r = Ring::new(200);
            for i in 0..10 {
                r.add_node(&format!("node-{i}"));
            }
            let keys = 20_000;
            let before = assignment(&r, keys);
            r.add_node("node-new");
            let after = assignment(&r, keys);
            let mut moved = 0;
            for (b, a) in before.iter().zip(&after) {
                if b != a {
                    moved += 1;
                    assert_eq!(a, "node-new", "a key moved between two old nodes");
                }
            }
            let fair = keys / 11;
            assert!(moved > fair / 2 && moved < fair * 3 / 2, "{moved} keys moved; about {fair} expected");
        }

        #[test]
        fn removing_a_node_moves_only_its_own_keys() {
            let mut r = Ring::new(200);
            for i in 0..10 {
                r.add_node(&format!("node-{i}"));
            }
            let before = assignment(&r, 20_000);
            r.remove_node("node-3");
            let after = assignment(&r, 20_000);
            for (b, a) in before.iter().zip(&after) {
                if b != "node-3" {
                    assert_eq!(b, a, "a key that did not belong to the removed node moved");
                } else {
                    assert_ne!(a, "node-3");
                }
            }
        }

        #[test]
        fn replicas_are_distinct_and_start_with_the_primary() {
            let mut r = Ring::new(100);
            for i in 0..6 {
                r.add_node(&format!("n{i}"));
            }
            for i in 0..500 {
                let key = format!("key-{i}");
                let reps = r.get_n(&key, 3);
                assert_eq!(reps.len(), 3);
                assert_eq!(reps[0], r.get(&key).unwrap());
                let mut s = reps.clone();
                s.sort();
                s.dedup();
                assert_eq!(s.len(), 3, "duplicate replicas {reps:?}");
            }
        }
        """,
    ],
    wrong=dict(
        one_point_per_node=sub(RING, "for i in 0..self.vnodes {", "for i in 0..1 {"),
        removal_leaves_the_points=sub(RING, "        self.points.retain(|_, n| n != name);\n", ""),
        modulo_placement=sub(RING, "        let h = hash64(key.as_bytes());\n        self.points.range(h..).next().or_else(|| self.points.iter().next()).map(|(_, n)| n.as_str())", "        let names: Vec<&String> = self.nodes.iter().collect();\n        if names.is_empty() {\n            return None;\n        }\n        Some(names[(hash64(key.as_bytes()) % names.len() as u64) as usize].as_str())"),
        no_wraparound=sub(RING, "self.points.range(h..).next().or_else(|| self.points.iter().next()).map(|(_, n)| n.as_str())", "self.points.range(h..).next().map(|(_, n)| n.as_str())"),
        replicas_may_repeat=sub(RING, "if !out.contains(&name.as_str()) {\n                out.push(name);\n            }", "out.push(name);"),
    ),
    hints=[("approach", "Keep `points: BTreeMap<u64, String>` (point to node name) and the set of node names. `add_node` inserts `vnodes` points `hash64(\"{name}#{i}\")`. `get` is `points.range(h..).next()`, falling back to the first entry when nothing is at or above `h`."),
           ("rust", "`BTreeMap::range(h..)` then `.chain(range(..h))` walks the circle once, starting just after the key. For replicas collect names while skipping ones you already have, until you have `n` or run out. `retain(|_, node| node != name)` removes a node's points."),
           ("edge", "Reject a second `add_node` of the same name (it would insert nothing new but must not double-count). The empty ring has no owner. With one point per node the arcs are wildly uneven: that is what the virtual nodes are for.")],
    notes=("Consistent hashing is how Dynamo, Cassandra, Riak, memcached clients (ketama) and many load balancers (Envoy's ring-hash, HAProxy `hash-type consistent`) place keys. Rendezvous (highest-random-weight) hashing and jump hash solve the same problem with different trade-offs: no ring to store, but O(n) per lookup or no arbitrary node removal.", "O(log P) per lookup, O(vnodes log P) per membership change", "O(nodes * vnodes)"),
    follow_up="1) Implement jump consistent hash and rendezvous hashing, and compare balance, memory and lookup cost to the ring. 2) Add per-node weights (more or fewer vnodes) and test that load follows the weights. 3) Bounded-load consistent hashing: cap each node at `(1 + eps) * average` and spill to the next node; when does that matter (hot keys)? 4) Use `get_n` for replication in a toy Dynamo: what do sloppy quorums and hinted handoff add? 5) Why do vnode points from different nodes colliding on the same hash matter, and what does your ring do?",
    related=["F4", "S4", "B4"],
))

# ---------------------------------------------------------------------------------------------------------------
OAMAP = """
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hash};

enum Slot<K, V> {
    Empty,
    Tombstone, // a removed entry: probe chains must continue past it
    Full(K, V),
}

fn empty_slots<K, V>(n: usize) -> Vec<Slot<K, V>> {
    (0..n).map(|_| Slot::Empty).collect()
}

/// An open-addressing hash map: one flat array, linear probing, tombstones for removal.
/// The capacity is a power of two (starting at 8); the table is kept under 7/8 full counting tombstones.
pub struct OaMap<K, V, S = RandomState> {
    slots: Vec<Slot<K, V>>,
    len: usize,
    tombs: usize,
    hasher: S,
}

impl<K: Hash + Eq, V> OaMap<K, V, RandomState> {
    pub fn new() -> Self {
        OaMap::with_hasher(RandomState::new())
    }
}

impl<K: Hash + Eq, V, S: BuildHasher> OaMap<K, V, S> {
    pub fn with_hasher(hasher: S) -> Self {
        OaMap { slots: empty_slots(8), len: 0, tombs: 0, hasher }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Number of slots (always a power of two).
    pub fn capacity(&self) -> usize {
        self.slots.len()
    }

    fn start(&self, key: &K) -> usize {
        (self.hasher.hash_one(key) as usize) & (self.slots.len() - 1)
    }

    fn find(&self, key: &K) -> Option<usize> {
        let mask = self.slots.len() - 1;
        let mut i = self.start(key);
        loop {
            match &self.slots[i] {
                Slot::Empty => return None, // an empty slot ends every probe chain
                Slot::Full(k, _) if k == key => return Some(i),
                _ => {}
            }
            i = (i + 1) & mask;
        }
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        match self.find(key) {
            Some(i) => match &self.slots[i] {
                Slot::Full(_, v) => Some(v),
                _ => unreachable!(),
            },
            None => None,
        }
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        match self.find(key) {
            Some(i) => match &mut self.slots[i] {
                Slot::Full(_, v) => Some(v),
                _ => unreachable!(),
            },
            None => None,
        }
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.find(key).is_some()
    }

    /// Rebuilds the table: doubles it if it is more than half full of live entries, otherwise rebuilds at the same size
    /// (which is what clears the tombstones).
    fn rehash(&mut self) {
        let cap = self.slots.len();
        let new_cap = if (self.len + 1) * 2 > cap { cap * 2 } else { cap };
        let old = std::mem::replace(&mut self.slots, empty_slots(new_cap));
        let mask = new_cap - 1;
        self.tombs = 0;
        for slot in old {
            if let Slot::Full(k, v) = slot {
                let mut i = (self.hasher.hash_one(&k) as usize) & mask;
                while !matches!(self.slots[i], Slot::Empty) {
                    i = (i + 1) & mask;
                }
                self.slots[i] = Slot::Full(k, v);
            }
        }
    }

    /// Inserts or replaces; returns the old value.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        if (self.len + self.tombs + 1) * 8 > self.slots.len() * 7 {
            self.rehash();
        }
        let mask = self.slots.len() - 1;
        let mut i = self.start(&key);
        let mut first_tomb = None;
        loop {
            match &mut self.slots[i] {
                Slot::Full(k, v) if *k == key => return Some(std::mem::replace(v, value)),
                Slot::Full(..) => {}
                Slot::Tombstone => {
                    if first_tomb.is_none() {
                        first_tomb = Some(i);
                    }
                }
                Slot::Empty => break,
            }
            i = (i + 1) & mask;
        }
        // The key is absent: reuse the first tombstone passed, else the empty slot that ended the chain.
        let target = match first_tomb {
            Some(t) => {
                self.tombs -= 1;
                t
            }
            None => i,
        };
        self.slots[target] = Slot::Full(key, value);
        self.len += 1;
        None
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        let i = self.find(key)?;
        let old = std::mem::replace(&mut self.slots[i], Slot::Tombstone);
        self.len -= 1;
        self.tombs += 1;
        match old {
            Slot::Full(_, v) => Some(v),
            _ => unreachable!(),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.slots.iter().filter_map(|s| match s {
            Slot::Full(k, v) => Some((k, v)),
            _ => None,
        })
    }
}
"""

OAMAP_STARTER = """
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hash};

/// An open-addressing hash map: one flat array, linear probing, tombstones for removal.
/// The capacity is a power of two (starting at 8); keep the table under 7/8 full, counting tombstones.
/// Probe from `hash & (capacity - 1)` to the next slot (wrapping) until you find the key or an empty slot.
pub struct OaMap<K, V, S = RandomState> {
    // TODO: a Vec of slots (empty / tombstone / full), the live count, the tombstone count, the hasher
    _marker: std::marker::PhantomData<(K, V, S)>,
}

impl<K: Hash + Eq, V> OaMap<K, V, RandomState> {
    pub fn new() -> Self {
        todo!()
    }
}

impl<K: Hash + Eq, V, S: BuildHasher> OaMap<K, V, S> {
    pub fn with_hasher(hasher: S) -> Self {
        todo!()
    }

    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        todo!()
    }

    /// Number of slots (always a power of two).
    pub fn capacity(&self) -> usize {
        todo!()
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        todo!()
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        todo!()
    }

    pub fn contains_key(&self, key: &K) -> bool {
        todo!()
    }

    /// Inserts or replaces; returns the old value. Removed entries leave tombstones that `insert` may reuse.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        todo!()
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        todo!()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        std::iter::empty()
    }
}
"""

OA_HELPERS = """
use std::hash::{BuildHasher, Hasher};

/// A hasher that sends every key to the same slot: all operations become one long probe chain.
#[derive(Default, Clone)]
struct AllCollide;
struct AllCollideHasher;
impl BuildHasher for AllCollide {
    type Hasher = AllCollideHasher;
    fn build_hasher(&self) -> AllCollideHasher {
        AllCollideHasher
    }
}
impl Hasher for AllCollideHasher {
    fn write(&mut self, _: &[u8]) {}
    fn finish(&self) -> u64 {
        7
    }
}
"""

P.append(dict(
    slug="open-addressing-map", title="An open-addressing hash map with tombstones", level="hard", stage="tables",
    tags=["hash map", "open addressing", "linear probing", "tombstones", "load factor", "BuildHasher", "hashbrown"],
    teaches=["How the fastest hash tables work: one flat array and linear probing, which is cache-friendly because the next candidate is the next slot.",
             "Why deletion needs a tombstone: emptying the slot would cut the probe chain of every key that collided past it.",
             "A load factor that counts tombstones, and a rehash that either grows or merely cleans, so churn does not slowly poison lookups."],
    statement="""
        Implement `OaMap<K, V, S = RandomState>`: a hash map stored in one `Vec` of slots (`Empty`, `Tombstone`, `Full(K, V)`), power-of-two capacity
        starting at 8, **linear probing**: start at `hash & (capacity - 1)` and step to the next slot, wrapping.

        `get` probes until it finds the key or an `Empty` slot (a `Tombstone` does not stop it). `remove` leaves a `Tombstone`. `insert` first searches the whole
        chain for the key (replacing its value), and otherwise uses the first tombstone it passed, or the empty slot that ended the chain. Before inserting, if
        `(live + tombstones + 1) / capacity` would exceed 7/8, rebuild: **double** the table if it is more than half full of live entries, else rebuild at the same size
        (this drops the tombstones).

        The hasher is generic (`S: BuildHasher`) so the tests can force every key to collide.
    """,
    starter=OAMAP_STARTER,
    solution=OAMAP,
    visible=[
        T("insert_get_remove", "basic", "{ let mut m = OaMap::new(); m.insert(\"a\", 1); m.insert(\"b\", 2); (m.get(&\"a\").copied(), m.get(&\"c\").copied(), m.remove(&\"a\"), m.get(&\"a\").copied(), m.len()) }", "(Some(1), None, Some(1), None, 1)"),
        T("insert_replaces_and_returns_the_old_value", "same key twice", "{ let mut m = OaMap::new(); let a = m.insert(1, \"x\"); let b = m.insert(1, \"y\"); (a, b, m.len(), m.get(&1).copied()) }", "(None, Some(\"x\"), 1, Some(\"y\"))"),
        T("grows_and_keeps_everything", "a thousand keys", "{ let mut m = OaMap::new(); for i in 0..1000u32 { m.insert(i, i * 2); } ((0..1000u32).all(|i| m.get(&i) == Some(&(i * 2))), m.len(), m.capacity().is_power_of_two(), m.capacity() >= 1000) }", "(true, 1000, true, true)"),
        T("get_mut_edits_in_place", "push to a value", "{ let mut m = OaMap::new(); m.insert(\"k\", vec![1]); m.get_mut(&\"k\").unwrap().push(2); m.get(&\"k\").cloned() }", "Some(vec![1, 2])"),
        T("iter_visits_every_entry", "sum", "{ let mut m = OaMap::new(); for i in 1..=10u32 { m.insert(i, i); } (m.iter().count(), m.iter().map(|(k, v)| k + v).sum::<u32>()) }", "(10, 110)"),
    ],
    hidden=[
        T("empty_map", "nothing inside", "{ let m: OaMap<u32, u32> = OaMap::new(); (m.len(), m.is_empty(), m.get(&1).copied()) }", "(0, true, None)"),
        OA_HELPERS + """
        #[test]
        fn all_keys_collide_but_everything_still_works() {
            let mut m = OaMap::with_hasher(AllCollide);
            for i in 0..200u32 {
                assert_eq!(m.insert(i, i + 1000), None);
            }
            for i in 0..200u32 {
                assert_eq!(m.get(&i), Some(&(i + 1000)));
            }
            check!("an absent key is not found even in a full chain", (m.get(&999), m.len()), (None, 200));
        }

        #[test]
        fn removal_does_not_cut_the_probe_chain() {
            let mut m = OaMap::with_hasher(AllCollide);
            for i in 0..6u32 {
                m.insert(i, i);
            }
            check!("remove one in the middle of the chain", m.remove(&2), Some(2));
            for i in [0u32, 1, 3, 4, 5] {
                assert_eq!(m.get(&i), Some(&i), "key {i} became unreachable after a removal in its chain");
            }
            check!("the removed key is gone", m.get(&2), None);
        }

        #[test]
        fn tombstones_are_reused_and_keys_are_never_duplicated() {
            let mut m = OaMap::with_hasher(AllCollide);
            for i in 0..5u32 {
                m.insert(i, i);
            }
            m.remove(&1);
            m.remove(&3);
            check!("re-inserting a key that sits after a tombstone replaces it", m.insert(4, 40), Some(4));
            check!("a new key", m.insert(9, 90), None);
            check!("len counts live entries only", m.len(), 4);
            check!("no duplicate of key 4", m.iter().filter(|(k, _)| **k == 4).count(), 1);
            check!("sum of values", m.iter().map(|(_, v)| *v).sum::<u32>(), 0 + 40 + 2 + 90);
        }

        #[test]
        fn insert_remove_churn_does_not_grow_the_table() {
            let mut m = OaMap::new();
            for i in 0..100_000u32 {
                m.insert(i, i);
                m.remove(&i);
            }
            check!("empty after churn", m.len(), 0);
            assert!(m.capacity() <= 32, "capacity {} after churn with at most one live entry: tombstones are not being cleaned", m.capacity());
            check!("still works", (m.insert(7, 7), m.get(&7).copied()), (None, Some(7)));
        }

        #[test]
        fn a_table_full_of_tombstones_still_terminates() {
            let r = std::sync::mpsc::channel();
            let tx = r.0;
            std::thread::spawn(move || {
                let mut m = OaMap::new();
                for i in 0..64u32 {
                    m.insert(i, i);
                }
                for i in 0..64u32 {
                    m.remove(&i);
                }
                // lots of tombstones, no live entries: a lookup of an absent key must hit an Empty slot
                let absent = m.get(&1_000_000);
                tx.send(absent.is_none()).ok();
            });
            assert_eq!(r.1.recv_timeout(std::time::Duration::from_secs(5)), Ok(true), "get() looped forever");
        }

        #[test]
        fn drops_each_value_once() {
            use std::cell::Cell;
            use std::rc::Rc;
            #[derive(Clone)]
            struct D(Rc<Cell<usize>>);
            impl Drop for D {
                fn drop(&mut self) {
                    self.0.set(self.0.get() + 1);
                }
            }
            let c = Rc::new(Cell::new(0));
            {
                let mut m = OaMap::new();
                for i in 0..100u32 {
                    m.insert(i, D(c.clone()));
                }
                let replaced = m.insert(5, D(c.clone()));
                drop(replaced);
                check!("the replaced value dropped once", c.get(), 1);
                for i in 0..50u32 {
                    drop(m.remove(&i));
                }
                check!("50 removed, each dropped once", c.get(), 51);
            }
            check!("the map's drop took the remaining 50", c.get(), 101);
        }

        #[test]
        fn random_ops_vs_std_hashmap() {
            let mut rng = anneal_prelude::Rng::new(7001);
            let mut mine: OaMap<u32, u64> = OaMap::new();
            let mut collide: OaMap<u32, u64, AllCollide> = OaMap::with_hasher(AllCollide);
            let mut model: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
            for step in 0..30_000u64 {
                let key = rng.below(300) as u32;
                match rng.below(3) {
                    0 => {
                        let a = mine.insert(key, step);
                        let b = collide.insert(key, step);
                        let c = model.insert(key, step);
                        assert_eq!((a, b), (c, c), "insert {key}");
                    }
                    1 => {
                        let a = mine.remove(&key);
                        let b = collide.remove(&key);
                        let c = model.remove(&key);
                        assert_eq!((a, b), (c, c), "remove {key}");
                    }
                    _ => {
                        let c = model.get(&key);
                        assert_eq!((mine.get(&key), collide.get(&key)), (c, c), "get {key}");
                    }
                }
                assert_eq!((mine.len(), collide.len()), (model.len(), model.len()));
            }
            let mut a: Vec<_> = mine.iter().map(|(k, v)| (*k, *v)).collect();
            let mut b: Vec<_> = model.iter().map(|(k, v)| (*k, *v)).collect();
            a.sort();
            b.sort();
            check!("same contents", a, b);
        }
        """,
    ],
    wrong=dict(
        remove_empties_the_slot=sub(OAMAP, "let old = std::mem::replace(&mut self.slots[i], Slot::Tombstone);\n        self.len -= 1;\n        self.tombs += 1;", "let old = std::mem::replace(&mut self.slots[i], Slot::Empty);\n        self.len -= 1;"),
        probe_stops_at_a_tombstone=sub(OAMAP, "                Slot::Empty => return None, // an empty slot ends every probe chain", "                Slot::Empty | Slot::Tombstone => return None,"),
        insert_does_not_search_past_tombstones=sub(OAMAP, "                Slot::Tombstone => {\n                    if first_tomb.is_none() {\n                        first_tomb = Some(i);\n                    }\n                }", "                Slot::Tombstone => break,"),
        tombstones_do_not_count_toward_the_load=sub(OAMAP, "if (self.len + self.tombs + 1) * 8 > self.slots.len() * 7 {", "if (self.len + 1) * 8 > self.slots.len() * 7 {"),
        rehash_never_cleans=sub(OAMAP, "let new_cap = if (self.len + 1) * 2 > cap { cap * 2 } else { cap };", "let new_cap = cap * 2;"),
    ),
    hints=[("approach", "`slots: Vec<Slot<K, V>>` with `Empty`, `Tombstone`, `Full(k, v)`. `find`: start at `hash & mask`, loop: `Empty` means absent, `Full` with an equal key means found, anything else keep going with `(i + 1) & mask`. `remove` swaps the slot for a `Tombstone`. `insert` walks the same chain remembering the first tombstone."),
           ("rust", "`BuildHasher::hash_one(&key)` hashes in one call. `std::mem::replace(&mut slot, new)` takes a value out of a slot you only borrow. Match `&mut self.slots[i]` with an arm guard `Slot::Full(k, v) if *k == key` to replace a value in place. The default type parameter `S = RandomState` lets `OaMap::new()` work while tests inject a hasher."),
           ("edge", "Count tombstones in the load factor, or a table of tombstones has no `Empty` slot and a miss loops forever. On rebuild only reinsert `Full` slots. Rebuild at the *same* size when the table is mostly tombstones, or insert/remove churn doubles the table forever.")],
    notes=("`hashbrown` (the std `HashMap`) is a refinement of this: it stores 7 bits of each hash in a control-byte array and compares 16 of them at once with SIMD (the F5 SWAR trick), which is why misses are cheap. The ideas here, flat storage, probing, tombstones and load factor, are exactly what its `RawTable` does, and Robin Hood hashing, quadratic probing and cuckoo hashing are the alternatives to know.", "O(1) expected, O(n) worst case under adversarial hashing", "O(capacity)"),
    follow_up="1) Add a control-byte array (one byte per slot: empty / deleted / top 7 hash bits) and compare 8 at a time with the F5 SWAR `zero_bytes` trick: how many key comparisons does a miss need now? 2) Implement Robin Hood insertion (steal from richer entries) and backward-shift deletion, which removes the need for tombstones. 3) Add `entry` API and `retain`. 4) Why does `RandomState` matter for a public service (HashDoS), and what does the `AllCollide` test show about the worst case? 5) Compare memory and speed with `std::collections::HashMap` using the F1 benchmarking tools.",
    related=["F4", "F5", "S4"],
))

STAGES = [("sketches-and-rings", "Sketches and rings", "medium"), ("tables", "Build a table", "hard")]

if __name__ == "__main__":
    n = write_track("f4-hashing-structures", "F4", "Hashing & purpose-built structures", "F", "sde3", 4,
                    "Hashing as an engineering tool: a Bloom filter, a count-min sketch, a consistent-hash ring and an open-addressing hash map.",
                    STAGES, P)
    print("F4", n)
