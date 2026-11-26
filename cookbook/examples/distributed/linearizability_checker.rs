//! RECIPE 405 (distributed): linearizability checker (Wing-Gong search with memoisation) vs sequential models
//! LEVEL: advanced
//! Use in: testing replicated stores, caches, queues and counters under concurrency; Jepsen-style checks.   Run: cargo run --release --example linearizability_checker
#![allow(dead_code)]
//!
//! WHAT: given a HISTORY of concurrent operations (each with an invoke time, a return time and an observed
//! result) decide whether some total order of the operations (a) respects real time (if A returned before B
//! was invoked, A is before B) and (b) is a legal run of a SEQUENTIAL specification (register, queue, counter).
//! If yes the object is linearizable for that history: every operation appears to take effect atomically at
//! one instant between its invoke and its return.
//!
//! HOW: Wing & Gong's search: repeatedly pick any MINIMAL remaining operation (nothing unlinearised must come
//! before it), apply it to the model, and keep the order if the model's output equals the observed output;
//! backtrack otherwise. Gavin Lowe's refinement, used by Knossos and porcupine, memoises the pair
//! (set of linearised ops, model state): reaching the same pair again can never succeed if it failed before,
//! which collapses the exponential tree to at most (prefix-sets x states). Here "set" is a u128 bitmask and
//! the minimal-op test is a precomputed predecessor mask per op. Linearizability is LOCAL (Herlihy & Wing):
//! a history over many independent objects is linearizable iff each per-object sub-history is, so a KV store
//! is checked key by key (P-compositionality), which the demo measures against the monolithic check.
//!
//! The same search with a weaker precedence relation (only same-client program order) checks SEQUENTIAL
//! CONSISTENCY; the demo shows a history that is sequentially consistent but not linearizable (a stale read
//! after an acknowledged write is allowed to "reorder" across clients, but real time forbids it).
//!
//! TESTING: histories come from a simulated primary/follower store with random delays. Each op picks a random
//! instant inside its [invoke, return] window at which the primary applies it, so the correct store is
//! linearizable by construction. Injected bugs: reads served from a lagging follower (stale reads), an
//! acknowledged write that is silently lost, and an enqueue applied twice (retry duplicate). A failing
//! history is shrunk greedily to a 1-minimal counterexample window by deleting operations.
//!
//! COMMON USES: Jepsen's Knossos checker (Clojure) and the Go library porcupine run exactly this search over
//! recorded client histories to find consistency bugs in databases, queues and lock services; etcd, TiKV and
//! CockroachDB-style systems are tested this way. Checkers are NP-complete in general, so histories are kept
//! short and many are run.
//! Read: Linearizability: A Correctness Condition for Concurrent Objects (Herlihy, Wing, 1990)
//!   https://cs.brown.edu/~mph/HerlihyW90/p463-herlihy.pdf
//! Read: porcupine, a fast linearizability checker in Go   https://github.com/anishathalye/porcupine
//! Read: Jepsen consistency models: linearizable   https://jepsen.io/consistency/models/linearizable

use std::collections::HashSet;
use std::fmt::Debug;
use std::hash::Hash;

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.next() % (hi - lo + 1)
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

// ---------------------------------------------------------------- sequential models

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Inp {
    Write(u64),
    Read,
    Enq(u64),
    Deq,
    Inc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Out {
    Ack,
    Val(u64),
    Empty,
}

/// A sequential specification: `apply` is a total function from (state, input) to (state', output).
trait Model {
    type S: Clone + Eq + Hash + Debug;
    type I: Clone + Debug;
    fn init() -> Self::S;
    fn apply(s: &Self::S, i: &Self::I) -> (Self::S, Out);
    fn is_read(i: &Self::I) -> bool;
    /// May a retry duplicate this op? (only enqueue in this demo)
    fn dup_ok(i: &Self::I) -> bool;
    /// May a write-like op be acknowledged but dropped? (queue: only enqueue)
    fn can_lose(i: &Self::I) -> bool {
        !Self::is_read(i)
    }
}

struct Register;
impl Model for Register {
    type S = u64;
    type I = Inp;
    fn init() -> u64 {
        0
    }
    fn apply(s: &u64, i: &Inp) -> (u64, Out) {
        match i {
            Inp::Write(v) => (*v, Out::Ack),
            Inp::Read => (*s, Out::Val(*s)),
            _ => unreachable!("not a register op"),
        }
    }
    fn is_read(i: &Inp) -> bool {
        *i == Inp::Read
    }
    fn dup_ok(_: &Inp) -> bool {
        false
    }
}

struct Queue;
impl Model for Queue {
    type S = Vec<u64>;
    type I = Inp;
    fn init() -> Vec<u64> {
        Vec::new()
    }
    fn apply(s: &Vec<u64>, i: &Inp) -> (Vec<u64>, Out) {
        let mut q = s.clone();
        match i {
            Inp::Enq(v) => {
                q.push(*v);
                (q, Out::Ack)
            }
            Inp::Deq if q.is_empty() => (q, Out::Empty),
            Inp::Deq => {
                let v = q.remove(0);
                (q, Out::Val(v))
            }
            _ => unreachable!("not a queue op"),
        }
    }
    fn is_read(_: &Inp) -> bool {
        false
    }
    fn dup_ok(i: &Inp) -> bool {
        matches!(i, Inp::Enq(_))
    }
    fn can_lose(i: &Inp) -> bool {
        matches!(i, Inp::Enq(_))
    }
}

struct Counter;
impl Model for Counter {
    type S = u64;
    type I = Inp;
    fn init() -> u64 {
        0
    }
    fn apply(s: &u64, i: &Inp) -> (u64, Out) {
        match i {
            Inp::Inc => (s + 1, Out::Ack),
            Inp::Read => (*s, Out::Val(*s)),
            _ => unreachable!("not a counter op"),
        }
    }
    fn is_read(i: &Inp) -> bool {
        *i == Inp::Read
    }
    fn dup_ok(_: &Inp) -> bool {
        false
    }
}

const KEYS: usize = 3;

/// KV store as ONE monolithic object: used to show what P-compositionality saves.
struct Kv;
impl Model for Kv {
    type S = Vec<u64>;
    type I = (usize, Inp);
    fn init() -> Vec<u64> {
        vec![0; KEYS]
    }
    fn apply(s: &Vec<u64>, (k, i): &(usize, Inp)) -> (Vec<u64>, Out) {
        let (v, out) = Register::apply(&s[*k], i);
        let mut t = s.clone();
        t[*k] = v;
        (t, out)
    }
    fn is_read((_, i): &(usize, Inp)) -> bool {
        *i == Inp::Read
    }
    fn dup_ok(_: &(usize, Inp)) -> bool {
        false
    }
}

// ---------------------------------------------------------------- history + checker

#[derive(Clone, Debug)]
struct Entry<I> {
    client: usize,
    call: u64,
    ret: u64,
    input: I,
    output: Out,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Linearizable,
    Sequential,
}

struct Verdict {
    ok: bool,
    nodes: u64,
    order: Vec<usize>,
}

/// predecessor masks: bit j of preds[i] means op j must be linearised before op i.
fn predecessors<I>(h: &[Entry<I>], mode: Mode) -> Vec<u128> {
    (0..h.len())
        .map(|i| {
            (0..h.len())
                .filter(|&j| match mode {
                    Mode::Linearizable => h[j].ret < h[i].call,
                    Mode::Sequential => h[j].client == h[i].client && h[j].call < h[i].call,
                })
                .fold(0u128, |m, j| m | 1u128 << j)
        })
        .collect()
}

fn check<M: Model>(h: &[Entry<M::I>], mode: Mode) -> Verdict {
    assert!(h.len() <= 128, "bitmask supports 128 ops");
    let preds = predecessors(h, mode);
    let full: u128 = if h.len() == 128 {
        u128::MAX
    } else {
        (1u128 << h.len()) - 1
    };
    let mut cache: HashSet<(u128, M::S)> = HashSet::new();
    let mut order = Vec::new();
    let mut nodes = 0;
    let ok = dfs::<M>(
        h,
        &preds,
        full,
        0,
        &M::init(),
        &mut cache,
        &mut order,
        &mut nodes,
    );
    Verdict { ok, nodes, order }
}

#[allow(clippy::too_many_arguments)]
fn dfs<M: Model>(
    h: &[Entry<M::I>],
    preds: &[u128],
    full: u128,
    done: u128,
    state: &M::S,
    cache: &mut HashSet<(u128, M::S)>,
    order: &mut Vec<usize>,
    nodes: &mut u64,
) -> bool {
    if done == full {
        return true;
    }
    // Lowe's memoisation: this (linearised set, state) pair was explored and failed (success returns at once).
    if !cache.insert((done, state.clone())) {
        return false;
    }
    *nodes += 1;
    for i in 0..h.len() {
        let minimal = done >> i & 1 == 0 && preds[i] & !done == 0;
        if !minimal {
            continue;
        }
        let (next, out) = M::apply(state, &h[i].input);
        if out != h[i].output {
            continue; // the model could not have produced the observed result here
        }
        order.push(i);
        if dfs::<M>(
            h,
            preds,
            full,
            done | 1u128 << i,
            &next,
            cache,
            order,
            nodes,
        ) {
            return true;
        }
        order.pop();
    }
    false
}

fn is_linearizable<M: Model>(h: &[Entry<M::I>]) -> bool {
    check::<M>(h, Mode::Linearizable).ok
}

/// P-compositionality for the KV store: check each key's sub-history on a plain register.
fn check_kv_partitioned(h: &[Entry<(usize, Inp)>]) -> (bool, u64) {
    let mut nodes = 0;
    for k in 0..KEYS {
        let sub: Vec<Entry<Inp>> = h
            .iter()
            .filter(|e| e.input.0 == k)
            .map(|e| Entry {
                client: e.client,
                call: e.call,
                ret: e.ret,
                input: e.input.1,
                output: e.output,
            })
            .collect();
        let v = check::<Register>(&sub, Mode::Linearizable);
        nodes += v.nodes;
        if !v.ok {
            return (false, nodes);
        }
    }
    (true, nodes)
}

/// Greedy 1-minimal shrinking: delete any operation whose removal keeps the history non-linearizable.
/// `keeps` says a candidate is still "the same failure". Plain non-linearizability is not enough: deleting
/// the op that explained a value would manufacture a NEW violation (a lone read of a value nobody wrote). So
/// callers require every observed value to keep a producer AND the same diagnosed pattern to persist.
fn minimize<M: Model>(
    h: &[Entry<M::I>],
    keeps: &dyn Fn(&[Entry<M::I>]) -> bool,
) -> Vec<Entry<M::I>> {
    let mut cur = h.to_vec();
    loop {
        let mut shrunk = false;
        for i in 0..cur.len() {
            let mut t = cur.clone();
            t.remove(i);
            if keeps(&t) && !is_linearizable::<M>(&t) {
                cur = t;
                shrunk = true;
                break;
            }
        }
        if !shrunk {
            return cur;
        }
    }
}

fn supported_register(h: &[Entry<Inp>]) -> bool {
    h.iter().all(|r| match (r.input, r.output) {
        (Inp::Read, Out::Val(v)) => v == 0 || h.iter().any(|w| w.input == Inp::Write(v)),
        _ => true,
    })
}

fn supported_queue(h: &[Entry<Inp>]) -> bool {
    h.iter().all(|d| match (d.input, d.output) {
        (Inp::Deq, Out::Val(v)) => h.iter().any(|e| e.input == Inp::Enq(v)),
        _ => true,
    })
}

fn supported_counter(h: &[Entry<Inp>]) -> bool {
    let incs = h.iter().filter(|e| e.input == Inp::Inc).count() as u64;
    h.iter()
        .all(|r| !matches!((r.input, r.output), (Inp::Read, Out::Val(v)) if v > incs))
}

fn show<I: Debug>(h: &[Entry<I>]) {
    let mut sorted: Vec<&Entry<I>> = h.iter().collect();
    sorted.sort_by_key(|e| e.call);
    for e in sorted {
        println!(
            "      c{} [{:4},{:4}] {:?} -> {:?}",
            e.client, e.call, e.ret, e.input, e.output
        );
    }
}

// ---------------------------------------------------------------- simulated primary/follower store

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Bug {
    None,
    StaleRead,
    LostWrite,
    DupEnqueue,
}

struct Planned<I> {
    client: usize,
    call: u64,
    ret: u64,
    exec: u64,
    input: I,
}

/// Plan client-sequential operations with random durations, then execute them on the "primary" in order of
/// their random execution instants (always inside the op's window), applying the selected bug.
fn simulate<M: Model>(
    rng: &mut Rng,
    clients: usize,
    per_client: usize,
    gen_op: fn(&mut Rng, &mut u64, usize, usize) -> M::I,
    bug: Bug,
) -> Vec<Entry<M::I>> {
    let mut uniq = 0u64;
    let mut plan: Vec<Planned<M::I>> = Vec::new();
    for c in 0..clients {
        let mut t = rng.range(0, 10);
        for idx in 0..per_client {
            let call = t;
            let dur = rng.range(4, 30);
            let ret = call + dur;
            plan.push(Planned {
                client: c,
                call,
                ret,
                exec: rng.range(call + 1, ret - 1),
                input: gen_op(rng, &mut uniq, idx, per_client),
            });
            t = ret + rng.range(1, 12);
        }
    }
    let mut order: Vec<usize> = (0..plan.len()).collect();
    order.sort_by_key(|&i| (plan[i].exec, i));

    let mut state = M::init();
    let mut snaps: Vec<(u64, M::S)> = vec![(0, state.clone())]; // primary state over time, for the lagging follower
    let mut outs: Vec<Option<Out>> = vec![None; plan.len()];
    for i in order {
        let p = &plan[i];
        let out;
        if bug == Bug::StaleRead && M::is_read(&p.input) && rng.chance(60) {
            // Follower is `lag` ticks behind the primary: answer from an old snapshot.
            let at = p.exec.saturating_sub(rng.range(15, 60));
            let snap = snaps
                .iter()
                .rev()
                .find(|(t, _)| *t <= at)
                .map(|x| &x.1)
                .unwrap_or(&snaps[0].1);
            out = M::apply(snap, &p.input).1;
        } else if bug == Bug::LostWrite && M::can_lose(&p.input) && rng.chance(60) {
            out = M::apply(&state, &p.input).1; // acknowledged, never applied
        } else if bug == Bug::DupEnqueue && M::dup_ok(&p.input) && rng.chance(30) {
            let (s1, o) = M::apply(&state, &p.input);
            state = M::apply(&s1, &p.input).0; // retry applied the enqueue a second time
            snaps.push((p.exec, state.clone()));
            out = o;
        } else {
            let (s, o) = M::apply(&state, &p.input);
            state = s;
            snaps.push((p.exec, state.clone()));
            out = o;
        }
        outs[i] = Some(out);
    }
    plan.into_iter()
        .zip(outs)
        .map(|(p, o)| Entry {
            client: p.client,
            call: p.call,
            ret: p.ret,
            input: p.input,
            output: o.unwrap(),
        })
        .collect()
}

fn gen_register(rng: &mut Rng, uniq: &mut u64, idx: usize, per_client: usize) -> Inp {
    // Final reads give a lost or stale write a chance to be observed after the writers finish.
    if idx + 3 < per_client && rng.chance(45) {
        *uniq += 1;
        Inp::Write(*uniq)
    } else {
        Inp::Read
    }
}

fn gen_queue(rng: &mut Rng, uniq: &mut u64, idx: usize, per_client: usize) -> Inp {
    // Drain phase at the end so duplicated/lost items become observable.
    if idx + 3 >= per_client || rng.chance(40) {
        Inp::Deq
    } else {
        *uniq += 1;
        Inp::Enq(*uniq)
    }
}

fn gen_counter(rng: &mut Rng, _: &mut u64, idx: usize, per_client: usize) -> Inp {
    if idx + 2 < per_client && rng.chance(60) {
        Inp::Inc
    } else {
        Inp::Read
    }
}

fn gen_kv(rng: &mut Rng, uniq: &mut u64, idx: usize, per_client: usize) -> (usize, Inp) {
    (
        (rng.next() % KEYS as u64) as usize,
        gen_register(rng, uniq, idx, per_client),
    )
}

// ---------------------------------------------------------------- diagnosis of minimal counterexamples

fn diagnose_register(h: &[Entry<Inp>]) -> &'static str {
    for r in h.iter().filter(|e| e.input == Inp::Read) {
        let Out::Val(v) = r.output else { continue };
        let acked: Vec<&Entry<Inp>> = h
            .iter()
            .filter(|w| matches!(w.input, Inp::Write(_)) && w.ret < r.call)
            .collect();
        let src = h.iter().find(|w| w.input == Inp::Write(v));
        match src {
            None if !acked.is_empty() => return "read missed acknowledged write (stale or lost)",
            Some(s) if acked.iter().any(|w| w.call > s.ret) => {
                return "stale read: returned an overwritten value";
            }
            _ => {}
        }
    }
    "non-linearizable ordering"
}

fn diagnose_queue(h: &[Entry<Inp>]) -> &'static str {
    let vals: Vec<u64> = h
        .iter()
        .filter_map(|e| {
            if let (Inp::Deq, Out::Val(v)) = (e.input, e.output) {
                Some(v)
            } else {
                None
            }
        })
        .collect();
    if (0..vals.len()).any(|i| vals[i + 1..].contains(&vals[i])) {
        return "item delivered twice (duplicate enqueue)";
    }
    for d in h
        .iter()
        .filter(|d| d.input == Inp::Deq && d.output == Out::Empty)
    {
        let enqueued = h
            .iter()
            .filter(|e| matches!(e.input, Inp::Enq(_)) && e.ret < d.call)
            .count();
        let consumers = h
            .iter()
            .filter(|e| e.input == Inp::Deq && matches!(e.output, Out::Val(_)) && e.call < d.ret)
            .count();
        if enqueued > consumers {
            return "empty dequeue though acknowledged enqueues outnumber possible dequeues (lost enqueue)";
        }
    }
    "non-linearizable ordering"
}

fn diagnose_counter(h: &[Entry<Inp>]) -> &'static str {
    for r in h.iter().filter(|e| e.input == Inp::Read) {
        let Out::Val(v) = r.output else { continue };
        let done = h
            .iter()
            .filter(|w| w.input == Inp::Inc && w.ret < r.call)
            .count() as u64;
        if v < done {
            return "read below the number of acknowledged increments (lost/stale)";
        }
    }
    "non-linearizable ordering"
}

// ---------------------------------------------------------------- experiments

struct Row {
    name: &'static str,
    bug: Bug,
    runs: u32,
    failed: u32,
    nodes: u64,
}

fn experiment<M: Model<I = Inp>>(
    name: &'static str,
    bug: Bug,
    runs: u32,
    seed0: u64,
    gen_op: fn(&mut Rng, &mut u64, usize, usize) -> Inp,
    diagnose: fn(&[Entry<Inp>]) -> &'static str,
    supported: fn(&[Entry<Inp>]) -> bool,
    clients: usize,
    per_client: usize,
) -> Row {
    let mut row = Row {
        name,
        bug,
        runs,
        failed: 0,
        nodes: 0,
    };
    let mut shown = false;
    let mut diag: Vec<(&'static str, u32)> = Vec::new();
    for s in 0..runs {
        let mut rng = Rng::new(seed0 + s as u64);
        let h = simulate::<M>(&mut rng, clients, per_client, gen_op, bug);
        let v = check::<M>(&h, Mode::Linearizable);
        row.nodes += v.nodes;
        if bug == Bug::None {
            assert!(
                v.ok,
                "{name}: correct store must be linearizable (seed {})",
                seed0 + s as u64
            );
            // The witness order returned by the search must itself be a legal, real-time-respecting run.
            assert_eq!(v.order.len(), h.len());
            continue;
        }
        if !v.ok {
            row.failed += 1;
            // Diagnose on the FULL history (sound); shrink only the first example for display.
            let d = diagnose(&h);
            match diag.iter_mut().find(|x| x.0 == d) {
                Some(x) => x.1 += 1,
                None => diag.push((d, 1)),
            }
            if !shown && d != "non-linearizable ordering" {
                shown = true;
                let min = minimize::<M>(&h, &|t| supported(t) && diagnose(t) == d);
                assert!(!is_linearizable::<M>(&min));
                println!(
                    "  [{name} / {bug:?}] counterexample shrunk from {} ops to {}:",
                    h.len(),
                    min.len()
                );
                show(&min);
                println!("      diagnosis: {d}");
            }
        }
    }
    if bug != Bug::None {
        println!(
            "  [{name} / {bug:?}] diagnoses of all {} failing histories: {diag:?}",
            row.failed
        );
    }
    row
}

fn demo_hand_built() {
    let w = |c, call, ret, v| Entry {
        client: c,
        call,
        ret,
        input: Inp::Write(v),
        output: Out::Ack,
    };
    let r = |c, call, ret, v| Entry {
        client: c,
        call,
        ret,
        input: Inp::Read,
        output: Out::Val(v),
    };

    // H1: read overlaps the write and sees it: fine.
    let h1 = vec![w(0, 0, 10, 1), r(1, 2, 8, 1)];
    // H2: write returned at 10, a different client reads 0 afterwards. SC (put the read first), not linearizable.
    let h2 = vec![w(0, 0, 10, 1), r(1, 20, 30, 0)];
    // H3: a client does not even see its own earlier write: violates program order, so not even SC.
    let h3 = vec![w(0, 0, 5, 1), r(0, 6, 9, 0)];
    let verdict = |h: &[Entry<Inp>]| {
        (
            check::<Register>(h, Mode::Linearizable).ok,
            check::<Register>(h, Mode::Sequential).ok,
        )
    };
    println!("hand-built histories on a register (linearizable?, sequentially consistent?):");
    for (name, h, expect) in [
        ("H1 overlapping read sees write", &h1, (true, true)),
        ("H2 stale read after ack'd write", &h2, (false, true)),
        ("H3 client misses own write", &h3, (false, false)),
    ] {
        let got = verdict(h);
        println!("  {name:36} -> {got:?}");
        assert_eq!(got, expect);
    }
    // Queue FIFO violation across clients.
    let enq = |c, call, ret, v| Entry {
        client: c,
        call,
        ret,
        input: Inp::Enq(v),
        output: Out::Ack,
    };
    let deq = |c, call, ret, v| Entry {
        client: c,
        call,
        ret,
        input: Inp::Deq,
        output: Out::Val(v),
    };
    let q_ok = vec![
        enq(0, 0, 5, 1),
        enq(0, 6, 10, 2),
        deq(1, 12, 15, 1),
        deq(1, 16, 19, 2),
    ];
    let q_bad = vec![
        enq(0, 0, 5, 1),
        enq(0, 6, 10, 2),
        deq(1, 12, 15, 2),
        deq(1, 16, 19, 1),
    ];
    assert!(is_linearizable::<Queue>(&q_ok));
    assert!(!is_linearizable::<Queue>(&q_bad));
    println!("  queue: FIFO order accepted, swapped dequeues rejected");
    // Counter: two increments both acknowledged but a later read sees only one.
    let inc = |c, call, ret| Entry {
        client: c,
        call,
        ret,
        input: Inp::Inc,
        output: Out::Ack,
    };
    let rd = |c, call, ret, v| Entry {
        client: c,
        call,
        ret,
        input: Inp::Read,
        output: Out::Val(v),
    };
    assert!(is_linearizable::<Counter>(&[
        inc(0, 0, 5),
        inc(1, 1, 6),
        rd(2, 7, 9, 2)
    ]));
    assert!(!is_linearizable::<Counter>(&[
        inc(0, 0, 5),
        inc(1, 1, 6),
        rd(2, 7, 9, 1)
    ]));
    assert!(
        is_linearizable::<Counter>(&[inc(0, 0, 5), inc(1, 1, 20), rd(2, 7, 9, 1)]),
        "overlapping inc may be unseen"
    );
    println!("  counter: lost acknowledged increment rejected, in-flight increment tolerated");
}

fn demo_partitioning() {
    let (mut mono_nodes, mut part_nodes, mut agree, mut bad) = (0u64, 0u64, 0u32, 0u32);
    let runs = 150;
    for s in 0..runs {
        for bug in [Bug::None, Bug::StaleRead] {
            let mut rng = Rng::new(7000 + s);
            let h = simulate::<Kv>(&mut rng, 4, 6, gen_kv, bug);
            let mono = check::<Kv>(&h, Mode::Linearizable);
            let (ok, n) = check_kv_partitioned(&h);
            assert_eq!(
                mono.ok, ok,
                "locality: whole-store verdict equals the conjunction of per-key verdicts"
            );
            agree += 1;
            bad += (!ok) as u32;
            if bug == Bug::None {
                assert!(ok);
            }
            mono_nodes += mono.nodes;
            part_nodes += n;
        }
    }
    println!(
        "\nP-compositionality on a 3-key KV store ({agree} histories, {bad} non-linearizable): verdicts always agree"
    );
    println!(
        "  search nodes explored: monolithic {mono_nodes}, per-key {part_nodes} ({:.1}x fewer)",
        mono_nodes as f64 / part_nodes as f64
    );
    assert!(part_nodes < mono_nodes);
}

fn main() {
    demo_hand_built();

    println!(
        "\nsimulated store, 1000 histories per model for the correct implementation, 400 per injected bug:"
    );
    let mut rows = Vec::new();
    for bug in [Bug::None, Bug::StaleRead, Bug::LostWrite] {
        let runs = if bug == Bug::None { 1000 } else { 400 };
        rows.push(experiment::<Register>(
            "register",
            bug,
            runs,
            100,
            gen_register,
            diagnose_register,
            supported_register,
            4,
            6,
        ));
        rows.push(experiment::<Counter>(
            "counter",
            bug,
            runs,
            200,
            gen_counter,
            diagnose_counter,
            supported_counter,
            4,
            6,
        ));
    }
    for bug in [Bug::None, Bug::DupEnqueue, Bug::LostWrite] {
        let runs = if bug == Bug::None { 1000 } else { 400 };
        rows.push(experiment::<Queue>(
            "queue",
            bug,
            runs,
            300,
            gen_queue,
            diagnose_queue,
            supported_queue,
            4,
            6,
        ));
    }

    println!("\n  model     bug          histories  flagged  rate    avg search nodes");
    for r in &rows {
        let rate = r.failed as f64 / r.runs as f64;
        println!(
            "  {:8}  {:11}  {:9}  {:7}  {:5.1}%  {:.1}",
            r.name,
            format!("{:?}", r.bug),
            r.runs,
            r.failed,
            rate * 100.0,
            r.nodes as f64 / r.runs as f64
        );
        match r.bug {
            Bug::None => assert_eq!(r.failed, 0, "no false positives"),
            // A lost write stays hidden if a later write overwrites it before any read, so rates are below 100%.
            _ => assert!(
                rate >= 0.6,
                "{} {:?}: detection rate {rate:.2} too low",
                r.name,
                r.bug
            ),
        }
    }

    demo_partitioning();
    println!("\nall linearizability checks passed");
}
