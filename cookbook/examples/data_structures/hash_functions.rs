//! RECIPE 120 (hashing): hash functions from scratch (FNV-1a, Fx, fmix64, a wyhash-style mixer) and how to MEASURE their quality.
//! LEVEL: intermediate
//! Use in: choosing/validating a hasher for HashMap, sharding, sketches, bloom filters.   Run: cargo run --release --example hash_functions
#![allow(dead_code)]
//!
//! WHAT: four small `std::hash::Hasher`s plugged into `HashMap` through `BuildHasherDefault`, plus a deliberately bad one (sum of bytes).
//! WHY: "looks random" is not a property you can eyeball; hash tables, bloom filters and partitioners all silently degrade on a biased hash.
//! HOW: three measurements.
//!  1. Avalanche: flip ONE input bit; a good 64-bit hash flips about 32 of 64 output bits (we assert 28..36 for the mixers that should pass).
//!     Plain FNV-1a and Fx only push entropy UPWARD (multiplication carries go to higher bits), so low output bits barely react; that is why
//!     hashbrown-style tables pair them with a finalizer or use the top bits. `fmix64` (MurmurHash3's finalizer) fixes exactly this.
//!  2. Bucket uniformity: chi-square over 1024 buckets (low 10 bits). Perfectly random gives about 1023 +- 45.
//!  3. Collisions of the full 64-bit value on 200k keys (expected: 0).
//! Finally a HashDoS demo: keys crafted to share a weak hash make a std map quadratic; std's randomly keyed SipHash (`RandomState`) is immune.
//!
//! | kind              | examples            | goal                                  | use when                                          |
//! |-------------------|---------------------|---------------------------------------|---------------------------------------------------|
//! | non-cryptographic | FNV, Fx, xxHash, wyhash | speed + good spread                | trusted keys, internal tables, sharding, sketches |
//! | keyed PRF (SipHash)| SipHash-1-3 (std)  | speed + resists chosen-key flooding   | keys an attacker can choose (HTTP params, JSON)   |
//! | cryptographic     | SHA-256, BLAKE3     | preimage/collision resistance         | integrity, signatures, content addressing         |
//!
//! COMMON USES: HashMap/HashSet hasher selection (fast_hashing), consistent hashing and sharding (consistent_hashing, hash_partitioning), bloom/cuckoo filters (bloom_filter, cuckoo_hashing),
//! content-defined chunking, deduplication fingerprints, and HashDoS hardening of servers that build maps from request data.
//! Read: https://nnethercote.github.io/perf-book/hashing.html and https://github.com/aappleby/smhasher and https://github.com/aappleby/smhasher/wiki/MurmurHash3
use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasher, BuildHasherDefault, Hasher, RandomState};
use std::hint::black_box;
use std::time::Instant;

/// Murmur3 64-bit finalizer: xor-shift + multiply rounds so every input bit reaches every output bit.
fn fmix64(mut k: u64) -> u64 {
    k ^= k >> 33;
    k = k.wrapping_mul(0xff51_afd7_ed55_8ccd);
    k ^= k >> 33;
    k = k.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    k ^ (k >> 33)
}
fn fmix32(mut h: u32) -> u32 {
    h ^= h >> 16;
    h = h.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 13;
    h = h.wrapping_mul(0xc2b2_ae35);
    h ^ (h >> 16)
}
/// 64x64 -> 128 multiply, folded: the core of wyhash/xxh3-style mixing (one multiply mixes both halves).
fn mum(a: u64, b: u64) -> u64 {
    let r = (a as u128) * (b as u128);
    (r as u64) ^ ((r >> 64) as u64)
}
/// Read up to 8 bytes as a zero-padded little-endian word.
fn word(chunk: &[u8]) -> u64 {
    let mut buf = [0u8; 8];
    buf[..chunk.len()].copy_from_slice(chunk);
    u64::from_le_bytes(buf)
}

struct Fnv1a(u64);
impl Default for Fnv1a {
    fn default() -> Self {
        Fnv1a(0xcbf2_9ce4_8422_2325) // offset basis
    }
}
impl Hasher for Fnv1a {
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3); // xor THEN multiply = "1a"
        }
    }
    fn finish(&self) -> u64 {
        self.0
    }
}

#[derive(Default)]
struct Fx(u64);
impl Hasher for Fx {
    fn write(&mut self, bytes: &[u8]) {
        for c in bytes.chunks(8) {
            self.0 = (self.0.rotate_left(5) ^ word(c)).wrapping_mul(0x517c_c1b7_2722_0a95);
        }
    }
    fn finish(&self) -> u64 {
        self.0
    }
}

/// Fx plus the murmur finalizer: same speed class, but the low bits now depend on all input bits.
#[derive(Default)]
struct FxFin(Fx);
impl Hasher for FxFin {
    fn write(&mut self, bytes: &[u8]) {
        self.0.write(bytes)
    }
    fn finish(&self) -> u64 {
        fmix64(self.0.0)
    }
}

/// wyhash-inspired: 8 bytes per step through a folded 128-bit multiply, length mixed in, multiply finalizer.
#[derive(Default)]
struct Mum(u64);
impl Hasher for Mum {
    fn write(&mut self, bytes: &[u8]) {
        for c in bytes.chunks(8) {
            self.0 = mum(
                self.0 ^ 0xa076_1d64_78bd_642f,
                word(c) ^ 0xe703_7ed1_a0b4_28db,
            );
        }
        self.0 = self.0.wrapping_add(bytes.len() as u64);
    }
    fn finish(&self) -> u64 {
        mum(self.0 ^ 0x8ebc_6af0_9c88_c6e3, 0x5899_65cc_7537_4cc3)
    }
}

/// The strawman: order-insensitive, tiny range, trivially collidable.
#[derive(Default)]
struct SumHash(u64);
impl Hasher for SumHash {
    fn write(&mut self, bytes: &[u8]) {
        self.0 += bytes.iter().map(|&b| b as u64).sum::<u64>();
    }
    fn finish(&self) -> u64 {
        self.0
    }
}

type HashFn = fn(&[u8]) -> u64;
fn via<H: Hasher + Default>(b: &[u8]) -> u64 {
    let mut h = H::default();
    h.write(b);
    h.finish()
}
/// fmix64 chained over 8-byte words (for one word this is exactly fmix64(key)).
fn fmix_bytes(b: &[u8]) -> u64 {
    b.chunks(8).fold(0, |h, c| fmix64(h ^ word(c)))
}
fn all_hashers() -> [(&'static str, HashFn); 6] {
    [
        ("fnv1a", via::<Fnv1a>),
        ("fx", via::<Fx>),
        ("fx+fmix64", via::<FxFin>),
        ("mum(wy-like)", via::<Mum>),
        ("fmix64(u64)", fmix_bytes),
        ("sum-of-bytes", via::<SumHash>),
    ]
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

/// Mean number of output bits that flip when one input bit flips (ideal: 32).
fn avalanche(h: HashFn, rng: &mut Rng) -> f64 {
    let (mut total, mut n) = (0u64, 0u64);
    for _ in 0..500 {
        let key = rng.next().to_le_bytes();
        let base = h(&key);
        for bit in 0..64 {
            let mut k = key;
            k[bit / 8] ^= 1 << (bit % 8);
            total += (h(&k) ^ base).count_ones() as u64;
            n += 1;
        }
    }
    total as f64 / n as f64
}

/// Pearson chi-square of keys over 1024 low-bit buckets (expected about 1023 for uniform).
fn chi_square(h: HashFn, keys: &[Vec<u8>]) -> f64 {
    let mut counts = [0u64; 1024];
    for k in keys {
        counts[(h(k) & 1023) as usize] += 1;
    }
    let e = keys.len() as f64 / 1024.0;
    counts.iter().map(|&c| (c as f64 - e).powi(2) / e).sum()
}

fn collisions(h: HashFn, keys: &[Vec<u8>]) -> usize {
    keys.iter()
        .map(|k| h(k))
        .collect::<HashSet<_>>()
        .len()
        .abs_diff(keys.len())
}

fn ns_per_hash(h: HashFn, key: &[u8], reps: u32) -> f64 {
    let t = Instant::now();
    let mut acc = 0u64;
    for _ in 0..reps {
        acc ^= h(black_box(key));
    }
    black_box(acc);
    t.elapsed().as_nanos() as f64 / reps as f64
}

fn exercise_map<S: BuildHasher + Default>() -> usize {
    let mut m: HashMap<String, usize, S> = HashMap::default();
    for i in 0..10_000 {
        m.insert(format!("key-{i}"), i);
    }
    assert!((0..10_000).all(|i| m.get(&format!("key-{i}")) == Some(&i)));
    m.len()
}

/// 10^4 distinct 8-byte keys with an identical byte sum: pairs (m+d, m-d) cancel out.
fn crafted_keys() -> Vec<String> {
    (0..10_000u32)
        .map(|i| {
            (0..4)
                .flat_map(|p| {
                    let d = (i / 10u32.pow(p) % 10) as u8;
                    [(100 + d) as char, (100 - d) as char]
                })
                .collect()
        })
        .collect()
}

fn fill_time<S: BuildHasher + Default>(keys: &[String]) -> f64 {
    let t = Instant::now();
    let mut m: HashMap<&str, u32, S> = HashMap::default();
    for (i, k) in keys.iter().enumerate() {
        m.insert(k.as_str(), i as u32);
    }
    assert_eq!(m.len(), keys.len());
    t.elapsed().as_secs_f64()
}

fn main() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let seq: Vec<Vec<u8>> = (0..100_000u64).map(|i| i.to_le_bytes().to_vec()).collect();
    let text: Vec<Vec<u8>> = (0..200_000)
        .map(|i| format!("user:{i}@example.com").into_bytes())
        .collect();

    println!(
        "{:<14}{:>10}{:>12}{:>12}{:>12}",
        "hasher", "avalanche", "chi2 seq", "chi2 text", "collisions"
    );
    let mut res = HashMap::new();
    for (name, h) in all_hashers() {
        let (a, c1, c2, col) = (
            avalanche(h, &mut rng),
            chi_square(h, &seq),
            chi_square(h, &text),
            collisions(h, &text),
        );
        println!("{name:<14}{a:>10.1}{c1:>12.0}{c2:>12.0}{col:>12}");
        res.insert(name, (a, c1, c2, col));
    }
    for good in ["fx+fmix64", "mum(wy-like)", "fmix64(u64)"] {
        let (a, c1, c2, _) = res[good];
        assert!((28.0..36.0).contains(&a), "{good} avalanche {a}");
        assert!(c1 < 1300.0 && c2 < 1300.0, "{good} chi2 {c1} {c2}");
    }
    // Lesson: a finalizer fixes AVALANCHE but cannot undo collisions Fx already made (rotate-xor-multiply per word loses information
    // between words), so Fx+fmix64 still collides on multi-word text keys while the others do not.
    assert_eq!(res["mum(wy-like)"].3, 0);
    assert_eq!(res["fmix64(u64)"].3, 0);
    assert!(res["fx+fmix64"].3 > 0 && res["fx"].3 > 0);
    assert!(
        res["fx"].0 < 28.0,
        "plain Fx should fail avalanche (low bits are weak)"
    );
    assert!(
        res["sum-of-bytes"].0 < 8.0
            && res["sum-of-bytes"].2 > 5000.0
            && res["sum-of-bytes"].3 > 1000
    );
    println!("fmix32(1)={:#x} fmix32(2)={:#x}", fmix32(1), fmix32(2));
    assert_ne!(fmix32(1), fmix32(2));

    println!("\nns/hash         8-byte key    4 KiB key");
    let big = vec![0xabu8; 4096];
    for (name, h) in all_hashers().into_iter().take(4) {
        println!(
            "{name:<14}{:>11.1}{:>13.0}",
            ns_per_hash(h, &[7u8; 8], 2_000_000),
            ns_per_hash(h, &big, 20_000)
        );
    }

    println!("\nplugged into HashMap<String,_,BuildHasherDefault<H>>:");
    assert_eq!(exercise_map::<BuildHasherDefault<Fnv1a>>(), 10_000);
    assert_eq!(exercise_map::<BuildHasherDefault<Fx>>(), 10_000);
    assert_eq!(exercise_map::<BuildHasherDefault<FxFin>>(), 10_000);
    assert_eq!(exercise_map::<BuildHasherDefault<Mum>>(), 10_000);
    assert_eq!(exercise_map::<RandomState>(), 10_000);
    println!("  all five hashers give a correct map");

    let keys = crafted_keys();
    assert_eq!(keys.iter().collect::<HashSet<_>>().len(), 10_000);
    let weak = fill_time::<BuildHasherDefault<SumHash>>(&keys);
    let safe = fill_time::<RandomState>(&keys);
    println!("\nHashDoS: 10k crafted keys, one byte-sum -> every key in one chain");
    println!(
        "  sum-of-bytes map: {:.1} ms (O(n^2))   RandomState (SipHash): {:.2} ms",
        weak * 1e3,
        safe * 1e3
    );
    assert!(weak > 5.0 * safe, "quadratic blowup expected");
    println!("OK");
}
