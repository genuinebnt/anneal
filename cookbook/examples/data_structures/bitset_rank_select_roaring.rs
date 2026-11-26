//! RECIPE 184 (data-structures): bitsets, rank/select, and Roaring-style compressed bitmaps.
//! LEVEL: advanced
//! Use in: bitmap indexes, search posting lists, Bloom-like filters, succinct data structures.   Run: cargo run --release --example bitset_rank_select_roaring
//!
//! WHAT: three layers. (1) `BitSet`: one bit per possible element in a Vec<u64>; set algebra runs a whole
//! 64-bit word at a time (so 64 elements per instruction) and iteration skips zeros with `trailing_zeros`.
//! (2) `RankSelect`: rank(i) = how many set bits below i; select(k) = position of the k-th set bit. A sampled
//! array of cumulative counts (one u32 per 8 words) makes rank O(1) (at most 8 popcounts) and select a binary
//! search over samples plus a short scan. (3) `Roaring`: a dense bitset over a 32-bit universe wastes memory
//! when sparse, and a sorted Vec<u32> wastes memory when dense. Roaring splits values by their HIGH 16 bits
//! into chunks, and each chunk picks its own container: a sorted array of low 16 bits (sparse, <= 4096 items,
//! 2 bytes each) or a fixed 8 KiB bitmap (dense). 4096 * 2 B = 8 KiB is exactly the break-even point.
//!
//! WHY word-at-a-time: AND/OR/ANDNOT of two bitsets is a linear scan of u64s that the CPU/compiler vectorizes;
//! it beats merging sorted lists unless one side is tiny, which is why arrays stay arrays.
//!
//! COMMON USES: database bitmap indexes (one bitmap per column value, AND/OR to combine predicates), search
//! engine posting lists (docs containing a term; AND = phrase/boolean query), set-membership of ids in
//! analytics engines, visited-sets in graph traversals, succinct tries/wavelet trees (rank/select is their core).
//! Real code: the `roaring` crate https://docs.rs/roaring
//! Read: https://roaringbitmap.org   https://arxiv.org/abs/1402.6407 (Better bitmap performance with Roaring)
#![allow(dead_code)]
use std::collections::BTreeSet;

// ---------- BitSet ----------
#[derive(Clone, Debug, PartialEq)]
struct BitSet {
    words: Vec<u64>,
    nbits: usize,
}

/// Iterator over set bit positions of one word: clear the lowest set bit each step (`w & (w - 1)`).
struct WordBits(u64);
impl Iterator for WordBits {
    type Item = usize;
    fn next(&mut self) -> Option<usize> {
        if self.0 == 0 {
            return None;
        }
        let tz = self.0.trailing_zeros() as usize;
        self.0 &= self.0 - 1;
        Some(tz)
    }
}

impl BitSet {
    fn new(nbits: usize) -> Self {
        BitSet {
            words: vec![0; nbits.div_ceil(64)],
            nbits,
        }
    }
    fn set(&mut self, i: usize) {
        assert!(i < self.nbits);
        self.words[i / 64] |= 1 << (i % 64);
    }
    fn clear(&mut self, i: usize) {
        assert!(i < self.nbits);
        self.words[i / 64] &= !(1 << (i % 64));
    }
    fn test(&self, i: usize) -> bool {
        i < self.nbits && self.words[i / 64] >> (i % 64) & 1 == 1
    }
    fn count_ones(&self) -> usize {
        self.words.iter().map(|w| w.count_ones() as usize).sum()
    }
    fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.words
            .iter()
            .enumerate()
            .flat_map(|(i, &w)| WordBits(w).map(move |b| i * 64 + b))
    }
    fn zip_words(&self, other: &BitSet, f: impl Fn(u64, u64) -> u64) -> BitSet {
        assert_eq!(self.nbits, other.nbits, "bitsets must share a universe");
        let words = self
            .words
            .iter()
            .zip(&other.words)
            .map(|(&a, &b)| f(a, b))
            .collect();
        BitSet {
            words,
            nbits: self.nbits,
        }
    }
    fn union(&self, o: &BitSet) -> BitSet {
        self.zip_words(o, |a, b| a | b)
    }
    fn intersect(&self, o: &BitSet) -> BitSet {
        self.zip_words(o, |a, b| a & b)
    }
    fn difference(&self, o: &BitSet) -> BitSet {
        self.zip_words(o, |a, b| a & !b)
    }
}

// ---------- Rank / Select ----------
const WORDS_PER_SAMPLE: usize = 8; // 512 bits per sample: 32 bits of counts per 512 bits = 6% overhead

struct RankSelect {
    bits: BitSet,
    samples: Vec<u32>, // samples[b] = ones before word b * WORDS_PER_SAMPLE
}

impl RankSelect {
    fn new(bits: BitSet) -> Self {
        let mut samples = Vec::with_capacity(bits.words.len() / WORDS_PER_SAMPLE + 1);
        let mut total = 0u32;
        for chunk in bits.words.chunks(WORDS_PER_SAMPLE) {
            samples.push(total);
            total += chunk.iter().map(|w| w.count_ones()).sum::<u32>();
        }
        if bits.words.len() % WORDS_PER_SAMPLE == 0 {
            samples.push(total); // so rank(nbits) always finds a sample
        }
        RankSelect { bits, samples }
    }
    fn total(&self) -> usize {
        self.bits.count_ones()
    }
    /// Number of set bits in positions [0, i).
    fn rank(&self, i: usize) -> usize {
        assert!(i <= self.bits.nbits);
        let (w, r) = (i / 64, i % 64);
        let first = w / WORDS_PER_SAMPLE * WORDS_PER_SAMPLE;
        let mut n = self.samples[w / WORDS_PER_SAMPLE] as usize;
        n += self.bits.words[first..w]
            .iter()
            .map(|x| x.count_ones() as usize)
            .sum::<usize>();
        if r > 0 {
            n += (self.bits.words[w] & ((1u64 << r) - 1)).count_ones() as usize;
        }
        n
    }
    /// Position of the k-th set bit (0-based), i.e. the inverse of rank.
    fn select(&self, k: usize) -> Option<usize> {
        if k >= self.total() {
            return None;
        }
        let block = self.samples.partition_point(|&s| s as usize <= k) - 1;
        let mut remaining = k - self.samples[block] as usize;
        for w in block * WORDS_PER_SAMPLE.. {
            let ones = self.bits.words[w].count_ones() as usize;
            if remaining < ones {
                return Some(w * 64 + select_in_word(self.bits.words[w], remaining));
            }
            remaining -= ones;
        }
        unreachable!("k < total guarantees a hit")
    }
}

/// Position of the k-th (0-based) set bit inside a word: drop the k lowest set bits, then take the lowest.
fn select_in_word(mut w: u64, k: usize) -> usize {
    for _ in 0..k {
        w &= w - 1;
    }
    w.trailing_zeros() as usize
}

// ---------- Roaring ----------
const ARRAY_MAX: usize = 4096;
const BITMAP_WORDS: usize = 1024; // 65536 bits

#[derive(Clone, Debug, PartialEq)]
enum Container {
    Array(Vec<u16>), // sorted, no duplicates, len <= ARRAY_MAX
    Bitmap { words: Vec<u64>, card: usize },
}

impl Container {
    fn len(&self) -> usize {
        match self {
            Container::Array(a) => a.len(),
            Container::Bitmap { card, .. } => *card,
        }
    }
    fn contains(&self, x: u16) -> bool {
        match self {
            Container::Array(a) => a.binary_search(&x).is_ok(),
            Container::Bitmap { words, .. } => words[x as usize / 64] >> (x % 64) & 1 == 1,
        }
    }
    fn to_bitmap(&self) -> (Vec<u64>, usize) {
        match self {
            Container::Bitmap { words, card } => (words.clone(), *card),
            Container::Array(a) => {
                let mut words = vec![0u64; BITMAP_WORDS];
                a.iter()
                    .for_each(|&x| words[x as usize / 64] |= 1 << (x % 64));
                (words, a.len())
            }
        }
    }
    /// Invariant keeper: more than ARRAY_MAX items => bitmap, otherwise array.
    fn from_bitmap(words: Vec<u64>, card: usize) -> Container {
        if card > ARRAY_MAX {
            return Container::Bitmap { words, card };
        }
        let arr = words
            .iter()
            .enumerate()
            .flat_map(|(i, &w)| WordBits(w).map(move |b| (i * 64 + b) as u16));
        Container::Array(arr.collect())
    }
    fn from_sorted(v: Vec<u16>) -> Container {
        if v.len() <= ARRAY_MAX {
            return Container::Array(v);
        }
        let (words, card) = Container::Array(v).to_bitmap();
        Container::Bitmap { words, card }
    }
    fn insert(&mut self, x: u16) -> bool {
        match self {
            Container::Array(a) => match a.binary_search(&x) {
                Ok(_) => false,
                Err(pos) => {
                    a.insert(pos, x);
                    if a.len() > ARRAY_MAX {
                        *self = Container::from_sorted(std::mem::take(a));
                    }
                    true
                }
            },
            Container::Bitmap { words, card } => {
                let (w, m) = (x as usize / 64, 1u64 << (x % 64));
                let fresh = words[w] & m == 0;
                words[w] |= m;
                *card += fresh as usize;
                fresh
            }
        }
    }
    fn remove(&mut self, x: u16) -> bool {
        match self {
            Container::Array(a) => a.binary_search(&x).map(|p| a.remove(p)).is_ok(),
            Container::Bitmap { words, card } => {
                let (w, m) = (x as usize / 64, 1u64 << (x % 64));
                let had = words[w] & m != 0;
                words[w] &= !m;
                if had {
                    *card -= 1;
                    if *card <= ARRAY_MAX {
                        *self = Container::from_bitmap(std::mem::take(words), *card);
                    }
                }
                had
            }
        }
    }
    fn iter(&self) -> Box<dyn Iterator<Item = u16> + '_> {
        match self {
            Container::Array(a) => Box::new(a.iter().copied()),
            Container::Bitmap { words, .. } => Box::new(
                words
                    .iter()
                    .enumerate()
                    .flat_map(|(i, &w)| WordBits(w).map(move |b| (i * 64 + b) as u16)),
            ),
        }
    }
    fn union(&self, other: &Container) -> Container {
        match (self, other) {
            (Container::Array(a), Container::Array(b)) => Container::from_sorted(merge_union(a, b)),
            (Container::Array(a), bm @ Container::Bitmap { .. })
            | (bm @ Container::Bitmap { .. }, Container::Array(a)) => {
                let (mut words, mut card) = bm.to_bitmap();
                for &x in a {
                    let (w, m) = (x as usize / 64, 1u64 << (x % 64));
                    card += (words[w] & m == 0) as usize;
                    words[w] |= m;
                }
                Container::Bitmap { words, card }
            }
            (Container::Bitmap { words: a, .. }, Container::Bitmap { words: b, .. }) => {
                let words: Vec<u64> = a.iter().zip(b).map(|(x, y)| x | y).collect();
                let card = words.iter().map(|w| w.count_ones() as usize).sum();
                Container::Bitmap { words, card }
            }
        }
    }
    fn intersect(&self, other: &Container) -> Container {
        match (self, other) {
            (Container::Array(a), Container::Array(b)) => Container::Array(merge_intersect(a, b)),
            (Container::Array(a), bm @ Container::Bitmap { .. })
            | (bm @ Container::Bitmap { .. }, Container::Array(a)) => {
                Container::Array(a.iter().copied().filter(|&x| bm.contains(x)).collect())
            }
            (Container::Bitmap { words: a, .. }, Container::Bitmap { words: b, .. }) => {
                let words: Vec<u64> = a.iter().zip(b).map(|(x, y)| x & y).collect();
                let card = words.iter().map(|w| w.count_ones() as usize).sum();
                Container::from_bitmap(words, card) // may shrink back to an array
            }
        }
    }
    fn memory_bytes(&self) -> usize {
        match self {
            Container::Array(a) => 2 * a.len(),
            Container::Bitmap { .. } => 8 * BITMAP_WORDS,
        }
    }
}

fn merge_union(a: &[u16], b: &[u16]) -> Vec<u16> {
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::with_capacity(a.len() + b.len());
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => (out.push(a[i]), i += 1).1,
            std::cmp::Ordering::Greater => (out.push(b[j]), j += 1).1,
            std::cmp::Ordering::Equal => (out.push(a[i]), i += 1, j += 1).2,
        }
    }
    out.extend_from_slice(&a[i..]);
    out.extend_from_slice(&b[j..]);
    out
}

fn merge_intersect(a: &[u16], b: &[u16]) -> Vec<u16> {
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                out.push(a[i]);
                i += 1;
                j += 1;
            }
        }
    }
    out
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Roaring {
    keys: Vec<u16>, // sorted high halves, parallel to `containers`
    containers: Vec<Container>,
}

impl Roaring {
    fn split(x: u32) -> (u16, u16) {
        ((x >> 16) as u16, x as u16)
    }
    fn insert(&mut self, x: u32) -> bool {
        let (hi, lo) = Self::split(x);
        match self.keys.binary_search(&hi) {
            Ok(i) => self.containers[i].insert(lo),
            Err(i) => {
                self.keys.insert(i, hi);
                self.containers.insert(i, Container::Array(vec![lo]));
                true
            }
        }
    }
    fn remove(&mut self, x: u32) -> bool {
        let (hi, lo) = Self::split(x);
        let Ok(i) = self.keys.binary_search(&hi) else {
            return false;
        };
        let had = self.containers[i].remove(lo);
        if self.containers[i].len() == 0 {
            self.keys.remove(i); // never keep empty chunks
            self.containers.remove(i);
        }
        had
    }
    fn contains(&self, x: u32) -> bool {
        let (hi, lo) = Self::split(x);
        self.keys
            .binary_search(&hi)
            .is_ok_and(|i| self.containers[i].contains(lo))
    }
    fn len(&self) -> usize {
        self.containers.iter().map(Container::len).sum()
    }
    fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        self.keys
            .iter()
            .zip(&self.containers)
            .flat_map(|(&k, c)| c.iter().map(move |lo| (k as u32) << 16 | lo as u32))
    }
    /// Walk both sorted key lists like a merge; matching keys combine containers.
    fn union(&self, o: &Roaring) -> Roaring {
        let (mut i, mut j) = (0, 0);
        let mut out = Roaring::default();
        while i < self.keys.len() || j < o.keys.len() {
            let (ka, kb) = (self.keys.get(i), o.keys.get(j));
            if kb.is_none() || ka.is_some_and(|a| a < kb.unwrap()) {
                out.push(self.keys[i], self.containers[i].clone());
                i += 1;
            } else if ka.is_none() || kb.is_some_and(|b| b < ka.unwrap()) {
                out.push(o.keys[j], o.containers[j].clone());
                j += 1;
            } else {
                out.push(self.keys[i], self.containers[i].union(&o.containers[j]));
                i += 1;
                j += 1;
            }
        }
        out
    }
    fn intersect(&self, o: &Roaring) -> Roaring {
        let (mut i, mut j) = (0, 0);
        let mut out = Roaring::default();
        while i < self.keys.len() && j < o.keys.len() {
            match self.keys[i].cmp(&o.keys[j]) {
                std::cmp::Ordering::Less => i += 1,
                std::cmp::Ordering::Greater => j += 1,
                std::cmp::Ordering::Equal => {
                    let c = self.containers[i].intersect(&o.containers[j]);
                    if c.len() > 0 {
                        out.push(self.keys[i], c);
                    }
                    i += 1;
                    j += 1;
                }
            }
        }
        out
    }
    fn push(&mut self, key: u16, c: Container) {
        self.keys.push(key);
        self.containers.push(c);
    }
    fn memory_bytes(&self) -> usize {
        self.containers.iter().map(|c| c.memory_bytes() + 4).sum() // +4: key and container tag
    }
}

// ---------- test support ----------
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// A random set whose chunks have deliberately mixed densities (so array/bitmap containers coexist).
fn random_chunked(rng: &mut Rng, chunks: u32) -> BTreeSet<u32> {
    const COUNTS: [u64; 8] = [0, 5, 300, 4095, 4096, 4097, 20_000, 65_536];
    let mut s = BTreeSet::new();
    for k in 0..chunks {
        if rng.below(4) == 0 {
            continue; // leave some keys absent
        }
        let n = COUNTS[rng.below(8) as usize];
        if n == 65_536 {
            s.extend((0..65_536u32).map(|lo| k << 16 | lo));
        } else {
            (0..n).for_each(|_| {
                let _ = s.insert(k << 16 | rng.below(65_536) as u32);
            });
        }
    }
    s
}

fn roaring_from(s: &BTreeSet<u32>) -> Roaring {
    let mut r = Roaring::default();
    s.iter().for_each(|&x| {
        let _ = r.insert(x);
    });
    r
}

fn check_roaring(r: &Roaring, s: &BTreeSet<u32>) {
    assert_eq!(r.len(), s.len());
    assert!(r.iter().eq(s.iter().copied()));
    for c in &r.containers {
        // the representation invariant is what keeps memory bounded
        match c {
            Container::Array(a) => {
                assert!(a.len() <= ARRAY_MAX && a.windows(2).all(|w| w[0] < w[1]))
            }
            Container::Bitmap { card, .. } => assert!(*card > ARRAY_MAX),
        }
    }
}

fn test_bitset_and_rank_select(rng: &mut Rng) {
    for &(nbits, density) in &[
        (1usize, 1u64),
        (63, 2),
        (64, 2),
        (65, 3),
        (512, 4),
        (1000, 2),
        (5000, 30),
        (20_000, 500),
    ] {
        let mut a = BitSet::new(nbits);
        let mut b = BitSet::new(nbits);
        let (mut sa, mut sb) = (BTreeSet::new(), BTreeSet::new());
        for i in 0..nbits {
            if rng.below(density) == 0 {
                a.set(i);
                sa.insert(i);
            }
            if rng.below(density) == 0 {
                b.set(i);
                sb.insert(i);
            }
        }
        assert!(a.iter().eq(sa.iter().copied()));
        assert_eq!(a.count_ones(), sa.len());
        assert!(a.union(&b).iter().eq(sa.union(&sb).copied()));
        assert!(a.intersect(&b).iter().eq(sa.intersection(&sb).copied()));
        assert!(a.difference(&b).iter().eq(sa.difference(&sb).copied()));
        if let Some(&x) = sa.iter().next() {
            a.clear(x);
            assert!(!a.test(x));
            a.set(x);
        }
        let rs = RankSelect::new(a.clone());
        let elems: Vec<usize> = sa.iter().copied().collect();
        for i in 0..=nbits {
            assert_eq!(
                rs.rank(i),
                elems.partition_point(|&e| e < i),
                "rank({i}) nbits={nbits}"
            );
        }
        for (k, &e) in elems.iter().enumerate() {
            assert_eq!(rs.select(k), Some(e));
            assert_eq!(rs.rank(e), k); // rank and select are inverses
        }
        assert_eq!(rs.select(elems.len()), None);
    }
}

fn test_roaring(rng: &mut Rng) {
    for _ in 0..12 {
        let (sa, sb) = (random_chunked(rng, 6), random_chunked(rng, 6));
        let (ra, rb) = (roaring_from(&sa), roaring_from(&sb));
        check_roaring(&ra, &sa);
        check_roaring(&rb, &sb);
        let u: BTreeSet<u32> = sa.union(&sb).copied().collect();
        let i: BTreeSet<u32> = sa.intersection(&sb).copied().collect();
        check_roaring(&ra.union(&rb), &u);
        check_roaring(&ra.intersect(&rb), &i);
        for _ in 0..200 {
            let x = (rng.below(6) << 16 | rng.below(65_536)) as u32;
            assert_eq!(ra.contains(x), sa.contains(&x));
        }
    }
    // Conversion boundary: 4096 stays an array, 4097 becomes a bitmap, removing one converts back.
    let mut r = Roaring::default();
    (0..4096u32).for_each(|x| {
        let _ = r.insert(x * 2);
    });
    assert!(matches!(r.containers[0], Container::Array(_)));
    r.insert(1);
    assert!(matches!(r.containers[0], Container::Bitmap { .. }));
    assert!(r.remove(1) && !r.remove(1));
    assert!(matches!(r.containers[0], Container::Array(_)));
    (0..4096u32).for_each(|x| {
        let _ = r.remove(x * 2);
    });
    assert_eq!(r.len(), 0);
    assert!(r.keys.is_empty());
}

fn main() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    test_bitset_and_rank_select(&mut rng);
    test_roaring(&mut rng);

    // Demo: rank/select on a small set.
    let mut b = BitSet::new(100);
    [3, 10, 64, 65, 99].iter().for_each(|&i| b.set(i));
    let rs = RankSelect::new(b);
    println!("set bits: {:?}", rs.bits.iter().collect::<Vec<_>>());
    println!(
        "rank(65)={} (bits below 65)  select(3)={:?} (4th set bit)",
        rs.rank(65),
        rs.select(3)
    );
    assert_eq!((rs.rank(65), rs.select(3)), (3, Some(65)));

    // Memory comparison: universe of 2^22 values (4M ids), varying cardinality.
    const UNIVERSE: u32 = 1 << 22;
    println!("\nmemory for subsets of a 4M-id universe (bytes):");
    println!(
        "{:>10} {:>12} {:>12} {:>12} {:>10}",
        "elements", "sorted u32", "dense bits", "roaring", "roaring/best"
    );
    let mut prev_ratio = f64::MAX;
    for &n in &[1_000u64, 20_000, 200_000, 1_000_000, 3_000_000] {
        let s: BTreeSet<u32> = (0..n).map(|_| rng.below(UNIVERSE as u64) as u32).collect();
        let r = roaring_from(&s);
        assert!(r.iter().eq(s.iter().copied()));
        let (list, dense, roar) = (4 * s.len(), UNIVERSE as usize / 8, r.memory_bytes());
        let ratio = roar as f64 / list.min(dense) as f64;
        println!(
            "{:>10} {:>12} {:>12} {:>12} {:>9.2}x",
            s.len(),
            list,
            dense,
            roar,
            ratio
        );
        assert!(roar < list.max(dense), "roaring should never be the worst");
        assert!(
            ratio < 1.2,
            "roaring stays within 20% of the better flat layout"
        );
        prev_ratio = prev_ratio.min(ratio);
    }
    println!(
        "roaring tracks the better of the two layouts at every density; best ratio {prev_ratio:.2}x"
    );
    println!("ok");
}
