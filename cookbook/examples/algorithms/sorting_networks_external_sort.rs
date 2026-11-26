//! RECIPE 276 (algorithms): TAOCP Vol 3 sorting: sorting networks (0-1 principle), merge insertion, replacement selection, external k-way merge, optimal BST (Knuth).
//! LEVEL: advanced
//! Use in: SIMD/GPU sorting kernels, tiny fixed-size sorts, sorting data larger than RAM, database run generation, search-tree design.   Run: cargo run --release --example sorting_networks_external_sort
#![allow(dead_code)]
//!
//! WHAT: five chapters of Knuth's The Art of Computer Programming, Volume 3 (Sorting and Searching), each self-checked.
//!  1. SORTING NETWORKS: a fixed list of comparators (i, j) meaning "put the min on wire i and the max on wire j". The
//!     comparator sequence does not depend on the data, so it is branch-free and SIMD/GPU friendly. We generate bubble,
//!     odd-even transposition, Batcher's BITONIC sorter and Batcher's ODD-EVEN MERGE sorter, count comparators and depth
//!     (parallel rounds), and prove each correct with the 0-1 PRINCIPLE: a network sorts every input iff it sorts all
//!     2^n inputs made of 0s and 1s (Knuth 5.3.4, Theorem Z). That reduces n! permutations to 2^n tests; we test n <= 16
//!     exhaustively and show that deleting any one comparator from a minimal network is caught.
//!  2. MERGE INSERTION (Ford-Johnson): pair up, sort the winners recursively, then insert the losers in Jacobsthal order
//!     (1,3,5,11,21,...) so every binary search is over exactly 2^k - 1 slots. A counting comparator shows it matches
//!     the information-theoretic bound ceil(log2 n!) for n <= 11 (and is one over at n = 12, which is provably optimal).
//!  3. REPLACEMENT SELECTION: a heap of M records tagged with a run number; a record smaller than the one just output must
//!     wait for the next run. On random data runs average 2M (Knuth 5.4.1: the snowplow argument); on sorted data a single
//!     run; on reversed data runs of exactly M.
//!  4. EXTERNAL k-WAY MERGE SORT on a simulated disk (blocks, read/write counters): replacement selection makes the runs,
//!     then each pass merges k runs at a time with a heap and one input buffer block per run. Passes = ceil(log_k(runs)),
//!     and every pass reads and writes every block once, so total I/O = blocks * 2 * (passes + 1). Larger k = fewer passes
//!     but more buffers (k+1 blocks of RAM).
//!  5. OPTIMAL BINARY SEARCH TREE: choose roots minimising sum p_i * depth_i. The O(n^3) DP, then KNUTH's observation that
//!     root[i][j-1] <= root[i][j] <= root[i+1][j], which makes it O(n^2). Equal cost asserted on 500 random tables, then
//!     compared with a sorted-insert (degenerate) BST, a random-insert BST and a perfectly balanced BST.
//!
//! WHY: when a problem is 'sort fixed small arrays', 'sort more than fits in memory' or 'build the best static search
//! structure', these are the textbook answers; every one is also an ORACLE-friendly algorithm (0-1 principle, counting).
//!
//! COMMON USES: sorting networks in SIMD/GPU sort kernels and small fixed-size sorts (e.g. sorting 3-16 keys inside
//! larger sort implementations); replacement selection and k-way merging for run generation and merge phases in database
//! engines and in external sorts such as GNU sort's merge of temporary files; optimal/weight-balanced BSTs for static
//! dictionaries with known access frequencies (compilers' keyword lookup, Huffman-like alphabetic codes).
//! Read: The Art of Computer Programming Vol 3 (Sorting and Searching): 5.3.4 (networks), 5.3.1 (merge insertion), 5.4.1 (replacement selection), 5.4 (external sorting), 6.2.2 (optimal BSTs)
//! Read: The Algorithm Design Manual (Skiena), catalog entries "Sorting" and "Searching" and the chapter on dynamic programming
//! Read: https://en.wikipedia.org/wiki/Sorting_network
//! Read: https://en.wikipedia.org/wiki/Merge-insertion_sort
//! Read: https://en.wikipedia.org/wiki/Optimal_binary_search_tree
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::time::Instant;

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            v.swap(i, self.below(i as u64 + 1) as usize);
        }
    }
}

// ================================================================= 1. sorting networks
/// (a, b): afterwards wire a holds the min and wire b the max. Need not have a < b.
type Net = Vec<(usize, usize)>;

fn bubble_net(n: usize) -> Net {
    let mut v = vec![];
    for i in 0..n {
        for j in 0..n - 1 - i {
            v.push((j, j + 1));
        }
    }
    v
}
fn odd_even_transposition(n: usize) -> Net {
    let mut v = vec![];
    for round in 0..n {
        let mut j = round % 2;
        while j + 1 < n {
            v.push((j, j + 1));
            j += 2;
        }
    }
    v
}
/// Batcher's bitonic sorter; n must be a power of two. Descending sub-blocks are expressed by reversing the comparator.
fn bitonic_net(n: usize) -> Net {
    assert!(n.is_power_of_two());
    let mut v = vec![];
    let mut k = 2;
    while k <= n {
        let mut j = k / 2;
        while j > 0 {
            for i in 0..n {
                let l = i ^ j;
                if l > i {
                    v.push(if i & k == 0 { (i, l) } else { (l, i) });
                }
            }
            j /= 2;
        }
        k *= 2;
    }
    v
}
/// Batcher's odd-even merge sort; works for any n (comparators beyond n are dropped).
fn odd_even_merge_net(n: usize) -> Net {
    let mut v = vec![];
    let mut p = 1;
    while p < n {
        let mut k = p;
        while k >= 1 {
            let mut j = k % p;
            while j + k < n {
                for i in 0..k.min(n - j - k) {
                    if (i + j) / (2 * p) == (i + j + k) / (2 * p) {
                        v.push((i + j, i + j + k));
                    }
                }
                j += 2 * k;
            }
            k /= 2;
        }
        p *= 2;
    }
    v
}
/// Parallel depth: the longest chain of comparators sharing wires.
fn depth(net: &Net, n: usize) -> usize {
    let mut level = vec![0usize; n];
    for &(a, b) in net {
        let l = level[a].max(level[b]) + 1;
        level[a] = l;
        level[b] = l;
    }
    level.into_iter().max().unwrap_or(0)
}
/// Apply a network to an array with no data-dependent branch (min/max compile to cmov / vector min,max).
fn apply_net<T: Ord + Copy>(net: &Net, a: &mut [T]) {
    for &(i, j) in net {
        let (x, y) = (a[i], a[j]);
        a[i] = x.min(y);
        a[j] = x.max(y);
    }
}
/// 0-1 principle: test all 2^n binary inputs; an input word has bit w = value on wire w. Sorted <=> ascending wires.
/// Returns the first failing input, if any.
fn zero_one_check(net: &Net, n: usize) -> Option<u32> {
    for input in 0u32..(1 << n) {
        let mut w = input;
        for &(a, b) in net {
            let (x, y) = ((w >> a) & 1, (w >> b) & 1);
            // wire a must get min(x,y) = x & y, wire b gets max = x | y
            let (lo, hi) = (x & y, x | y);
            w = (w & !(1 << a) & !(1 << b)) | (lo << a) | (hi << b);
        }
        // sorted ascending: zeros on low wires, so w must look like 1..10..0 from the top: w = ones in top positions
        let ones = w.count_ones();
        let want = if ones == 0 {
            0
        } else {
            ((1u32 << ones) - 1) << (n as u32 - ones)
        };
        if w != want {
            return Some(input);
        }
    }
    None
}

// ================================================================= 2. merge insertion
fn fj_sort(v: &[usize], cmp: &mut u64) -> Vec<usize> {
    let n = v.len();
    if n <= 1 {
        return v.to_vec();
    }
    let m = n / 2;
    let mut pairs = Vec::with_capacity(m); // (winner, loser)
    for i in 0..m {
        let (a, b) = (v[2 * i], v[2 * i + 1]);
        *cmp += 1;
        pairs.push(if a < b { (b, a) } else { (a, b) });
    }
    let straggler = if n % 2 == 1 { Some(v[n - 1]) } else { None };
    let winners: Vec<usize> = pairs.iter().map(|p| p.0).collect();
    let sw = fj_sort(&winners, cmp);
    let loser_of: HashMap<usize, usize> = pairs.into_iter().collect();
    let mut chain = Vec::with_capacity(n);
    chain.push(loser_of[&sw[0]]); // l1 < w1 < everything: free
    chain.extend(&sw);
    let total = m + straggler.is_some() as usize;
    let (mut tprev, mut tk) = (1usize, 3usize);
    while tprev < total {
        for i in (tprev + 1..=tk.min(total)).rev() {
            let (x, bound) = if i <= m {
                let w = sw[i - 1];
                (loser_of[&w], chain.iter().position(|&c| c == w).unwrap())
            } else {
                (straggler.unwrap(), chain.len())
            };
            let (mut lo, mut hi) = (0, bound);
            while lo < hi {
                let mid = (lo + hi) / 2;
                *cmp += 1;
                if x < chain[mid] {
                    hi = mid
                } else {
                    lo = mid + 1
                }
            }
            chain.insert(lo, x);
        }
        let next = tk + 2 * tprev; // Jacobsthal: 1, 3, 5, 11, 21, 43, ...
        tprev = tk;
        tk = next;
    }
    chain
}
fn ceil_log2_factorial(n: usize) -> u32 {
    let lg: f64 = (2..=n).map(|k| (k as f64).log2()).sum();
    (lg - 1e-9).ceil() as u32
}
/// Ford-Johnson worst case F(n) = sum_{k=1}^n ceil(log2(3k/4)).
fn fj_worst_formula(n: usize) -> u32 {
    (1..=n)
        .map(|k| ((3 * k) as f64 / 4.0).log2().ceil() as u32)
        .sum()
}
fn permutations(n: usize, f: &mut impl FnMut(&[usize])) {
    fn go(a: &mut Vec<usize>, k: usize, f: &mut impl FnMut(&[usize])) {
        if k == a.len() {
            f(a);
            return;
        }
        for i in k..a.len() {
            a.swap(k, i);
            go(a, k + 1, f);
            a.swap(k, i);
        }
    }
    go(&mut (0..n).collect(), 0, f);
}

// ================================================================= 3. replacement selection
fn replacement_selection(input: &[u64], m: usize) -> Vec<Vec<u64>> {
    let mut heap: BinaryHeap<Reverse<(usize, u64)>> = BinaryHeap::new(); // (run, key)
    let mut it = input.iter();
    for &x in it.by_ref().take(m) {
        heap.push(Reverse((0, x)));
    }
    let mut runs: Vec<Vec<u64>> = vec![vec![]];
    while let Some(Reverse((run, key))) = heap.pop() {
        while runs.len() <= run {
            runs.push(vec![]);
        }
        runs[run].push(key);
        if let Some(&x) = it.next() {
            // x can still join the current run only if it is not smaller than what we just wrote
            heap.push(Reverse((if x >= key { run } else { run + 1 }, x)));
        }
    }
    runs.retain(|r| !r.is_empty());
    runs
}

// ================================================================= 4. external merge sort over a simulated disk
const B: usize = 64; // records per block
struct Disk {
    blocks: Vec<Vec<u64>>,
    reads: u64,
    writes: u64,
}
impl Disk {
    fn write(&mut self, b: Vec<u64>) -> usize {
        self.writes += 1;
        self.blocks.push(b);
        self.blocks.len() - 1
    }
    fn read(&mut self, id: usize) -> Vec<u64> {
        self.reads += 1;
        self.blocks[id].clone()
    }
}
#[derive(Clone)]
struct Run {
    blocks: Vec<usize>,
}
fn write_run(disk: &mut Disk, data: &[u64]) -> Run {
    Run {
        blocks: data.chunks(B).map(|c| disk.write(c.to_vec())).collect(),
    }
}
struct Reader {
    blocks: Vec<usize>,
    next: usize,
    buf: Vec<u64>,
    pos: usize,
}
impl Reader {
    fn new(run: &Run, disk: &mut Disk) -> Self {
        let mut r = Reader {
            blocks: run.blocks.clone(),
            next: 0,
            buf: vec![],
            pos: 0,
        };
        r.refill(disk);
        r
    }
    fn refill(&mut self, disk: &mut Disk) {
        if self.next < self.blocks.len() {
            self.buf = disk.read(self.blocks[self.next]);
            self.next += 1;
            self.pos = 0;
        } else {
            self.buf.clear();
        }
    }
    fn pop(&mut self, disk: &mut Disk) -> Option<u64> {
        if self.pos >= self.buf.len() {
            return None;
        }
        let v = self.buf[self.pos];
        self.pos += 1;
        if self.pos >= self.buf.len() {
            self.refill(disk);
        }
        Some(v)
    }
}
fn merge_runs(disk: &mut Disk, runs: &[Run]) -> Run {
    let mut readers: Vec<Reader> = runs.iter().map(|r| Reader::new(r, disk)).collect();
    let mut heap = BinaryHeap::new();
    for (i, r) in readers.iter_mut().enumerate() {
        if let Some(v) = r.pop(disk) {
            heap.push(Reverse((v, i)));
        }
    }
    let (mut out, mut ids) = (Vec::with_capacity(B), vec![]);
    while let Some(Reverse((v, i))) = heap.pop() {
        out.push(v);
        if out.len() == B {
            ids.push(disk.write(std::mem::take(&mut out)));
        }
        if let Some(nv) = readers[i].pop(disk) {
            heap.push(Reverse((nv, i)));
        }
    }
    if !out.is_empty() {
        ids.push(disk.write(out));
    }
    Run { blocks: ids }
}
/// Returns (sorted data, passes, initial runs, reads, writes).
fn external_sort(data: &[u64], mem_records: usize, k: usize) -> (Vec<u64>, u32, usize, u64, u64) {
    let mut disk = Disk {
        blocks: vec![],
        reads: 0,
        writes: 0,
    };
    let mut runs: Vec<Run> = replacement_selection(data, mem_records)
        .iter()
        .map(|r| write_run(&mut disk, r))
        .collect();
    let initial = runs.len();
    let mut passes = 0;
    while runs.len() > 1 {
        let mut next = vec![];
        for group in runs.chunks(k) {
            next.push(if group.len() == 1 {
                group[0].clone()
            } else {
                merge_runs(&mut disk, group)
            });
        }
        runs = next;
        passes += 1;
    }
    let mut out = vec![];
    if let Some(r) = runs.first() {
        for &b in &r.blocks {
            out.extend(disk.read(b));
        }
    }
    (out, passes, initial, disk.reads, disk.writes)
}
fn ceil_log(runs: usize, k: usize) -> u32 {
    let (mut p, mut cap) = (0, 1usize);
    while cap < runs {
        cap *= k;
        p += 1;
    }
    p
}

// ================================================================= 5. optimal BST
/// Classic cubic DP. p[0..n) are frequencies (successful searches only). cost[i][j] over keys i..j (half-open).
fn obst_cubic(p: &[u64]) -> (u64, Vec<Vec<usize>>) {
    let n = p.len();
    let pre: Vec<u64> = std::iter::once(0)
        .chain(p.iter().scan(0, |s, &x| {
            *s += x;
            Some(*s)
        }))
        .collect();
    let mut cost = vec![vec![0u64; n + 1]; n + 1];
    let mut root = vec![vec![0usize; n + 1]; n + 1];
    for len in 1..=n {
        for i in 0..=n - len {
            let j = i + len;
            let (mut best, mut br) = (u64::MAX, i);
            for r in i..j {
                let c = cost[i][r] + cost[r + 1][j];
                if c < best {
                    best = c;
                    br = r;
                }
            }
            cost[i][j] = best + pre[j] - pre[i];
            root[i][j] = br;
        }
    }
    (cost[0][n], root)
}
/// Knuth: the optimal root of (i,j) lies between the optimal roots of (i,j-1) and (i+1,j): amortised O(n^2).
fn obst_knuth(p: &[u64]) -> (u64, Vec<Vec<usize>>) {
    let n = p.len();
    let pre: Vec<u64> = std::iter::once(0)
        .chain(p.iter().scan(0, |s, &x| {
            *s += x;
            Some(*s)
        }))
        .collect();
    let mut cost = vec![vec![0u64; n + 1]; n + 1];
    let mut root = vec![vec![0usize; n + 1]; n + 1];
    for i in 0..n {
        cost[i][i + 1] = p[i];
        root[i][i + 1] = i;
    }
    for len in 2..=n {
        for i in 0..=n - len {
            let j = i + len;
            let (mut best, mut br) = (u64::MAX, i);
            for r in root[i][j - 1]..=root[i + 1][j] {
                let c = cost[i][r] + cost[r + 1][j];
                if c < best {
                    best = c;
                    br = r;
                }
            }
            cost[i][j] = best + pre[j] - pre[i];
            root[i][j] = br;
        }
    }
    (cost[0][n], root)
}
/// sum p_i * depth_i (root depth 1) of the tree described by a root table.
fn tree_cost(p: &[u64], root: &[Vec<usize>], i: usize, j: usize, d: u64) -> u64 {
    if i >= j {
        return 0;
    }
    let r = root[i][j];
    p[r] * d + tree_cost(p, root, i, r, d + 1) + tree_cost(p, root, r + 1, j, d + 1)
}
fn balanced_cost(p: &[u64], i: usize, j: usize, d: u64) -> u64 {
    if i >= j {
        return 0;
    }
    let r = (i + j) / 2;
    p[r] * d + balanced_cost(p, i, r, d + 1) + balanced_cost(p, r + 1, j, d + 1)
}
/// Plain unbalanced BST built by inserting key indices in the given order.
fn insertion_bst_cost(p: &[u64], order: &[usize]) -> u64 {
    let n = p.len();
    let (mut l, mut r) = (vec![usize::MAX; n], vec![usize::MAX; n]);
    let mut total = 0;
    let root = order[0];
    total += p[root];
    for &k in &order[1..] {
        let (mut cur, mut d) = (root, 1);
        loop {
            d += 1;
            let slot = if k < cur { &mut l[cur] } else { &mut r[cur] };
            if *slot == usize::MAX {
                *slot = k;
                break;
            }
            cur = *slot;
        }
        total += p[k] * d;
    }
    total
}

fn main() {
    let mut rng = Rng::new(276);

    // ---- 1. networks
    println!(
        "== sorting networks (comparators / depth), all verified by the 0-1 principle (2^n inputs)"
    );
    println!("   n | bubble | odd-even transp. | bitonic | Batcher odd-even merge");
    for n in [2usize, 3, 4, 5, 6, 7, 8, 12, 16] {
        let (b, t, m) = (
            bubble_net(n),
            odd_even_transposition(n),
            odd_even_merge_net(n),
        );
        let bit = if n.is_power_of_two() {
            Some(bitonic_net(n))
        } else {
            None
        };
        for net in [&b, &t, &m].into_iter().chain(bit.as_ref()) {
            assert_eq!(zero_one_check(net, n), None, "n={n}");
        }
        let cell = |net: &Net| format!("{:>3}/{:<2}", net.len(), depth(net, n));
        println!(
            "  {n:>2} | {} | {}        | {} | {}",
            cell(&b),
            cell(&t),
            bit.as_ref().map_or("   -  ".into(), |x| cell(x)),
            cell(&m)
        );
    }
    assert_eq!(odd_even_merge_net(8).len(), 19); // matches the best known size for 8 wires
    assert_eq!(odd_even_merge_net(16).len(), 63);
    assert_eq!(bitonic_net(16).len(), 80);
    assert_eq!(depth(&odd_even_merge_net(16), 16), 10);
    assert_eq!(depth(&bitonic_net(16), 16), 10);
    // sorting all permutations of 8 through the 19-comparator network, and the 0-1 check's power
    let net8 = odd_even_merge_net(8);
    let mut seen = 0;
    permutations(8, &mut |p| {
        let mut a: Vec<usize> = p.to_vec();
        apply_net(&net8, &mut a);
        assert!(a.windows(2).all(|w| w[0] < w[1]));
        seen += 1;
    });
    assert_eq!(seen, 40_320);
    // delete each comparator in turn: how many deletions break the network? (a minimal network breaks every time)
    let net = odd_even_merge_net(8);
    let broken = (0..net.len())
        .filter(|&skip| {
            let cut: Net = net
                .iter()
                .enumerate()
                .filter(|&(i, _)| i != skip)
                .map(|(_, &c)| c)
                .collect();
            zero_one_check(&cut, 8).is_some()
        })
        .count();
    println!(
        "  deleting one comparator of the 19-comparator 8-wire network: {broken}/19 deletions are caught by the 0-1 test"
    );
    assert!(broken >= 15);
    // random arrays of u32 with n = 16
    let net16 = odd_even_merge_net(16);
    let t = Instant::now();
    for _ in 0..20_000 {
        let mut a = [0u32; 16];
        for x in a.iter_mut() {
            *x = rng.next() as u32 % 1000;
        }
        let mut want = a;
        want.sort();
        apply_net(&net16, &mut a);
        assert_eq!(a, want);
    }
    println!(
        "  20000 random 16-element arrays sorted by the 63-comparator network and checked in {:?}",
        t.elapsed()
    );

    // ---- 2. merge insertion
    println!("== merge insertion (Ford-Johnson) comparisons: worst over inputs vs ceil(log2 n!)");
    println!("   n | lower bound | F(n) formula | FJ worst observed | binary-insertion worst");
    for n in 1..=12usize {
        let mut worst = 0u64;
        let mut check = |p: &[usize]| {
            let mut c = 0;
            let s = fj_sort(p, &mut c);
            assert!(s.windows(2).all(|w| w[0] < w[1]), "unsorted");
            worst = worst.max(c);
        };
        if n <= 8 {
            permutations(n, &mut |p| check(p));
        } else {
            for _ in 0..30_000 {
                let mut p: Vec<usize> = (0..n).collect();
                rng.shuffle(&mut p);
                check(&p);
            }
        }
        let (lb, f) = (ceil_log2_factorial(n), fj_worst_formula(n));
        let bin: u32 = (1..=n).map(|k| (k as f64).log2().ceil() as u32).sum();
        println!("  {n:>2} | {lb:>11} | {f:>12} | {worst:>17} | {bin:>5}");
        assert!(worst as u32 <= f && lb <= f);
        if n <= 8 {
            assert_eq!(
                worst as u32, f,
                "exhaustive worst case must equal the formula"
            );
        }
        if n <= 11 {
            assert_eq!(
                lb, f,
                "FJ meets the information-theoretic bound for n <= 11"
            );
        }
    }
    assert_eq!(fj_worst_formula(12), 30);
    assert_eq!(ceil_log2_factorial(12), 29);

    // ---- 3. replacement selection
    println!("== replacement selection run formation, M = 1000 records of memory");
    let m = 1000;
    let rand_data: Vec<u64> = (0..400_000).map(|_| rng.next()).collect();
    let runs = replacement_selection(&rand_data, m);
    assert!(runs.iter().all(|r| r.windows(2).all(|w| w[0] <= w[1])));
    assert_eq!(runs.iter().map(|r| r.len()).sum::<usize>(), rand_data.len());
    let body = &runs[..runs.len() - 1];
    let avg = body.iter().map(|r| r.len()).sum::<usize>() as f64 / body.len() as f64;
    println!(
        "  random: {} runs, average length {avg:.0} = {:.2} x M (theory: 2 M)",
        runs.len(),
        avg / m as f64
    );
    assert!((1.8..2.2).contains(&(avg / m as f64)));
    let sorted: Vec<u64> = (0..50_000).collect();
    assert_eq!(replacement_selection(&sorted, m).len(), 1);
    let rev: Vec<u64> = (0..50_000).rev().collect();
    let rr = replacement_selection(&rev, m);
    assert!(rr[..rr.len() - 1].iter().all(|r| r.len() == m));
    println!(
        "  sorted input: 1 run; reversed input: {} runs of exactly M (worst case)",
        rr.len()
    );

    // ---- 4. external sort
    println!(
        "== external k-way merge sort, N = 200000 records, block = {B} records, memory = 1000 records"
    );
    let data: Vec<u64> = (0..200_000).map(|_| rng.below(1_000_000_000)).collect();
    let mut want = data.clone();
    want.sort();
    let blocks = data.len().div_ceil(B) as u64;
    println!("   k | initial runs | passes | reads | writes | buffers needed");
    for k in [2usize, 4, 8, 16, 64, 256] {
        let (out, passes, runs0, reads, writes) = external_sort(&data, 1000, k);
        assert_eq!(out, want);
        assert_eq!(passes, ceil_log(runs0, k), "passes = ceil(log_k runs)");
        // each merge pass touches every block about once (partial blocks at run ends add a little)
        assert!(reads <= (passes as u64 + 1) * (blocks + runs0 as u64));
        assert!(reads >= passes as u64 * blocks);
        println!(
            "  {k:>3} | {runs0:>12} | {passes:>6} | {reads:>5} | {writes:>6} | {}",
            k + 1
        );
    }
    let (_, p2, r0, _, _) = external_sort(&data, 1000, 2);
    assert!(r0 > 64 && p2 >= 7);

    // ---- 5. optimal BST
    println!("== optimal BST: Knuth O(n^2) vs O(n^3) DP");
    for _ in 0..500 {
        let n = 1 + rng.below(40) as usize;
        let p: Vec<u64> = (0..n).map(|_| 1 + rng.below(1000)).collect();
        let (c3, r3) = obst_cubic(&p);
        let (c2, r2) = obst_knuth(&p);
        assert_eq!(c3, c2, "{p:?}");
        assert_eq!(tree_cost(&p, &r2, 0, n, 1), c2);
        assert_eq!(tree_cost(&p, &r3, 0, n, 1), c3);
    }
    println!(
        "  500 random tables (n <= 40): equal optimal cost, and each tree's depth-weighted cost equals the DP value"
    );
    let n = 600;
    let p: Vec<u64> = (0..n).map(|_| 1 + rng.below(100_000)).collect();
    let (t3, t2) = (Instant::now(), ());
    let (c3, _) = obst_cubic(&p);
    let d3 = t3.elapsed();
    let t = Instant::now();
    let (c2, _) = obst_knuth(&p);
    let _ = t2;
    println!(
        "  n = {n}: cubic {d3:?}, Knuth {:?}, same cost {}",
        t.elapsed(),
        c3 == c2
    );
    assert_eq!(c3, c2);
    // expected search cost for Zipf-like frequencies assigned to keys in random order
    let n = 255usize;
    let mut p: Vec<u64> = (1..=n as u64).map(|r| 1_000_000 / r).collect();
    rng.shuffle(&mut p);
    let total: u64 = p.iter().sum();
    let (opt, _) = obst_knuth(&p);
    let sorted_ins = insertion_bst_cost(&p, &(0..n).collect::<Vec<_>>());
    let mut order: Vec<usize> = (0..n).collect();
    rng.shuffle(&mut order);
    let random_ins = insertion_bst_cost(&p, &order);
    let balanced = balanced_cost(&p, 0, n, 1);
    let e = |c: u64| c as f64 / total as f64;
    println!(
        "  expected comparisons per search, {n} keys with Zipf weights: optimal {:.2}, balanced {:.2}, random-insert {:.2}, sorted-insert {:.2}",
        e(opt),
        e(balanced),
        e(random_ins),
        e(sorted_ins)
    );
    assert!(opt <= balanced && opt <= random_ins && opt < sorted_ins);
    assert!(e(balanced) < e(sorted_ins));
    println!("OK");
}
