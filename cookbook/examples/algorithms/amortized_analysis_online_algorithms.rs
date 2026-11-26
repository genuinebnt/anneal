//! RECIPE 889 (algorithms): amortised analysis (aggregate / accounting / potential) and online algorithms (competitive analysis).
//! LEVEL: advanced
//! Use in: sizing growth policies of Vec/hash tables, proving "O(1) amortised" claims, cache/page replacement policy choice, rent-or-buy decisions, load balancing, ad allocation.   Run: cargo run --release --example amortized_analysis_online_algorithms
//! COMMON USES: Vec/String/HashMap growth (std doubles capacity), binary counters and carry chains, queues built from two stacks, union-find in Kruskal/connected components, splay trees and self-adjusting caches, OS page replacement (LRU/CLOCK approximate it), CDN/buffer-pool caches, list schedulers (greedy list scheduling), ad-allocation (online matching).
//! Course: CMU 15-451/651 (amortised analysis and online algorithms lectures); Sleator and Tarjan, "Amortized efficiency of list update and paging rules" (1985); Karp, Vazirani, Vazirani (1990) for RANKING.
#![allow(dead_code)]
//!
//! PART 1, amortised analysis. A sequence of n operations costs T(n); the amortised cost is T(n)/n, a guarantee for the
//! WORST sequence (no probability involved). Three proof methods, each demonstrated by an instrumented structure that
//! CHECKS the proof at runtime:
//!   - aggregate: count everything, divide. Dynamic array: doubling copies < 2n elements in total; additive growth
//!     (+k) copies ~ n^2/(2k), which is why growth must be multiplicative.
//!   - accounting (banker's): charge each operation a fixed number of credits, store unspent credits on elements, and
//!     check the bank never goes negative. Two-stack queue: 4 credits per element pay for push, pop, re-push, pop.
//!   - potential (physicist's): amortised = actual + (Phi_after - Phi_before) with Phi >= Phi_0. Binary counter with
//!     Phi = number of one bits gives EXACTLY 2 per increment; doubling array with Phi = 2*len - cap gives exactly 3.
//!   Also: union-find with union by size + path halving (finds bounded by log2 n steps each, checked), and a splay
//!   tree whose rotation count is checked against the Sleator-Tarjan bound m(3 log2 n + 1) + n log2 n.
//!   Fibonacci heap decrease-key (described only): cut the node and cascade cuts upward through nodes that already lost a
//!   child (marks). Phi = trees + 2*marked; a cascade of c cuts pays for itself because each cut un-marks a node, so
//!   decrease-key is O(1) amortised and extract-min O(log n) amortised, which gives Dijkstra O(E + V log V).
//!
//! PART 2, online algorithms. The input arrives piece by piece and decisions are irrevocable. We measure the
//! COMPETITIVE RATIO: worst case of ALG(sigma)/OPT(sigma) where OPT is the offline optimum that sees the whole input
//! (an "adversary"). We assert the proven bounds on 2000 random instances and on the adversarial constructions:
//!   - ski rental: deterministic break-even rule is (2 - 1/B)-competitive; the randomised strategy of Karlin et al.
//!     achieves 1/(1-(1-1/B)^B) -> e/(e-1) = 1.58, the best possible against an oblivious adversary.
//!   - paging: LRU and FIFO are k-competitive (up to an additive k); Belady's farthest-in-future is optimal offline; the
//!     adversary that always requests the page missing from the cache forces LRU to fault on EVERY request.
//!   - list update: move-to-front cost <= 2*C_A - m + n(n-1)/2 for any static ordering A.
//!   - online bipartite matching: greedy is 1/2 (tight instance below); RANKING (one random permutation) gets 1-1/e.
//!   - load balancing: greedy list scheduling is (2 - 1/m)-competitive, tight example m(m-1) unit jobs then one of size m.
//! WHY the order matters: the potential function is the same trick in both halves (list update, paging and splay
//! all use "inversions"/"log sizes" as Phi), so reading the two parts together shows one proof technique.
//! Read: https://en.wikipedia.org/wiki/Amortized_analysis ; https://en.wikipedia.org/wiki/Competitive_analysis_%28online_algorithm%29 ;
//! Sleator and Tarjan 1985 (cited by title only, not re-read for this recipe); Borodin and El-Yaniv, "Online Computation and Competitive Analysis" (book, title only).
//!
//! TRY NEXT:
//!   - [S] Change `Growth::Doubling` to a factor of 1.5 in `DynArray::push` and re-derive the per-push constant; update the
//!         asserted potential (hint: Phi = 3*len - 2*cap?) and find the amortised cost; predict, then run.
//!   - [S] Predict how many rotations `splay_demo` needs for random keys versus keys 0..n repeated in order, then add a
//!         third access pattern (a working set of 8 keys) and compare with the "working set theorem" prediction.
//!   - [M] Add FWF (flush-when-full) to the paging section and show on the adversarial sequence and on random sequences
//!         that it is also k-competitive but faults more than LRU on average; extend `paging_random` to assert it.
//!   - [M] Real-world port: make `Ranking` choose among neighbours for ad slots with budgets (weights) and read about
//!         the balance/water-filling variant (Mehta et al., AdWords); compare against greedy on a skewed instance.
//!   - [A] PROJECT: combine this recipe with `sorting_searching` and `thread_pool_work_stealing`: build a simulated
//!         scheduler that assigns sorting jobs online with greedy list scheduling, measure its makespan against the
//!         offline LPT order, and assert the (2 - 1/m) bound across thread counts.

use std::collections::VecDeque;

// ---------------------------------------------------------------------------------------------------------------------
// Seeded randomness (xorshift64*), so every run is identical.
// ---------------------------------------------------------------------------------------------------------------------
struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn chance(&mut self, p: f64) -> bool {
        ((self.next() >> 11) as f64 / (1u64 << 53) as f64) < p
    }
    fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = self.below(i + 1);
            v.swap(i, j);
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// 1a. Dynamic array growth: aggregate and potential method.
// ---------------------------------------------------------------------------------------------------------------------
#[derive(Clone, Copy)]
enum Growth {
    Doubling,
    Additive(usize),
}

/// A counting model of a growable array: it stores no data, only length, capacity and the number of element copies
/// made by reallocations. That is all the cost model needs.
struct DynArray {
    len: usize,
    cap: usize,
    copies: u64,
    policy: Growth,
}

impl DynArray {
    fn new(policy: Growth) -> Self {
        DynArray {
            len: 0,
            cap: 1,
            copies: 0,
            policy,
        }
    }
    /// Returns the ACTUAL cost of the push: 1 write plus the elements copied by a reallocation.
    fn push(&mut self) -> u64 {
        let mut cost = 1;
        if self.len == self.cap {
            self.cap = match self.policy {
                Growth::Doubling => self.cap * 2,
                Growth::Additive(k) => self.cap + k,
            };
            self.copies += self.len as u64;
            cost += self.len as u64;
        }
        self.len += 1;
        cost
    }
    /// Potential for doubling: Phi = 2*len - cap. After the first push it is >= 0 (see the check in `dynarray_demo`).
    fn phi(&self) -> i64 {
        2 * self.len as i64 - self.cap as i64
    }
}

fn dynarray_demo() {
    // Aggregate: total copies for doubling stay below 2n for every n.
    let mut a = DynArray::new(Growth::Doubling);
    let mut total_actual = 0u64;
    for n in 1..=100_000u64 {
        let phi_before = a.phi();
        let c = a.push();
        total_actual += c;
        let amortised = c as i64 + a.phi() - phi_before;
        // Potential method: every push has amortised cost EXACTLY 3 (1 write + 2 credits), growth or not.
        assert_eq!(amortised, 3, "push {n}");
        assert!(a.copies < 2 * n, "aggregate bound violated at n={n}");
    }
    println!(
        "doubling: n=100000 copies={} (< 2n), total actual cost={}",
        a.copies, total_actual
    );

    // Additive growth: quadratic. Quadrupling n multiplies copies by ~16.
    let run = |n: usize| {
        let mut b = DynArray::new(Growth::Additive(16));
        for _ in 0..n {
            b.push();
        }
        b.copies
    };
    let (c1, c2) = (run(10_000), run(40_000));
    println!(
        "additive(+16): copies n=10k {c1}, n=40k {c2}, ratio {:.1} (quadratic ~ 16)",
        c2 as f64 / c1 as f64
    );
    assert!(
        c2 as f64 / c1 as f64 > 12.0,
        "additive growth should be quadratic"
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// 1b. Binary counter: potential method (Phi = number of one bits).
// ---------------------------------------------------------------------------------------------------------------------
struct Counter {
    bits: Vec<bool>,
}
impl Counter {
    fn ones(&self) -> i64 {
        self.bits.iter().filter(|&&b| b).count() as i64
    }
    /// Returns the actual cost = number of bit flips.
    fn increment(&mut self) -> i64 {
        let mut flips = 0;
        let mut i = 0;
        while i < self.bits.len() && self.bits[i] {
            self.bits[i] = false;
            flips += 1;
            i += 1;
        }
        if i == self.bits.len() {
            self.bits.push(false);
        }
        self.bits[i] = true;
        flips + 1
    }
}

fn counter_demo() {
    let mut c = Counter { bits: vec![false] };
    let mut total = 0;
    let n = 1 << 16;
    for _ in 0..n {
        let before = c.ones();
        let actual = c.increment();
        total += actual;
        // actual = t+1 (t trailing ones); Phi changes by 1 - t; so amortised = 2 exactly.
        assert_eq!(actual + c.ones() - before, 2);
    }
    assert!(total <= 2 * n);
    println!(
        "binary counter: {n} increments cost {total} flips (<= 2n = {})",
        2 * n
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// 1c. Two-stack queue: accounting method (a bank of credits that must never go negative).
// ---------------------------------------------------------------------------------------------------------------------
#[derive(Default)]
struct TwoStackQueue {
    inbox: Vec<u32>,
    outbox: Vec<u32>,
    primitive_ops: u64,
}
impl TwoStackQueue {
    fn enqueue(&mut self, x: u32) {
        self.inbox.push(x);
        self.primitive_ops += 1;
    }
    fn dequeue(&mut self) -> Option<u32> {
        if self.outbox.is_empty() {
            while let Some(x) = self.inbox.pop() {
                self.outbox.push(x);
                self.primitive_ops += 2; // one pop + one push
            }
        }
        let r = self.outbox.pop();
        if r.is_some() {
            self.primitive_ops += 1;
        }
        r
    }
}

fn queue_demo() {
    let mut rng = Rng::new(7);
    for trial in 0..200 {
        let mut q = TwoStackQueue::default();
        let mut model: VecDeque<u32> = VecDeque::new();
        let mut enqueues = 0u64;
        let bias = 0.3 + 0.4 * (trial as f64 / 200.0);
        for step in 0..2000u32 {
            if rng.chance(bias) || model.is_empty() {
                q.enqueue(step);
                model.push_back(step);
                enqueues += 1;
            } else {
                assert_eq!(q.dequeue(), model.pop_front());
            }
            // bank = credits paid (4 per enqueue) minus real work done; must stay non-negative at every step.
            let bank = 4 * enqueues as i64 - q.primitive_ops as i64;
            assert!(bank >= 0, "bank went negative: accounting argument broken");
        }
    }
    println!("two-stack queue: 200 random runs, bank of 4 credits/enqueue never negative");
}

// ---------------------------------------------------------------------------------------------------------------------
// 1d. Union-find: union by size + path halving. Without compression, union by size alone bounds a find by log2 n.
// ---------------------------------------------------------------------------------------------------------------------
struct Dsu {
    parent: Vec<usize>,
    size: Vec<usize>,
    steps: u64,
    finds: u64,
    halve: bool,
}
impl Dsu {
    fn new(n: usize, halve: bool) -> Self {
        Dsu {
            parent: (0..n).collect(),
            size: vec![1; n],
            steps: 0,
            finds: 0,
            halve,
        }
    }
    fn find(&mut self, mut x: usize) -> usize {
        self.finds += 1;
        while self.parent[x] != x {
            if self.halve {
                self.parent[x] = self.parent[self.parent[x]];
            }
            x = self.parent[x];
            self.steps += 1;
        }
        x
    }
    fn union(&mut self, a: usize, b: usize) {
        let (mut a, mut b) = (self.find(a), self.find(b));
        if a == b {
            return;
        }
        if self.size[a] < self.size[b] {
            std::mem::swap(&mut a, &mut b);
        }
        self.parent[b] = a;
        self.size[a] += self.size[b];
    }
}

fn dsu_demo() {
    let mut rng = Rng::new(11);
    for _ in 0..50 {
        let n = 2 + rng.below(500);
        let mut plain = Dsu::new(n, false);
        let mut fast = Dsu::new(n, true);
        let mut label: Vec<usize> = (0..n).collect(); // oracle: relabel on union
        for _ in 0..4 * n {
            let (a, b) = (rng.below(n), rng.below(n));
            if rng.chance(0.5) {
                plain.union(a, b);
                fast.union(a, b);
                let (la, lb) = (label[a], label[b]);
                if la != lb {
                    for l in label.iter_mut() {
                        if *l == lb {
                            *l = la;
                        }
                    }
                }
            } else {
                let same = label[a] == label[b];
                assert_eq!(plain.find(a) == plain.find(b), same);
                assert_eq!(fast.find(a) == fast.find(b), same);
            }
        }
        let bound = (n as f64).log2().floor() as u64;
        assert!(
            plain.steps <= plain.finds * bound.max(1),
            "union by size must bound every find by log2 n"
        );
        assert!(fast.steps <= plain.steps.max(fast.finds * bound.max(1)));
    }
    // A big adversarial-ish run printed for intuition.
    let n = 200_000;
    let (mut plain, mut fast) = (Dsu::new(n, false), Dsu::new(n, true));
    let mut rng = Rng::new(3);
    for _ in 0..n {
        let (a, b) = (rng.below(n), rng.below(n));
        plain.union(a, b);
        fast.union(a, b);
    }
    println!(
        "union-find n=200k: avg steps/find  size-only {:.2}   +path halving {:.2}",
        plain.steps as f64 / plain.finds as f64,
        fast.steps as f64 / fast.finds as f64
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// 1e. Splay tree (arena, parent pointers, bottom-up splay). We only need access + rotation counting.
// ---------------------------------------------------------------------------------------------------------------------
const NIL: usize = usize::MAX;
struct Splay {
    l: Vec<usize>,
    r: Vec<usize>,
    p: Vec<usize>,
    root: usize,
    rotations: u64,
}
impl Splay {
    /// Node i holds key i. Built as a right-leaning CHAIN (the worst shape): potential <= n log2 n.
    fn chain(n: usize) -> Self {
        let mut s = Splay {
            l: vec![NIL; n],
            r: vec![NIL; n],
            p: vec![NIL; n],
            root: 0,
            rotations: 0,
        };
        for i in 0..n - 1 {
            s.r[i] = i + 1;
            s.p[i + 1] = i;
        }
        s
    }
    fn rotate(&mut self, x: usize) {
        let p = self.p[x];
        let g = self.p[p];
        if self.l[p] == x {
            let b = self.r[x];
            self.l[p] = b;
            if b != NIL {
                self.p[b] = p;
            }
            self.r[x] = p;
        } else {
            let b = self.l[x];
            self.r[p] = b;
            if b != NIL {
                self.p[b] = p;
            }
            self.l[x] = p;
        }
        self.p[p] = x;
        self.p[x] = g;
        if g == NIL {
            self.root = x;
        } else if self.l[g] == p {
            self.l[g] = x;
        } else {
            self.r[g] = x;
        }
        self.rotations += 1;
    }
    fn splay(&mut self, x: usize) {
        while self.p[x] != NIL {
            let p = self.p[x];
            let g = self.p[p];
            if g == NIL {
                self.rotate(x); // zig
            } else if (self.l[g] == p) == (self.l[p] == x) {
                self.rotate(p); // zig-zig: rotate the PARENT first (this ordering is what makes the amortised proof work)
                self.rotate(x);
            } else {
                self.rotate(x); // zig-zag
                self.rotate(x);
            }
        }
    }
    fn inorder(&self) -> Vec<usize> {
        let (mut out, mut stack, mut cur) = (vec![], vec![], self.root);
        while cur != NIL || !stack.is_empty() {
            while cur != NIL {
                stack.push(cur);
                cur = self.l[cur];
            }
            let x = stack.pop().unwrap();
            out.push(x);
            cur = self.r[x];
        }
        out
    }
}

fn splay_demo() {
    let n = 4096usize;
    let log = (n as f64).log2();
    let mut rng = Rng::new(99);
    let patterns: Vec<(&str, Vec<usize>)> = vec![
        ("random", (0..4 * n).map(|_| rng.below(n)).collect()),
        ("sequential x4", (0..4 * n).map(|i| i % n).collect()),
        ("same key", vec![17; 4 * n]),
    ];
    for (name, seq) in patterns {
        let mut s = Splay::chain(n);
        for &k in &seq {
            s.splay(k);
            assert_eq!(s.root, k);
        }
        assert_eq!(
            s.inorder(),
            (0..n).collect::<Vec<_>>(),
            "BST order must survive splaying"
        );
        let m = seq.len() as f64;
        let bound = m * (3.0 * log + 1.0) + n as f64 * log;
        assert!(
            (s.rotations as f64) <= bound,
            "{name}: {} rotations > bound {bound}",
            s.rotations
        );
        println!(
            "splay {name:14} {} rotations ({:.2}/access, bound {:.1}/access)",
            s.rotations,
            s.rotations as f64 / m,
            bound / m
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// 2a. Ski rental. Rent costs 1/day, buying costs B; the season length n is unknown.
// ---------------------------------------------------------------------------------------------------------------------
fn ski_opt(b: u64, n: u64) -> u64 {
    n.min(b)
}
/// Deterministic break-even: rent for B-1 days, buy on day B.
fn ski_det(b: u64, n: u64) -> u64 {
    if n < b { n } else { (b - 1) + b }
}
/// Strategy i: rent i-1 days, buy on day i (if the season lasts that long).
fn ski_strategy(b: u64, i: u64, n: u64) -> u64 {
    if n < i { n } else { (i - 1) + b }
}
/// Expected cost of Karlin et al.'s randomised rule: p_i proportional to ((B-1)/B)^(B-i), i = 1..B.
fn ski_random_expected(b: u64, n: u64) -> f64 {
    let q = (b as f64 - 1.0) / b as f64;
    let norm = b as f64 * (1.0 - q.powi(b as i32));
    let mut psum = 0.0;
    let mut e = 0.0;
    for i in 1..=b {
        let p = q.powi((b - i) as i32) / norm;
        psum += p;
        e += p * ski_strategy(b, i, n) as f64;
    }
    assert!((psum - 1.0).abs() < 1e-9, "probabilities must sum to 1");
    e
}

fn ski_demo() {
    let (mut worst_det, mut worst_rand) = (0f64, 0f64);
    for b in 1..=40u64 {
        let det_bound = 2.0 - 1.0 / b as f64;
        let rand_bound = 1.0 / (1.0 - (1.0 - 1.0 / b as f64).powi(b as i32));
        for n in 1..=4 * b {
            let opt = ski_opt(b, n) as f64;
            let d = ski_det(b, n) as f64 / opt;
            let r = ski_random_expected(b, n) / opt;
            assert!(
                d <= det_bound + 1e-12,
                "det ratio {d} > {det_bound} (B={b}, n={n})"
            );
            assert!(
                r <= rand_bound + 1e-9,
                "rand ratio {r} > {rand_bound} (B={b}, n={n})"
            );
            if b == 40 {
                worst_det = worst_det.max(d);
                worst_rand = worst_rand.max(r);
            }
        }
    }
    println!(
        "ski rental B=40: worst deterministic {worst_det:.3} (bound 2-1/B = {:.3}), worst randomised {worst_rand:.3} (e/(e-1) = {:.3})",
        2.0 - 1.0 / 40.0,
        std::f64::consts::E / (std::f64::consts::E - 1.0)
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// 2b. Paging: LRU, FIFO, and the offline optimum (Belady: evict the page requested farthest in the future).
// ---------------------------------------------------------------------------------------------------------------------
fn lru_faults(seq: &[usize], k: usize) -> usize {
    let mut cache: Vec<usize> = Vec::new(); // most recently used last
    let mut faults = 0;
    for &x in seq {
        if let Some(pos) = cache.iter().position(|&c| c == x) {
            cache.remove(pos);
        } else {
            faults += 1;
            if cache.len() == k {
                cache.remove(0);
            }
        }
        cache.push(x);
    }
    faults
}

fn fifo_faults(seq: &[usize], k: usize) -> usize {
    let mut cache: VecDeque<usize> = VecDeque::new();
    let mut faults = 0;
    for &x in seq {
        if !cache.contains(&x) {
            faults += 1;
            if cache.len() == k {
                cache.pop_front();
            }
            cache.push_back(x);
        }
    }
    faults
}

fn belady_faults(seq: &[usize], k: usize) -> usize {
    let mut cache: Vec<usize> = Vec::new();
    let mut faults = 0;
    for (t, &x) in seq.iter().enumerate() {
        if cache.contains(&x) {
            continue;
        }
        faults += 1;
        if cache.len() == k {
            // next use time of each cached page (usize::MAX = never again => perfect victim)
            let next = |p: usize| {
                seq[t + 1..]
                    .iter()
                    .position(|&y| y == p)
                    .map_or(usize::MAX, |d| d)
            };
            let victim = (0..k).max_by_key(|&i| next(cache[i])).unwrap();
            cache.swap_remove(victim);
        }
        cache.push(x);
    }
    faults
}

/// The adversary: k+1 pages, always request the page that is NOT in the (deterministic) algorithm's cache.
fn adversarial_for_lru(k: usize, len: usize) -> Vec<usize> {
    let mut seq: Vec<usize> = (0..=k).collect(); // cold start fills the cache; page k then evicts page 0
    let mut cache: Vec<usize> = (1..=k).collect(); // LRU order after the first k+1 requests (0 evicted)
    while seq.len() < len {
        let missing = (0..=k).find(|p| !cache.contains(p)).unwrap();
        seq.push(missing);
        cache.remove(0); // LRU victim is the oldest
        cache.push(missing);
    }
    seq
}

fn paging_adversarial() {
    for k in 2..=8 {
        let seq = adversarial_for_lru(k, 60 * k);
        let (lru, opt) = (lru_faults(&seq, k), belady_faults(&seq, k));
        assert_eq!(
            lru,
            seq.len(),
            "LRU must fault on every request of its own adversary"
        );
        // OPT faults at most once per k requests after the k+1 compulsory ones.
        assert!(opt <= k + 1 + seq.len() / k, "k={k}: opt={opt}");
        assert!(
            lru as f64 / opt as f64 >= k as f64 * 0.8,
            "ratio should approach k"
        );
        if k == 4 || k == 8 {
            println!(
                "paging adversary k={k}: LRU {lru} faults, OPT {opt}, ratio {:.2} (bound k={k})",
                lru as f64 / opt as f64
            );
        }
    }
}

fn paging_random() {
    let mut rng = Rng::new(2024);
    let (mut worst_lru, mut worst_fifo) = (0f64, 0f64);
    for _ in 0..2000 {
        let k = 1 + rng.below(5);
        let universe = k + 1 + rng.below(4);
        let len = 40 + rng.below(80);
        // mix of uniform and locality-heavy requests
        let local = rng.chance(0.5);
        let mut cur = 0;
        let seq: Vec<usize> = (0..len)
            .map(|_| {
                if local && rng.chance(0.7) {
                    cur
                } else {
                    cur = rng.below(universe);
                    cur
                }
            })
            .collect();
        let (lru, fifo, opt) = (
            lru_faults(&seq, k),
            fifo_faults(&seq, k),
            belady_faults(&seq, k),
        );
        assert!(opt <= lru && opt <= fifo, "OPT must be a lower bound");
        assert!(
            lru <= k * opt + k,
            "LRU k-competitive: {lru} > {k}*{opt}+{k}"
        );
        assert!(
            fifo <= k * opt + k,
            "FIFO k-competitive: {fifo} > {k}*{opt}+{k}"
        );
        worst_lru = worst_lru.max(lru as f64 / opt as f64 / k as f64);
        worst_fifo = worst_fifo.max(fifo as f64 / opt as f64 / k as f64);
    }
    println!(
        "paging random x2000: worst LRU/(k*OPT) {worst_lru:.2}, FIFO/(k*OPT) {worst_fifo:.2} (both <= 1 + slack; typical inputs are far from the adversary)"
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// 2c. List update: move-to-front vs the best STATIC ordering (sort by frequency).
// ---------------------------------------------------------------------------------------------------------------------
fn list_cost_static(order: &[usize], reqs: &[usize]) -> u64 {
    let mut pos = vec![0; order.len()];
    for (i, &x) in order.iter().enumerate() {
        pos[x] = i as u64 + 1;
    }
    reqs.iter().map(|&r| pos[r]).sum()
}

fn mtf_cost(n: usize, reqs: &[usize]) -> u64 {
    let mut list: Vec<usize> = (0..n).collect();
    let mut cost = 0;
    for &r in reqs {
        let i = list.iter().position(|&x| x == r).unwrap();
        cost += i as u64 + 1;
        let x = list.remove(i);
        list.insert(0, x);
    }
    cost
}

fn list_update_demo() {
    let mut rng = Rng::new(5);
    for _ in 0..2000 {
        let n = 2 + rng.below(12);
        let m = 20 + rng.below(200);
        let skew = rng.chance(0.5);
        let reqs: Vec<usize> = (0..m)
            .map(|_| {
                if skew && rng.chance(0.6) {
                    rng.below(2)
                } else {
                    rng.below(n)
                }
            })
            .collect();
        let mut freq = vec![0usize; n];
        reqs.iter().for_each(|&r| freq[r] += 1);
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by_key(|&x| std::cmp::Reverse(freq[x]));
        let opt_static = list_cost_static(&order, &reqs);
        let mtf = mtf_cost(n, &reqs);
        // Sleator-Tarjan potential argument: Phi = inversions vs A's list, initial Phi <= n(n-1)/2.
        let bound = 2 * opt_static as i64 - m as i64 + (n * (n - 1) / 2) as i64;
        assert!(
            mtf as i64 <= bound,
            "MTF {mtf} > 2*{opt_static} - {m} + inv"
        );
    }
    println!("list update x2000: MTF <= 2*static_opt - m + n(n-1)/2 always held");
}

// ---------------------------------------------------------------------------------------------------------------------
// 2d. Online bipartite matching: greedy vs RANKING. Offline side L (n vertices), online vertices arrive with neighbours.
// ---------------------------------------------------------------------------------------------------------------------
/// `rank[u]` decides ties: lower rank is preferred. Greedy = identity ranks; RANKING = a random permutation.
fn online_match(adj: &[Vec<usize>], n_left: usize, rank: &[usize]) -> usize {
    let mut taken = vec![false; n_left];
    let mut size = 0;
    for nbrs in adj {
        if let Some(&u) = nbrs
            .iter()
            .filter(|&&u| !taken[u])
            .min_by_key(|&&u| rank[u])
        {
            taken[u] = true;
            size += 1;
        }
    }
    size
}

/// Offline maximum matching via Kuhn's augmenting paths (the oracle).
fn max_matching(adj: &[Vec<usize>], n_left: usize) -> usize {
    fn try_aug(v: usize, adj: &[Vec<usize>], seen: &mut [bool], owner: &mut [usize]) -> bool {
        for &u in &adj[v] {
            if seen[u] {
                continue;
            }
            seen[u] = true;
            if owner[u] == NIL || try_aug(owner[u], adj, seen, owner) {
                owner[u] = v;
                return true;
            }
        }
        false
    }
    let mut owner = vec![NIL; n_left];
    (0..adj.len())
        .filter(|&v| try_aug(v, adj, &mut vec![false; n_left], &mut owner))
        .count()
}

fn matching_demo() {
    let mut rng = Rng::new(31);
    // Random instances: any maximal matching is >= OPT/2 (greedy and RANKING are both maximal).
    for _ in 0..2000 {
        let (nl, nr) = (1 + rng.below(9), 1 + rng.below(9));
        let adj: Vec<Vec<usize>> = (0..nr)
            .map(|_| (0..nl).filter(|_| rng.chance(0.3)).collect())
            .collect();
        let opt = max_matching(&adj, nl);
        let mut rank: Vec<usize> = (0..nl).collect();
        rng.shuffle(&mut rank);
        let r = online_match(&adj, nl, &rank);
        let g = online_match(&adj, nl, &(0..nl).collect::<Vec<_>>());
        assert!(
            2 * r >= opt && 2 * g >= opt && r <= opt && g <= opt,
            "r={r} g={g} opt={opt}"
        );
    }

    // Tight instance for deterministic greedy: online j is adjacent to offline 0..n-1-j. OPT = n, greedy = n/2.
    let n = 40;
    let adj: Vec<Vec<usize>> = (0..n).map(|j| (0..n - j).collect()).collect();
    assert_eq!(max_matching(&adj, n), n);
    let greedy = online_match(&adj, n, &(0..n).collect::<Vec<_>>());
    assert_eq!(greedy, n / 2);
    let trials = 4000;
    let mut total = 0usize;
    for _ in 0..trials {
        let mut rank: Vec<usize> = (0..n).collect();
        rng.shuffle(&mut rank);
        total += online_match(&adj, n, &rank);
    }
    let mean = total as f64 / trials as f64;
    let target = (1.0 - 1.0 / std::f64::consts::E) * n as f64;
    println!(
        "online matching n={n}: greedy {greedy} (=n/2), RANKING mean {mean:.2}, (1-1/e)*OPT = {target:.2}"
    );
    // RANKING's guarantee is on the EXPECTATION; allow Monte-Carlo noise of 0.5 matches.
    assert!(
        mean >= target - 0.5,
        "RANKING mean {mean} below the 1-1/e guarantee {target}"
    );
    assert!(
        mean > greedy as f64 + 2.0,
        "randomisation should clearly beat the adversary for greedy"
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// 2e. Load balancing: greedy list scheduling on m identical machines vs the brute-force optimum.
// ---------------------------------------------------------------------------------------------------------------------
fn greedy_makespan(jobs: &[u64], m: usize) -> u64 {
    let mut load = vec![0u64; m];
    for &j in jobs {
        let i = (0..m).min_by_key(|&i| load[i]).unwrap();
        load[i] += j;
    }
    *load.iter().max().unwrap()
}

fn opt_makespan(jobs: &[u64], m: usize) -> u64 {
    fn go(i: usize, jobs: &[u64], load: &mut Vec<u64>, best: &mut u64) {
        let cur = *load.iter().max().unwrap();
        if cur >= *best {
            return;
        }
        if i == jobs.len() {
            *best = cur;
            return;
        }
        for k in 0..load.len() {
            load[k] += jobs[i];
            go(i + 1, jobs, load, best);
            load[k] -= jobs[i];
        }
    }
    let mut best = u64::MAX;
    go(0, jobs, &mut vec![0; m], &mut best);
    best
}

fn load_balancing_demo() {
    let mut rng = Rng::new(77);
    for _ in 0..2000 {
        let m = 2 + rng.below(3);
        let n = 1 + rng.below(8);
        let jobs: Vec<u64> = (0..n).map(|_| 1 + rng.below(20) as u64).collect();
        let (g, o) = (greedy_makespan(&jobs, m), opt_makespan(&jobs, m));
        assert!(g >= o);
        // (2 - 1/m) * OPT, kept in integers: m*g <= (2m-1)*o
        assert!(
            m as u64 * g <= (2 * m as u64 - 1) * o,
            "greedy {g} vs opt {o} on m={m} jobs {jobs:?}"
        );
    }
    for m in 2..=10usize {
        let mut jobs = vec![1u64; m * (m - 1)];
        jobs.push(m as u64);
        let g = greedy_makespan(&jobs, m);
        assert_eq!(g, 2 * m as u64 - 1, "tight example reaches (2-1/m)*OPT");
        assert_eq!(opt_makespan(&jobs[m * (m - 1) - 0..], 1), m as u64); // sanity: the lone big job
    }
    println!(
        "load balancing: 2000 random instances within 2-1/m; tight example m(m-1) unit jobs + one size-m hits 2m-1 vs m"
    );
}

fn main() {
    println!("== amortised analysis ==");
    dynarray_demo();
    counter_demo();
    queue_demo();
    dsu_demo();
    splay_demo();
    println!("== online algorithms ==");
    ski_demo();
    paging_adversarial();
    paging_random();
    list_update_demo();
    matching_demo();
    load_balancing_demo();
    println!("all amortised and competitive-ratio bounds verified");
}
