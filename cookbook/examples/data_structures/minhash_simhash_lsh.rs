//! RECIPE 126 (probabilistic): MinHash + banded LSH + SimHash for near-duplicate detection.
//! LEVEL: advanced
//! Use in: dedup of documents/pages, plagiarism and clone detection, similar-item search.   Run: cargo run --release --example minhash_simhash_lsh
//!
//! MINHASH: turn a document into a set of k-word shingles. Under a random ordering of all shingles, the smallest
//! shingle of A and B together is equally likely to be any of them. Both sets have the same smallest one only when it
//! is shared, so the chance of agreement is the Jaccard similarity J. A seeded 64-bit mixer only imitates a random
//! ordering, so this file measures how close it comes. With n independent slots the share of equal slots estimates J
//! with a standard error of sqrt(J(1-J)/n).
//!
//! BANDING (LSH): cut the signature into b bands of r rows. Two documents are candidates if any band is wholly equal.
//! For similarity s the chance is 1 - (1 - s^r)^b. Candidates still have to be checked with the exact similarity.
//!
//! SIMHASH: one 64-bit fingerprint per document. Each token adds +1 or -1 to each of 64 sums, and the signs are the
//! fingerprint. Similar token counts give fingerprints that differ in few bits. A sum of zero counts as a 0 bit, which
//! pulls short documents with an even number of tokens towards each other. A few very frequent words can decide
//! every bit, so that unrelated documents look alike.
//!
//! COMMON USES: web crawlers dropping near-duplicate pages, news clustering, spam and plagiarism detection, removing
//! repeats from training data, and proposing candidate pairs for record matching.
//! Read: https://research.google.com/pubs/archive/33026.pdf
//! Read: https://en.wikipedia.org/wiki/MinHash
//! Read: https://en.wikipedia.org/wiki/SimHash
#![allow(dead_code)]
use std::collections::{BTreeSet, HashMap, HashSet};
use std::f64::consts::PI;

/// xorshift64, for repeatable test data.
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

/// The SplitMix64 step: it changes about half the output bits when one input bit changes.
fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// FNV-1a over bytes. Fine for a demo; anyone who chooses the input can force collisions.
fn fnv(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ b as u64).wrapping_mul(0x100_0000_01b3)
    })
}

/// Lowercased runs of letters and digits, hashed. Punctuation does not stick to words.
fn tokens(text: &str) -> Vec<u64> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| fnv(&w.to_lowercase()))
        .collect()
}

/// Every run of k tokens, each hashed in order. A text shorter than k has no shingles. `windows` panics if k is 0.
fn shingles(tokens: &[u64], k: usize) -> HashSet<u64> {
    tokens
        .windows(k)
        .map(|w| w.iter().fold(0, |a, &t| mix(a ^ t)))
        .collect()
}

/// Shared items over all items. Two empty sets have no similarity to report.
fn jaccard(a: &HashSet<u64>, b: &HashSet<u64>) -> Option<f64> {
    let shared = a.intersection(b).count();
    let all = a.len() + b.len() - shared;
    (all > 0).then(|| shared as f64 / all as f64)
}

struct MinHash {
    seeds: Vec<u64>,
}

impl MinHash {
    fn new(n: usize, rng: &mut Rng) -> Self {
        assert!(n > 0, "a signature needs at least one slot");
        MinHash {
            seeds: (0..n).map(|_| rng.next()).collect(),
        }
    }

    /// The smallest hash per seed. An empty set has no smallest item, so it has no signature.
    fn signature(&self, set: &HashSet<u64>) -> Option<Vec<u64>> {
        self.seeds
            .iter()
            .map(|&seed| set.iter().map(|&x| mix(x ^ seed)).min())
            .collect()
    }
}

/// The share of equal slots. Both signatures must come from the same `MinHash`.
fn estimate_jaccard(a: &[u64], b: &[u64]) -> f64 {
    assert!(
        a.len() == b.len() && !a.is_empty(),
        "signatures differ in length"
    );
    a.iter().zip(b).filter(|(x, y)| x == y).count() as f64 / a.len() as f64
}

#[derive(Clone, Copy)]
struct Bands {
    bands: usize,
    rows: usize,
}

impl Bands {
    /// The chance that two sets of similarity `s` share at least one band, for ideal hashes.
    fn chance(self, s: f64) -> f64 {
        1.0 - (1.0 - s.powi(self.rows as i32)).powi(self.bands as i32)
    }

    /// The similarity at which that chance is one half.
    fn half_point(self) -> f64 {
        (1.0 - 0.5f64.powf(1.0 / self.bands as f64)).powf(1.0 / self.rows as f64)
    }

    /// One bucket key per band: the band's number and its rows, hashed.
    fn keys(self, signature: &[u64]) -> Vec<(usize, u64)> {
        assert_eq!(
            signature.len(),
            self.bands * self.rows,
            "signature does not fit the bands"
        );
        signature
            .chunks(self.rows)
            .map(|rows| rows.iter().fold(0, |a, &v| mix(a ^ v)))
            .enumerate()
            .collect()
    }
}

/// Put each document in one bucket per band; documents that meet in a bucket are a candidate pair.
/// A bucket of m documents makes m(m-1)/2 pairs, so a very common text needs its own handling.
fn candidate_pairs(signatures: &[Option<Vec<u64>>], shape: Bands) -> BTreeSet<(usize, usize)> {
    let mut buckets: HashMap<(usize, u64), Vec<usize>> = HashMap::new();
    let mut pairs = BTreeSet::new();
    for (id, signature) in signatures.iter().enumerate() {
        let Some(signature) = signature else {
            continue;
        };
        for key in shape.keys(signature) {
            let bucket = buckets.entry(key).or_default();
            pairs.extend(bucket.iter().map(|&earlier| (earlier, id)));
            bucket.push(id);
        }
    }
    pairs
}

/// Candidates whose exact similarity reaches `at_least`.
fn near_duplicates(
    sets: &[HashSet<u64>],
    candidates: &BTreeSet<(usize, usize)>,
    at_least: f64,
) -> Vec<(usize, usize, f64)> {
    candidates
        .iter()
        .filter_map(|&(a, b)| {
            let j = jaccard(&sets[a], &sets[b])?;
            (j >= at_least).then_some((a, b, j))
        })
        .collect()
}

/// A repeated token votes once per time it appears. No tokens means no fingerprint.
fn simhash(tokens: &[u64]) -> Option<u64> {
    if tokens.is_empty() {
        return None;
    }
    let mut sums = [0i64; 64];
    for &token in tokens {
        let hash = mix(token);
        for (bit, sum) in sums.iter_mut().enumerate() {
            *sum += if (hash >> bit) & 1 == 1 { 1 } else { -1 };
        }
    }
    Some(
        sums.iter()
            .enumerate()
            .filter(|&(_, &sum)| sum > 0)
            .fold(0, |fingerprint, (bit, _)| fingerprint | (1 << bit)),
    )
}

/// In how many bit positions two fingerprints differ.
fn hamming(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

// ---- oracles, written without the code above ----

/// Jaccard by looking through plain lists.
fn naive_jaccard(a: &[u64], b: &[u64]) -> f64 {
    let mut all: Vec<u64> = a.iter().chain(b).copied().collect();
    all.sort_unstable();
    all.dedup();
    let shared = all
        .iter()
        .filter(|x| a.contains(x) && b.contains(x))
        .count();
    shared as f64 / all.len() as f64
}

/// Every pair, compared band by band.
fn naive_candidates(signatures: &[Option<Vec<u64>>], rows: usize) -> BTreeSet<(usize, usize)> {
    let mut pairs = BTreeSet::new();
    for a in 0..signatures.len() {
        for b in a + 1..signatures.len() {
            let (Some(x), Some(y)) = (&signatures[a], &signatures[b]) else {
                continue;
            };
            if x.chunks(rows).zip(y.chunks(rows)).any(|(p, q)| p == q) {
                pairs.insert((a, b));
            }
        }
    }
    pairs
}

/// The chance that one SimHash bit differs, if every token's bit is a fair coin. Two documents of distinct
/// tokens: `shared` in both, `own` more in each. This counts every case, including sums of zero.
fn simhash_bit_chance(shared: u32, own: u32) -> f64 {
    // chance[k]: k of n coins are heads, so their sum of +1 and -1 is 2k - n
    fn coins(n: u32) -> Vec<f64> {
        (0..n).fold(vec![1.0], |previous, _| {
            let mut next = vec![0.0; previous.len() + 1];
            for (k, p) in previous.iter().enumerate() {
                next[k] += p / 2.0;
                next[k + 1] += p / 2.0;
            }
            next
        })
    }
    let sum = |k: usize, n: u32| 2 * k as i64 - n as i64;
    let (common, private) = (coins(shared), coins(own));
    common
        .iter()
        .enumerate()
        .map(|(k, p)| {
            // given the shared sum, each document's bit is 1 with the same chance, independently
            let one: f64 = private
                .iter()
                .enumerate()
                .filter(|&(m, _)| sum(k, shared) + sum(m, own) > 0)
                .map(|(_, q)| q)
                .sum();
            p * 2.0 * one * (1.0 - one)
        })
        .sum()
}

// ---- test data ----

/// Two sets with exactly `shared` common items and `own` more each, never reused.
fn set_pair(next_id: &mut u64, shared: u64, own: u64) -> (HashSet<u64>, HashSet<u64>) {
    let at = *next_id;
    *next_id += shared + 2 * own;
    let a = (at..at + shared + own).collect();
    let b = (at..at + shared)
        .chain(at + shared + own..at + shared + 2 * own)
        .collect();
    (a, b)
}

fn random_doc(rng: &mut Rng, len: usize) -> Vec<u64> {
    (0..len).map(|_| mix(rng.below(100_000) + 7)).collect()
}

/// A copy in which each token is replaced with the given chance.
fn edited_copy(rng: &mut Rng, doc: &[u64], percent: u64) -> Vec<u64> {
    doc.iter()
        .map(|&t| {
            if rng.below(100) < percent {
                mix(rng.below(100_000) + 7)
            } else {
                t
            }
        })
        .collect()
}

// ---- checks ----

/// Whether a call stops with a panic. The panic message is not printed.
fn panics<T>(call: impl FnOnce() -> T) -> bool {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(call)).is_err();
    std::panic::set_hook(hook);
    failed
}

fn check_input_edges() {
    // equal rows in DIFFERENT bands are not a match, and a document never pairs with itself
    let two = Bands { bands: 2, rows: 2 };
    let moved = [Some(vec![1, 2, 3, 4]), Some(vec![3, 4, 1, 2])];
    assert!(candidate_pairs(&moved, two).is_empty(), "met across bands");
    assert!(candidate_pairs(&[Some(vec![1, 2, 1, 2])], two).is_empty());

    // settings that would give a quiet wrong answer are refused
    let shape = Bands { bands: 4, rows: 2 };
    assert!(
        panics(|| shape.keys(&[0; 9])),
        "9 slots do not fit 4 bands of 2"
    );
    assert!(
        panics(|| estimate_jaccard(&[1, 2], &[1])),
        "unequal signatures"
    );
    assert!(panics(|| shingles(&[1], 0)));
    assert!(panics(|| MinHash::new(0, &mut Rng(1))));

    assert_eq!(tokens("The dog. THE, dog!"), tokens("the dog the dog"));
    assert!(tokens("  ?! -- ").is_empty());
    assert!(shingles(&tokens("two words"), 3).is_empty());
    assert_eq!(shingles(&tokens("one two three"), 3).len(), 1);
    assert_ne!(
        shingles(&tokens("dog bites man"), 3),
        shingles(&tokens("man bites dog"), 3)
    );

    let mut rng = Rng(1);
    let hashes = MinHash::new(8, &mut rng);
    let empty = HashSet::new();
    assert_eq!(hashes.signature(&empty), None);
    assert_eq!(jaccard(&empty, &empty), None);
    assert_eq!(jaccard(&empty, &HashSet::from([1])), Some(0.0));
    assert_eq!(simhash(&[]), None);
    // two documents without tokens must not meet in a bucket
    assert!(candidate_pairs(&[None, None], shape).is_empty());

    // hostile text: very long, no separators, and letters that grow when lowercased
    assert_eq!(tokens(&"a".repeat(1_000_000)).len(), 1);
    assert_eq!(tokens(&"İ,".repeat(10_000)).len(), 10_000);
    let alphabet = ['a', 'B', ' ', '.', 'é', 'İ', '漢', '\u{301}', '\0', '🦀'];
    for _ in 0..2_000 {
        let text: String = (0..rng.below(40))
            .map(|_| alphabet[rng.below(10) as usize])
            .collect();
        let found = tokens(&text);
        let set = shingles(&found, 3);
        assert_eq!(set.is_empty(), found.len() < 3, "{text:?}");
        assert_eq!(
            hashes.signature(&set).is_some(),
            !set.is_empty(),
            "{text:?}"
        );
        assert_eq!(simhash(&found).is_some(), !found.is_empty(), "{text:?}");
    }
}

fn check_against_naive() {
    let mut rng = Rng(2);
    let mut met = 0;
    for _ in 0..300 {
        // a tiny universe, so that bands really do collide
        let sets: Vec<HashSet<u64>> = (0..12)
            .map(|_| (0..rng.below(7)).map(|_| rng.below(9)).collect())
            .collect();
        let shape = Bands {
            bands: 1 + rng.below(5) as usize,
            rows: 1 + rng.below(3) as usize,
        };
        let hashes = MinHash::new(shape.bands * shape.rows, &mut rng);
        let signatures: Vec<_> = sets.iter().map(|s| hashes.signature(s)).collect();
        let found = candidate_pairs(&signatures, shape);
        assert_eq!(found, naive_candidates(&signatures, shape.rows));
        met += found.len();

        let lists: Vec<Vec<u64>> = sets.iter().map(|s| s.iter().copied().collect()).collect();
        for a in 0..sets.len() {
            if let Some(j) = jaccard(&sets[a], &sets[0]) {
                assert!((j - naive_jaccard(&lists[a], &lists[0])).abs() < 1e-12);
            }
        }
        for (a, b, j) in near_duplicates(&sets, &found, 0.5) {
            assert!(j >= 0.5 && found.contains(&(a, b)));
        }
    }
    assert!(met > 500, "the random cases hardly ever collided");
}

/// The mean and spread of the estimate over many fresh sets and fresh seeds.
fn measure_estimate(rng: &mut Rng, slots: usize, shared: u64, own: u64) -> (f64, f64, f64) {
    const TRIALS: usize = 1_500;
    let truth = shared as f64 / (shared + 2 * own) as f64;
    let mut next_id = 0;
    let estimates: Vec<f64> = (0..TRIALS)
        .map(|_| {
            let (a, b) = set_pair(&mut next_id, shared, own);
            let hashes = MinHash::new(slots, rng);
            estimate_jaccard(
                &hashes.signature(&a).unwrap(),
                &hashes.signature(&b).unwrap(),
            )
        })
        .collect();
    let mean = estimates.iter().sum::<f64>() / TRIALS as f64;
    let spread =
        (estimates.iter().map(|e| (e - truth).powi(2)).sum::<f64>() / TRIALS as f64).sqrt();
    (truth, mean, spread)
}

fn check_estimate_error() {
    let mut rng = Rng(0xDEAD_BEEF_CAFE_F00D);
    println!("MinHash estimate over 1,500 fresh trials:");
    println!("  slots  true J   mean     spread   formula");
    // sets of consecutive numbers: regular input that a weak hash would handle badly
    for (shared, own) in [(10, 45), (30, 35), (50, 25), (80, 10)] {
        for slots in [32, 128] {
            let (truth, mean, spread) = measure_estimate(&mut rng, slots, shared, own);
            let formula = (truth * (1.0 - truth) / slots as f64).sqrt();
            println!("  {slots:>5}  {truth:.3}    {mean:.3}    {spread:.4}   {formula:.4}");
            assert!(
                (mean - truth).abs() < 4.0 * formula / 1_500f64.sqrt(),
                "the estimate is biased"
            );
            // the spread of 1,500 trials is itself known to a few percent
            assert!(
                (spread / formula - 1.0).abs() < 0.08,
                "the error does not match the formula"
            );
        }
    }
}

fn check_banding() {
    let mut rng = Rng(3);
    const TRIALS: usize = 1_500;
    println!("Chance of becoming candidates (formula / measured over 1,500 pairs):");
    let mut next_id = 0;
    for shape in [Bands { bands: 32, rows: 4 }, Bands { bands: 16, rows: 8 }] {
        println!(
            "  b={} r={}: half at s={:.3}",
            shape.bands,
            shape.rows,
            shape.half_point()
        );
        assert!((shape.chance(shape.half_point()) - 0.5).abs() < 1e-9);
        for (shared, own) in [(20, 40), (30, 35), (40, 30), (60, 20), (80, 10)] {
            let s = shared as f64 / (shared + 2 * own) as f64;
            let hits = (0..TRIALS)
                .filter(|_| {
                    let (a, b) = set_pair(&mut next_id, shared, own);
                    let hashes = MinHash::new(shape.bands * shape.rows, &mut rng);
                    let signatures = [hashes.signature(&a), hashes.signature(&b)];
                    !candidate_pairs(&signatures, shape).is_empty()
                })
                .count();
            let (measured, formula) = (hits as f64 / TRIALS as f64, shape.chance(s));
            println!("    s={s:.1}: {formula:.3} / {measured:.3}");
            let allowed = 4.0 * (formula * (1.0 - formula) / TRIALS as f64).sqrt() + 0.002;
            assert!((measured - formula).abs() < allowed, "off the curve at {s}");
        }
    }
}

/// The share of differing fingerprint bits over many fresh pairs of distinct tokens.
fn measure_simhash(next_id: &mut u64, shared: u64, own: u64, trials: u32) -> f64 {
    let differing: u32 = (0..trials)
        .map(|_| {
            let (a, b) = set_pair(next_id, shared, own);
            let (a, b): (Vec<u64>, Vec<u64>) = (a.into_iter().collect(), b.into_iter().collect());
            hamming(simhash(&a).unwrap(), simhash(&b).unwrap())
        })
        .sum();
    differing as f64 / (64 * trials) as f64
}

fn check_simhash() {
    // 2 shared and 2 own tokens each, worked out by hand: 1/2 * 2*(1/4)(3/4) + 1/4 * 2*(3/4)(1/4) = 9/32
    assert!((simhash_bit_chance(2, 2) - 9.0 / 32.0).abs() < 1e-12);
    assert!((simhash_bit_chance(0, 1) - 0.5).abs() < 1e-12);

    let mut next_id = 0;
    println!("SimHash: differing bits out of 64 (angle rule / exact count / measured):");
    for (shared, own, trials) in [
        (0, 4, 4_000),
        (0, 5, 4_000),
        (2, 2, 4_000),
        (0, 200, 400),
        (100, 100, 400),
        (180, 20, 400),
    ] {
        // for sets of distinct tokens, the cosine is shared / size
        let angle_rule = (shared as f64 / (shared + own) as f64).acos() / PI;
        let exact = simhash_bit_chance(shared as u32, own as u32);
        let measured = measure_simhash(&mut next_id, shared, own, trials);
        println!(
            "  shared {shared:>3}, own {own:>3}: {:>5.1} / {:>5.1} / {:>5.1}",
            64.0 * angle_rule,
            64.0 * exact,
            64.0 * measured
        );
        // 5 standard errors; a long document's bits are not quite independent of each other
        let allowed = 5.0 * (0.25 / (64 * trials) as f64).sqrt() + 0.001;
        assert!(
            (measured - exact).abs() < allowed,
            "measured and counted differ"
        );
    }

    // word order does not reach the fingerprint, and a repeated word votes again
    assert_eq!(
        simhash(&tokens("dog bites man")),
        simhash(&tokens("man bites dog"))
    );
    let mostly_spam = tokens("spam spam spam spam spam eggs ham");
    assert_eq!(simhash(&mostly_spam), simhash(&tokens("spam")));
    // two tokens: where their bits differ the sum is 0, and that gives a 0 bit
    assert_eq!(simhash(&[1, 2]), Some(mix(1) & mix(2)));
}

/// Differing bits predicted by the angle between two lists of word counts, when the documents share the words
/// counted in `shared` and each also has `own` words of its own, used once each.
fn angle_rule_bits(shared: &[u64], own: u64) -> f64 {
    let dot: f64 = shared.iter().map(|&c| (c * c) as f64).sum();
    64.0 * (dot / (dot + own as f64)).acos() / PI
}

/// The mean differing bits for such pairs of documents.
fn measure_weighted(next_id: &mut u64, shared: &[u64], own: u64) -> f64 {
    const TRIALS: u32 = 400;
    let differing: u32 = (0..TRIALS)
        .map(|_| {
            let at = *next_id;
            *next_id += shared.len() as u64 + 2 * own;
            let common: Vec<u64> = shared
                .iter()
                .enumerate()
                .flat_map(|(word, &count)| (0..count).map(move |_| at + word as u64))
                .collect();
            let start = at + shared.len() as u64;
            let document = |from: u64| -> Vec<u64> {
                common.iter().copied().chain(from..from + own).collect()
            };
            let (a, b) = (document(start), document(start + own));
            hamming(simhash(&a).unwrap(), simhash(&b).unwrap())
        })
        .sum();
    differing as f64 / TRIALS as f64
}

/// A few heavy words: the +1 and -1 sums no longer follow the angle rule.
fn check_dominant_words() {
    let mut next_id = 1 << 32;
    // 20 shared words with counts 100, 50, 33, ... 5, much as in natural text
    let skewed: Vec<u64> = (1..=20).map(|rank| 100 / rank).collect();
    println!("Unrelated except for common words, 200 own words each (angle rule / measured bits):");
    let mut results = Vec::new();
    for (name, shared) in [
        ("one word 100 times", vec![100]),
        ("20 skewed words", skewed),
    ] {
        let (rule, measured) = (
            angle_rule_bits(&shared, 200),
            measure_weighted(&mut next_id, &shared, 200),
        );
        println!("  {name}: {rule:.1} / {measured:.1}");
        results.push((rule, measured));
    }
    let (rule, measured) = results[0];
    assert!(rule > 4.0 && measured < 0.1, "one word decides every bit");
    assert!(results[1].1 < 6.0, "unrelated documents still look close");
}

/// Twelve unrelated documents and edited copies of the first five.
fn check_corpus() {
    let mut rng = Rng(4);
    let shape = Bands { bands: 32, rows: 4 };
    let hashes = MinHash::new(shape.bands * shape.rows, &mut rng);
    let mut docs: Vec<Vec<u64>> = (0..12).map(|_| random_doc(&mut rng, 200)).collect();
    for i in 0..5 {
        let copy = edited_copy(&mut rng, &docs[i], 5);
        docs.push(copy);
    }
    let sets: Vec<_> = docs.iter().map(|d| shingles(d, 3)).collect();
    let signatures: Vec<_> = sets.iter().map(|s| hashes.signature(s)).collect();
    let candidates = candidate_pairs(&signatures, shape);
    let confirmed = near_duplicates(&sets, &candidates, 0.5);
    let pairs: Vec<_> = confirmed.iter().map(|&(a, b, _)| (a, b)).collect();
    assert_eq!(pairs, (0..5).map(|i| (i, 12 + i)).collect::<Vec<_>>());
    for &(a, b, j) in &confirmed {
        println!("  documents {a} and {b}: Jaccard {j:.3}");
    }
    println!(
        "Corpus: {} candidates out of {} pairs, {} confirmed",
        candidates.len(),
        17 * 16 / 2,
        confirmed.len()
    );
}

fn main() {
    // a first look: one changed word
    let a = tokens("The library opens at nine and closes at five on every weekday.");
    let b = tokens("The library opens at nine and closes at six on every weekday.");
    let (x, y) = (shingles(&a, 3), shingles(&b, 3));
    let mut rng = Rng(5);
    let hashes = MinHash::new(128, &mut rng);
    let estimate = estimate_jaccard(
        &hashes.signature(&x).unwrap(),
        &hashes.signature(&y).unwrap(),
    );
    println!(
        "One word changed: Jaccard {:.3}, estimate {estimate:.3}, SimHash bits {}",
        jaccard(&x, &y).unwrap(),
        hamming(simhash(&a).unwrap(), simhash(&b).unwrap())
    );

    check_input_edges();
    check_against_naive();
    check_estimate_error();
    check_banding();
    check_simhash();
    check_dominant_words();
    check_corpus();
    println!("All checks passed.");
}
