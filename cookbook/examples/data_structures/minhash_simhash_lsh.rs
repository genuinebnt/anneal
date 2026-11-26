//! RECIPE 126 (probabilistic): MinHash + banded LSH + SimHash for near-duplicate detection.
//! LEVEL: advanced
//! Use in: dedup of documents/pages, plagiarism and clone detection, similar-item search.   Run: cargo run --release --example minhash_simhash_lsh
//!
//! MINHASH: represent a document as a set of k-word shingles. For a random hash function h,
//! P[min h(A) == min h(B)] = Jaccard(A,B) exactly (the min is equally likely to be any element of
//! A u B, and it is shared only if it lies in A n B). Use n hash functions -> an n-slot signature;
//! the fraction of equal slots estimates Jaccard with std error sqrt(J(1-J)/n).
//! LSH (banding): comparing all pairs is O(N^2). Split the n = b*r signature into b bands of r rows;
//! two docs become CANDIDATES if ANY band matches entirely (bucket by band hash). With similarity s:
//!     P[candidate] = 1 - (1 - s^r)^b          (the "S-curve")
//! The threshold sits near (1/b)^(1/r): above it nearly always found, below it nearly never. More
//! rows per band = sharper & stricter; more bands = more recall & more false candidates.
//! SIMHASH: a different idea for text. Hash every token to 64 bits, add +1/-1 per bit position
//! across tokens, keep the sign of each sum. Similar docs get fingerprints with SMALL HAMMING
//! DISTANCE (cosine-like similarity), unrelated docs ~32 of 64 bits. Cheap: one u64 per document.
//!
//! COMMON USES: web crawlers dropping near-duplicate pages, news/article clustering, spam and
//! plagiarism detection, deduplicating training corpora for ML, finding similar records or
//! products, and (SimHash) near-duplicate detection in large web-scale crawls. MinHash also
//! estimates set similarity for recommendation and entity matching.
//! Read: https://en.wikipedia.org/wiki/MinHash
//! Read: https://en.wikipedia.org/wiki/Locality-sensitive_hashing
//! Read: https://en.wikipedia.org/wiki/SimHash
#![allow(dead_code)]
use std::collections::{BTreeSet, HashMap, HashSet};

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

fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn fnv(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ b as u64).wrapping_mul(0x100_0000_01b3)
    })
}

/// Text -> token hashes (lowercased words).
fn tokenize(text: &str) -> Vec<u64> {
    text.split_whitespace()
        .map(|w| fnv(&w.to_lowercase()))
        .collect()
}

/// Order-sensitive k-word shingles, each hashed to u64.
fn shingles(tokens: &[u64], k: usize) -> HashSet<u64> {
    tokens
        .windows(k)
        .map(|w| w.iter().fold(0, |a, &t| mix(a ^ t)))
        .collect()
}

fn jaccard(a: &HashSet<u64>, b: &HashSet<u64>) -> f64 {
    a.intersection(b).count() as f64 / a.union(b).count() as f64
}

struct MinHash {
    seeds: Vec<u64>,
}
impl MinHash {
    fn new(n: usize, rng: &mut Rng) -> Self {
        MinHash {
            seeds: (0..n).map(|_| rng.next()).collect(),
        }
    }
    /// Seeded XOR before a strong mixer stands in for n independent permutations.
    fn signature(&self, set: &HashSet<u64>) -> Vec<u64> {
        self.seeds
            .iter()
            .map(|&s| set.iter().map(|&x| mix(x ^ s)).min().unwrap())
            .collect()
    }
}

fn estimate_jaccard(a: &[u64], b: &[u64]) -> f64 {
    a.iter().zip(b).filter(|(x, y)| x == y).count() as f64 / a.len() as f64
}

fn s_curve(s: f64, bands: usize, rows: usize) -> f64 {
    1.0 - (1.0 - s.powi(rows as i32)).powi(bands as i32)
}

fn band_keys(sig: &[u64], rows: usize) -> Vec<u64> {
    sig.chunks(rows)
        .enumerate()
        .map(|(i, c)| c.iter().fold(mix(i as u64), |a, &v| mix(a ^ v)))
        .collect()
}

/// Bucket docs by (band index, band hash); any shared bucket makes a candidate pair.
fn candidate_pairs(sigs: &[Vec<u64>], rows: usize) -> BTreeSet<(usize, usize)> {
    let mut buckets: HashMap<(usize, u64), Vec<usize>> = HashMap::new();
    for (id, sig) in sigs.iter().enumerate() {
        for (band, key) in band_keys(sig, rows).into_iter().enumerate() {
            buckets.entry((band, key)).or_default().push(id);
        }
    }
    let mut pairs = BTreeSet::new();
    for ids in buckets.values() {
        for (i, &a) in ids.iter().enumerate() {
            for &b in &ids[i + 1..] {
                pairs.insert((a.min(b), a.max(b)));
            }
        }
    }
    pairs
}

fn simhash(tokens: &[u64]) -> u64 {
    let mut sums = [0i32; 64];
    for &t in tokens {
        let h = mix(t);
        for (bit, s) in sums.iter_mut().enumerate() {
            *s += if (h >> bit) & 1 == 1 { 1 } else { -1 };
        }
    }
    sums.iter().enumerate().fold(
        0u64,
        |fp, (bit, &s)| if s > 0 { fp | (1 << bit) } else { fp },
    )
}

fn hamming(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

/// Random document of `len` tokens from a 100k-word vocabulary.
fn random_doc(rng: &mut Rng, len: usize) -> Vec<u64> {
    (0..len).map(|_| mix(rng.below(100_000) + 7)).collect()
}

/// Copy of `doc` where each token is independently replaced with probability `rate_pct`%.
fn near_duplicate(rng: &mut Rng, doc: &[u64], rate_pct: u64) -> Vec<u64> {
    doc.iter()
        .map(|&t| {
            if rng.below(100) < rate_pct {
                mix(rng.below(100_000) + 7)
            } else {
                t
            }
        })
        .collect()
}

fn main() {
    let mut rng = Rng(0xDEAD_BEEF_CAFE_F00D);
    let (bands, rows) = (32, 4);
    let mh = MinHash::new(bands * rows, &mut rng);

    // 0) Tiny real-text demo of the pipeline.
    let t1 = tokenize("the quick brown fox jumps over the lazy dog near the river bank today");
    let t2 = tokenize("the quick brown fox jumps over the lazy cat near the river bank today");
    let (s1, s2) = (shingles(&t1, 2), shingles(&t2, 2));
    println!(
        "demo text: exact Jaccard {:.2}, simhash hamming {}",
        jaccard(&s1, &s2),
        hamming(simhash(&t1), simhash(&t2))
    );

    // 1) MinHash estimate vs exact Jaccard on documents with controlled overlap.
    let base = random_doc(&mut rng, 300);
    for rate in [2, 10, 30, 60] {
        let other = near_duplicate(&mut rng, &base, rate);
        let (a, b) = (shingles(&base, 3), shingles(&other, 3));
        let (exact, est) = (
            jaccard(&a, &b),
            estimate_jaccard(&mh.signature(&a), &mh.signature(&b)),
        );
        println!("edit rate {rate:>2}%: exact J={exact:.3} minhash est={est:.3}");
        assert!((exact - est).abs() < 0.15, "minhash off by too much");
    }

    // 2) S-curve: theory vs measured candidate rate on random set pairs of known Jaccard.
    println!(
        "S-curve for b={bands}, r={rows} (threshold ~ {:.2})",
        (1.0 / bands as f64).powf(1.0 / rows as f64)
    );
    let mut next_id = 1_000_000u64; // unique element ids so overlaps are exactly controlled
    for target in [0.2, 0.4, 0.6, 0.8] {
        let shared = (200.0f64 * target / (1.0 + target)).round() as u64; // |A|=|B|=100
        let exact_j = shared as f64 / (200 - shared) as f64;
        let trials = 300;
        let mut hits = 0;
        for _ in 0..trials {
            let a: HashSet<u64> = (0..100).map(|i| next_id + i).collect();
            let b: HashSet<u64> = (0..shared)
                .map(|i| next_id + i)
                .chain((0..100 - shared).map(|i| next_id + 100 + i))
                .collect();
            next_id += 200;
            let (sa, sb) = (mh.signature(&a), mh.signature(&b));
            hits += (0..bands).any(|i| sa[i * rows..(i + 1) * rows] == sb[i * rows..(i + 1) * rows])
                as usize;
        }
        let (measured, theory) = (hits as f64 / trials as f64, s_curve(exact_j, bands, rows));
        println!("  J={exact_j:.2}: P(candidate) theory {theory:.3} measured {measured:.3}");
        assert!((measured - theory).abs() < 0.12);
    }

    // 3) Corpus: 12 unrelated docs + near-duplicates (5% edits) of docs 0..5.
    let mut docs: Vec<Vec<u64>> = (0..12).map(|_| random_doc(&mut rng, 200)).collect();
    for i in 0..5 {
        let dup = near_duplicate(&mut rng, &docs[i], 5);
        docs.push(dup);
    }
    let sets: Vec<_> = docs.iter().map(|d| shingles(d, 3)).collect();
    let sigs: Vec<_> = sets.iter().map(|s| mh.signature(s)).collect();
    let found = candidate_pairs(&sigs, rows);
    let expected: BTreeSet<_> = (0..5).map(|i| (i, 12 + i)).collect();
    println!("LSH candidates: {found:?}");
    assert!(expected.is_subset(&found), "missed a near-duplicate");
    assert!(
        found
            .iter()
            .all(|&(a, b)| jaccard(&sets[a], &sets[b]) > 0.3),
        "unrelated pair became a candidate"
    );

    // 4) SimHash: near-duplicates are close in Hamming distance, unrelated docs are ~32 bits apart.
    let fps: Vec<u64> = docs.iter().map(|d| simhash(d)).collect();
    let near_max = (0..5).map(|i| hamming(fps[i], fps[12 + i])).max().unwrap();
    let far_min = (0..12)
        .flat_map(|a| (a + 1..12).map(move |b| (a, b)))
        .map(|(a, b)| hamming(fps[a], fps[b]))
        .min()
        .unwrap();
    println!(
        "simhash: worst near-duplicate distance {near_max}, closest unrelated pair {far_min} (of 64 bits)"
    );
    assert!(near_max <= 12 && far_min >= 16 && near_max < far_min);
}
