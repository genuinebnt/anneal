//! RECIPE 730 (database): Transaction theory: schedules, conflict/view serializability, recoverability classes, ACID, and Adya's isolation phenomena.
//! LEVEL: advanced
//! Use in: anything that interleaves transactions (SQL engines, STMs, key-value stores); the vocabulary for reasoning about isolation bugs.   Run: cargo run --release --example transaction_theory_serializability
#![allow(dead_code)]
//!
//! A SCHEDULE (history) is an interleaving of the read / write / commit / abort operations of several transactions that keeps
//! each transaction's own order. Part 1 treats schedules in the textbook way, Part 2 in Adya's way.
//! * CONFLICT SERIALIZABILITY: two operations conflict if they touch the same object, belong to different transactions and at
//!   least one is a write. Draw an edge Ti -> Tj for every conflicting pair where Ti's operation comes first (the PRECEDENCE
//!   GRAPH). The schedule is conflict-serializable iff the graph (over committed transactions) is acyclic, and any
//!   topological order is an equivalent serial order. This is what lock-based protocols (2PL) and timestamp ordering guarantee.
//! * VIEW SERIALIZABILITY is weaker: the schedule only has to give every read the same source (reads-from) and every object the
//!   same final writer as SOME serial schedule. Deciding it is NP-complete, so we brute-force all permutations (fine for 4
//!   transactions). Every conflict-serializable schedule is view-serializable but not conversely: the gap needs BLIND WRITES
//!   (a write that no read of that transaction preceded), e.g. `r1(x) w2(x) w1(x) w3(x)`.
//! * RECOVERABILITY is about aborts, not about order. RECOVERABLE (RC): a transaction commits only after every transaction it read
//!   from has committed. AVOIDS CASCADING ABORTS (ACA): reads only see committed data, so an abort never drags readers along.
//!   STRICT: nobody reads OR overwrites an object while its last writer is uncommitted, so abort = restore the before-image.
//!   The classes nest: strict < ACA < recoverable, asserted on 5000 random schedules, together with CSR < VSR.
//! * ACID is shown as scripted scenarios on a toy log-based store (atomicity via undo, consistency via a constraint,
//!   isolation via the lost-update interleaving, durability via log replay after a crash).
//! * ADYA'S FORMALISM defines isolation without locks: build the DIRECT SERIALIZATION GRAPH (DSG) of the committed
//!   transactions with edges ww (Tj overwrites Ti's version), wr (Tj reads Ti's version), rw (Ti read a version that Tj
//!   overwrote: ANTI-DEPENDENCY). Phenomena: G0 = ww cycle (dirty write); G1a = read of an aborted write; G1b = read of an
//!   intermediate (non-final) write; G1c = cycle of ww/wr edges; G-single = cycle with exactly one rw edge (what snapshot
//!   isolation forbids); G2-item = cycle with item rw edges; G2 = any cycle (needs predicate rw edges to differ from G2-item).
//!   Levels: READ UNCOMMITTED forbids G0; READ COMMITTED adds G1; REPEATABLE READ (Adya's PL-2.99) adds G2-item; SNAPSHOT adds
//!   G-single instead of G2-item (so write skew passes); SERIALIZABLE forbids G2. The table at the end runs scripted histories.
//!   Caveat: SI's start-ordering condition (G-SIa) needs timestamps and is not modelled; scripted SI histories obey it.
//!   A cross-check proves the DSG and the precedence graph agree on 3000 random all-commit schedules.
//!
//! COMMON USES: reasoning about 2PL / T/O / OCC / MVCC correctness, Jepsen-style history checkers (Elle builds the same
//! ww/wr/rw graph), explaining PostgreSQL's isolation levels, test oracles for transaction engines.
//! Course: CMU 15-445 Lecture 17 (concurrency control theory) and CMU 15-721 isolation levels lecture.
//! Read: "A Critique of ANSI SQL Isolation Levels" (Berenson et al., 1995) https://arxiv.org/pdf/cs/0701157 ,
//! "Weak Consistency: A Generalized Theory and Optimistic Implementations" (Adya, thesis 1999), PostgreSQL isolation docs
//! https://www.postgresql.org/docs/current/transaction-iso.html
use std::collections::{BTreeMap, BTreeSet, HashMap};

// ------------------------------------------------------------------------------------------------------------------
// Part 1: classic schedules
// ------------------------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    R,
    W,
    C,
    A,
}

/// One operation. `obj` is meaningless for commit / abort. Transactions are numbered from 0 internally, printed from 1.
#[derive(Clone, Copy, Debug)]
struct Op {
    tx: usize,
    kind: Kind,
    obj: usize,
}
type Schedule = Vec<Op>;

fn show(s: &[Op]) -> String {
    let parts: Vec<String> = s
        .iter()
        .map(|o| match o.kind {
            Kind::R => format!("r{}({})", o.tx + 1, (b'x' + o.obj as u8) as char),
            Kind::W => format!("w{}({})", o.tx + 1, (b'x' + o.obj as u8) as char),
            Kind::C => format!("c{}", o.tx + 1),
            Kind::A => format!("a{}", o.tx + 1),
        })
        .collect();
    parts.join(" ")
}

/// Parse `"r1(x) w2(x) c1 c2"`.
fn parse(s: &str) -> Schedule {
    s.split_whitespace()
        .map(|t| {
            let b = t.as_bytes();
            let kind = match b[0] {
                b'r' => Kind::R,
                b'w' => Kind::W,
                b'c' => Kind::C,
                b'a' => Kind::A,
                _ => panic!("bad token {t}"),
            };
            let obj = if b.len() >= 4 {
                (b[3] - b'x') as usize
            } else {
                0
            };
            Op {
                tx: (b[1] - b'1') as usize,
                kind,
                obj,
            }
        })
        .collect()
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Generate a LEGAL interleaving: each transaction's own operations stay in order, each ends with exactly one commit or abort,
/// and the merge picks the next transaction at random (weighted by how much it has left, which gives a uniform-ish shuffle).
/// `unique_writes` forbids writing the same object twice in one transaction (needed for the DSG cross-check).
fn gen_schedule(
    rng: &mut Rng,
    n_tx: usize,
    n_obj: usize,
    max_ops: usize,
    abort_pct: usize,
    unique_writes: bool,
) -> Schedule {
    let mut per_tx: Vec<Vec<Op>> = Vec::new();
    for tx in 0..n_tx {
        let mut ops = Vec::new();
        let mut written = BTreeSet::new();
        for _ in 0..1 + rng.below(max_ops) {
            let obj = rng.below(n_obj);
            let kind = if rng.below(2) == 0 { Kind::R } else { Kind::W };
            if kind == Kind::W && unique_writes && !written.insert(obj) {
                continue;
            }
            ops.push(Op { tx, kind, obj });
        }
        let end = if rng.below(100) < abort_pct {
            Kind::A
        } else {
            Kind::C
        };
        ops.push(Op {
            tx,
            kind: end,
            obj: 0,
        });
        per_tx.push(ops);
    }
    let mut cursor = vec![0usize; n_tx];
    let mut out = Vec::new();
    loop {
        let left: usize = (0..n_tx).map(|t| per_tx[t].len() - cursor[t]).sum();
        if left == 0 {
            return out;
        }
        let mut pick = rng.below(left);
        for t in 0..n_tx {
            let rem = per_tx[t].len() - cursor[t];
            if pick < rem {
                out.push(per_tx[t][cursor[t]]);
                cursor[t] += 1;
                break;
            }
            pick -= rem;
        }
    }
}

fn committed(s: &[Op]) -> BTreeSet<usize> {
    s.iter()
        .filter(|o| o.kind == Kind::C)
        .map(|o| o.tx)
        .collect()
}

/// Operations of committed transactions only: the "committed projection" that serializability is defined on.
fn projection(s: &[Op]) -> Schedule {
    let c = committed(s);
    s.iter().filter(|o| c.contains(&o.tx)).copied().collect()
}

fn precedence_edges(s: &[Op]) -> BTreeSet<(usize, usize)> {
    let p = projection(s);
    let mut e = BTreeSet::new();
    for (i, a) in p.iter().enumerate() {
        for b in &p[i + 1..] {
            let data = |k: Kind| k == Kind::R || k == Kind::W;
            if data(a.kind)
                && data(b.kind)
                && a.obj == b.obj
                && a.tx != b.tx
                && (a.kind == Kind::W || b.kind == Kind::W)
            {
                e.insert((a.tx, b.tx));
            }
        }
    }
    e
}

/// Kahn's algorithm; picks the smallest ready node so the printed serial order is deterministic. None = cycle.
fn topo_order(nodes: &BTreeSet<usize>, edges: &BTreeSet<(usize, usize)>) -> Option<Vec<usize>> {
    let mut indeg: BTreeMap<usize, usize> = nodes.iter().map(|&n| (n, 0)).collect();
    for &(_, b) in edges {
        *indeg.get_mut(&b).unwrap() += 1;
    }
    let mut order = Vec::new();
    while let Some((&n, _)) = indeg.iter().find(|(_, d)| **d == 0) {
        indeg.remove(&n);
        order.push(n);
        for &(a, b) in edges {
            if a == n {
                if let Some(d) = indeg.get_mut(&b) {
                    *d -= 1;
                }
            }
        }
    }
    if indeg.is_empty() { Some(order) } else { None }
}

/// Some serial order equivalent to the schedule, if it is conflict-serializable.
fn conflict_serial_order(s: &[Op]) -> Option<Vec<usize>> {
    topo_order(&committed(s), &precedence_edges(s))
}

/// Everything view equivalence compares: for each transaction the sequence of (object, source writer or None = initial value)
/// of its reads, and for each object the final writer.
#[derive(PartialEq, Eq, Debug)]
struct View {
    reads: BTreeMap<usize, Vec<(usize, Option<usize>)>>,
    finals: BTreeMap<usize, usize>,
}

fn view_of(ops: &[Op]) -> View {
    let mut last: HashMap<usize, usize> = HashMap::new();
    let mut v = View {
        reads: BTreeMap::new(),
        finals: BTreeMap::new(),
    };
    for o in ops {
        match o.kind {
            Kind::R => v
                .reads
                .entry(o.tx)
                .or_default()
                .push((o.obj, last.get(&o.obj).copied())),
            Kind::W => {
                last.insert(o.obj, o.tx);
            }
            _ => {}
        }
    }
    v.finals = last.into_iter().collect();
    v
}

fn permutations(items: &[usize]) -> Vec<Vec<usize>> {
    if items.len() <= 1 {
        return vec![items.to_vec()];
    }
    let mut out = Vec::new();
    for i in 0..items.len() {
        let mut rest = items.to_vec();
        let x = rest.remove(i);
        for mut p in permutations(&rest) {
            p.insert(0, x);
            out.push(p);
        }
    }
    out
}

fn view_serial_order(s: &[Op]) -> Option<Vec<usize>> {
    let p = projection(s);
    let target = view_of(&p);
    let txs: Vec<usize> = committed(s).into_iter().collect();
    permutations(&txs).into_iter().find(|perm| {
        let serial: Vec<Op> = perm
            .iter()
            .flat_map(|&t| p.iter().filter(move |o| o.tx == t).copied())
            .collect();
        view_of(&serial) == target
    })
}

/// Walk the schedule keeping, per object, the stack of live (not aborted) writers: an abort undoes that transaction's writes.
/// Calls `f(position, op, writer_on_top_of_stack_for_this_object, committed_set_so_far)` for each read / write.
fn walk_live(s: &[Op], mut f: impl FnMut(usize, &Op, Option<usize>, &BTreeSet<usize>)) {
    let mut stacks: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut done: BTreeSet<usize> = BTreeSet::new();
    for (i, o) in s.iter().enumerate() {
        match o.kind {
            Kind::R | Kind::W => {
                let top = stacks.get(&o.obj).and_then(|st| st.last().copied());
                f(i, o, top, &done);
                if o.kind == Kind::W {
                    stacks.entry(o.obj).or_default().push(o.tx);
                }
            }
            Kind::C => {
                done.insert(o.tx);
            }
            Kind::A => stacks.values_mut().for_each(|st| st.retain(|&t| t != o.tx)),
        }
    }
}

/// RECOVERABLE: whenever Tj reads from Ti (i != j) and Tj commits, Ti committed earlier.
fn is_recoverable(s: &[Op]) -> bool {
    let commit_pos: HashMap<usize, usize> = s
        .iter()
        .enumerate()
        .filter(|(_, o)| o.kind == Kind::C)
        .map(|(i, o)| (o.tx, i))
        .collect();
    let mut ok = true;
    walk_live(s, |_, o, top, _| {
        if let (Kind::R, Some(w)) = (o.kind, top) {
            if w != o.tx {
                if let Some(&cj) = commit_pos.get(&o.tx) {
                    ok &= commit_pos.get(&w).is_some_and(|&ci| ci < cj);
                }
            }
        }
    });
    ok
}

/// ACA: every read sees an already committed (or own) write.
fn is_aca(s: &[Op]) -> bool {
    let mut ok = true;
    walk_live(s, |_, o, top, done| {
        if let (Kind::R, Some(w)) = (o.kind, top) {
            ok &= w == o.tx || done.contains(&w);
        }
    });
    ok
}

/// STRICT: neither reads nor writes touch an object whose last live writer is uncommitted (and not self).
fn is_strict(s: &[Op]) -> bool {
    let mut ok = true;
    walk_live(s, |_, o, top, done| {
        if let Some(w) = top {
            ok &= w == o.tx || done.contains(&w);
        }
    });
    ok
}

fn has_blind_write(s: &[Op]) -> bool {
    let mut read: BTreeSet<(usize, usize)> = BTreeSet::new();
    for o in s {
        match o.kind {
            Kind::R => {
                read.insert((o.tx, o.obj));
            }
            Kind::W if !read.contains(&(o.tx, o.obj)) => return true,
            _ => {}
        }
    }
    false
}

fn names(order: &[usize]) -> String {
    order
        .iter()
        .map(|t| format!("T{}", t + 1))
        .collect::<Vec<_>>()
        .join(" -> ")
}

fn demo_serializability() {
    println!("-- conflict vs view serializability (committed projection)");
    for text in [
        "r1(x) w1(x) r2(x) w2(x) c1 c2",
        "r1(x) r2(x) w1(x) w2(x) c1 c2",
        "r1(x) w2(x) w1(x) w3(x) c1 c2 c3",
    ] {
        let s = parse(text);
        let cs = conflict_serial_order(&s);
        let vs = view_serial_order(&s);
        let fmt = |o: &Option<Vec<usize>>| o.as_ref().map_or("no".to_string(), |v| names(v));
        println!(
            "   {text:<36} conflict: {:<14} view: {}",
            fmt(&cs),
            fmt(&vs)
        );
    }
    let s = parse("r1(x) w2(x) w1(x) w3(x) c1 c2 c3");
    assert!(conflict_serial_order(&s).is_none(), "T1 -> T2 and T2 -> T1");
    assert_eq!(
        view_serial_order(&s),
        Some(vec![0, 1, 2]),
        "view-equivalent to T1 T2 T3 thanks to blind writes"
    );
    assert!(has_blind_write(&s));
    assert_eq!(
        conflict_serial_order(&parse("r1(x) w1(x) r2(x) w2(x) c1 c2")),
        Some(vec![0, 1])
    );
    assert!(
        conflict_serial_order(&parse("r1(x) r2(x) w1(x) w2(x) c1 c2")).is_none(),
        "lost update has a cycle"
    );
}

fn demo_recoverability() {
    println!("-- recoverability classes (the classic examples)");
    let rows = [
        ("w1(x) r2(x) c2 c1", false, false, false),
        ("w1(x) r2(x) c1 c2", true, false, false),
        ("w1(x) r2(x) a1 a2", true, false, false),
        ("w1(x) c1 r2(x) w2(x) c2", true, true, true),
        ("w1(x) w2(x) c1 c2", true, true, false),
    ];
    println!(
        "   {:<28}{:>12}{:>6}{:>8}",
        "schedule", "recoverable", "ACA", "strict"
    );
    for (t, rec, aca, strict) in rows {
        let s = parse(t);
        println!(
            "   {t:<28}{:>12}{:>6}{:>8}",
            is_recoverable(&s),
            is_aca(&s),
            is_strict(&s)
        );
        assert_eq!(
            (is_recoverable(&s), is_aca(&s), is_strict(&s)),
            (rec, aca, strict),
            "{t}"
        );
    }
}

fn random_hierarchy() {
    println!("-- 5000 random schedules (3-4 txns, 2-3 objects, 25% aborts): class inclusions");
    let mut rng = Rng(0x7a3d_91f5_c0de_1234);
    let (mut n, mut rec, mut aca, mut strict, mut csr, mut vsr) = (0, 0, 0, 0, 0, 0);
    let (mut rec_not_aca, mut aca_not_strict, mut vsr_not_csr, mut blind) = (0, 0, 0, 0);
    let mut shown = false;
    for _ in 0..5000 {
        let (nt, no) = (3 + rng.below(2), 2 + rng.below(2));
        let s = gen_schedule(&mut rng, nt, no, 3, 25, false);
        let (r, a, st) = (is_recoverable(&s), is_aca(&s), is_strict(&s));
        let (c, v) = (
            conflict_serial_order(&s).is_some(),
            view_serial_order(&s).is_some(),
        );
        assert!(!st || a, "strict implies ACA: {}", show(&s));
        assert!(!a || r, "ACA implies recoverable: {}", show(&s));
        assert!(!c || v, "CSR implies VSR: {}", show(&s));
        if let Some(order) = conflict_serial_order(&s) {
            // the topological order must really be equivalent: same conflict direction for every pair
            let pos: HashMap<usize, usize> =
                order.iter().enumerate().map(|(i, &t)| (t, i)).collect();
            assert!(precedence_edges(&s).iter().all(|(a, b)| pos[a] < pos[b]));
        }
        n += 1;
        (rec, aca, strict, csr, vsr) = (
            rec + r as i32,
            aca + a as i32,
            strict + st as i32,
            csr + c as i32,
            vsr + v as i32,
        );
        rec_not_aca += (r && !a) as i32;
        aca_not_strict += (a && !st) as i32;
        if v && !c {
            vsr_not_csr += 1;
            blind += has_blind_write(&projection(&s)) as i32;
            if !shown {
                shown = true;
                println!(
                    "   a random view-but-not-conflict-serializable schedule: {}",
                    show(&s)
                );
            }
        }
    }
    println!(
        "   n={n}  recoverable={rec}  ACA={aca}  strict={strict}  conflict-ser={csr}  view-ser={vsr}"
    );
    println!(
        "   recoverable\\ACA={rec_not_aca}  ACA\\strict={aca_not_strict}  VSR\\CSR={vsr_not_csr} (with blind writes: {blind})"
    );
    assert!(
        strict < aca && aca < rec,
        "strict subset ACA subset recoverable, all proper"
    );
    assert!(csr < vsr, "CSR is a proper subset of VSR");
    assert!(rec_not_aca > 0 && aca_not_strict > 0 && vsr_not_csr > 0);
}

// ------------------------------------------------------------------------------------------------------------------
// ACID, scripted on a toy store
// ------------------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum LogRec {
    Update {
        tx: u32,
        key: &'static str,
        before: i64,
        after: i64,
    },
    Commit(u32),
}

/// In-memory table + write-ahead log that survives crashes. Undo walks the log backwards; recovery redoes committed txns.
struct Toy {
    mem: BTreeMap<&'static str, i64>,
    log: Vec<LogRec>,
}

impl Toy {
    fn set(&mut self, tx: u32, key: &'static str, v: i64) {
        let before = self.mem[key];
        self.log.push(LogRec::Update {
            tx,
            key,
            before,
            after: v,
        }); // log first (WAL), then change memory
        self.mem.insert(key, v);
    }
    fn abort(&mut self, tx: u32) {
        let undo: Vec<_> = self
            .log
            .iter()
            .rev()
            .filter_map(|r| {
                if let LogRec::Update {
                    tx: t, key, before, ..
                } = r
                {
                    (*t == tx).then_some((*key, *before))
                } else {
                    None
                }
            })
            .collect();
        for (k, b) in undo {
            self.mem.insert(k, b);
        }
    }
    fn commit(&mut self, tx: u32) {
        self.log.push(LogRec::Commit(tx));
    }
    /// A crash loses `mem`; recovery rebuilds it from the initial state by replaying committed transactions only.
    fn crash_and_recover(&mut self, initial: &BTreeMap<&'static str, i64>) {
        let winners: BTreeSet<u32> = self
            .log
            .iter()
            .filter_map(|r| {
                if let LogRec::Commit(t) = r {
                    Some(*t)
                } else {
                    None
                }
            })
            .collect();
        self.mem = initial.clone();
        for r in &self.log {
            if let LogRec::Update { tx, key, after, .. } = r {
                if winners.contains(tx) {
                    self.mem.insert(key, *after);
                }
            }
        }
    }
}

fn demo_acid() {
    println!("-- ACID scenarios");
    let init: BTreeMap<&'static str, i64> = [("alice", 100), ("bob", 50)].into_iter().collect();
    // Atomicity: a transfer that fails half-way leaves no trace.
    let mut db = Toy {
        mem: init.clone(),
        log: vec![],
    };
    db.set(1, "alice", 70);
    db.abort(1); // "bob's account is frozen": everything of T1 is undone
    assert_eq!(db.mem, init);
    println!("   A  aborted transfer rolled back: {:?}", db.mem);
    // Consistency: the application invariant balance >= 0 decides commit vs abort.
    let mut db = Toy {
        mem: init.clone(),
        log: vec![],
    };
    db.set(2, "alice", 100 - 130);
    if db.mem["alice"] < 0 {
        db.abort(2);
    } else {
        db.commit(2);
    }
    assert_eq!(db.mem["alice"], 100);
    println!(
        "   C  overdraft rejected, invariant holds: alice = {}",
        db.mem["alice"]
    );
    // Isolation: the same two transfers, serial vs the lost-update interleaving r1 r2 w1 w2.
    let (a1, a2) = (100 - 10, 100 - 20); // both read 100 before either wrote
    assert_ne!(a2, 100 - 10 - 20, "interleaved result loses T1's update");
    let s = parse("r1(x) r2(x) w1(x) w2(x) c1 c2");
    assert!(conflict_serial_order(&s).is_none());
    println!(
        "   I  interleaving {} gives {a2} instead of {} (not serializable)",
        show(&s),
        100 - 10 - 20
    );
    let _ = a1;
    // Durability: committed T3 survives the crash, uncommitted T4 does not.
    let mut db = Toy {
        mem: init.clone(),
        log: vec![],
    };
    db.set(3, "bob", 80);
    db.commit(3);
    db.set(4, "alice", 1);
    db.crash_and_recover(&init);
    assert_eq!((db.mem["bob"], db.mem["alice"]), (80, 100));
    println!(
        "   D  after crash: {:?} (T3 committed kept, T4 uncommitted gone)",
        db.mem
    );
}

// ------------------------------------------------------------------------------------------------------------------
// Part 2: Adya histories and the direct serialization graph
// ------------------------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
enum HK {
    /// read of the version of `obj` written by transaction `ver` (0 = initial); `pred` marks a predicate (scan) read
    R {
        obj: usize,
        ver: usize,
        pred: bool,
    },
    W(usize),
    C,
    A,
}
#[derive(Clone, Copy, Debug)]
struct HOp {
    tx: usize,
    kind: HK,
}

/// `r1(x0)` = T1 reads the version of x installed by T0 (initial); `w1(x)`; `s1(x0,y0)` = scan reading several versions; `c1`; `a1`.
fn parse_hist(s: &str) -> Vec<HOp> {
    let mut out = Vec::new();
    for t in s.split_whitespace() {
        let b = t.as_bytes();
        let tx = (b[1] - b'0') as usize;
        let inner: Vec<&str> = if t.len() > 3 {
            t[3..t.len() - 1].split(',').collect()
        } else {
            vec![]
        };
        let rd = |x: &str, pred| HK::R {
            obj: (x.as_bytes()[0] - b'x') as usize,
            ver: (x.as_bytes()[1] - b'0') as usize,
            pred,
        };
        match b[0] {
            b'r' => out.push(HOp {
                tx,
                kind: rd(inner[0], false),
            }),
            b's' => out.extend(inner.iter().map(|x| HOp {
                tx,
                kind: rd(x, true),
            })),
            b'w' => out.push(HOp {
                tx,
                kind: HK::W((inner[0].as_bytes()[0] - b'x') as usize),
            }),
            b'c' => out.push(HOp { tx, kind: HK::C }),
            b'a' => out.push(HOp { tx, kind: HK::A }),
            _ => panic!("bad token {t}"),
        }
    }
    out
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Edge {
    Ww,
    Wr,
    Rw,
    RwPred,
}

#[derive(Default, Debug, PartialEq, Eq, Clone)]
struct Phenomena {
    g0: bool,
    g1a: bool,
    g1b: bool,
    g1c: bool,
    g_single: bool,
    g2_item: bool,
    g2: bool,
}

impl Phenomena {
    fn list(&self) -> String {
        let all = [
            ("G0", self.g0),
            ("G1a", self.g1a),
            ("G1b", self.g1b),
            ("G1c", self.g1c),
            ("G-single", self.g_single),
            ("G2-item", self.g2_item),
            ("G2", self.g2),
        ];
        let v: Vec<&str> = all.iter().filter(|(_, b)| *b).map(|(n, _)| *n).collect();
        if v.is_empty() {
            "none".into()
        } else {
            v.join(",")
        }
    }
    fn g1(&self) -> bool {
        self.g1a || self.g1b || self.g1c
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Level {
    ReadUncommitted,
    ReadCommitted,
    RepeatableRead,
    Snapshot,
    Serializable,
}
const LEVELS: [Level; 5] = [
    Level::ReadUncommitted,
    Level::ReadCommitted,
    Level::RepeatableRead,
    Level::Snapshot,
    Level::Serializable,
];

fn permits(l: Level, p: &Phenomena) -> bool {
    match l {
        Level::ReadUncommitted => !p.g0,
        Level::ReadCommitted => !p.g0 && !p.g1(),
        Level::RepeatableRead => !p.g0 && !p.g1() && !p.g2_item,
        Level::Snapshot => !p.g0 && !p.g1() && !p.g_single,
        Level::Serializable => !p.g0 && !p.g1() && !p.g2,
    }
}

fn reachable(
    n: usize,
    edges: &[(usize, usize, Edge)],
    from: usize,
    to: usize,
    allow: &[Edge],
) -> bool {
    let mut seen = vec![false; n];
    let mut stack = vec![from];
    while let Some(u) = stack.pop() {
        if u == to {
            return true;
        }
        if std::mem::replace(&mut seen[u], true) {
            continue;
        }
        stack.extend(
            edges
                .iter()
                .filter(|(a, _, k)| *a == u && allow.contains(k))
                .map(|(_, b, _)| *b),
        );
    }
    false
}

/// Build the DSG of the committed transactions and detect the phenomena. The version order of an object is the order of its
/// committed writers' LAST writes in the history (what in-place or install-at-commit engines produce).
fn analyze(h: &[HOp]) -> (Phenomena, Vec<(usize, usize, Edge)>) {
    let n = h.iter().map(|o| o.tx).max().unwrap() + 1;
    let ended = |t: usize, k: fn(&HK) -> bool| h.iter().any(|o| o.tx == t && k(&o.kind));
    let commit = |t: usize| ended(t, |k| matches!(k, HK::C));
    let abort = |t: usize| ended(t, |k| matches!(k, HK::A));
    let mut last_write: BTreeMap<(usize, usize), usize> = BTreeMap::new(); // (obj, tx) -> position of last write
    for (i, o) in h.iter().enumerate() {
        if let HK::W(x) = o.kind {
            last_write.insert((x, o.tx), i);
        }
    }
    let mut vo: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (&(x, t), &pos) in &last_write {
        if commit(t) {
            vo.entry(x).or_default().push(pos * 100 + t); // sort key keeps position order, tx recoverable below
        }
    }
    for v in vo.values_mut() {
        v.sort();
        v.iter_mut().for_each(|k| *k %= 100);
    }
    let mut p = Phenomena::default();
    let mut edges = Vec::new();
    for order in vo.values() {
        edges.extend(order.windows(2).map(|w| (w[0], w[1], Edge::Ww)));
    }
    for (i, o) in h.iter().enumerate() {
        let HK::R { obj, ver, pred } = o.kind else {
            continue;
        };
        if !commit(o.tx) || ver == o.tx {
            continue;
        }
        if ver != 0 && abort(ver) {
            p.g1a = true;
            continue;
        }
        if ver != 0 {
            // intermediate read: the writer wrote the object again after this read
            p.g1b |= last_write.get(&(obj, ver)).is_some_and(|&lw| lw > i);
            edges.push((ver, o.tx, Edge::Wr));
        }
        let order = vo.get(&obj).cloned().unwrap_or_default();
        let next = if ver == 0 {
            order.first().copied()
        } else {
            order
                .iter()
                .position(|&t| t == ver)
                .and_then(|k| order.get(k + 1).copied())
        };
        if let Some(nx) = next.filter(|&nx| nx != o.tx) {
            edges.push((o.tx, nx, if pred { Edge::RwPred } else { Edge::Rw }));
        }
    }
    let cyc = |e: &(usize, usize, Edge), allow: &[Edge]| reachable(n, &edges, e.1, e.0, allow);
    let flow = [Edge::Ww, Edge::Wr];
    p.g0 = edges.iter().any(|e| e.2 == Edge::Ww && cyc(e, &[Edge::Ww]));
    p.g1c = edges.iter().any(|e| flow.contains(&e.2) && cyc(e, &flow));
    p.g_single = edges
        .iter()
        .any(|e| matches!(e.2, Edge::Rw | Edge::RwPred) && cyc(e, &flow));
    p.g2_item = edges
        .iter()
        .any(|e| e.2 == Edge::Rw && cyc(e, &[Edge::Ww, Edge::Wr, Edge::Rw]));
    p.g2 = edges.iter().any(|e| {
        matches!(e.2, Edge::Rw | Edge::RwPred)
            && cyc(e, &[Edge::Ww, Edge::Wr, Edge::Rw, Edge::RwPred])
    });
    p.g2 |= p.g1c; // a cycle made only of ww/wr edges is a cycle too
    (p, edges)
}

fn isolation_table() {
    println!(
        "-- isolation levels vs phenomena: scripted histories analysed through the DSG (yes = level ALLOWS the history)"
    );
    // name, history, expected [RU, RC, RR, SI, SER]
    let cases: [(&str, &str, [bool; 5]); 9] = [
        (
            "dirty write",
            "w1(x) w2(x) w2(y) w1(y) c1 c2",
            [false, false, false, false, false],
        ),
        (
            "dirty read (G1a)",
            "w1(x) r2(x1) a1 c2",
            [true, false, false, false, false],
        ),
        (
            "intermediate read (G1b)",
            "w1(x) r2(x1) w1(x) c1 c2",
            [true, false, false, false, false],
        ),
        (
            "circular info flow (G1c)",
            "w1(x) w2(y) r1(y2) r2(x1) c1 c2",
            [true, false, false, false, false],
        ),
        (
            "non-repeatable read",
            "r1(x0) w2(x) c2 r1(x2) c1",
            [true, true, false, false, false],
        ),
        (
            "lost update",
            "r1(x0) r2(x0) w1(x) w2(x) c1 c2",
            [true, true, false, false, false],
        ),
        (
            "write skew",
            "r1(x0) r1(y0) r2(x0) r2(y0) w1(x) w2(y) c1 c2",
            [true, true, false, true, false],
        ),
        (
            "phantom (predicate rw)",
            "s1(x0,y0) r2(z0) w1(z) w2(y) c1 c2",
            [true, true, true, true, false],
        ),
        ("serial", "r1(x0) w1(x) c1 r2(x1) w2(x) c2", [true; 5]),
    ];
    println!(
        "   {:<26}{:<24}{:>4}{:>4}{:>4}{:>4}{:>5}",
        "history", "phenomena", "RU", "RC", "RR", "SI", "SER"
    );
    let yn = |b: bool| if b { "yes" } else { "no" };
    for (name, text, want) in cases {
        let (p, _) = analyze(&parse_hist(text));
        let got: Vec<bool> = LEVELS.iter().map(|&l| permits(l, &p)).collect();
        println!(
            "   {name:<26}{:<24}{:>4}{:>4}{:>4}{:>4}{:>5}",
            p.list(),
            yn(got[0]),
            yn(got[1]),
            yn(got[2]),
            yn(got[3]),
            yn(got[4])
        );
        assert_eq!(got, want, "{name}: {text}");
    }
    println!(
        "   note: Adya's REPEATABLE READ (PL-2.99) forbids write skew; PostgreSQL's 'repeatable read' is really SNAPSHOT and allows it."
    );
    let (p, e) = analyze(&parse_hist("r1(x0) r1(y0) r2(x0) r2(y0) w1(x) w2(y) c1 c2"));
    println!(
        "   write-skew DSG edges {e:?}  (cycle of two rw edges: {})",
        p.g2_item
    );
}

/// Translate a Part-1 schedule in which every transaction commits and writes each object at most once into an Adya history:
/// a read sees the latest write before it in the schedule. Tx numbers shift by one (0 = initial database).
fn to_history(s: &[Op]) -> Vec<HOp> {
    let mut last: HashMap<usize, usize> = HashMap::new();
    s.iter()
        .map(|o| {
            let tx = o.tx + 1;
            let kind = match o.kind {
                Kind::R => HK::R {
                    obj: o.obj,
                    ver: last.get(&o.obj).copied().unwrap_or(0),
                    pred: false,
                },
                Kind::W => {
                    last.insert(o.obj, tx);
                    HK::W(o.obj)
                }
                Kind::C => HK::C,
                Kind::A => HK::A,
            };
            HOp { tx, kind }
        })
        .collect()
}

fn dsg_vs_precedence_graph() {
    println!(
        "-- cross-check: DSG acyclic <=> precedence graph acyclic, on 3000 random all-commit schedules"
    );
    let mut rng = Rng(0x51ed_270b_a5ce_9001);
    let (mut ser, mut non) = (0, 0);
    for _ in 0..3000 {
        let (nt, no) = (2 + rng.below(3), 2 + rng.below(2));
        let s = gen_schedule(&mut rng, nt, no, 3, 0, true);
        let (p, _) = analyze(&to_history(&s));
        let csr = conflict_serial_order(&s).is_some();
        assert_eq!(csr, !p.g2, "disagreement on {}", show(&s));
        assert!(
            !p.g1a && !p.g1b,
            "all-commit schedules with unique writes have no G1a/G1b: {}",
            show(&s)
        );
        if csr { ser += 1 } else { non += 1 }
    }
    println!("   serializable {ser}, not serializable {non}: verdicts identical");
    assert!(ser > 0 && non > 0);
}

fn main() {
    demo_serializability();
    demo_recoverability();
    random_hierarchy();
    demo_acid();
    isolation_table();
    dsg_vs_precedence_graph();
    println!("all transaction theory checks passed");
}
