//! RECIPE 137 (dp): a zoo of DP shapes, each with a brute-force oracle and seeded differential tests.
//! LEVEL: advanced
//! Use in: recognising which DP "shape" a problem has before writing code.   Run: cargo run --release --example dp_zoo
#![allow(dead_code)]
//!
//! Most DP problems are one of a handful of shapes. Name the shape and the state/recurrence follows:
//!
//!   shape     | state                          | example here              | seen in the wild
//!   ----------+--------------------------------+---------------------------+----------------------------------------
//!   1D        | best[i] over a prefix          | LIS, weighted intervals   | resource/slot scheduling, patience diffs
//!   2D        | table over two sequences       | (see dp_edit_distance edit distance)   | diff tools, spell checkers, alignment
//!   interval  | best[i][j] over a sub-range    | matrix-chain order        | query optimisers ordering joins, parsing
//!   bitmask   | best[subset][last]             | Held-Karp TSP (n <= 12)   | route planning, small-set orderings
//!   tree      | (take, skip) per subtree       | max independent set       | org charts, dependency pruning, UI layout
//!   digit     | (position, remainder, tight)   | digit-sum counting <= N   | counting IDs/serials with a property
//!   bitset    | reachable sums as a bit row    | subset sum / partition    | load balancing, change-making feasibility
//!
//! Every function below has a slow, obviously-correct oracle (brute force or O(n^2)) and main() compares the two on
//! hundreds of seeded random inputs. The oracle is the specification; the DP is the optimisation.
//! The ideas to keep: (1) pick a state that makes the future independent of the past, (2) fix an evaluation order
//! (prefix, interval length, mask popcount, post-order), (3) keep a back-pointer if you need the witness, not just the value.
//! COMMON USES: LIS powers patience-diff style line matching; interval DP orders joins in classic System-R style
//! optimisers (restricted to small relation counts because it is exponential for bushy plans); Held-Karp is the
//! exact fallback for small routing problems; tree DP is how independent-set / vertex-cover style questions become
//! linear on hierarchies; bitset subset-sum gives a 64x constant-factor win over a bool table.
//! Read: https://cp-algorithms.com/dynamic_programming/intro-to-dp.html and https://en.wikipedia.org/wiki/Held%E2%80%93Karp_algorithm

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

// ---------- 1D: longest increasing subsequence, O(n log n) ----------

/// Patience sorting: `tails[k]` is the index of the smallest possible last element of an increasing subsequence of
/// length k+1. Tails stay sorted, so each element binary-searches its pile. `prev` links let us rebuild one witness.
fn lis(a: &[i64]) -> Vec<usize> {
    let mut tails: Vec<usize> = Vec::new();
    let mut prev = vec![usize::MAX; a.len()];
    for (i, &x) in a.iter().enumerate() {
        // strictly increasing: first pile whose top is >= x gets replaced
        let k = tails.partition_point(|&t| a[t] < x);
        if k > 0 {
            prev[i] = tails[k - 1];
        }
        if k == tails.len() {
            tails.push(i);
        } else {
            tails[k] = i;
        }
    }
    let mut out = Vec::with_capacity(tails.len());
    let mut cur = tails.last().copied();
    while let Some(i) = cur {
        out.push(i);
        cur = (prev[i] != usize::MAX).then_some(prev[i]);
    }
    out.reverse();
    out
}

fn lis_oracle(a: &[i64]) -> usize {
    let mut best = vec![1usize; a.len()];
    for i in 0..a.len() {
        for j in 0..i {
            if a[j] < a[i] {
                best[i] = best[i].max(best[j] + 1);
            }
        }
    }
    best.into_iter().max().unwrap_or(0)
}

// ---------- interval: matrix-chain multiplication ----------

/// Matrix i has shape dims[i] x dims[i+1]. cost[i][j] = cheapest way to multiply matrices i..=j; the last
/// multiplication splits at some k, so we try every k. Intervals are filled by increasing length.
fn matrix_chain(dims: &[u64]) -> (u64, String) {
    let n = dims.len() - 1;
    let mut cost = vec![vec![0u64; n]; n];
    let mut split = vec![vec![0usize; n]; n];
    for len in 2..=n {
        for i in 0..=n - len {
            let j = i + len - 1;
            cost[i][j] = u64::MAX;
            for k in i..j {
                let c = cost[i][k] + cost[k + 1][j] + dims[i] * dims[k + 1] * dims[j + 1];
                if c < cost[i][j] {
                    cost[i][j] = c;
                    split[i][j] = k;
                }
            }
        }
    }
    fn paren(split: &[Vec<usize>], i: usize, j: usize) -> String {
        if i == j {
            return format!("M{i}");
        }
        let k = split[i][j];
        format!("({} x {})", paren(split, i, k), paren(split, k + 1, j))
    }
    (cost[0][n - 1], paren(&split, 0, n - 1))
}

/// Oracle: plain recursion over every split, no memo (exponential, fine for <= 8 matrices).
fn matrix_chain_oracle(dims: &[u64], i: usize, j: usize) -> u64 {
    if i == j {
        return 0;
    }
    (i..j)
        .map(|k| {
            matrix_chain_oracle(dims, i, k)
                + matrix_chain_oracle(dims, k + 1, j)
                + dims[i] * dims[k + 1] * dims[j + 1]
        })
        .min()
        .unwrap()
}

// ---------- bitmask: Held-Karp TSP ----------

/// Shortest tour from node 0 visiting every node once and returning. dp[mask][j] = cheapest path that starts at 0,
/// visits exactly the set `mask` of nodes 1..n (bit j-1 for node j) and ends at j. O(2^n * n^2) vs O(n!) brute force.
fn held_karp(d: &[Vec<u64>]) -> u64 {
    let n = d.len();
    if n == 1 {
        return 0;
    }
    let m = n - 1;
    let full = (1usize << m) - 1;
    let mut dp = vec![vec![u64::MAX; m]; 1 << m];
    for j in 0..m {
        dp[1 << j][j] = d[0][j + 1];
    }
    for mask in 1..=full {
        for last in 0..m {
            let cur = dp[mask][last];
            if mask & (1 << last) == 0 || cur == u64::MAX {
                continue;
            }
            for next in 0..m {
                if mask & (1 << next) != 0 {
                    continue;
                }
                let nm = mask | (1 << next);
                let c = cur + d[last + 1][next + 1];
                if c < dp[nm][next] {
                    dp[nm][next] = c;
                }
            }
        }
    }
    (0..m).map(|j| dp[full][j] + d[j + 1][0]).min().unwrap()
}

fn tsp_oracle(d: &[Vec<u64>]) -> u64 {
    fn go(d: &[Vec<u64>], at: usize, left: &mut Vec<usize>, acc: u64) -> u64 {
        if left.is_empty() {
            return acc + d[at][0];
        }
        let mut best = u64::MAX;
        for idx in 0..left.len() {
            let v = left.remove(idx);
            best = best.min(go(d, v, left, acc + d[at][v]));
            left.insert(idx, v);
        }
        best
    }
    go(d, 0, &mut (1..d.len()).collect(), 0)
}

// ---------- 1D + binary search: weighted interval scheduling ----------

#[derive(Clone, Copy, Debug)]
struct Job {
    start: u32,
    end: u32, // half-open [start, end)
    weight: u64,
}

/// Sort by end time. best[i] = max weight using the first i jobs. Job i is either skipped, or taken together with the
/// best of the jobs that end no later than it starts: found by binary search because ends are sorted.
fn weighted_intervals(jobs: &[Job]) -> u64 {
    let mut js = jobs.to_vec();
    js.sort_by_key(|j| j.end);
    let mut best = vec![0u64; js.len() + 1];
    for i in 1..=js.len() {
        let j = js[i - 1];
        let compatible = js[..i - 1].partition_point(|o| o.end <= j.start);
        best[i] = best[i - 1].max(best[compatible] + j.weight);
    }
    best[js.len()]
}

fn intervals_oracle(jobs: &[Job]) -> u64 {
    let mut best = 0;
    for mask in 0u32..(1 << jobs.len()) {
        let pick: Vec<&Job> = (0..jobs.len())
            .filter(|&i| mask >> i & 1 == 1)
            .map(|i| &jobs[i])
            .collect();
        let ok = pick.iter().enumerate().all(|(a, x)| {
            pick[a + 1..]
                .iter()
                .all(|y| x.end <= y.start || y.end <= x.start)
        });
        if ok {
            best = best.max(pick.iter().map(|j| j.weight).sum());
        }
    }
    best
}

// ---------- tree: maximum weight independent set ----------

/// take[v] = w[v] + sum(skip[child]); skip[v] = sum(max(take, skip)[child]). Children must be finished before parents,
/// so we record a DFS preorder with an explicit stack and sweep it in reverse: no recursion, no stack overflow.
fn tree_mis(weights: &[u64], adj: &[Vec<usize>]) -> u64 {
    let n = weights.len();
    let (mut order, mut parent, mut stack) =
        (Vec::with_capacity(n), vec![usize::MAX; n], vec![0usize]);
    while let Some(v) = stack.pop() {
        order.push(v);
        for &c in &adj[v] {
            if c != parent[v] {
                parent[c] = v;
                stack.push(c);
            }
        }
    }
    let mut take: Vec<u64> = weights.to_vec();
    let mut skip = vec![0u64; n];
    for &v in order.iter().rev() {
        let p = parent[v];
        if p != usize::MAX {
            take[p] += skip[v];
            skip[p] += take[v].max(skip[v]);
        }
    }
    take[0].max(skip[0])
}

fn tree_mis_oracle(weights: &[u64], edges: &[(usize, usize)]) -> u64 {
    let mut best = 0;
    for mask in 0u32..(1 << weights.len()) {
        if edges
            .iter()
            .all(|&(a, b)| !(mask >> a & 1 == 1 && mask >> b & 1 == 1))
        {
            best = best.max(
                (0..weights.len())
                    .filter(|&i| mask >> i & 1 == 1)
                    .map(|i| weights[i])
                    .sum(),
            );
        }
    }
    best
}

// ---------- bitset: subset sum / partition ----------

/// Row of bits; bit s set means "sum s is reachable". Adding item x is `row |= row << x`, done 64 sums per word.
struct Bits {
    words: Vec<u64>,
}

impl Bits {
    fn new(nbits: usize) -> Bits {
        let mut b = Bits {
            words: vec![0; nbits / 64 + 1],
        };
        b.words[0] = 1; // sum 0 is reachable with the empty subset
        b
    }
    fn get(&self, i: usize) -> bool {
        self.words[i / 64] >> (i % 64) & 1 == 1
    }
    fn or_shl(&mut self, s: usize) {
        let (ws, bs) = (s / 64, s % 64);
        // walk from the top so sources (lower words) are still unmodified
        for i in (ws..self.words.len()).rev() {
            let mut v = self.words[i - ws] << bs;
            if bs > 0 && i > ws {
                v |= self.words[i - ws - 1] >> (64 - bs);
            }
            self.words[i] |= v;
        }
    }
}

fn reachable_sums(items: &[usize], limit: usize) -> Bits {
    let mut b = Bits::new(limit);
    for &x in items {
        b.or_shl(x);
    }
    b
}

/// Smallest possible |sum(A) - sum(B)| over all splits into two groups.
fn min_partition_diff(items: &[usize]) -> usize {
    let total: usize = items.iter().sum();
    let b = reachable_sums(items, total / 2);
    let best = (0..=total / 2).rev().find(|&s| b.get(s)).unwrap();
    total - 2 * best
}

fn subset_sum_oracle(items: &[usize], limit: usize) -> Vec<bool> {
    let mut ok = vec![false; limit + 1];
    ok[0] = true;
    for &x in items {
        for s in (x..=limit).rev() {
            ok[s] |= ok[s - x];
        }
    }
    ok
}

// ---------- digit DP ----------

/// Count x in [0, n] whose decimal digit sum is divisible by k. State per position: (sum mod k, still tight to n?).
/// "Tight" means the prefix equals n's prefix, so the next digit is capped; once we go below it, any digit is allowed.
fn count_digit_sum_div(n: u64, k: usize) -> u64 {
    let digits: Vec<usize> = n.to_string().bytes().map(|b| (b - b'0') as usize).collect();
    let mut free = vec![0u64; k]; // prefixes already strictly below n
    let mut tight_rem = 0usize; // the single prefix equal to n's prefix
    for &d in &digits {
        let mut next = vec![0u64; k];
        for r in 0..k {
            for dig in 0..10 {
                next[(r + dig) % k] += free[r];
            }
        }
        for dig in 0..d {
            next[(tight_rem + dig) % k] += 1; // leave the tight path by going lower
        }
        tight_rem = (tight_rem + d) % k;
        free = next;
    }
    free[0] + u64::from(tight_rem == 0)
}

fn digit_oracle(n: u64, k: usize) -> u64 {
    (0..=n)
        .filter(|x| {
            x.to_string()
                .bytes()
                .map(|b| (b - b'0') as usize)
                .sum::<usize>()
                % k
                == 0
        })
        .count() as u64
}

fn main() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);

    // LIS
    let demo = [10, 9, 2, 5, 3, 7, 101, 18, 4, 20];
    let w = lis(&demo);
    println!(
        "LIS of {demo:?} = {:?}",
        w.iter().map(|&i| demo[i]).collect::<Vec<_>>()
    );
    for _ in 0..500 {
        let a: Vec<i64> = (0..rng.below(40)).map(|_| rng.below(20) as i64).collect();
        let w = lis(&a);
        assert_eq!(w.len(), lis_oracle(&a));
        assert!(
            w.windows(2).all(|p| p[0] < p[1] && a[p[0]] < a[p[1]]),
            "witness must be strictly increasing"
        );
    }
    let big: Vec<i64> = (0..200_000).map(|_| rng.below(1_000_000) as i64).collect();
    println!("LIS of 200k random numbers: {}", lis(&big).len());

    // matrix chain
    let (c, p) = matrix_chain(&[10, 30, 5, 60]);
    println!("matrix chain 10x30,30x5,5x60: cost {c} via {p}");
    assert_eq!(c, 4500);
    for _ in 0..300 {
        let n = 1 + rng.below(7) as usize;
        let dims: Vec<u64> = (0..=n).map(|_| 1 + rng.below(30)).collect();
        assert_eq!(matrix_chain(&dims).0, matrix_chain_oracle(&dims, 0, n - 1));
    }

    // TSP
    for _ in 0..100 {
        let n = 1 + rng.below(8) as usize;
        let d: Vec<Vec<u64>> = (0..n)
            .map(|i| {
                (0..n)
                    .map(|j| if i == j { 0 } else { 1 + rng.below(50) })
                    .collect()
            })
            .collect();
        assert_eq!(held_karp(&d), tsp_oracle(&d));
    }
    let d12: Vec<Vec<u64>> = (0..12)
        .map(|i| {
            (0..12)
                .map(|j| if i == j { 0 } else { 1 + rng.below(100) })
                .collect()
        })
        .collect();
    println!("Held-Karp on 12 random nodes: tour {}", held_karp(&d12));

    // weighted intervals
    for _ in 0..300 {
        let jobs: Vec<Job> = (0..rng.below(11))
            .map(|_| {
                let s = rng.below(30) as u32;
                Job {
                    start: s,
                    end: s + 1 + rng.below(8) as u32,
                    weight: 1 + rng.below(20),
                }
            })
            .collect();
        assert_eq!(weighted_intervals(&jobs), intervals_oracle(&jobs));
    }

    // tree MIS
    for _ in 0..300 {
        let n = 1 + rng.below(12) as usize;
        let weights: Vec<u64> = (0..n).map(|_| 1 + rng.below(10)).collect();
        let mut adj = vec![vec![]; n];
        let mut edges = vec![];
        for v in 1..n {
            let p = rng.below(v as u64) as usize;
            adj[p].push(v);
            adj[v].push(p);
            edges.push((p, v));
        }
        assert_eq!(tree_mis(&weights, &adj), tree_mis_oracle(&weights, &edges));
    }
    // a path of 300k nodes would overflow the stack with naive recursion; weights 1 => ceil(n/2)
    let n = 300_000;
    let mut path = vec![vec![]; n];
    for v in 1..n {
        path[v - 1].push(v);
        path[v].push(v - 1);
    }
    assert_eq!(tree_mis(&vec![1; n], &path), (n as u64).div_ceil(2));
    println!("tree MIS on a {n}-node path: {}", n / 2);

    // bitset subset sum / partition
    for _ in 0..300 {
        let items: Vec<usize> = (0..rng.below(15))
            .map(|_| 1 + rng.below(90) as usize)
            .collect();
        let limit = 400;
        let (bits, ok) = (
            reachable_sums(&items, limit),
            subset_sum_oracle(&items, limit),
        );
        assert!(
            (0..=limit).all(|s| bits.get(s) == ok[s]),
            "bitset must agree with the bool table"
        );
        let total: usize = items.iter().sum();
        let o = subset_sum_oracle(&items, total);
        let want = (0..=total / 2)
            .rev()
            .find(|&s| o[s])
            .map(|s| total - 2 * s)
            .unwrap();
        assert_eq!(min_partition_diff(&items), want);
    }
    println!(
        "partition [3,1,4,2,2,1] min diff = {}",
        min_partition_diff(&[3, 1, 4, 2, 2, 1])
    );

    // digit DP
    for _ in 0..200 {
        let (n, k) = (rng.below(6000), 1 + rng.below(12) as usize);
        assert_eq!(count_digit_sum_div(n, k), digit_oracle(n, k), "n={n} k={k}");
    }
    println!(
        "numbers <= 10^18 with digit sum % 7 == 0: {}",
        count_digit_sum_div(1_000_000_000_000_000_000, 7)
    );
    println!("all DP zoo checks passed");
}
