//! RECIPE 138 (strings): KMP, Z-algorithm, Rabin-Karp and the suffix array + Kasai LCP, with differential tests.
//! LEVEL: advanced
//! Use in: any "find / count / compare substrings" job beyond str::find, and as building blocks for text indexes.   Run: cargo run --release --example string_algos
#![allow(dead_code)]
//!
//! Two families. Single-pattern scanners (KMP, Z, Rabin-Karp) preprocess the PATTERN and scan the text once in O(n + m).
//! Indexes (suffix array + LCP) preprocess the TEXT once so that many later queries are cheap.
//!   * KMP prefix function pi[i] = length of the longest proper border of pat[..=i]; on a mismatch we fall back along
//!     borders instead of re-reading text, which is why `aaaa...ab` cannot make it quadratic.
//!   * Z-algorithm z[i] = longest common prefix of s and s[i..]; matches of P in T are the z-values >= |P| in P#T.
//!   * Rabin-Karp compares a rolling hash of each window; equal hashes are only CANDIDATES, so every hit is verified
//!     byte by byte. We use modulus 2^61-1 (fast mulmod through u128) and also a tiny modulus to show verification working.
//!   * Suffix array sorts all suffixes by prefix doubling: rank by the first 2^k bytes, then by pairs (rank[i], rank[i+2^k]).
//!     O(n log^2 n) with a comparison sort. Kasai computes LCP of neighbouring suffixes in O(n) by noticing that
//!     lcp can drop by at most 1 when moving from suffix i to suffix i+1.
//!   * With SA + LCP: longest repeated substring = max LCP; distinct substrings = n(n+1)/2 - sum(LCP);
//!     substring search = binary search over sorted suffixes, O(m log n).
//! COMMON USES: text search and grep-like tools (Z/KMP style scanners, SIMD two-way variants inside std/memchr);
//! genome indexing (suffix arrays / FM-indexes locate reads in a reference); plagiarism and duplicate detection
//! (shared long substrings = large LCP between documents); compression (the Burrows-Wheeler transform used by bzip2
//! is built from the suffix array); log deduplication via rolling hashes (content-defined chunking uses the same idea).
//! Read: https://cp-algorithms.com/string/prefix-function.html and https://cp-algorithms.com/string/suffix-array.html
use std::collections::HashSet;

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
    /// random string of length in [min, max)
    fn string(&mut self, min: u64, max: u64, alphabet: u8) -> Vec<u8> {
        let len = min + self.below(max - min);
        (0..len)
            .map(|_| b'a' + self.below(alphabet as u64) as u8)
            .collect()
    }
}

// ---------- KMP ----------

fn prefix_function(s: &[u8]) -> Vec<usize> {
    let mut pi = vec![0; s.len()];
    for i in 1..s.len() {
        let mut k = pi[i - 1];
        while k > 0 && s[i] != s[k] {
            k = pi[k - 1]; // shorter border of the border we just failed to extend
        }
        if s[i] == s[k] {
            k += 1;
        }
        pi[i] = k;
    }
    pi
}

/// Start offsets of every (possibly overlapping) occurrence of `pat` in `text`.
fn kmp_search(text: &[u8], pat: &[u8]) -> Vec<usize> {
    let pi = prefix_function(pat);
    let (mut k, mut out) = (0, vec![]);
    for (i, &c) in text.iter().enumerate() {
        while k > 0 && (k == pat.len() || c != pat[k]) {
            k = pi[k - 1];
        }
        if c == pat[k] {
            k += 1;
        }
        if k == pat.len() {
            out.push(i + 1 - k);
        }
    }
    out
}

// ---------- Z-algorithm ----------

fn z_function(s: &[u8]) -> Vec<usize> {
    let n = s.len();
    let mut z = vec![0; n];
    let (mut l, mut r) = (0, 0); // [l, r) is the rightmost known match of a prefix
    for i in 1..n {
        if i < r {
            z[i] = (r - i).min(z[i - l]); // reuse what the window already told us
        }
        while i + z[i] < n && s[z[i]] == s[i + z[i]] {
            z[i] += 1;
        }
        if i + z[i] > r {
            (l, r) = (i, i + z[i]);
        }
    }
    z
}

fn z_search(text: &[u8], pat: &[u8]) -> Vec<usize> {
    // 0 never occurs in our lowercase inputs, so no z-value can run across the separator
    let joined: Vec<u8> = pat
        .iter()
        .chain([0u8].iter())
        .chain(text)
        .copied()
        .collect();
    let z = z_function(&joined);
    (pat.len() + 1..joined.len())
        .filter(|&i| z[i] >= pat.len())
        .map(|i| i - pat.len() - 1)
        .collect()
}

// ---------- Rabin-Karp ----------

const MERSENNE61: u64 = (1 << 61) - 1;

fn mulmod(a: u64, b: u64, m: u64) -> u64 {
    (a as u128 * b as u128 % m as u128) as u64
}

/// Returns (matches, false_positives): hash hits that failed verification.
fn rabin_karp(text: &[u8], pat: &[u8], base: u64, modulus: u64) -> (Vec<usize>, usize) {
    let m = pat.len();
    if m > text.len() {
        return (vec![], 0);
    }
    let hash = |w: &[u8]| {
        w.iter()
            .fold(0, |h, &c| (mulmod(h, base, modulus) + c as u64) % modulus)
    };
    let top = (0..m - 1).fold(1, |p, _| mulmod(p, base, modulus)); // base^(m-1): weight of the byte leaving the window
    let target = hash(pat);
    let mut h = hash(&text[..m]);
    let (mut out, mut false_pos) = (vec![], 0);
    for i in 0..=text.len() - m {
        if h == target {
            if &text[i..i + m] == pat {
                out.push(i); // verified: hash equality alone never proves a match
            } else {
                false_pos += 1;
            }
        }
        if i + m < text.len() {
            let drop = mulmod(text[i] as u64, top, modulus);
            h = (mulmod((h + modulus - drop) % modulus, base, modulus) + text[i + m] as u64)
                % modulus;
        }
    }
    (out, false_pos)
}

// ---------- suffix array, Kasai ----------

fn suffix_array(s: &[u8]) -> Vec<usize> {
    let n = s.len();
    let mut sa: Vec<usize> = (0..n).collect();
    let mut rank: Vec<i64> = s.iter().map(|&c| c as i64).collect();
    let mut k = 1;
    while k < n {
        // key = (rank of first k bytes, rank of the next k bytes); -1 means "ran off the end", which sorts first
        let key = |i: usize, rank: &[i64]| (rank[i], if i + k < n { rank[i + k] } else { -1 });
        sa.sort_by_key(|&i| key(i, &rank));
        let mut next = vec![0i64; n];
        for w in 1..n {
            next[sa[w]] = next[sa[w - 1]] + i64::from(key(sa[w - 1], &rank) != key(sa[w], &rank));
        }
        rank = next;
        if rank[sa[n - 1]] as usize == n - 1 {
            break; // all ranks distinct: fully sorted
        }
        k *= 2;
    }
    sa
}

/// lcp[i] = longest common prefix of suffixes sa[i-1] and sa[i] (lcp[0] = 0).
fn kasai(s: &[u8], sa: &[usize]) -> Vec<usize> {
    let n = s.len();
    let mut rank = vec![0; n];
    for (r, &p) in sa.iter().enumerate() {
        rank[p] = r;
    }
    let (mut lcp, mut h) = (vec![0; n], 0);
    for i in 0..n {
        if rank[i] > 0 {
            let j = sa[rank[i] - 1];
            while i + h < n && j + h < n && s[i + h] == s[j + h] {
                h += 1;
            }
            lcp[rank[i]] = h;
            h = h.saturating_sub(1); // the next suffix shares at least h-1 bytes with its predecessor
        } else {
            h = 0;
        }
    }
    lcp
}

fn longest_repeated(s: &[u8], sa: &[usize], lcp: &[usize]) -> Vec<u8> {
    match (0..s.len()).max_by_key(|&i| (lcp[i], std::cmp::Reverse(sa[i]))) {
        Some(i) if lcp[i] > 0 => s[sa[i]..sa[i] + lcp[i]].to_vec(),
        _ => vec![],
    }
}

fn count_distinct(s: &[u8], lcp: &[usize]) -> usize {
    s.len() * (s.len() + 1) / 2 - lcp.iter().sum::<usize>()
}

/// All occurrences of `pat`, sorted by position: two binary searches bound the block of suffixes that start with it.
fn sa_search(s: &[u8], sa: &[usize], pat: &[u8]) -> Vec<usize> {
    let prefix = |p: usize| &s[p..(p + pat.len()).min(s.len())];
    let lo = sa.partition_point(|&p| prefix(p) < pat);
    let hi = sa.partition_point(|&p| prefix(p) <= pat);
    let mut v = sa[lo..hi].to_vec();
    v.sort_unstable();
    v
}

// ---------- oracles ----------

fn naive_find(text: &[u8], pat: &[u8]) -> Vec<usize> {
    (0..=text.len().saturating_sub(pat.len()))
        .filter(|&i| text[i..].starts_with(pat))
        .collect()
}

fn naive_distinct(s: &[u8]) -> usize {
    let mut set = HashSet::new();
    for i in 0..s.len() {
        for j in i + 1..=s.len() {
            set.insert(&s[i..j]);
        }
    }
    set.len()
}

fn naive_longest_repeated_len(s: &[u8]) -> usize {
    (1..s.len())
        .rev()
        .find(|&l| {
            s.windows(l)
                .enumerate()
                .any(|(i, w)| s[i + 1..].windows(l).any(|v| v == w))
        })
        .unwrap_or(0)
}

fn check_all(text: &[u8], pat: &[u8], sa: &[usize]) {
    let want = naive_find(text, pat);
    let std_find: Vec<usize> = (0..text.len())
        .filter(|&i| text[i..].starts_with(pat))
        .collect();
    assert_eq!(want, std_find);
    assert_eq!(kmp_search(text, pat), want, "kmp {text:?} {pat:?}");
    assert_eq!(z_search(text, pat), want, "z");
    assert_eq!(rabin_karp(text, pat, 257, MERSENNE61).0, want, "rk strong");
    assert_eq!(
        rabin_karp(text, pat, 257, 101).0,
        want,
        "rk weak modulus still correct thanks to verification"
    );
    assert_eq!(sa_search(text, sa, pat), want, "sa");
}

fn main() {
    let banana = b"banana";
    let sa = suffix_array(banana);
    let lcp = kasai(banana, &sa);
    println!("banana: SA={sa:?} LCP={lcp:?}");
    assert_eq!(sa, [5, 3, 1, 0, 4, 2]);
    assert_eq!(lcp, [0, 1, 3, 0, 0, 2]);
    println!(
        "longest repeated = {:?}, distinct substrings = {}",
        String::from_utf8(longest_repeated(banana, &sa, &lcp)).unwrap(),
        count_distinct(banana, &lcp)
    );
    assert_eq!(longest_repeated(banana, &sa, &lcp), b"ana");
    assert_eq!(count_distinct(banana, &lcp), 15);
    assert_eq!(prefix_function(b"abacabab"), [0, 0, 1, 0, 1, 2, 3, 2]);
    assert_eq!(z_function(b"aabxaab"), [0, 1, 0, 0, 3, 1, 0]);

    let mut rng = Rng(0xDEAD_BEEF_CAFE_F00D);
    for round in 0..1500 {
        let alphabet = 1 + (round % 4) as u8; // alphabet 1 is the adversarial aaaa... case
        let text = rng.string(0, 60, alphabet);
        let sa = suffix_array(&text);
        let lcp = kasai(&text, &sa);
        // SA must really be sorted, LCP must be exact
        assert!(sa.windows(2).all(|w| text[w[0]..] < text[w[1]..]));
        for i in 1..sa.len() {
            let (a, b) = (&text[sa[i - 1]..], &text[sa[i]..]);
            assert_eq!(lcp[i], a.iter().zip(b).take_while(|(x, y)| x == y).count());
        }
        assert_eq!(count_distinct(&text, &lcp), naive_distinct(&text));
        assert_eq!(
            longest_repeated(&text, &sa, &lcp).len(),
            naive_longest_repeated_len(&text)
        );
        for _ in 0..6 {
            let pat = rng.string(1, 6, alphabet);
            check_all(&text, &pat, &sa);
        }
        if text.len() > 3 {
            let at = rng.below(text.len() as u64 - 2) as usize; // pattern taken from the text: guaranteed hit
            check_all(&text, &text[at..(at + 3).min(text.len())], &sa);
        }
    }
    // adversarial runs: worst case for naive scanning, trivial for KMP/Z
    let text = vec![b'a'; 5000];
    let mut pat = vec![b'a'; 40];
    pat.push(b'b');
    assert!(kmp_search(&text, &pat).is_empty() && z_search(&text, &pat).is_empty());
    assert_eq!(kmp_search(&text, &pat[..40]).len(), 5000 - 40 + 1);
    let sa = suffix_array(&text);
    assert_eq!(sa_search(&text, &sa, &pat[..40]).len(), 4961);

    // weak modulus: many hash collisions, all rejected by verification
    let text = rng.string(20_000, 20_001, 2);
    let (hits, fp) = rabin_karp(&text, b"abbab", 3, 7);
    assert_eq!(hits, naive_find(&text, b"abbab"));
    let (_, fp_strong) = rabin_karp(&text, b"abbab", 257, MERSENNE61);
    println!(
        "rabin-karp: {} true hits, {fp} false positives with modulus 7, {fp_strong} with 2^61-1",
        hits.len()
    );
    assert!(fp > 0 && fp_strong == 0);
    println!("all string-algorithm checks passed");
}
