//! RECIPE 122 (distributed): partitioning keys across N nodes: modulo, consistent-hash ring, rendezvous (HRW), Jump Consistent Hash, Maglev table.
//! LEVEL: intermediate
//! Use in: sharding caches/databases, load balancers, work distribution; choosing how much data moves when the cluster resizes.   Run: cargo run --release --example hash_partitioning
#![allow(dead_code)]
//!
//! WHAT: five ways to answer "which node owns this key?", all behind one `Partitioner` trait, measured on 100k keys and 10 nodes.
//! WHY: with `hash % N`, changing N reassigns almost EVERY key (a cache stampede or a mass data migration). Good schemes move only about 1/(N+1)
//! of keys when a node is added, the theoretical minimum, and those keys all move TO the new node.
//! HOW (and the price of each):
//!  - modulo: O(1) time and memory, but moves ~N/(N+1) of the keys on resize.
//!  - ring (consistent_hashing style): nodes get many virtual points on a u64 ring; a key goes to the next point clockwise. Memory N*vnodes, lookup O(log) via BTreeMap;
//!    balance improves with more vnodes (about 1/sqrt(vnodes) spread).
//!  - rendezvous / highest random weight: score every (key,node) pair and take the max. No state, perfect balance in expectation, but O(N) per lookup.
//!  - jump hash (Lamping & Veach, https://arxiv.org/abs/1406.2294): a few lines, no memory, O(ln N) time, near-perfect balance; but nodes are only
//!    numbered 0..N, so you can only add or remove the LAST one (fine for shards, not for arbitrary failures).
//!  - Maglev (Google's load balancer): fill a prime-sized lookup table by letting nodes take turns claiming slots along their own permutation.
//!    O(1) lookup; balance is within about 1% of ideal; resizing disrupts a bit more than the minimum.
//!
//! COMMON USES: cache-cluster sharding (client-side hashing in memcached-style clients), Dynamo-style stores using ring partitioning, load balancers
//! (Maglev), shard assignment by numeric shard id (jump hash), choosing replicas per key (HRW top-k).
//! Read: https://arxiv.org/abs/1406.2294 and https://en.wikipedia.org/wiki/Rendezvous_hashing and https://research.google/pubs/maglev-a-fast-and-reliable-software-network-load-balancer/
use std::collections::BTreeMap;
use std::hint::black_box;
use std::time::Instant;

/// splitmix64-style finalizer: our deterministic, well-mixed hash for keys and node names.
fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}
fn node_hash(node: usize, salt: u64) -> u64 {
    mix(((node as u64) << 32) ^ salt)
}

trait Partitioner {
    fn assign(&self, key: u64) -> usize;
}

struct Modulo(usize);
impl Partitioner for Modulo {
    fn assign(&self, key: u64) -> usize {
        (mix(key) % self.0 as u64) as usize
    }
}

struct Ring(BTreeMap<u64, usize>);
impl Ring {
    fn new(nodes: usize, vnodes: u64) -> Self {
        Ring(
            (0..nodes)
                .flat_map(|n| (0..vnodes).map(move |v| (node_hash(n, v), n)))
                .collect(),
        )
    }
}
impl Partitioner for Ring {
    fn assign(&self, key: u64) -> usize {
        let h = mix(key);
        // first point at or after h, wrapping to the smallest point
        *self
            .0
            .range(h..)
            .next()
            .or_else(|| self.0.iter().next())
            .unwrap()
            .1
    }
}

struct Rendezvous(usize);
impl Partitioner for Rendezvous {
    fn assign(&self, key: u64) -> usize {
        let h = mix(key);
        // each node's weight for this key is independent of the other nodes, so removing a node only affects keys it won
        (0..self.0)
            .max_by_key(|&n| mix(h ^ node_hash(n, 77)))
            .unwrap()
    }
}

/// Jump Consistent Hash, transcribed from the paper (Figure 1): `key` is advanced by an LCG; each step decides whether the key jumps to a later bucket.
fn jump_hash(mut key: u64, buckets: i32) -> i32 {
    let (mut b, mut j) = (-1i64, 0i64);
    while j < buckets as i64 {
        b = j;
        key = key.wrapping_mul(2_862_933_555_777_941_757).wrapping_add(1);
        j = ((b + 1) as f64 * ((1u64 << 31) as f64 / ((key >> 33) + 1) as f64)) as i64;
    }
    b as i32
}
struct Jump(usize);
impl Partitioner for Jump {
    fn assign(&self, key: u64) -> usize {
        jump_hash(mix(key), self.0 as i32) as usize
    }
}

struct Maglev(Vec<usize>);
impl Maglev {
    const M: usize = 65_537; // prime, much larger than the node count
    fn new(nodes: usize) -> Self {
        let m = Self::M;
        // each node walks its own permutation: slot_j = (offset + j*skip) mod M; skip in 1..M-1 is coprime to prime M so it visits every slot
        let perms: Vec<(usize, usize)> = (0..nodes)
            .map(|n| {
                (
                    (node_hash(n, 1) % m as u64) as usize,
                    (node_hash(n, 2) % (m as u64 - 1)) as usize + 1,
                )
            })
            .collect();
        let (mut next, mut table, mut filled) = (vec![0usize; nodes], vec![usize::MAX; m], 0);
        while filled < m {
            for n in 0..nodes {
                // claim the next still-free slot on node n's permutation (round robin keeps shares equal)
                loop {
                    let slot = (perms[n].0 + next[n] * perms[n].1) % m;
                    next[n] += 1;
                    if table[slot] == usize::MAX {
                        table[slot] = n;
                        filled += 1;
                        break;
                    }
                }
                if filled == m {
                    break;
                }
            }
        }
        Maglev(table)
    }
}
impl Partitioner for Maglev {
    fn assign(&self, key: u64) -> usize {
        self.0[(mix(key) % Self::M as u64) as usize]
    }
}

const KEYS: u64 = 100_000;

fn imbalance(p: &dyn Partitioner, nodes: usize) -> f64 {
    let mut counts = vec![0u64; nodes];
    (0..KEYS).for_each(|k| counts[p.assign(k)] += 1);
    *counts.iter().max().unwrap() as f64 / (KEYS as f64 / nodes as f64)
}

/// Fraction of keys whose owner changed between the two layouts, and whether every moved key went to node `target` (if given).
fn moved(before: &dyn Partitioner, after: &dyn Partitioner, target: Option<usize>) -> (f64, bool) {
    let mut n = 0;
    let mut all_to_target = true;
    for k in 0..KEYS {
        let (a, b) = (before.assign(k), after.assign(k));
        if a != b {
            n += 1;
            all_to_target &= target.is_none_or(|t| b == t);
        }
    }
    (n as f64 / KEYS as f64, all_to_target)
}

fn ns_per_lookup(p: &dyn Partitioner) -> f64 {
    let t = Instant::now();
    let mut acc = 0usize;
    for k in 0..200_000u64 {
        acc ^= p.assign(black_box(k));
    }
    black_box(acc);
    t.elapsed().as_nanos() as f64 / 200_000.0
}

fn main() {
    const N: usize = 10;
    let schemes: [(&str, Box<dyn Fn(usize) -> Box<dyn Partitioner>>, &str); 5] = [
        (
            "modulo",
            Box::new(|n| Box::new(Modulo(n))),
            "O(1) memory, O(1) time",
        ),
        (
            "ring v=200",
            Box::new(|n| Box::new(Ring::new(n, 200))),
            "N*200 points, O(log) time",
        ),
        (
            "rendezvous",
            Box::new(|n| Box::new(Rendezvous(n))),
            "O(N) memory/time per key",
        ),
        (
            "jump",
            Box::new(|n| Box::new(Jump(n))),
            "O(1) memory, O(ln N) time",
        ),
        (
            "maglev",
            Box::new(|n| Box::new(Maglev::new(n))),
            "65537-entry table, O(1) time",
        ),
    ];
    println!(
        "{:<12}{:>10}{:>12}{:>12}{:>10}   notes",
        "scheme", "max/mean", "moved(add)", "moved(rem)", "ns/key"
    );
    let mut r = std::collections::HashMap::new();
    for (name, build, notes) in &schemes {
        let (base, plus, minus) = (build(N), build(N + 1), build(N - 1));
        let imb = imbalance(&*base, N);
        let (add, add_to_new) = moved(&*base, &*plus, Some(N));
        let (rem, _) = moved(&*base, &*minus, None);
        println!(
            "{name:<12}{imb:>10.3}{add:>12.3}{rem:>12.3}{:>10.1}   {notes}",
            ns_per_lookup(&*base)
        );
        r.insert(*name, (imb, add, rem, add_to_new));
    }
    let ideal = 1.0 / (N + 1) as f64;
    println!("ideal move fraction when adding the 11th node: {ideal:.3}");

    assert!(
        r["modulo"].1 > 0.8 && r["modulo"].2 > 0.8,
        "modulo reshuffles most keys"
    );
    for s in ["jump", "rendezvous"] {
        assert!((0.06..0.13).contains(&r[s].1), "{s} add moved {}", r[s].1);
        assert!(r[s].3, "{s}: keys may only move to the NEW node");
        assert!(r[s].0 < 1.1, "{s} imbalance {}", r[s].0);
    }
    assert!((0.05..0.16).contains(&r["jump"].2) && (0.05..0.16).contains(&r["rendezvous"].2));
    assert!(
        r["ring v=200"].0 < 1.4
            && (0.04..0.2).contains(&r["ring v=200"].1)
            && r["ring v=200"].2 < 0.25
    );
    assert!(
        r["maglev"].0 < 1.1 && r["maglev"].1 < 0.4 && r["maglev"].2 < 0.5,
        "maglev disruption: {:?}",
        r["maglev"]
    );

    // Why vnodes: a ring with a single point per node is badly unbalanced.
    let (one, many) = (
        imbalance(&Ring::new(N, 1), N),
        imbalance(&Ring::new(N, 200), N),
    );
    println!("ring imbalance: 1 vnode/node = {one:.2}, 200 vnodes/node = {many:.2}");
    assert!(one > many && many < 1.4);

    // Jump hash sanity: with a single bucket every key maps to bucket 0.
    assert!((0..1000u64).all(|k| jump_hash(k, 1) == 0));
    println!("OK");
}
