//! RECIPE 883 (hpc): MPI-style collectives over a simulated message-passing network (threads + channels) with alpha-beta cost models
//! LEVEL: advanced
//! Use in: understanding what MPI_Allreduce/Bcast/Alltoall really do, gradient all-reduce in distributed training, choosing algorithms by message size.   Run: cargo run --release --example mpi_collectives_simulation
//! COMMON USES: data-parallel deep learning gradient sync (ring and tree allreduce in NCCL-style libraries), MPI implementations (MPICH/Open MPI pick algorithms by message size and rank count), distributed sort/shuffle (alltoall), parallel linear algebra (broadcast/reduce on process grids).
//! Course: Thakur, Rabenseifner, Gropp, "Optimization of Collective Communication Operations in MPICH" (IJHPCA 2005); CS267 lectures on communication cost models.
#![allow(dead_code)]
//!
//! WHAT: N "ranks" are threads. Each owns a mailbox (an mpsc channel receiver) and a sender to every other rank, exactly the
//! point-to-point layer MPI gives you (`send(dst, tag, data)`, `recv(src, tag)`). On top we build the collectives from
//! point-to-point calls only: broadcast and reduce (binomial tree), allreduce (ring, recursive doubling, binomial
//! reduce+bcast, Rabenseifner reduce-scatter + allgather), allgather (ring), alltoall (pairwise), scatter/gather (linear),
//! barrier (dissemination).
//!
//! WHY a simulation: sends are buffered (unbounded channels) like MPI's eager protocol, so simultaneous "exchange with partner"
//! steps cannot deadlock, and we can COUNT every message and byte exactly. The cost model of each algorithm is a closed
//! formula; this recipe asserts the measured totals EQUAL the formulas for every rank count 1..16, including non-powers of two.
//!
//! The alpha-beta model: sending m bytes costs alpha + beta*m (alpha = latency, beta = per-byte time). On the critical path:
//!   binomial bcast/reduce  lg p * (alpha + n beta)
//!   ring allreduce         2 (p-1) alpha + 2 (p-1)/p * n beta
//!   recursive doubling     lg p * (alpha + n beta)
//!   Rabenseifner           2 lg p alpha + 2 (p-1)/p * n beta
//! (gamma, the per-byte reduction compute cost, is dropped.) Small n: latency dominates, so recursive doubling (few rounds)
//! wins. Large n: bandwidth dominates, so the 2 (p-1)/p * n beta algorithms (ring, Rabenseifner) win; Rabenseifner has the ring's
//! bandwidth with only 2 lg p rounds. We compute the crossover and check it against the analytic value.
//!
//! Non-power-of-two ranks: recursive doubling and Rabenseifner first FOLD: with pof2 = largest power of two <= p and rem = p-pof2,
//! the first 2*rem ranks pair up (even sends its vector to the odd neighbour and sits out), the remaining pof2 ranks run the
//! power-of-two algorithm, then the odd ones send the result back. This is the MPICH approach.
//!
//! Elements are `i64` with wrapping add so reduction is exact and order-independent: results must equal the sequential oracle
//! bit for bit. (Floating-point sum order would change results: see parallel_prefix_scan_reduction.)
//! Read: https://www.mcs.anl.gov/~thakur/papers/ijhpca-coll.pdf ; https://www.mpich.org/static/docs/latest/ ; https://www.mpi-forum.org/docs/
//! TRY NEXT:
//!   - [S] Change `bcast` to a flat tree (root sends p-1 messages) and predict `bcast_formula` for message COUNT and critical-path rounds;
//!         which of the two stays equal and which changes? Update the formula and the assertion.
//!   - [S] Make `allreduce_ring` use 3 ranks and n = 2 (fewer elements than ranks): predict which chunks are empty and whether the byte
//!         formula 2(p-1)n still holds; then run it.
//!   - [M] Add `reduce_scatter_halving` as a public collective with its own formula (bytes = pof2 (1 - 1/pof2) n elements) and verify
//!         against the oracle for 1..16 ranks; then use it to build a distributed histogram merge.
//!   - [M] Real-world port: model a data-parallel training step, p workers each with a gradient of n elements, and use `model_cost`
//!         to find, for p = 64 and a 100 Gbit/s link, the gradient size where you should switch from recursive doubling to ring.
//!   - [A] PROJECT: combine with `thread_pool_work_stealing` and `parallel_sample_sort_radix`: implement a distributed sample sort over this
//!         rank world (splitter allgather, then alltoall of buckets) and assert the global order and total message counts.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering::SeqCst};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

// ---------- deterministic RNG ----------

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

// ---------- the simulated network ----------

struct Msg {
    src: usize,
    tag: u32,
    data: Vec<i64>,
}

struct Stats {
    msgs: AtomicU64,
    elems: AtomicU64,
}

/// One MPI-like process. `send` is buffered and never blocks; `recv` blocks until a matching (src, tag) message arrives,
/// stashing non-matching messages (MPI's "unexpected message queue").
struct Rank<'a> {
    rank: usize,
    size: usize,
    tx: Vec<Sender<Msg>>,
    rx: Receiver<Msg>,
    stash: Vec<Msg>,
    stats: &'a Stats,
}

impl Rank<'_> {
    fn send(&self, dst: usize, tag: u32, data: Vec<i64>) {
        assert_ne!(dst, self.rank, "self-sends are a bug in a collective");
        self.stats.msgs.fetch_add(1, SeqCst);
        self.stats.elems.fetch_add(data.len() as u64, SeqCst);
        self.tx[dst]
            .send(Msg {
                src: self.rank,
                tag,
                data,
            })
            .unwrap();
    }

    fn recv(&mut self, src: usize, tag: u32) -> Vec<i64> {
        if let Some(i) = self.stash.iter().position(|m| m.src == src && m.tag == tag) {
            return self.stash.remove(i).data;
        }
        loop {
            let m = self.rx.recv().expect("network closed while waiting");
            if m.src == src && m.tag == tag {
                return m.data;
            }
            self.stash.push(m);
        }
    }
}

/// Spawn `p` rank threads running `f`; returns each rank's result plus total (messages, elements) sent.
fn run_world<T: Send>(p: usize, f: impl Fn(&mut Rank) -> T + Sync) -> (Vec<T>, u64, u64) {
    let stats = Stats {
        msgs: AtomicU64::new(0),
        elems: AtomicU64::new(0),
    };
    let (txs, rxs): (Vec<_>, Vec<_>) = (0..p).map(|_| channel::<Msg>()).unzip();
    let f = &f;
    let stats_ref = &stats;
    let out = thread::scope(|s| {
        let hs: Vec<_> = rxs
            .into_iter()
            .enumerate()
            .map(|(rank, rx)| {
                let tx = txs.clone();
                s.spawn(move || {
                    let mut r = Rank {
                        rank,
                        size: p,
                        tx,
                        rx,
                        stash: vec![],
                        stats: stats_ref,
                    };
                    f(&mut r)
                })
            })
            .collect();
        drop(txs);
        hs.into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>()
    });
    (out, stats.msgs.load(SeqCst), stats.elems.load(SeqCst))
}

// ---------- helpers ----------

fn add_into(acc: &mut [i64], other: &[i64]) {
    assert_eq!(acc.len(), other.len());
    for (a, b) in acc.iter_mut().zip(other) {
        *a = a.wrapping_add(*b);
    }
}

fn lg_ceil(p: usize) -> u32 {
    if p <= 1 {
        0
    } else {
        usize::BITS - (p - 1).leading_zeros()
    }
}

fn pof2_floor(p: usize) -> usize {
    1 << (usize::BITS - 1 - p.leading_zeros())
}

fn is_pof2(p: usize) -> bool {
    p.is_power_of_two()
}

// ---------- collectives ----------

const T_BCAST: u32 = 1;
const T_REDUCE: u32 = 2;
const T_RING_RS: u32 = 3;
const T_RING_AG: u32 = 4;
const T_RD: u32 = 5;
const T_FOLD: u32 = 6;
const T_UNFOLD: u32 = 7;
const T_RAB_RS: u32 = 8;
const T_RAB_AG: u32 = 9;
const T_ALLGATHER: u32 = 10;
const T_ALLTOALL: u32 = 11;
const T_SCATTER: u32 = 12;
const T_GATHER: u32 = 13;
const T_BARRIER: u32 = 14;

/// Binomial-tree broadcast from `root`. `data` is valid on the root on entry and on every rank on exit.
/// Rounds: ceil(lg p). Messages: p-1.
fn bcast(r: &mut Rank, root: usize, data: &mut Vec<i64>) {
    let p = r.size;
    let rel = (r.rank + p - root) % p;
    let mut mask = 1;
    while mask < p {
        if rel & mask != 0 {
            *data = r.recv((rel - mask + root) % p, T_BCAST);
            break;
        }
        mask <<= 1;
    }
    mask >>= 1;
    while mask > 0 {
        if rel + mask < p {
            r.send((rel + mask + root) % p, T_BCAST, data.clone());
        }
        mask >>= 1;
    }
}

/// Binomial-tree reduce (sum) to `root`; the result is valid on the root only.
fn reduce(r: &mut Rank, root: usize, data: &mut Vec<i64>) {
    let p = r.size;
    let rel = (r.rank + p - root) % p;
    let mut mask = 1;
    while mask < p {
        if rel & mask != 0 {
            r.send((rel - mask + root) % p, T_REDUCE, data.clone());
            break;
        } else if rel + mask < p {
            let other = r.recv((rel + mask + root) % p, T_REDUCE);
            add_into(data, &other);
        }
        mask <<= 1;
    }
}

/// allreduce = reduce to 0 then bcast from 0: 2 (p-1) messages, 2 ceil(lg p) rounds.
fn allreduce_tree(r: &mut Rank, data: &mut Vec<i64>) {
    reduce(r, 0, data);
    bcast(r, 0, data);
}

fn chunk_bounds(n: usize, p: usize, i: usize) -> (usize, usize) {
    (i * n / p, (i + 1) * n / p)
}

/// Ring allreduce: reduce-scatter around the ring (p-1 steps) then allgather around the ring (p-1 steps).
/// Every rank sends 2(p-1) messages; each chunk travels p-1 hops per phase, so total elements = 2 (p-1) n.
/// This is bandwidth-optimal, and what GPU libraries use for large tensors.
fn allreduce_ring(r: &mut Rank, data: &mut Vec<i64>) {
    let (p, me, n) = (r.size, r.rank, data.len());
    let right = (me + 1) % p;
    let left = (me + p - 1) % p;
    for s in 0..p - 1 {
        let (lo, hi) = chunk_bounds(n, p, (me + p - s) % p);
        r.send(right, T_RING_RS, data[lo..hi].to_vec());
        let (lo, hi) = chunk_bounds(n, p, (me + p - s - 1) % p);
        let got = r.recv(left, T_RING_RS);
        add_into(&mut data[lo..hi], &got);
    }
    // now rank `me` holds the fully reduced chunk (me + 1) % p
    for s in 0..p - 1 {
        let (lo, hi) = chunk_bounds(n, p, (me + 1 + p - s) % p);
        r.send(right, T_RING_AG, data[lo..hi].to_vec());
        let (lo, hi) = chunk_bounds(n, p, (me + p - s) % p);
        let got = r.recv(left, T_RING_AG);
        data[lo..hi].copy_from_slice(&got);
    }
}

/// MPICH-style fold of a non-power-of-two world. Returns this rank's index in the reduced power-of-two world, or None if
/// the rank sits out (its data was handed to the odd neighbour).
fn fold_in(r: &mut Rank, data: &mut Vec<i64>) -> Option<usize> {
    let pof2 = pof2_floor(r.size);
    let rem = r.size - pof2;
    if r.rank < 2 * rem {
        if r.rank % 2 == 0 {
            r.send(r.rank + 1, T_FOLD, data.clone());
            None
        } else {
            let other = r.recv(r.rank - 1, T_FOLD);
            add_into(data, &other);
            Some(r.rank / 2)
        }
    } else {
        Some(r.rank - rem)
    }
}

fn fold_out(r: &mut Rank, data: &mut Vec<i64>) {
    let rem = r.size - pof2_floor(r.size);
    if r.rank < 2 * rem {
        if r.rank % 2 == 1 {
            r.send(r.rank - 1, T_UNFOLD, data.clone());
        } else {
            *data = r.recv(r.rank + 1, T_UNFOLD);
        }
    }
}

/// map an index in the reduced world back to the real rank
fn unfold_rank(newrank: usize, rem: usize) -> usize {
    if newrank < rem {
        newrank * 2 + 1
    } else {
        newrank + rem
    }
}

/// Recursive doubling: in round k, exchange the WHOLE vector with partner newrank ^ 2^k and add. lg(pof2) rounds.
fn allreduce_recursive_doubling(r: &mut Rank, data: &mut Vec<i64>) {
    let pof2 = pof2_floor(r.size);
    let rem = r.size - pof2;
    if let Some(nr) = fold_in(r, data) {
        let mut mask = 1;
        while mask < pof2 {
            let partner = unfold_rank(nr ^ mask, rem);
            r.send(partner, T_RD + 100 * mask as u32, data.clone());
            let other = r.recv(partner, T_RD + 100 * mask as u32);
            add_into(data, &other);
            mask <<= 1;
        }
    }
    fold_out(r, data);
}

/// Rabenseifner: reduce-scatter by recursive halving, then allgather by recursive doubling. Requires n % pof2 == 0 here
/// (equal blocks keep the byte formula exact; real MPI handles remainders with uneven blocks).
fn allreduce_rabenseifner(r: &mut Rank, data: &mut Vec<i64>) {
    let n = data.len();
    let pof2 = pof2_floor(r.size);
    let rem = r.size - pof2;
    assert_eq!(
        n % pof2,
        0,
        "this teaching version needs n divisible by the power-of-two core"
    );
    if let Some(nr) = fold_in(r, data) {
        let (mut lo, mut hi) = (0, n);
        // phase 1: recursive halving. Highest bit first; the rank whose bit is 0 keeps the lower half.
        let mut mask = pof2 >> 1;
        while mask > 0 {
            let partner = unfold_rank(nr ^ mask, rem);
            let mid = (lo + hi) / 2;
            let keep_low = nr & mask == 0;
            let (send_rng, keep_rng) = if keep_low {
                ((mid, hi), (lo, mid))
            } else {
                ((lo, mid), (mid, hi))
            };
            r.send(
                partner,
                T_RAB_RS + 100 * mask as u32,
                data[send_rng.0..send_rng.1].to_vec(),
            );
            let got = r.recv(partner, T_RAB_RS + 100 * mask as u32);
            add_into(&mut data[keep_rng.0..keep_rng.1], &got);
            (lo, hi) = keep_rng;
            mask >>= 1;
        }
        // now this rank owns the fully reduced block [lo, hi) == block number nr
        // phase 2: recursive doubling allgather, lowest bit first; the sibling block is adjacent.
        let mut mask = 1;
        while mask < pof2 {
            let partner = unfold_rank(nr ^ mask, rem);
            let size = hi - lo;
            r.send(partner, T_RAB_AG + 100 * mask as u32, data[lo..hi].to_vec());
            let got = r.recv(partner, T_RAB_AG + 100 * mask as u32);
            let (plo, phi) = if nr & mask == 0 {
                (hi, hi + size)
            } else {
                (lo - size, lo)
            };
            data[plo..phi].copy_from_slice(&got);
            lo = lo.min(plo);
            hi = hi.max(phi);
            mask <<= 1;
        }
        assert_eq!((lo, hi), (0, n));
    }
    fold_out(r, data);
}

/// Ring allgather: each rank contributes `block`; everyone ends with all blocks concatenated in rank order.
fn allgather_ring(r: &mut Rank, block: &[i64]) -> Vec<i64> {
    let (p, me, b) = (r.size, r.rank, block.len());
    let mut out = vec![0i64; p * b];
    out[me * b..(me + 1) * b].copy_from_slice(block);
    let right = (me + 1) % p;
    let left = (me + p - 1) % p;
    for s in 0..p - 1 {
        let send_idx = (me + p - s) % p;
        r.send(
            right,
            T_ALLGATHER,
            out[send_idx * b..(send_idx + 1) * b].to_vec(),
        );
        let got = r.recv(left, T_ALLGATHER);
        let recv_idx = (me + p - s - 1) % p;
        out[recv_idx * b..(recv_idx + 1) * b].copy_from_slice(&got);
    }
    out
}

/// Pairwise alltoall: input is p blocks of b; block j goes to rank j. Output block i came from rank i.
fn alltoall(r: &mut Rank, input: &[i64], b: usize) -> Vec<i64> {
    let (p, me) = (r.size, r.rank);
    let mut out = vec![0i64; p * b];
    out[me * b..(me + 1) * b].copy_from_slice(&input[me * b..(me + 1) * b]);
    for d in 1..p {
        let dst = (me + d) % p;
        r.send(dst, T_ALLTOALL, input[dst * b..(dst + 1) * b].to_vec());
    }
    for d in 1..p {
        let src = (me + p - d) % p;
        let got = r.recv(src, T_ALLTOALL);
        out[src * b..(src + 1) * b].copy_from_slice(&got);
    }
    out
}

/// Linear scatter: root holds p blocks of b; rank i receives block i.
fn scatter(r: &mut Rank, root: usize, all: &[i64], b: usize) -> Vec<i64> {
    if r.rank == root {
        for i in (0..r.size).filter(|&i| i != root) {
            r.send(i, T_SCATTER, all[i * b..(i + 1) * b].to_vec());
        }
        all[root * b..(root + 1) * b].to_vec()
    } else {
        r.recv(root, T_SCATTER)
    }
}

/// Linear gather: inverse of scatter; returns Some(concatenation) on the root.
fn gather(r: &mut Rank, root: usize, block: &[i64]) -> Option<Vec<i64>> {
    if r.rank == root {
        let b = block.len();
        let mut out = vec![0i64; r.size * b];
        for i in 0..r.size {
            if i == root {
                out[i * b..(i + 1) * b].copy_from_slice(block);
            } else {
                let got = r.recv(i, T_GATHER);
                out[i * b..(i + 1) * b].copy_from_slice(&got);
            }
        }
        Some(out)
    } else {
        r.send(root, T_GATHER, block.to_vec());
        None
    }
}

/// Dissemination barrier: in round k, send a token to rank + 2^k and wait for one from rank - 2^k. After ceil(lg p)
/// rounds every rank has (transitively) heard from every other. Messages: p ceil(lg p), zero payload.
fn barrier(r: &mut Rank) {
    let p = r.size;
    let mut k = 1;
    while k < p {
        r.send((r.rank + k) % p, T_BARRIER + 100 * k as u32, vec![]);
        r.recv((r.rank + p - k) % p, T_BARRIER + 100 * k as u32);
        k <<= 1;
    }
}

// ---------- cost model: formulas for exact message / element counts ----------

#[derive(Debug, PartialEq, Clone, Copy)]
struct Traffic {
    msgs: u64,
    elems: u64,
}

fn bcast_formula(p: usize, n: usize) -> Traffic {
    Traffic {
        msgs: (p - 1) as u64,
        elems: ((p - 1) * n) as u64,
    }
}
fn reduce_formula(p: usize, n: usize) -> Traffic {
    bcast_formula(p, n)
}
fn tree_allreduce_formula(p: usize, n: usize) -> Traffic {
    let t = bcast_formula(p, n);
    Traffic {
        msgs: 2 * t.msgs,
        elems: 2 * t.elems,
    }
}
fn ring_formula(p: usize, n: usize) -> Traffic {
    Traffic {
        msgs: (2 * p * (p - 1)) as u64,
        elems: (2 * (p - 1) * n) as u64,
    }
}
fn rd_formula(p: usize, n: usize) -> Traffic {
    let pof2 = pof2_floor(p);
    let rem = p - pof2;
    let lg = pof2.trailing_zeros() as usize;
    Traffic {
        msgs: (2 * rem + pof2 * lg) as u64,
        elems: ((2 * rem + pof2 * lg) * n) as u64,
    }
}
fn rabenseifner_formula(p: usize, n: usize) -> Traffic {
    let pof2 = pof2_floor(p);
    let rem = p - pof2;
    let lg = pof2.trailing_zeros() as usize;
    Traffic {
        msgs: (2 * rem + 2 * pof2 * lg) as u64,
        elems: (2 * n * (rem + pof2 - 1)) as u64,
    }
}
fn allgather_formula(p: usize, b: usize) -> Traffic {
    Traffic {
        msgs: (p * (p - 1)) as u64,
        elems: (p * (p - 1) * b) as u64,
    }
}
fn alltoall_formula(p: usize, b: usize) -> Traffic {
    allgather_formula(p, b)
}
fn scatter_formula(p: usize, b: usize) -> Traffic {
    Traffic {
        msgs: (p - 1) as u64,
        elems: ((p - 1) * b) as u64,
    }
}
fn barrier_formula(p: usize) -> Traffic {
    Traffic {
        msgs: (p as u64) * lg_ceil(p) as u64,
        elems: 0,
    }
}

/// Critical-path time in the alpha-beta model (gamma = 0). `n_bytes` is the vector size in bytes.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Algo {
    RecursiveDoubling,
    Ring,
    Rabenseifner,
    TreeReduceBcast,
}

fn model_cost(algo: Algo, p: usize, n_bytes: f64, alpha: f64, beta: f64) -> f64 {
    let pf = p as f64;
    let lg = lg_ceil(p) as f64;
    match algo {
        Algo::RecursiveDoubling => lg * (alpha + n_bytes * beta),
        Algo::Ring => 2.0 * (pf - 1.0) * alpha + 2.0 * (pf - 1.0) / pf * n_bytes * beta,
        Algo::Rabenseifner => 2.0 * lg * alpha + 2.0 * (pf - 1.0) / pf * n_bytes * beta,
        Algo::TreeReduceBcast => 2.0 * lg * (alpha + n_bytes * beta),
    }
}

// ---------- verification ----------

fn make_input(seed: u64, rank: usize, n: usize) -> Vec<i64> {
    let mut rng = Rng::new(seed ^ (rank as u64 + 1).wrapping_mul(0x1234_5678_9ABC_DEF1));
    (0..n)
        .map(|_| (rng.next_u64() >> 3) as i64 - (1 << 60))
        .collect()
}

fn oracle_sum(seed: u64, p: usize, n: usize) -> Vec<i64> {
    let mut acc = vec![0i64; n];
    for r in 0..p {
        add_into(&mut acc, &make_input(seed, r, n));
    }
    acc
}

fn check_traffic(name: &str, p: usize, got: (u64, u64), want: Traffic) {
    assert_eq!(
        Traffic {
            msgs: got.0,
            elems: got.1
        },
        want,
        "{name} p={p}: measured traffic differs from the cost-model formula"
    );
}

fn verify_allreduces(seed: u64, p: usize, n: usize) {
    let want = oracle_sum(seed, p, n);
    type Alg = fn(&mut Rank, &mut Vec<i64>);
    let algs: [(&str, Alg, Traffic); 4] = [
        ("tree", allreduce_tree, tree_allreduce_formula(p, n)),
        ("ring", allreduce_ring, ring_formula(p, n)),
        (
            "recursive-doubling",
            allreduce_recursive_doubling,
            rd_formula(p, n),
        ),
        (
            "rabenseifner",
            allreduce_rabenseifner,
            rabenseifner_formula(p, n),
        ),
    ];
    for (name, alg, formula) in algs {
        let (res, m, e) = run_world(p, |r| {
            let mut d = make_input(seed, r.rank, n);
            alg(r, &mut d);
            d
        });
        for (rank, d) in res.iter().enumerate() {
            assert_eq!(
                *d, want,
                "{name} p={p} n={n}: rank {rank} result differs from sequential sum"
            );
        }
        check_traffic(name, p, (m, e), formula);
    }
}

fn verify_bcast_reduce(seed: u64, p: usize, n: usize) {
    for root in [0, p / 2, p - 1] {
        let payload = make_input(seed, 99, n);
        let (res, m, e) = run_world(p, |r| {
            let mut d = if r.rank == root {
                payload.clone()
            } else {
                vec![0; n]
            };
            bcast(r, root, &mut d);
            d
        });
        assert!(res.iter().all(|d| *d == payload), "bcast p={p} root={root}");
        check_traffic("bcast", p, (m, e), bcast_formula(p, n));

        let want = oracle_sum(seed, p, n);
        let (res, m, e) = run_world(p, |r| {
            let mut d = make_input(seed, r.rank, n);
            reduce(r, root, &mut d);
            d
        });
        assert_eq!(res[root], want, "reduce p={p} root={root}");
        check_traffic("reduce", p, (m, e), reduce_formula(p, n));
    }
}

fn verify_data_movement(seed: u64, p: usize, b: usize) {
    // allgather
    let (res, m, e) = run_world(p, |r| allgather_ring(r, &make_input(seed, r.rank, b)));
    let want: Vec<i64> = (0..p).flat_map(|i| make_input(seed, i, b)).collect();
    assert!(res.iter().all(|d| *d == want), "allgather p={p}");
    check_traffic("allgather", p, (m, e), allgather_formula(p, b));

    // alltoall: out_r block i == in_i block r
    let (res, m, e) = run_world(p, |r| alltoall(r, &make_input(seed, r.rank, p * b), b));
    for r in 0..p {
        for i in 0..p {
            let src = make_input(seed, i, p * b);
            assert_eq!(
                res[r][i * b..(i + 1) * b],
                src[r * b..(r + 1) * b],
                "alltoall p={p} r={r} i={i}"
            );
        }
    }
    check_traffic("alltoall", p, (m, e), alltoall_formula(p, b));

    // scatter then gather roundtrip
    let root = p - 1;
    let all = make_input(seed, 7, p * b);
    let (res, m, e) = run_world(p, |r| {
        let mine = scatter(r, root, &all, b);
        assert_eq!(mine, all[r.rank * b..(r.rank + 1) * b]);
        gather(r, root, &mine)
    });
    assert_eq!(
        res[root].as_deref(),
        Some(&all[..]),
        "gather roundtrip p={p}"
    );
    // scatter p-1 msgs + gather p-1 msgs
    let sf = scatter_formula(p, b);
    check_traffic(
        "scatter+gather",
        p,
        (m, e),
        Traffic {
            msgs: 2 * sf.msgs,
            elems: 2 * sf.elems,
        },
    );
}

fn verify_barrier(p: usize) {
    let arrived = AtomicUsize::new(0);
    let (_, m, e) = run_world(p, |r| {
        arrived.fetch_add(1, SeqCst);
        barrier(r);
        // nobody may pass the barrier before everybody has arrived
        assert_eq!(
            arrived.load(SeqCst),
            p,
            "rank {} left barrier early",
            r.rank
        );
    });
    check_traffic("barrier", p, (m, e), barrier_formula(p));
}

// ---------- crossover analysis ----------

fn crossover_analysis() {
    let (alpha, beta) = (1.0e-6, 1.0e-9 / 12.5); // 1 us latency, 100 Gbit/s = 12.5 GB/s => beta s/byte
    for p in [4usize, 16, 64, 256] {
        // scan message sizes in powers of two: the best algorithm sequence must be monotone
        let mut best_seq: Vec<(f64, Algo)> = vec![];
        let mut n = 8.0f64;
        while n <= 1.0e9 {
            let algos = [
                Algo::RecursiveDoubling,
                Algo::Ring,
                Algo::Rabenseifner,
                Algo::TreeReduceBcast,
            ];
            let best = algos
                .iter()
                .copied()
                .min_by(|a, b| {
                    model_cost(*a, p, n, alpha, beta).total_cmp(&model_cost(*b, p, n, alpha, beta))
                })
                .unwrap();
            best_seq.push((n, best));
            n *= 2.0;
        }
        // Tree (reduce+bcast) is never strictly best: recursive doubling does the same rounds with half the cost.
        assert!(best_seq.iter().all(|(_, a)| *a != Algo::TreeReduceBcast));
        // Small messages: recursive doubling. Huge messages: bandwidth-optimal (ring or Rabenseifner).
        assert_eq!(best_seq.first().unwrap().1, Algo::RecursiveDoubling);
        let last = best_seq.last().unwrap().1;
        assert!(matches!(last, Algo::Ring | Algo::Rabenseifner));
        // Single switch point from recursive doubling to something else.
        let switches = best_seq.windows(2).filter(|w| w[0].1 != w[1].1).count();
        assert!(
            switches <= 2,
            "p={p}: expected at most 2 regime changes, got {switches}"
        );
        // analytic RD <-> Rabenseifner crossover: lg*(a + n b) = 2 lg a + 2 (p-1)/p n b  =>  n* = lg a / (b (lg - 2(p-1)/p))
        let lg = lg_ceil(p) as f64;
        let denom = beta * (lg - 2.0 * (p as f64 - 1.0) / p as f64);
        if denom > 0.0 {
            let nstar = lg * alpha / denom;
            let below = model_cost(Algo::RecursiveDoubling, p, nstar * 0.9, alpha, beta)
                < model_cost(Algo::Rabenseifner, p, nstar * 0.9, alpha, beta);
            let above = model_cost(Algo::RecursiveDoubling, p, nstar * 1.1, alpha, beta)
                > model_cost(Algo::Rabenseifner, p, nstar * 1.1, alpha, beta);
            assert!(
                below && above,
                "analytic crossover {nstar} must bracket the sign change (p={p})"
            );
            println!(
                "   p={p:>3}: recursive doubling -> Rabenseifner at n* = {:>9.0} bytes (analytic)",
                nstar
            );
        } else {
            println!(
                "   p={p:>3}: recursive doubling always wins against Rabenseifner (lg p <= 2(p-1)/p)"
            );
        }
        let desc: Vec<String> = {
            let mut v: Vec<String> = vec![];
            let mut prev: Option<Algo> = None;
            for (n, a) in &best_seq {
                if prev != Some(*a) {
                    v.push(format!("{a:?} from {n:.0} B"));
                    prev = Some(*a);
                }
            }
            v
        };
        println!("            model winners: {}", desc.join(" | "));
    }
    // p = 2: lg p = 2(p-1)/p = 1, so Rabenseifner has NO bandwidth advantage and pays double latency.
    for p in [2usize] {
        for n in [8.0, 1e3, 1e6, 1e9] {
            assert!(
                model_cost(Algo::RecursiveDoubling, p, n, alpha, beta)
                    <= model_cost(Algo::Rabenseifner, p, n, alpha, beta) + 1e-18
            );
        }
    }
}

// ---------- main ----------

fn main() {
    let t0 = std::time::Instant::now();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut rng = Rng::new(883);

    // n multiple of 16 so every power-of-two core size up to 16 divides it (Rabenseifner requirement).
    for p in 1..=16usize {
        for _ in 0..3 {
            let n = 16 * (1 + (rng.next_u64() % 12) as usize);
            let seed = rng.next_u64();
            verify_allreduces(seed, p, n);
            *counts.entry("allreduce x4 algorithms").or_default() += 1;
            verify_bcast_reduce(seed, p, n);
            *counts.entry("bcast+reduce (3 roots)").or_default() += 1;
        }
        let b = 1 + (rng.next_u64() % 5) as usize;
        verify_data_movement(rng.next_u64(), p, b);
        *counts
            .entry("allgather/alltoall/scatter/gather")
            .or_default() += 1;
        verify_barrier(p);
        *counts.entry("barrier").or_default() += 1;
    }
    for (k, v) in &counts {
        println!(
            "verified {v:>3} configurations of {k} for p = 1..=16 (results == sequential; traffic == formula)"
        );
    }

    // The ring allreduce also works when n < p (some chunks are empty).
    for p in 2..=9usize {
        let n = 3;
        let want = oracle_sum(5, p, n);
        let (res, m, e) = run_world(p, |r| {
            let mut d = make_input(5, r.rank, n);
            allreduce_ring(r, &mut d);
            d
        });
        assert!(res.iter().all(|d| *d == want));
        check_traffic("ring n<p", p, (m, e), ring_formula(p, n));
    }
    println!("ring allreduce correct when the vector is shorter than the rank count");

    // Show a traffic table for p = 13 (non power of two), n = 64.
    let (p, n) = (13, 64);
    println!("traffic for p={p}, n={n} elements (messages, elements):");
    for (name, t) in [
        ("tree reduce+bcast", tree_allreduce_formula(p, n)),
        ("ring", ring_formula(p, n)),
        ("recursive doubling", rd_formula(p, n)),
        ("rabenseifner", rabenseifner_formula(p, n)),
    ] {
        println!("   {name:<20} msgs {:>5}  elems {:>6}", t.msgs, t.elems);
    }

    println!("cost-model crossovers (alpha = 1 us, 100 Gbit/s link):");
    crossover_analysis();
    println!("OK in {:.2?}", t0.elapsed());
}
