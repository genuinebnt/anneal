//! RECIPE 270 (combinatorics): generating combinatorial objects as iterators (Gray codes, combinations, permutations, partitions, Catalan, necklaces, de Bruijn).
//! LEVEL: advanced
//! Use in: exhaustive search, test-case enumeration, puzzle solvers, hardware test sequences.   Run: cargo run --release --example combinatorial_generation
#![allow(dead_code)]
//!
//! Knuth (TAOCP Vol 4A, 7.2.1) treats generation as a first-class problem: visit every object of a family exactly
//! once, with O(1) AMORTISED work per object, ideally changing as little as possible between neighbours. Every
//! generator here is an `Iterator` that keeps a tiny state and mutates it in place (the `Vec` yielded by some of
//! them is a clone for convenience; the state update itself is the amortised O(1) part). Each is checked against
//! a known count and a brute-force oracle for small n.
//!
//! - Gray code: gray(i) = i ^ (i >> 1); neighbours differ in ONE bit.
//! - Combinations: lexicographic in-place successor, and Gosper's hack (next mask with the same popcount).
//! - Permutations: next_permutation (lexicographic), Heap's (one swap per step), Steinhaus-Johnson-Trotter
//!   (adjacent transposition per step; this version scans for the mobile element in O(n), Even's speed-up makes it O(1)).
//! - Rank/unrank via the factorial number system (Lehmer code).
//! - Integer partitions p(n), set partitions via restricted growth strings (Bell numbers), Dyck words <-> binary
//!   trees (Catalan numbers), Lyndon words/necklaces (Duval/FKM), de Bruijn sequences from Lyndon words.
//!
//! COMMON USES: exhaustive and combinatorial test generation, Gray codes in rotary encoders and error-minimising
//! labelling, de Bruijn sequences in lock-picking/brute-force analysis and position encoding, necklace counting in
//! chemistry/music/cryptography, Gosper's hack for subset-DP over bitmasks, next_permutation in contest code and
//! the C++ standard library.
//! Read: Knuth, The Art of Computer Programming Vol 4A (Combinatorial Algorithms, Part 1), section 7.2.1;
//! Skiena, The Algorithm Design Manual (generating permutations/subsets/partitions in the problem catalog).
//! https://en.wikipedia.org/wiki/Gray_code  https://en.wikipedia.org/wiki/Heap%27s_algorithm
//! https://www-cs-faculty.stanford.edu/~knuth/taocp.html

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

fn factorial(n: usize) -> u64 {
    (1..=n as u64).product()
}

// ---------------------------------------------------------------- subsets and Gray code
/// All subsets of an n-set as bitmasks in plain binary order.
fn subsets(n: u32) -> impl Iterator<Item = u64> {
    0..(1u64 << n)
}

/// Binary reflected Gray code: consecutive values differ in exactly one bit.
struct Gray {
    i: u64,
    end: u64,
}
impl Gray {
    fn new(n: u32) -> Self {
        Gray { i: 0, end: 1 << n }
    }
}
impl Iterator for Gray {
    type Item = u64;
    fn next(&mut self) -> Option<u64> {
        if self.i == self.end {
            return None;
        }
        let g = self.i ^ (self.i >> 1);
        self.i += 1;
        Some(g)
    }
}

fn gray_inverse(mut g: u64) -> u64 {
    let mut n = 0;
    while g != 0 {
        n ^= g;
        g >>= 1;
    }
    n
}

// ---------------------------------------------------------------- combinations
/// k-combinations of 0..n in lexicographic order, successor computed in place.
struct Combos {
    n: usize,
    c: Vec<usize>,
    started: bool,
    done: bool,
}
impl Combos {
    fn new(n: usize, k: usize) -> Self {
        Combos {
            n,
            c: (0..k).collect(),
            started: false,
            done: k > n,
        }
    }
}
impl Iterator for Combos {
    type Item = Vec<usize>;
    fn next(&mut self) -> Option<Vec<usize>> {
        if self.done {
            return None;
        }
        if !self.started {
            self.started = true;
            return Some(self.c.clone());
        }
        let k = self.c.len();
        // rightmost position that can still be incremented
        let mut i = k;
        while i > 0 && self.c[i - 1] == self.n - k + i - 1 {
            i -= 1;
        }
        if i == 0 {
            self.done = true;
            return None;
        }
        self.c[i - 1] += 1;
        for j in i..k {
            self.c[j] = self.c[j - 1] + 1;
        }
        Some(self.c.clone())
    }
}

/// Gosper's hack: the next larger integer with the same number of set bits.
fn gosper_next(x: u64) -> u64 {
    let c = x & x.wrapping_neg();
    let r = x + c;
    (((r ^ x) >> 2) / c) | r
}

/// All n-bit masks with exactly k bits set, in increasing numeric order.
struct Gosper {
    cur: Option<u64>,
    limit: u64,
}
impl Gosper {
    fn new(n: u32, k: u32) -> Self {
        let first = if k == 0 { 0 } else { (1u64 << k) - 1 };
        Gosper {
            cur: if k > n { None } else { Some(first) },
            limit: 1 << n,
        }
    }
}
impl Iterator for Gosper {
    type Item = u64;
    fn next(&mut self) -> Option<u64> {
        let x = self.cur?;
        self.cur = if x == 0 {
            None
        } else {
            let nx = gosper_next(x);
            if nx >= self.limit { None } else { Some(nx) }
        };
        Some(x)
    }
}

fn binom(n: u64, k: u64) -> u64 {
    if k > n {
        return 0;
    }
    let k = k.min(n - k);
    let mut r = 1u64;
    for i in 0..k {
        r = r * (n - i) / (i + 1);
    }
    r
}

// ---------------------------------------------------------------- permutations
/// Lexicographic successor; returns false (and leaves a sorted slice) after the last permutation.
fn next_permutation<T: Ord>(a: &mut [T]) -> bool {
    if a.len() < 2 {
        return false;
    }
    let mut i = a.len() - 1;
    while i > 0 && a[i - 1] >= a[i] {
        i -= 1;
    }
    if i == 0 {
        a.reverse();
        return false;
    }
    let mut j = a.len() - 1;
    while a[j] <= a[i - 1] {
        j -= 1;
    }
    a.swap(i - 1, j);
    a[i..].reverse();
    true
}

struct LexPerms {
    a: Vec<usize>,
    done: bool,
}
impl Iterator for LexPerms {
    type Item = Vec<usize>;
    fn next(&mut self) -> Option<Vec<usize>> {
        if self.done {
            return None;
        }
        let out = self.a.clone();
        self.done = !next_permutation(&mut self.a);
        Some(out)
    }
}

/// Heap's algorithm, iterative form: exactly one swap between consecutive permutations.
struct Heap {
    a: Vec<usize>,
    c: Vec<usize>,
    i: usize,
    first: bool,
}
impl Heap {
    fn new(n: usize) -> Self {
        Heap {
            a: (0..n).collect(),
            c: vec![0; n],
            i: 1,
            first: true,
        }
    }
}
impl Iterator for Heap {
    type Item = Vec<usize>;
    fn next(&mut self) -> Option<Vec<usize>> {
        if self.first {
            self.first = false;
            return Some(self.a.clone());
        }
        while self.i < self.a.len() {
            if self.c[self.i] < self.i {
                if self.i % 2 == 0 {
                    self.a.swap(0, self.i);
                } else {
                    self.a.swap(self.c[self.i], self.i);
                }
                self.c[self.i] += 1;
                self.i = 1;
                return Some(self.a.clone());
            } else {
                self.c[self.i] = 0;
                self.i += 1;
            }
        }
        None
    }
}

/// Steinhaus-Johnson-Trotter "plain changes": each step swaps the largest mobile element with its neighbour.
struct Sjt {
    p: Vec<usize>,
    dir: Vec<i32>,
    first: bool,
    done: bool,
}
impl Sjt {
    fn new(n: usize) -> Self {
        Sjt {
            p: (0..n).collect(),
            dir: vec![-1; n],
            first: true,
            done: false,
        }
    }
}
impl Iterator for Sjt {
    type Item = Vec<usize>;
    fn next(&mut self) -> Option<Vec<usize>> {
        if self.done {
            return None;
        }
        if self.first {
            self.first = false;
            return Some(self.p.clone());
        }
        let n = self.p.len();
        // the largest mobile element: its neighbour in the direction it points is smaller
        let mut best: Option<usize> = None;
        for i in 0..n {
            let j = i as i32 + self.dir[i];
            if j < 0 || j >= n as i32 {
                continue;
            }
            if self.p[i] > self.p[j as usize] && best.is_none_or(|b| self.p[i] > self.p[b]) {
                best = Some(i);
            }
        }
        let Some(i) = best else {
            self.done = true;
            return None;
        };
        let j = (i as i32 + self.dir[i]) as usize;
        let moved = self.p[i];
        self.p.swap(i, j);
        self.dir.swap(i, j);
        for k in 0..n {
            if self.p[k] > moved {
                self.dir[k] = -self.dir[k];
            }
        }
        Some(self.p.clone())
    }
}

/// Lehmer-code rank of a permutation of 0..n (factorial number system).
fn perm_rank(p: &[usize]) -> u64 {
    let n = p.len();
    let mut r = 0;
    for i in 0..n {
        let smaller = p[i + 1..].iter().filter(|&&x| x < p[i]).count() as u64;
        r += smaller * factorial(n - 1 - i);
    }
    r
}

fn perm_unrank(n: usize, mut r: u64) -> Vec<usize> {
    let mut pool: Vec<usize> = (0..n).collect();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let f = factorial(n - 1 - i);
        out.push(pool.remove((r / f) as usize));
        r %= f;
    }
    out
}

fn differ_positions(a: &[usize], b: &[usize]) -> Vec<usize> {
    (0..a.len()).filter(|&i| a[i] != b[i]).collect()
}

// ---------------------------------------------------------------- integer partitions
/// Partitions of n into non-increasing parts, in reverse lexicographic order ([n] first, [1,1,..] last).
struct Partitions {
    p: Vec<usize>,
    done: bool,
}
impl Partitions {
    fn new(n: usize) -> Self {
        Partitions {
            p: if n == 0 { vec![] } else { vec![n] },
            done: false,
        }
    }
}
impl Iterator for Partitions {
    type Item = Vec<usize>;
    fn next(&mut self) -> Option<Vec<usize>> {
        if self.done {
            return None;
        }
        let out = self.p.clone();
        let mut rem = 0;
        while self.p.last() == Some(&1) {
            self.p.pop();
            rem += 1;
        }
        match self.p.last_mut() {
            None => self.done = true,
            Some(last) => {
                *last -= 1;
                rem += 1;
                let v = *last;
                while rem > v {
                    self.p.push(v);
                    rem -= v;
                }
                if rem > 0 {
                    self.p.push(rem);
                }
            }
        }
        Some(out)
    }
}

/// Oracle: p(n) by the coin-change DP.
fn partition_count_dp(n: usize) -> u64 {
    let mut ways = vec![0u64; n + 1];
    ways[0] = 1;
    for part in 1..=n {
        for s in part..=n {
            ways[s] += ways[s - part];
        }
    }
    ways[n]
}

// ---------------------------------------------------------------- set partitions
/// Restricted growth strings: a[0]=0 and a[i] <= 1 + max(a[..i]). Each string is one set partition.
struct Rgs {
    a: Vec<usize>,
    done: bool,
}
impl Rgs {
    fn new(n: usize) -> Self {
        Rgs {
            a: vec![0; n],
            done: n == 0,
        }
    }
}
impl Iterator for Rgs {
    type Item = Vec<usize>;
    fn next(&mut self) -> Option<Vec<usize>> {
        if self.done {
            return None;
        }
        let out = self.a.clone();
        // prefix maxima, then find the rightmost position that can grow
        let n = self.a.len();
        let mut pm = vec![0; n];
        for i in 1..n {
            pm[i] = pm[i - 1].max(self.a[i - 1]);
        }
        let mut i = n;
        loop {
            if i == 1 || n == 0 {
                self.done = true;
                break;
            }
            i -= 1;
            if self.a[i] <= pm[i] {
                self.a[i] += 1;
                for j in i + 1..n {
                    self.a[j] = 0;
                }
                break;
            }
        }
        Some(out)
    }
}

fn bell_numbers(upto: usize) -> Vec<u64> {
    // Bell triangle
    let mut bells = vec![1u64];
    let mut row = vec![1u64];
    for _ in 1..=upto {
        let mut next = vec![*row.last().unwrap()];
        for x in &row {
            next.push(next.last().unwrap() + x);
        }
        bells.push(next[0]);
        row = next;
    }
    bells // bells[n] = B(n)
}

// ---------------------------------------------------------------- Catalan: Dyck words and binary trees
/// Balanced parentheses (true = open) of semi-length n in lexicographic order with open < close.
struct Dyck {
    s: Vec<bool>,
    done: bool,
}
impl Dyck {
    fn new(n: usize) -> Self {
        let mut s = vec![true; n];
        s.extend(vec![false; n]);
        Dyck { s, done: false }
    }
}
impl Iterator for Dyck {
    type Item = Vec<bool>;
    fn next(&mut self) -> Option<Vec<bool>> {
        if self.done {
            return None;
        }
        let out = self.s.clone();
        let len = self.s.len();
        let mut bal_before = vec![0i32; len + 1];
        for i in 0..len {
            bal_before[i + 1] = bal_before[i] + if self.s[i] { 1 } else { -1 };
        }
        // rightmost '(' that can become ')' while the rest can still be completed
        let mut found = None;
        for i in (0..len).rev() {
            if self.s[i] && bal_before[i] >= 1 {
                let b = bal_before[i] - 1;
                let r = (len - i - 1) as i32;
                if b <= r {
                    found = Some((i, b, r));
                    break;
                }
            }
        }
        match found {
            None => self.done = true,
            Some((i, b, r)) => {
                self.s[i] = false;
                let opens = ((r - b) / 2) as usize;
                for (k, slot) in self.s[i + 1..].iter_mut().enumerate() {
                    *slot = k < opens;
                }
            }
        }
        Some(out)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Tree {
    Leaf,
    Node(Box<Tree>, Box<Tree>),
}

/// Dyck word w = ( L ) R  <->  tree Node(L, R).
fn word_to_tree(w: &[bool]) -> Tree {
    if w.is_empty() {
        return Tree::Leaf;
    }
    let mut depth = 0;
    let mut close = 0;
    for (i, &b) in w.iter().enumerate() {
        depth += if b { 1 } else { -1 };
        if depth == 0 {
            close = i;
            break;
        }
    }
    Tree::Node(
        Box::new(word_to_tree(&w[1..close])),
        Box::new(word_to_tree(&w[close + 1..])),
    )
}

fn tree_to_word(t: &Tree, out: &mut Vec<bool>) {
    if let Tree::Node(l, r) = t {
        out.push(true);
        tree_to_word(l, out);
        out.push(false);
        tree_to_word(r, out);
    }
}

/// Oracle: every binary tree with n internal nodes, by direct recursion on structure.
fn all_trees(n: usize) -> Vec<Tree> {
    if n == 0 {
        return vec![Tree::Leaf];
    }
    let mut out = vec![];
    for left in 0..n {
        for l in all_trees(left) {
            for r in all_trees(n - 1 - left) {
                out.push(Tree::Node(Box::new(l.clone()), Box::new(r)));
            }
        }
    }
    out
}

fn catalan(n: u64) -> u64 {
    binom(2 * n, n) / (n + 1)
}

// ---------------------------------------------------------------- Lyndon words, necklaces, de Bruijn
/// Duval's algorithm: all Lyndon words over 0..k of length <= n, in lexicographic order.
struct Lyndon {
    w: Vec<usize>,
    n: usize,
    k: usize,
    started: bool,
}
impl Lyndon {
    fn new(n: usize, k: usize) -> Self {
        Lyndon {
            w: vec![],
            n,
            k,
            started: false,
        }
    }
}
impl Iterator for Lyndon {
    type Item = Vec<usize>;
    fn next(&mut self) -> Option<Vec<usize>> {
        if !self.started {
            self.started = true;
            self.w = vec![0];
            return Some(self.w.clone());
        }
        let m = self.w.len();
        while self.w.len() < self.n {
            let x = self.w[self.w.len() - m];
            self.w.push(x);
        }
        while self.w.last() == Some(&(self.k - 1)) {
            self.w.pop();
        }
        *self.w.last_mut()? += 1;
        Some(self.w.clone())
    }
}

/// Necklaces (rotation classes) of length n over k letters: repeat each Lyndon word whose length divides n.
fn necklaces(n: usize, k: usize) -> Vec<Vec<usize>> {
    Lyndon::new(n, k)
        .filter(|w| n % w.len() == 0)
        .map(|w| w.iter().cycle().take(n).cloned().collect())
        .collect()
}

fn de_bruijn(n: usize, k: usize) -> Vec<usize> {
    Lyndon::new(n, k)
        .filter(|w| n % w.len() == 0)
        .flatten()
        .collect()
}

fn gcd(a: usize, b: usize) -> usize {
    if b == 0 { a } else { gcd(b, a % b) }
}

fn necklace_count_formula(n: usize, k: usize) -> usize {
    // (1/n) sum_{d|n} phi(d) k^(n/d), with phi(d) = #{j in 1..=d : gcd(j,d)=1}
    let mut s = 0;
    for d in (1..=n).filter(|d| n % d == 0) {
        let phi = (1..=d).filter(|&j| gcd(j, d) == 1).count();
        s += phi * k.pow((n / d) as u32);
    }
    s / n
}

fn min_rotation(w: &[usize]) -> Vec<usize> {
    (0..w.len())
        .map(|r| {
            w.iter()
                .cycle()
                .skip(r)
                .take(w.len())
                .cloned()
                .collect::<Vec<_>>()
        })
        .min()
        .unwrap()
}

fn main() {
    // ---- subsets and Gray code
    for n in 1..=12u32 {
        assert_eq!(subsets(n).count(), 1 << n);
        let codes: Vec<u64> = Gray::new(n).collect();
        let mut seen = codes.clone();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), 1 << n, "Gray code visits every subset once");
        for w in codes.windows(2) {
            assert_eq!((w[0] ^ w[1]).count_ones(), 1, "single-bit change");
        }
        for (i, &g) in codes.iter().enumerate() {
            assert_eq!(gray_inverse(g), i as u64);
        }
        assert_eq!(
            (codes[0] ^ codes[codes.len() - 1]).count_ones(),
            1,
            "cyclic"
        );
    }
    println!(
        "Gray(3): {:?}",
        Gray::new(3).map(|g| format!("{g:03b}")).collect::<Vec<_>>()
    );

    // ---- combinations
    for n in 0..=12usize {
        for k in 0..=n {
            let lex: Vec<Vec<usize>> = Combos::new(n, k).collect();
            assert_eq!(lex.len() as u64, binom(n as u64, k as u64));
            let mut sorted = lex.clone();
            sorted.sort();
            assert_eq!(sorted, lex, "lexicographic order");
            // oracle: filter all masks by popcount
            let brute: Vec<u64> = (0..1u64 << n)
                .filter(|m| m.count_ones() as usize == k)
                .collect();
            let gos: Vec<u64> = Gosper::new(n as u32, k as u32).collect();
            assert_eq!(gos, brute, "Gosper order = increasing numeric order");
            let as_masks: std::collections::BTreeSet<u64> = lex
                .iter()
                .map(|c| c.iter().fold(0u64, |m, &i| m | 1 << i))
                .collect();
            assert_eq!(as_masks, brute.iter().cloned().collect());
        }
    }
    println!(
        "C(5,3) lex: {:?}",
        Combos::new(5, 3).take(4).collect::<Vec<_>>()
    );

    // ---- permutations
    for n in 1..=7usize {
        let f = factorial(n) as usize;
        let lex: Vec<Vec<usize>> = LexPerms {
            a: (0..n).collect(),
            done: false,
        }
        .collect();
        assert_eq!(lex.len(), f);
        assert!(lex.windows(2).all(|w| w[0] < w[1]));

        let heap: Vec<Vec<usize>> = Heap::new(n).collect();
        assert_eq!(heap.len(), f);
        for w in heap.windows(2) {
            assert_eq!(
                differ_positions(&w[0], &w[1]).len(),
                2,
                "Heap: one swap per step"
            );
        }
        let sjt: Vec<Vec<usize>> = Sjt::new(n).collect();
        assert_eq!(sjt.len(), f);
        for w in sjt.windows(2) {
            let d = differ_positions(&w[0], &w[1]);
            assert!(
                d.len() == 2 && d[1] == d[0] + 1,
                "SJT: adjacent transposition"
            );
        }
        for set in [&heap, &sjt] {
            let mut s = set.clone();
            s.sort();
            assert_eq!(s, lex, "same set of permutations");
        }
    }
    println!("SJT(3): {:?}", Sjt::new(3).collect::<Vec<_>>());

    // ---- ranking / unranking
    let all7: Vec<Vec<usize>> = LexPerms {
        a: (0..7).collect(),
        done: false,
    }
    .collect();
    for (i, p) in all7.iter().enumerate() {
        assert_eq!(perm_rank(p), i as u64, "lex rank is the index");
        assert_eq!(&perm_unrank(7, i as u64), p);
        assert_eq!(perm_rank(&perm_unrank(7, i as u64)), i as u64);
    }
    let mut rng = Rng(0x5eed_1234_abcd_ef01);
    let r = rng.next() % factorial(15);
    assert_eq!(
        perm_rank(&perm_unrank(15, r)),
        r,
        "works far beyond enumerable sizes"
    );
    println!(
        "rank/unrank verified for all 5040 permutations of 7; unrank(15, {r}) = {:?}",
        perm_unrank(15, r)
    );

    // ---- integer partitions
    let known = [
        1u64, 2, 3, 5, 7, 11, 15, 22, 30, 42, 56, 77, 101, 135, 176, 231, 297, 385, 490, 627, 792,
        1002, 1255, 1575, 1958, 2436, 3010, 3718, 4565, 5604,
    ];
    for n in 1..=30usize {
        let count = Partitions::new(n).count() as u64;
        assert_eq!(count, known[n - 1], "p({n})");
        assert_eq!(count, partition_count_dp(n));
    }
    for p in Partitions::new(12) {
        assert_eq!(p.iter().sum::<usize>(), 12);
        assert!(p.windows(2).all(|w| w[0] >= w[1]));
    }
    println!(
        "partitions of 5: {:?}",
        Partitions::new(5).collect::<Vec<_>>()
    );

    // ---- set partitions
    let bell = bell_numbers(10);
    assert_eq!(&bell[1..9], &[1, 2, 5, 15, 52, 203, 877, 4140]);
    for n in 1..=9usize {
        let all: Vec<Vec<usize>> = Rgs::new(n).collect();
        assert_eq!(all.len() as u64, bell[n], "Bell({n})");
        for a in &all {
            let mut mx = 0;
            for (i, &x) in a.iter().enumerate() {
                assert!(i == 0 && x == 0 || x <= mx + 1);
                mx = mx.max(x);
            }
        }
        let mut s = all.clone();
        s.sort();
        s.dedup();
        assert_eq!(s.len(), all.len());
    }
    println!("set partitions of 3: {:?}", Rgs::new(3).collect::<Vec<_>>());

    // ---- Catalan: Dyck words <-> binary trees
    let cat = [1u64, 1, 2, 5, 14, 42, 132, 429, 1430, 4862];
    for n in 0..=9usize {
        let words: Vec<Vec<bool>> = Dyck::new(n).collect();
        assert_eq!(words.len() as u64, cat[n]);
        assert_eq!(catalan(n as u64), cat[n]);
        let mut sorted = words.clone();
        sorted.sort_by_key(|w| w.iter().map(|&b| !b).collect::<Vec<_>>()); // open < close
        assert_eq!(sorted, words);
        let trees = all_trees(n);
        assert_eq!(trees.len() as u64, cat[n]);
        // bijection: tree -> word is onto the Dyck words, word -> tree is its inverse
        let mut from_trees: Vec<Vec<bool>> = trees
            .iter()
            .map(|t| {
                let mut w = vec![];
                tree_to_word(t, &mut w);
                w
            })
            .collect();
        from_trees.sort_by_key(|w| w.iter().map(|&b| !b).collect::<Vec<_>>());
        assert_eq!(from_trees, words);
        for w in &words {
            let t = word_to_tree(w);
            let mut back = vec![];
            tree_to_word(&t, &mut back);
            assert_eq!(&back, w);
        }
    }
    let show = |w: &Vec<bool>| {
        w.iter()
            .map(|&b| if b { '(' } else { ')' })
            .collect::<String>()
    };
    println!(
        "Dyck n=3: {:?}",
        Dyck::new(3).map(|w| show(&w)).collect::<Vec<_>>()
    );

    // ---- necklaces and Lyndon words
    let lyndon_bin = [2usize, 1, 2, 3, 6, 9, 18, 30];
    let neck_bin = [2usize, 3, 4, 6, 8, 14, 20, 36];
    for n in 1..=8usize {
        let ly = Lyndon::new(n, 2).filter(|w| w.len() == n).count();
        assert_eq!(ly, lyndon_bin[n - 1], "Lyndon words of length {n}");
        assert_eq!(necklaces(n, 2).len(), neck_bin[n - 1]);
    }
    for k in 2..=4usize {
        for n in 1..=6usize {
            if k.pow(n as u32) > 5000 {
                continue;
            }
            let ns = necklaces(n, k);
            assert_eq!(ns.len(), necklace_count_formula(n, k));
            // oracle: brute-force canonical rotations of every word
            let mut canon = std::collections::BTreeSet::new();
            for code in 0..k.pow(n as u32) {
                let w: Vec<usize> = (0..n).map(|i| code / k.pow(i as u32) % k).collect();
                canon.insert(min_rotation(&w));
            }
            assert_eq!(canon, ns.iter().cloned().collect());
        }
    }
    println!(
        "binary Lyndon words (len<=4): {:?}",
        Lyndon::new(4, 2).collect::<Vec<_>>()
    );

    // ---- de Bruijn sequences
    for (k, n) in [
        (2usize, 1usize),
        (2, 2),
        (2, 3),
        (2, 4),
        (2, 8),
        (3, 2),
        (3, 4),
        (3, 5),
        (4, 3),
        (5, 3),
    ] {
        let s = de_bruijn(n, k);
        assert_eq!(s.len(), k.pow(n as u32));
        let mut seen = std::collections::HashSet::new();
        for i in 0..s.len() {
            let gram: Vec<usize> = (0..n).map(|j| s[(i + j) % s.len()]).collect();
            assert!(seen.insert(gram), "n-gram repeated");
        }
        assert_eq!(seen.len(), k.pow(n as u32), "every n-gram exactly once");
    }
    println!(
        "de Bruijn B(2,4) = {}",
        de_bruijn(4, 2)
            .iter()
            .map(|d| d.to_string())
            .collect::<String>()
    );
    println!("all generators verified");
}
