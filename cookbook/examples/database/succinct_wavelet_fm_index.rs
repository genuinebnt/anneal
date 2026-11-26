//! RECIPE 845 (database): succinct data structures: rank/select bitvector, wavelet matrix, BWT, FM-index, Elias-Fano.
//! LEVEL: advanced
//! Use in: full-text and genome search in compressed space, column stores with compressed posting lists, any "index smaller than the data".   Run: cargo run --release --example succinct_wavelet_fm_index
//! COMMON USES: bowtie/BWA read aligners (FM-index over a genome), the `sdsl-lite` library, Elias-Fano posting lists in search
//! engines (PISA and the Partitioned Elias-Fano line of work), wavelet trees for rank/quantile queries over
//! sequences and sorted columns, succinct tries and graph representations, bzip2 (BWT as the transform).
//! Course: Navarro "Compact Data Structures" (the book); Ferragina-Manzini "Opportunistic Data Structures with Applications".
//!
//! WHAT. A SUCCINCT structure stores data in close to its information-theoretic minimum AND still answers queries in place.
//! Building blocks, bottom up:
//!   1. RANK/SELECT BITVECTOR. rank1(i) = ones in [0,i); select1(k) = position of the k-th one. Two-level sampling: every 2048 bits a
//!      u32 cumulative count (superblock), every 256 bits a u16 count RELATIVE to its superblock (so 16 bits suffice), then at most 3
//!      full words plus a masked partial word are popcounted. Extra space ~7.8% of the bits, rank in O(1). select = binary search over
//!      superblocks, scan <= 8 blocks, scan <= 4 words, pick the bit inside a word (O(log n) here; a sampled-select directory makes
//!      it O(1) practically).
//!   2. WAVELET MATRIX over a sequence of symbols from an alphabet of sigma values: one bitvector per bit LEVEL (MSB first); at each
//!      level the sequence is stably partitioned by that bit (zeros first), remembering `zeros[level]` = number of zeros. Navigation
//!      uses only rank on the level bitvectors: bit 0 goes to rank0(i), bit 1 goes to zeros + rank1(i). From that: access(i),
//!      rank(c,i), select(c,k), the k-th smallest in a range (QUANTILE, hence median), and count of values in [lo,hi) over a position
//!      range, all in O(log sigma) time and n*ceil(log2 sigma) bits (+ overhead). The matrix variant keeps one contiguous bitvector
//!      per level and no pointers, which is why it is faster and simpler than the classic wavelet TREE.
//!   3. BURROWS-WHEELER TRANSFORM. Append a unique smallest sentinel `\0`, sort all suffixes (suffix array SA, built here by prefix
//!      doubling), and write BWT[i] = T[SA[i]-1]. Equal contexts end up adjacent, which compresses well (bzip2) and, more importantly,
//!      gives the LF-MAPPING: the character at BWT row i is the character that PRECEDES row i's suffix, and its rank among equal
//!      characters is preserved when we move to the row of the preceding suffix: LF(i) = C[c] + rank_c(BWT, i), where C[c] counts
//!      characters smaller than c. Iterating LF walks the text backwards, so the BWT inverts without storing the text.
//!   4. FM-INDEX = wavelet matrix over the BWT + C array. BACKWARD SEARCH: for the pattern's characters from the LAST to the first,
//!      keep the row range [l,r) of suffixes starting with the processed suffix of the pattern: l = C[c] + rank_c(l), r = C[c] + rank_c(r).
//!      Empty range = no match; otherwise count = r-l, in O(|P| log sigma) with no access to the text. LOCATE needs positions: store
//!      SA[row] only for rows whose text position is a multiple of `rate`, then walk LF from any row until a sampled row is hit
//!      (<= rate-1 steps) and add the steps. `rate` trades space (32 bits per sample) against locate time.
//!   5. ELIAS-FANO encodes a monotone sequence of m values below u in about m*(2 + log2(u/m)) bits: split each value into low
//!      l = floor(log2(u/m)) bits (stored verbatim in a packed array) and high bits (stored as a unary-coded BITVECTOR where the i-th one
//!      sits at position (v_i >> l) + i). access(i) = (select1(i) - i) << l | low_i; next_geq(x) uses select0 to jump straight to the
//!      bucket of x's high part. This is how huge sorted id lists (inverted index postings) are kept compressed yet randomly accessible.
//!
//! Alphabet remapping: the FM-index maps the bytes actually present to dense ids so the wavelet matrix has ceil(log2 sigma) levels
//! (DNA: 5 symbols with the sentinel -> 3 bits per symbol, not 8). This implementation uses PLAIN bitvectors, so it stores
//! n*ceil(log2 sigma) bits, above the entropy n*H0; swapping in RRR/entropy-compressed bitvectors or Huffman-shaped wavelets is
//! what reaches n*H0 (and n*H_k with a BWT) in the literature. The report prints that gap honestly.
//!
//! CHECKS. 2000 random strings (alphabets of size 1..26, lengths 1..300, sample rates 1/4/16) and random present/absent patterns:
//! FM `count` and `locate` equal a naive scan; 2000 random sequences: wavelet access/rank/select/quantile/range_count equal brute
//! force; random bitvectors (all-zero, all-one, lengths not multiples of 64) equal naive rank/select0/select1; BWT equals the sorted
//! rotations and inverts; 2000 random Elias-Fano sequences (empty, duplicates, dense, sparse): access and next_geq equal a plain Vec.
//! Space (bytes, bits per character) for DNA-like and English-like text vs plain text, suffix array and entropy baselines is printed.
//! Read: Navarro, "Compact Data Structures: A Practical Approach" https://users.dcc.uchile.cl/~gnavarro/CDSbook/
//!       Ferragina, Manzini, "Opportunistic Data Structures with Applications" FOCS 2000 (cited by title; URL not checked)
//!       Claude, Navarro, Ordonez, "The wavelet matrix" (cited by title); FM-index https://en.wikipedia.org/wiki/FM-index
//!       Vigna, "Quasi-succinct indices" (Elias-Fano for inverted indexes) https://arxiv.org/abs/1206.4300
//!
//! TRY NEXT:
//!   - [S] Add `extract(start, len)` to `FmIndex` (display text without storing it) by also sampling the inverse SA; check against the original string.
//!   - [S] Predict how `bits_per_char` changes if you build `WaveletMatrix` over the 256-symbol byte alphabet for the DNA text (levels = 8), then measure.
//!   - [M] Give `RankSelect` a select sample every 4096 ones to make `select1` O(1)-ish; confirm identical answers with the naive oracle in `check_bitvector`.
//!   - [M] Support `count` of a pattern with a `?` wildcard by branching the backward search over every symbol at that position; compare with a naive regex-free scan.
//!   - [M] Port: index the lines of a log file with `FmIndex`, then find all lines containing a substring using `locate` plus an Elias-Fano-coded line-start array (`next_geq` maps an offset to a line).
//!   - [A] Project: combine with `bitset_rank_select_roaring` and `trie_radix` (or `aho_corasick`): build a compressed suffix-array-backed dictionary and
//!     compare space and query time for substring search against the trie-based approaches.
#![allow(dead_code)]

use std::collections::HashMap;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

// ---------------- 1. rank/select bitvector ----------------
const SUPER: usize = 2048; // bits per superblock
const BLK: usize = 256; // bits per block (4 words)
struct RankSelect {
    words: Vec<u64>,
    n: usize,
    sup: Vec<u32>, // ones before each superblock
    blk: Vec<u16>, // ones between the superblock start and each block
    ones: usize,
}
impl RankSelect {
    fn from_bits(bits: impl Iterator<Item = bool>) -> Self {
        let mut words = vec![];
        let mut n = 0;
        for b in bits {
            if n % 64 == 0 {
                words.push(0u64);
            }
            if b {
                *words.last_mut().unwrap() |= 1 << (n % 64);
            }
            n += 1;
        }
        let (nsup, nblk) = (n / SUPER + 1, n / BLK + 1);
        let (mut sup, mut blk) = (vec![0u32; nsup], vec![0u16; nblk]);
        let mut total = 0usize;
        for b in 0..nblk {
            if (b * BLK) % SUPER == 0 {
                sup[b * BLK / SUPER] = total as u32;
            }
            blk[b] = (total - sup[b * BLK / SUPER] as usize) as u16;
            for w in b * 4..(b * 4 + 4).min(words.len()) {
                total += words[w].count_ones() as usize;
            }
        }
        RankSelect {
            words,
            n,
            sup,
            blk,
            ones: total,
        }
    }
    fn get(&self, i: usize) -> bool {
        self.words[i / 64] >> (i % 64) & 1 == 1
    }
    fn rank1(&self, i: usize) -> usize {
        debug_assert!(i <= self.n);
        let mut r = self.sup[i / SUPER] as usize + self.blk[i / BLK] as usize;
        for w in (i / BLK * 4)..(i / 64) {
            r += self.words[w].count_ones() as usize;
        }
        if i % 64 > 0 {
            r += (self.words[i / 64] & ((1u64 << (i % 64)) - 1)).count_ones() as usize;
        }
        r
    }
    fn rank0(&self, i: usize) -> usize {
        i - self.rank1(i)
    }
    /// Shared select: `count_before(sb/block)` is ones (or zeros) before a boundary; `bit_in_word` picks the k-th wanted bit.
    fn select(&self, k: usize, want_one: bool) -> usize {
        let before = |bits_before: usize, ones_before: usize| {
            if want_one {
                ones_before
            } else {
                bits_before - ones_before
            }
        };
        let total = if want_one {
            self.ones
        } else {
            self.n - self.ones
        };
        assert!(k < total, "select out of range");
        // binary search for the largest superblock whose count-before <= k
        let (mut lo, mut hi) = (0, self.sup.len());
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if before(mid * SUPER, self.sup[mid] as usize) <= k {
                lo = mid
            } else {
                hi = mid
            }
        }
        let sb = lo;
        let mut k = k - before(sb * SUPER, self.sup[sb] as usize);
        let first_blk = sb * SUPER / BLK;
        let mut bk = first_blk;
        while bk + 1 < (first_blk + SUPER / BLK).min(self.blk.len())
            && before((bk + 1 - first_blk) * BLK, self.blk[bk + 1] as usize) <= k
        {
            bk += 1;
        }
        k -= before((bk - first_blk) * BLK, self.blk[bk] as usize);
        let mut w = bk * 4;
        loop {
            let word = if want_one {
                self.words[w]
            } else {
                !self.words[w]
            };
            let c = word.count_ones() as usize;
            if k < c {
                let mut x = word;
                for _ in 0..k {
                    x &= x - 1; // clear the lowest set bit
                }
                return w * 64 + x.trailing_zeros() as usize;
            }
            k -= c;
            w += 1;
        }
    }
    fn select1(&self, k: usize) -> usize {
        self.select(k, true)
    }
    fn select0(&self, k: usize) -> usize {
        self.select(k, false)
    }
    fn size_bytes(&self) -> usize {
        self.words.len() * 8 + self.sup.len() * 4 + self.blk.len() * 2
    }
}

// ---------------- 2. wavelet matrix ----------------
struct WaveletMatrix {
    levels: usize,
    n: usize,
    bv: Vec<RankSelect>,
    zeros: Vec<usize>,
}
impl WaveletMatrix {
    fn new(data: &[u32], levels: usize) -> Self {
        assert!(data.iter().all(|&v| (v as u64) < 1 << levels));
        let mut cur = data.to_vec();
        let (mut bv, mut zeros) = (vec![], vec![]);
        for l in 0..levels {
            let bit = levels - 1 - l;
            bv.push(RankSelect::from_bits(cur.iter().map(|v| v >> bit & 1 == 1)));
            let (z, o): (Vec<u32>, Vec<u32>) = cur.iter().partition(|&&v| v >> bit & 1 == 0);
            zeros.push(z.len());
            cur = z.into_iter().chain(o).collect(); // stable partition: zeros first
        }
        WaveletMatrix {
            levels,
            n: data.len(),
            bv,
            zeros,
        }
    }
    fn bit(&self, c: u32, l: usize) -> bool {
        c >> (self.levels - 1 - l) & 1 == 1
    }
    fn step(&self, l: usize, b: bool, i: usize) -> usize {
        if b {
            self.zeros[l] + self.bv[l].rank1(i)
        } else {
            self.bv[l].rank0(i)
        }
    }
    fn access(&self, mut i: usize) -> u32 {
        let mut c = 0;
        for l in 0..self.levels {
            let b = self.bv[l].get(i);
            c |= (b as u32) << (self.levels - 1 - l);
            i = self.step(l, b, i);
        }
        c
    }
    /// Occurrences of `c` in positions [0,i).
    fn rank(&self, c: u32, i: usize) -> usize {
        let (mut s, mut e) = (0, i);
        for l in 0..self.levels {
            let b = self.bit(c, l);
            s = self.step(l, b, s);
            e = self.step(l, b, e);
        }
        e - s
    }
    /// Position of the k-th (0-based) occurrence of `c`, if any.
    fn select(&self, c: u32, k: usize) -> Option<usize> {
        if k >= self.rank(c, self.n) {
            return None;
        }
        let mut s = 0;
        for l in 0..self.levels {
            s = self.step(l, self.bit(c, l), s);
        }
        let mut p = s + k; // position of that occurrence in the bottom-level order
        for l in (0..self.levels).rev() {
            p = if self.bit(c, l) {
                self.bv[l].select1(p - self.zeros[l])
            } else {
                self.bv[l].select0(p)
            };
        }
        Some(p)
    }
    /// k-th smallest (0-based) value among positions [l,r).
    fn quantile(&self, mut l: usize, mut r: usize, mut k: usize) -> u32 {
        assert!(k < r - l);
        let mut c = 0;
        for lv in 0..self.levels {
            let z = self.bv[lv].rank0(r) - self.bv[lv].rank0(l);
            if k < z {
                l = self.bv[lv].rank0(l);
                r = self.bv[lv].rank0(r);
            } else {
                k -= z;
                c |= 1 << (self.levels - 1 - lv);
                l = self.step(lv, true, l);
                r = self.step(lv, true, r);
            }
        }
        c
    }
    /// How many values < x in positions [l,r).
    fn count_less(&self, mut l: usize, mut r: usize, x: u64) -> usize {
        if x >= 1 << self.levels {
            return r - l;
        }
        let mut cnt = 0;
        for lv in 0..self.levels {
            let one = x >> (self.levels - 1 - lv) & 1 == 1;
            if one {
                cnt += self.bv[lv].rank0(r) - self.bv[lv].rank0(l); // everything with a 0 here is smaller
            }
            l = self.step(lv, one, l);
            r = self.step(lv, one, r);
        }
        cnt
    }
    /// Values v with lo <= v < hi among positions [l,r).
    fn range_count(&self, l: usize, r: usize, lo: u64, hi: u64) -> usize {
        self.count_less(l, r, hi) - self.count_less(l, r, lo)
    }
    fn size_bytes(&self) -> usize {
        self.bv.iter().map(|b| b.size_bytes()).sum::<usize>() + self.zeros.len() * 8
    }
}

// ---------------- 3. suffix array and BWT ----------------
fn suffix_array(t: &[u8]) -> Vec<usize> {
    let n = t.len();
    let mut sa: Vec<usize> = (0..n).collect();
    let mut rank: Vec<usize> = t.iter().map(|&b| b as usize).collect();
    let mut k = 1;
    loop {
        let key =
            |rank: &[usize], i: usize| (rank[i], if i + k < n { rank[i + k] as isize } else { -1 });
        sa.sort_by_key(|&i| key(&rank, i));
        let mut tmp = vec![0usize; n];
        for i in 1..n {
            tmp[sa[i]] = tmp[sa[i - 1]] + (key(&rank, sa[i]) != key(&rank, sa[i - 1])) as usize;
        }
        rank = tmp;
        if n == 0 || rank[sa[n - 1]] == n - 1 {
            return sa;
        }
        k *= 2;
    }
}
fn bwt_from_sa(t: &[u8], sa: &[usize]) -> Vec<u8> {
    sa.iter()
        .map(|&p| if p == 0 { t[t.len() - 1] } else { t[p - 1] })
        .collect()
}
/// Invert via LF using plain counting (not the wavelet): proves the transform is lossless.
fn inverse_bwt(bwt: &[u8]) -> Vec<u8> {
    let mut c = [0usize; 257];
    for &b in bwt {
        c[b as usize + 1] += 1;
    }
    for i in 0..256 {
        c[i + 1] += c[i];
    }
    let mut seen = [0usize; 256];
    let lf: Vec<usize> = bwt
        .iter()
        .map(|&b| {
            let r = c[b as usize] + seen[b as usize];
            seen[b as usize] += 1;
            r
        })
        .collect();
    let mut out = vec![0u8; bwt.len()];
    let mut i = 0; // row 0 is the suffix "\0"
    for k in (0..bwt.len()).rev() {
        out[k] = bwt[i];
        i = lf[i];
    }
    out.rotate_left(1); // the sentinel was written first; move it back to the end
    out
}

// ---------------- 4. FM-index ----------------
struct FmIndex {
    wm: WaveletMatrix,
    ids: [i32; 256], // byte -> dense id, -1 if absent
    c: Vec<usize>,   // c[id] = number of BWT chars with smaller id
    marked: RankSelect,
    samples: Vec<u32>,
    rate: usize,
    n: usize, // text length including the sentinel
}
impl FmIndex {
    fn build(s: &[u8], rate: usize) -> Self {
        assert!(!s.contains(&0), "byte 0 is the sentinel");
        let mut t = s.to_vec();
        t.push(0);
        let sa = suffix_array(&t);
        let bwt = bwt_from_sa(&t, &sa);
        let mut present = [false; 256];
        for &b in &t {
            present[b as usize] = true;
        }
        let mut ids = [-1i32; 256];
        let mut sigma = 0;
        for b in 0..256 {
            if present[b] {
                ids[b] = sigma;
                sigma += 1;
            }
        }
        let levels = ((sigma as f64).log2().ceil() as usize).max(1);
        let syms: Vec<u32> = bwt.iter().map(|&b| ids[b as usize] as u32).collect();
        let mut c = vec![0usize; sigma as usize + 1];
        for &x in &syms {
            c[x as usize + 1] += 1;
        }
        for i in 0..sigma as usize {
            c[i + 1] += c[i];
        }
        let marked = RankSelect::from_bits(sa.iter().map(|&p| p % rate == 0));
        let samples = sa
            .iter()
            .filter(|&&p| p % rate == 0)
            .map(|&p| p as u32)
            .collect();
        FmIndex {
            wm: WaveletMatrix::new(&syms, levels),
            ids,
            c,
            marked,
            samples,
            rate,
            n: t.len(),
        }
    }
    /// Row range [l,r) of suffixes that start with `p`.
    fn range(&self, p: &[u8]) -> (usize, usize) {
        let (mut l, mut r) = (0, self.n);
        for &ch in p.iter().rev() {
            let id = self.ids[ch as usize];
            if id < 0 {
                return (0, 0);
            }
            let id = id as u32;
            l = self.c[id as usize] + self.wm.rank(id, l);
            r = self.c[id as usize] + self.wm.rank(id, r);
            if l >= r {
                return (0, 0);
            }
        }
        (l, r)
    }
    fn count(&self, p: &[u8]) -> usize {
        let (l, r) = self.range(p);
        r - l
    }
    fn lf(&self, row: usize) -> usize {
        let id = self.wm.access(row);
        self.c[id as usize] + self.wm.rank(id, row)
    }
    fn locate_row(&self, mut row: usize) -> usize {
        let mut steps = 0;
        while !self.marked.get(row) {
            row = self.lf(row);
            steps += 1;
        }
        assert!(steps < self.rate, "walked more than rate-1 steps");
        self.samples[self.marked.rank1(row)] as usize + steps
    }
    fn locate(&self, p: &[u8]) -> Vec<usize> {
        let (l, r) = self.range(p);
        let mut v: Vec<usize> = (l..r).map(|row| self.locate_row(row)).collect();
        v.sort_unstable();
        v
    }
    fn size_bytes(&self) -> usize {
        self.wm.size_bytes() + self.marked.size_bytes() + self.samples.len() * 4 + self.c.len() * 8
    }
}

// ---------------- 5. Elias-Fano ----------------
struct EliasFano {
    m: usize,
    u: u64,
    l: usize,
    low: Vec<u64>,
    high: RankSelect,
}
impl EliasFano {
    fn new(vals: &[u64], u: u64) -> Self {
        assert!(
            vals.windows(2).all(|w| w[0] <= w[1]) && vals.iter().all(|&v| v < u.max(1)),
            "monotone values below u required"
        );
        let m = vals.len();
        let l = if m == 0 || u / m as u64 <= 1 {
            0
        } else {
            (u / m as u64).ilog2() as usize
        };
        let mut low = vec![0u64; (m * l).div_ceil(64) + 1];
        for (i, &v) in vals.iter().enumerate() {
            let x = v & ((1u64 << l) - 1);
            for b in 0..l {
                if x >> b & 1 == 1 {
                    low[(i * l + b) / 64] |= 1 << ((i * l + b) % 64);
                }
            }
        }
        let hlen = m + (u >> l) as usize + 1;
        let mut hi = vec![false; hlen];
        for (i, &v) in vals.iter().enumerate() {
            hi[(v >> l) as usize + i] = true;
        }
        EliasFano {
            m,
            u,
            l,
            low,
            high: RankSelect::from_bits(hi.into_iter()),
        }
    }
    fn low_at(&self, i: usize) -> u64 {
        (0..self.l).fold(0, |a, b| {
            a | (self.low[(i * self.l + b) / 64] >> ((i * self.l + b) % 64) & 1) << b
        })
    }
    fn access(&self, i: usize) -> u64 {
        assert!(i < self.m);
        ((self.high.select1(i) - i) as u64) << self.l | self.low_at(i)
    }
    /// Index and value of the first element >= x, if any.
    fn next_geq(&self, x: u64) -> Option<(usize, u64)> {
        if x >= self.u {
            return None;
        }
        let hi = (x >> self.l) as usize;
        // elements whose high part is < hi: ones before the (hi-1)-th zero
        let mut i = if hi == 0 {
            0
        } else {
            self.high.select0(hi - 1) - (hi - 1)
        };
        while i < self.m {
            let v = self.access(i);
            if v >= x {
                return Some((i, v));
            }
            i += 1;
        }
        None
    }
    fn size_bytes(&self) -> usize {
        self.low.len() * 8 + self.high.size_bytes()
    }
}

// ---------------- checks ----------------
fn check_bitvector() {
    let mut r = Rng(845);
    for case in 0..400 {
        let n = match case % 5 {
            0 => 0,
            1 => 1 + r.below(70) as usize,
            _ => r.below(9000) as usize,
        };
        let density = [0, 1, 50, 99, 100][r.below(5) as usize];
        let bits: Vec<bool> = (0..n).map(|_| r.below(100) < density).collect();
        let rs = RankSelect::from_bits(bits.iter().copied());
        let (mut ones, mut zeros) = (0, 0);
        for i in 0..=n {
            assert_eq!(rs.rank1(i), ones, "rank1({i}) n={n}");
            assert_eq!(rs.rank0(i), zeros);
            if i < n {
                assert_eq!(rs.get(i), bits[i]);
                if bits[i] {
                    assert_eq!(rs.select1(ones), i, "select1({ones})");
                    ones += 1;
                } else {
                    assert_eq!(rs.select0(zeros), i, "select0({zeros})");
                    zeros += 1;
                }
            }
        }
    }
    println!(
        "rank/select bitvector: rank1/rank0/select1/select0 equal naive counting on 400 vectors (empty, sparse, dense, ragged lengths)"
    );
}

fn check_wavelet() {
    let mut r = Rng(0x57AB);
    for _ in 0..2000 {
        let levels = 1 + r.below(8) as usize;
        let n = 1 + r.below(200) as usize;
        let sigma = 1u64 << levels;
        let data: Vec<u32> = (0..n).map(|_| r.below(sigma) as u32).collect();
        let wm = WaveletMatrix::new(&data, levels);
        for i in 0..n {
            assert_eq!(wm.access(i), data[i], "access({i})");
        }
        for _ in 0..8 {
            let c = r.below(sigma) as u32;
            let i = r.below(n as u64 + 1) as usize;
            assert_eq!(
                wm.rank(c, i),
                data[..i].iter().filter(|&&v| v == c).count(),
                "rank({c},{i})"
            );
            let occ: Vec<usize> = (0..n).filter(|&p| data[p] == c).collect();
            let k = r.below(occ.len() as u64 + 2) as usize;
            assert_eq!(wm.select(c, k), occ.get(k).copied(), "select({c},{k})");
            let (a, b) = (r.below(n as u64) as usize, r.below(n as u64 + 1) as usize);
            let (l, rr) = (a.min(b), a.max(b));
            if rr > l {
                let mut sorted = data[l..rr].to_vec();
                sorted.sort_unstable();
                let k = r.below((rr - l) as u64) as usize;
                assert_eq!(wm.quantile(l, rr, k), sorted[k], "quantile({l},{rr},{k})");
            }
            let (lo, hi) = (r.below(sigma + 1), r.below(sigma + 1));
            let (lo, hi) = (lo.min(hi), lo.max(hi));
            assert_eq!(
                wm.range_count(l, rr, lo, hi),
                data[l..rr]
                    .iter()
                    .filter(|&&v| (v as u64) >= lo && (v as u64) < hi)
                    .count(),
                "range_count"
            );
        }
    }
    println!(
        "wavelet matrix: access/rank/select/quantile/range_count equal brute force on 2000 random sequences (1-8 levels)"
    );
}

fn naive_occurrences(s: &[u8], p: &[u8]) -> Vec<usize> {
    if p.len() > s.len() {
        return vec![];
    }
    (0..=s.len() - p.len())
        .filter(|&i| &s[i..i + p.len()] == p)
        .collect()
}
fn random_text(r: &mut Rng, alpha: u64, len: usize) -> Vec<u8> {
    (0..len).map(|_| b'a' + r.below(alpha) as u8).collect()
}
fn check_bwt_and_fm() {
    let mut r = Rng(0xF312);
    // BWT equals sorted rotations on small inputs and inverts
    for _ in 0..300 {
        let (alpha, len) = (1 + r.below(4), 1 + r.below(40) as usize);
        let mut s = random_text(&mut r, alpha, len);
        s.push(0);
        let sa = suffix_array(&s);
        let mut rot: Vec<usize> = (0..s.len()).collect();
        rot.sort_by(|&a, &b| s[a..].cmp(&s[b..])); // suffix order == rotation order thanks to the unique sentinel
        assert_eq!(sa, rot);
        let bwt = bwt_from_sa(&s, &sa);
        let naive: Vec<u8> = rot
            .iter()
            .map(|&i| s[(i + s.len() - 1) % s.len()])
            .collect();
        assert_eq!(bwt, naive);
        assert_eq!(inverse_bwt(&bwt), s, "BWT must invert");
    }
    let (mut matches, mut absent) = (0usize, 0usize);
    for case in 0..2000 {
        let alpha = [1u64, 2, 4, 26][r.below(4) as usize];
        let len = 1 + r.below(300) as usize;
        let s = random_text(&mut r, alpha, len);
        let rate = [1usize, 4, 16][case % 3];
        let fm = FmIndex::build(&s, rate);
        for _ in 0..6 {
            let p: Vec<u8> = if r.below(2) == 0 {
                let l = 1 + r.below(8.min(len as u64)) as usize;
                let st = r.below((len - l + 1) as u64) as usize;
                s[st..st + l].to_vec() // present
            } else {
                let l = 1 + r.below(8) as usize;
                random_text(&mut r, alpha + 1, l) // often absent (alphabet one larger)
            };
            let want = naive_occurrences(&s, &p);
            assert_eq!(
                fm.count(&p),
                want.len(),
                "count {:?} in {:?}",
                String::from_utf8_lossy(&p),
                String::from_utf8_lossy(&s)
            );
            assert_eq!(
                fm.locate(&p),
                want,
                "locate {:?}",
                String::from_utf8_lossy(&p)
            );
            if want.is_empty() {
                absent += 1
            } else {
                matches += 1
            }
        }
    }
    println!(
        "BWT == sorted rotations and inverts (300 strings); FM count/locate == naive scan on 2000 texts ({matches} hits, {absent} misses)"
    );
}

fn check_elias_fano() {
    let mut r = Rng(0xE11A);
    for case in 0..2000 {
        let m = if case % 17 == 0 {
            0
        } else {
            r.below(200) as usize
        };
        let u = match case % 4 {
            0 => (m as u64).max(1),
            1 => m as u64 * 3 + 1,
            2 => 1 << (4 + r.below(24)),
            _ => 1 + r.below(1_000_000),
        };
        let mut v: Vec<u64> = (0..m).map(|_| r.below(u)).collect();
        v.sort_unstable(); // duplicates allowed
        let ef = EliasFano::new(&v, u);
        for (i, &x) in v.iter().enumerate() {
            assert_eq!(
                ef.access(i),
                x,
                "access({i}) case {case} (m={m}, u={u}, l={})",
                ef.l
            );
        }
        for _ in 0..10 {
            let x = r.below(u + 2);
            let want = v.iter().position(|&y| y >= x).map(|i| (i, v[i]));
            assert_eq!(ef.next_geq(x), want, "next_geq({x}) case {case}");
        }
    }
    println!(
        "Elias-Fano: access and next_geq equal a plain Vec on 2000 monotone sequences (empty, dense, sparse, duplicates)"
    );
}

fn h0(t: &[u8]) -> f64 {
    let mut c = [0usize; 256];
    for &b in t {
        c[b as usize] += 1;
    }
    c.iter()
        .filter(|&&x| x > 0)
        .map(|&x| -(x as f64 / t.len() as f64) * (x as f64 / t.len() as f64).log2())
        .sum()
}
/// Empirical order-k entropy: entropy of the next symbol given the previous k.
fn hk(t: &[u8], k: usize) -> f64 {
    let mut ctx: HashMap<&[u8], HashMap<u8, usize>> = HashMap::new();
    for i in k..t.len() {
        *ctx.entry(&t[i - k..i])
            .or_default()
            .entry(t[i])
            .or_insert(0) += 1;
    }
    let mut bits = 0.0;
    for m in ctx.values() {
        let tot: usize = m.values().sum();
        bits += m
            .values()
            .map(|&c| -(c as f64) * (c as f64 / tot as f64).log2())
            .sum::<f64>();
    }
    bits / (t.len() - k) as f64
}
fn space_report() {
    let mut r = Rng(0x5EED);
    let dna = random_text(&mut r, 4, 150_000)
        .into_iter()
        .map(|b| b"ACGT"[(b - b'a') as usize])
        .collect::<Vec<u8>>();
    let words = [
        "the", "of", "and", "index", "query", "tree", "data", "table", "join", "column", "page",
        "log", "key", "scan", "block", "cache",
    ];
    let mut eng = vec![];
    while eng.len() < 150_000 {
        eng.extend_from_slice(words[r.below(words.len() as u64) as usize].as_bytes());
        eng.push(b' ');
    }
    eng.truncate(150_000);
    println!(
        "\nspace (bits per input character); plain text = 8, suffix array = 32, entropy rows are lower bounds for ANY order-k coder:"
    );
    println!(
        "{:>12} {:>8} {:>8} {:>10} {:>10} {:>10} {:>10} {:>10}",
        "text", "H0", "H3", "FM rate 8", "FM rate 32", "FM rate128", "levels", "SA (u32)"
    );
    for (name, t) in [("DNA", &dna), ("word soup", &eng)] {
        let sizes: Vec<f64> = [8usize, 32, 128]
            .iter()
            .map(|&rate| FmIndex::build(t, rate).size_bytes() as f64 * 8.0 / t.len() as f64)
            .collect();
        let fm = FmIndex::build(t, 32);
        println!(
            "{name:>12} {:>8.2} {:>8.2} {:>10.2} {:>10.2} {:>10.2} {:>10} {:>10}",
            h0(t),
            hk(t, 3),
            sizes[0],
            sizes[1],
            sizes[2],
            fm.wm.levels,
            32
        );
        assert!(
            sizes[2] < sizes[0],
            "a sparser SA sample must shrink the index"
        );
        assert!(
            sizes[1] < 32.0,
            "the FM-index must be smaller than a suffix array"
        );
        // sanity: the index still answers queries on the large text
        let p = &t[1000..1012];
        assert_eq!(fm.locate(p), naive_occurrences(t, p));
    }
    let dna_fm = FmIndex::build(&dna, 32).size_bytes() as f64 * 8.0 / dna.len() as f64;
    assert!(
        dna_fm < 8.0 * 1.0,
        "DNA FM-index (3 levels) should beat plain 8-bit text"
    );
    // Elias-Fano vs plain
    let m = 100_000usize;
    let u = 1u64 << 30;
    let mut v: Vec<u64> = (0..m).map(|_| r.below(u)).collect();
    v.sort_unstable();
    let ef = EliasFano::new(&v, u);
    println!(
        "Elias-Fano: {m} sorted ids below 2^30: {:.2} bits/id (bound m*(2+log2(u/m)) = {:.2}) vs 64 plain, 32 as u32",
        ef.size_bytes() as f64 * 8.0 / m as f64,
        2.0 + (u as f64 / m as f64).log2()
    );
    assert!(
        ef.size_bytes() * 8 < m * 16,
        "Elias-Fano should be below 16 bits per id here"
    );
    assert_eq!(ef.access(54_321), v[54_321]);
    let rs = RankSelect::from_bits((0..1 << 20).map(|i| (i * 2654435761u64 >> 7) & 1 == 1));
    println!(
        "rank/select overhead: {:.1}% over the raw bits ({} bytes for {} bits)",
        (rs.size_bytes() as f64 / (rs.words.len() * 8) as f64 - 1.0) * 100.0,
        rs.size_bytes(),
        rs.n
    );
}

fn main() {
    check_bitvector();
    check_wavelet();
    check_bwt_and_fm();
    check_elias_fano();
    space_report();
    println!("all checks passed");
}
