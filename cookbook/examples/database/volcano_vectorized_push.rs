//! RECIPE 211 (database): one query, four execution models: Volcano, vectorized, push/data-centric, fused loop.
//! LEVEL: advanced
//! Use in: query engines, ETL/dataframe kernels, any "interpret a plan over lots of rows" code.   Run: cargo run --release --example volcano_vectorized_push
#![allow(dead_code)]
//!
//! WHAT: `SELECT category, SUM(price*qty), COUNT(*) FROM t WHERE price > X AND qty < Y GROUP BY category` over 5M generated
//! rows, executed four ways, with identical results asserted:
//!  (1) VOLCANO (iterator/pull): `trait Operator { open/next/close }`, boxed Scan -> Filter -> Project -> HashAggregate.
//!      Every tuple costs one *virtual call per operator* (indirect branch, no inlining, row passed through registers/stack).
//!  (2) VECTORIZED (MonetDB/X100 style): operators exchange BATCHES of 1024 rows stored by column, with a SELECTION VECTOR
//!      (indices of surviving rows). The virtual call is amortised over 1024 rows and the inner loops are tight, per-column,
//!      branch-light loops that the compiler can unroll/SIMD, and whose working set (1024 * few columns) stays in L1.
//!  (3) PUSH / data-centric (HyPer style): the scan pushes each tuple into a fused closure chain filter -> project -> aggregate.
//!      Closures are monomorphised, so the whole pipeline inlines into ONE loop (what a query compiler would emit as code).
//!  (4) HAND-WRITTEN FUSED LOOP: the speed-of-light baseline you would write by hand for exactly this query.
//! WHY the gaps: the Volcano model pays interpretation overhead (virtual dispatch + unpredictable indirect branches + a
//! data-dependent `if` per tuple + poor instruction-cache/register locality because control hops between 4 operators per
//! row). Vectorization buys back most of it by amortising dispatch; push/compiled removes dispatch entirely and keeps values in
//! registers across operators, but cannot SIMD as easily when it has per-tuple branches.
//! Also shown: `Box<dyn Operator>` vs generic composition; the open/next/close lifecycle; LIMIT early exit (pull stops upstream
//! naturally; push needs an explicit cancellation signal, here a `bool` return).
//! Numbers are chosen as multiples of 0.25 so f64 sums are exact and results can be compared with `==` whatever the order.
//! COMMON USES: analytic engines (row-at-a-time interpreters in classic OLTP engines; vectorized in column stores and many
//! modern OLAP engines; JIT/push in compiled engines), dataframe libraries, streaming operators, iterator chains in Rust itself
//! (`Iterator` is a pull model that the compiler monomorphises into a fused loop, i.e. model 3).
//! Read: https://15445.courses.cs.cmu.edu/ (Query Execution lectures), https://www.cs.cmu.edu/~15721/ (Advanced DB: vectorization vs
//! compilation), and the paper "MonetDB/X100: Hyper-Pipelining Query Execution" (CIDR 2005).
use std::cell::Cell;
use std::collections::HashMap;
use std::hint::black_box;
use std::time::Instant;

const N: usize = 5_000_000;
const NCAT: usize = 16;
const BATCH: usize = 1024;
const PRICE_MIN: f64 = 50.0; // WHERE price > 50.0
const QTY_MAX: u32 = 60; //     AND   qty < 60

// ---------------------------------------------------------------- data
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// Columnar table: one Vec per column (the layout a column store keeps on disk and in memory).
struct Table {
    id: Vec<u32>,
    cat: Vec<u8>,
    price: Vec<f64>,
    qty: Vec<u32>,
}

fn gen_table(n: usize) -> Table {
    let mut r = Rng(0x9E37_79B9_7F4A_7C15);
    let mut t = Table {
        id: Vec::with_capacity(n),
        cat: Vec::with_capacity(n),
        price: Vec::with_capacity(n),
        qty: Vec::with_capacity(n),
    };
    for i in 0..n {
        let x = r.next();
        t.id.push(i as u32);
        t.cat.push((x % NCAT as u64) as u8);
        t.price.push(((x >> 8) % 400) as f64 / 4.0); // 0.00 .. 99.75, multiples of 0.25
        t.qty.push(((x >> 24) % 100) as u32);
    }
    t
}

/// Query result: per category (sum(price*qty), count).
type Res = [(f64, u64); NCAT];

// ---------------------------------------------------------------- (1) Volcano
#[derive(Clone, Copy)]
struct Row {
    cat: u8,
    price: f64,
    qty: u32,
    rev: f64, // filled by Project; the aggregate reuses `rev` as SUM and `qty` as COUNT in its output rows
}

trait Operator {
    fn open(&mut self) {}
    fn next(&mut self) -> Option<Row>;
    fn close(&mut self) {}
}

/// A boxed operator is itself an operator, so boxed and generic nodes can be mixed freely.
impl<'a> Operator for Box<dyn Operator + 'a> {
    fn open(&mut self) {
        (**self).open()
    }
    fn next(&mut self) -> Option<Row> {
        (**self).next()
    }
    fn close(&mut self) {
        (**self).close()
    }
}

struct Scan<'a> {
    t: &'a Table,
    pos: usize,
    scanned: &'a Cell<usize>,
}
impl Operator for Scan<'_> {
    fn open(&mut self) {
        self.pos = 0;
    }
    fn next(&mut self) -> Option<Row> {
        if self.pos >= self.t.id.len() {
            return None;
        }
        let i = self.pos;
        self.pos += 1;
        self.scanned.set(self.scanned.get() + 1);
        Some(Row {
            cat: self.t.cat[i],
            price: self.t.price[i],
            qty: self.t.qty[i],
            rev: 0.0,
        })
    }
}

/// Filter, generic over its child: `Filter<Scan>` is monomorphised (no virtual call to the child).
/// `Filter<Box<dyn Operator>>` is the classic boxed form.
struct Filter<C: Operator> {
    child: C,
}
impl<C: Operator> Operator for Filter<C> {
    fn open(&mut self) {
        self.child.open()
    }
    fn next(&mut self) -> Option<Row> {
        loop {
            let r = self.child.next()?; // pull until a row qualifies
            if r.price > PRICE_MIN && r.qty < QTY_MAX {
                return Some(r);
            }
        }
    }
    fn close(&mut self) {
        self.child.close()
    }
}

struct Project<C: Operator> {
    child: C,
}
impl<C: Operator> Operator for Project<C> {
    fn open(&mut self) {
        self.child.open()
    }
    fn next(&mut self) -> Option<Row> {
        let mut r = self.child.next()?;
        r.rev = r.price * r.qty as f64;
        Some(r)
    }
    fn close(&mut self) {
        self.child.close()
    }
}

/// Pipeline breaker: the first `next()` drains the child into a hash table, then emits one row per group.
struct HashAggregate<C: Operator> {
    child: C,
    out: Vec<Row>,
    built: bool,
}
impl<C: Operator> Operator for HashAggregate<C> {
    fn open(&mut self) {
        self.child.open();
        self.built = false;
    }
    fn next(&mut self) -> Option<Row> {
        if !self.built {
            let mut groups: HashMap<u8, (f64, u32)> = HashMap::new();
            while let Some(r) = self.child.next() {
                let e = groups.entry(r.cat).or_insert((0.0, 0));
                e.0 += r.rev;
                e.1 += 1;
            }
            self.out = groups
                .into_iter()
                .map(|(cat, (s, c))| Row {
                    cat,
                    price: 0.0,
                    qty: c,
                    rev: s,
                })
                .collect();
            self.built = true;
        }
        self.out.pop()
    }
    fn close(&mut self) {
        self.child.close()
    }
}

struct Limit<C: Operator> {
    child: C,
    left: usize,
}
impl<C: Operator> Operator for Limit<C> {
    fn open(&mut self) {
        self.child.open()
    }
    fn next(&mut self) -> Option<Row> {
        if self.left == 0 {
            return None; // stop WITHOUT pulling the child: upstream work simply never happens
        }
        self.left -= 1;
        self.child.next()
    }
    fn close(&mut self) {
        self.child.close()
    }
}

fn drain_agg(mut root: impl Operator) -> Res {
    let mut res = [(0.0, 0u64); NCAT];
    root.open();
    while let Some(r) = root.next() {
        res[r.cat as usize] = (r.rev, r.qty as u64);
    }
    root.close();
    res
}

fn run_volcano_boxed(t: &Table, scanned: &Cell<usize>) -> Res {
    let scan: Box<dyn Operator + '_> = Box::new(Scan { t, pos: 0, scanned });
    let filter: Box<dyn Operator + '_> = Box::new(Filter { child: scan });
    let project: Box<dyn Operator + '_> = Box::new(Project { child: filter });
    let agg: Box<dyn Operator + '_> = Box::new(HashAggregate {
        child: project,
        out: vec![],
        built: false,
    });
    drain_agg(agg)
}

/// Same plan, composed with generics: the compiler sees the concrete type
/// `HashAggregate<Project<Filter<Scan>>>` and can inline `next()` through the chain.
fn run_volcano_generic(t: &Table, scanned: &Cell<usize>) -> Res {
    let plan = HashAggregate {
        child: Project {
            child: Filter {
                child: Scan { t, pos: 0, scanned },
            },
        },
        out: vec![],
        built: false,
    };
    drain_agg(plan)
}

// ---------------------------------------------------------------- (2) Vectorized
/// A batch of up to 1024 rows, columnar, plus a selection vector naming the live rows.
struct Batch {
    cat: Vec<u8>,
    price: Vec<f64>,
    qty: Vec<u32>,
    rev: Vec<f64>,
    sel: Vec<u16>, // indices of rows that survived filtering
    len: usize,
}
impl Batch {
    fn new() -> Self {
        Batch {
            cat: vec![0; BATCH],
            price: vec![0.0; BATCH],
            qty: vec![0; BATCH],
            rev: vec![0.0; BATCH],
            sel: vec![0; BATCH],
            len: 0,
        }
    }
}

fn scan_batch(t: &Table, start: usize, b: &mut Batch) -> bool {
    if start >= t.id.len() {
        return false;
    }
    let end = (start + BATCH).min(t.id.len());
    b.len = end - start;
    b.cat[..b.len].copy_from_slice(&t.cat[start..end]);
    b.price[..b.len].copy_from_slice(&t.price[start..end]);
    b.qty[..b.len].copy_from_slice(&t.qty[start..end]);
    for (i, s) in b.sel[..b.len].iter_mut().enumerate() {
        *s = i as u16; // initially every row is selected
    }
    true
}

/// Primitive 1: refine the selection by `price > X`. Branch-free: always write, advance the cursor by the predicate (0 or 1).
fn sel_price_gt(b: &mut Batch, x: f64) {
    let mut k = 0;
    for j in 0..b.len {
        let i = b.sel[j] as usize;
        b.sel[k] = i as u16;
        k += (b.price[i] > x) as usize;
    }
    b.len = k;
}
/// Primitive 2: refine by `qty < Y`.
fn sel_qty_lt(b: &mut Batch, y: u32) {
    let mut k = 0;
    for j in 0..b.len {
        let i = b.sel[j] as usize;
        b.sel[k] = i as u16;
        k += (b.qty[i] < y) as usize;
    }
    b.len = k;
}
/// Primitive 3: compute price*qty for selected rows only. Primitive 4: aggregate selected rows.
fn project_rev(b: &mut Batch) {
    for j in 0..b.len {
        let i = b.sel[j] as usize;
        b.rev[i] = b.price[i] * b.qty[i] as f64;
    }
}
fn aggregate(b: &Batch, res: &mut Res) {
    for j in 0..b.len {
        let i = b.sel[j] as usize;
        let e = &mut res[b.cat[i] as usize];
        e.0 += b.rev[i];
        e.1 += 1;
    }
}

fn run_vectorized(t: &Table) -> Res {
    let mut res = [(0.0, 0u64); NCAT];
    let mut b = Batch::new();
    let mut pos = 0;
    while scan_batch(t, pos, &mut b) {
        pos += BATCH;
        // `len` now means "number of selected rows"; the scan set it to the batch size and the selection is the identity.
        sel_price_gt(&mut b, PRICE_MIN);
        sel_qty_lt(&mut b, QTY_MAX);
        project_rev(&mut b);
        aggregate(&b, &mut res);
    }
    res
}

// ---------------------------------------------------------------- (3) Push / data-centric
#[derive(Clone, Copy)]
struct Tuple {
    cat: u8,
    price: f64,
    qty: u32,
}

/// Each stage takes its consumer and returns a closure; `FnMut(..) -> bool` where `false` means "stop, I am satisfied".
fn filter_stage(mut next: impl FnMut(Tuple) -> bool) -> impl FnMut(Tuple) -> bool {
    move |t| {
        if t.price > PRICE_MIN && t.qty < QTY_MAX {
            next(t)
        } else {
            true
        }
    }
}
fn project_stage(mut next: impl FnMut(u8, f64) -> bool) -> impl FnMut(Tuple) -> bool {
    move |t| next(t.cat, t.price * t.qty as f64)
}
fn scan_push(t: &Table, mut sink: impl FnMut(Tuple) -> bool) {
    for i in 0..t.id.len() {
        if !sink(Tuple {
            cat: t.cat[i],
            price: t.price[i],
            qty: t.qty[i],
        }) {
            return; // cancellation: the producer must check, otherwise upstream work cannot be stopped
        }
    }
}

fn run_push(t: &Table) -> Res {
    let mut res = [(0.0, 0u64); NCAT];
    let agg = |cat: u8, rev: f64| {
        let e = &mut res[cat as usize];
        e.0 += rev;
        e.1 += 1;
        true
    };
    scan_push(t, filter_stage(project_stage(agg)));
    res
}

// ---------------------------------------------------------------- (4) Hand-fused loop
fn run_fused(t: &Table) -> Res {
    let mut res = [(0.0, 0u64); NCAT];
    for ((&c, &p), &q) in t.cat.iter().zip(&t.price).zip(&t.qty) {
        if p > PRICE_MIN && q < QTY_MAX {
            let e = &mut res[c as usize];
            e.0 += p * q as f64;
            e.1 += 1;
        }
    }
    res
}

// ---------------------------------------------------------------- harness
fn median_time<R>(mut f: impl FnMut() -> R) -> (R, f64) {
    let mut times = vec![];
    let mut out = None;
    for _ in 0..3 {
        let t = Instant::now();
        out = Some(black_box(f()));
        times.push(t.elapsed().as_secs_f64());
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    (out.unwrap(), times[1])
}

fn main() {
    if cfg!(debug_assertions) {
        println!("WARNING: debug build: numbers are meaningless. Use --release.\n");
    }
    let t = gen_table(N);
    let scanned = Cell::new(0);

    let (r_boxed, s_boxed) = median_time(|| run_volcano_boxed(&t, &scanned));
    let (r_gen, s_gen) = median_time(|| run_volcano_generic(&t, &scanned));
    let (r_vec, s_vec) = median_time(|| run_vectorized(&t));
    let (r_push, s_push) = median_time(|| run_push(&t));
    let (r_fused, s_fused) = median_time(|| run_fused(&t));

    // All five agree exactly (sums of multiples of 0.25 below 2^53 are exact in f64, so order does not matter).
    for (name, r) in [
        ("volcano boxed", &r_boxed),
        ("volcano generic", &r_gen),
        ("vectorized", &r_vec),
        ("push", &r_push),
    ] {
        assert!(*r == r_fused, "{name} differs from the fused baseline");
    }
    let total: u64 = r_fused.iter().map(|g| g.1).sum();
    assert!(total > 0 && total < N as u64);
    println!(
        "query matched {total} of {N} rows; group 0: sum={:.2} count={}\n",
        r_fused[0].0, r_fused[0].1
    );

    println!("{:<34}{:>10}{:>16}", "model", "median", "rows/second");
    for (name, s) in [
        ("1a volcano, Box<dyn Operator>", s_boxed),
        ("1b volcano, generic operators", s_gen),
        ("2  vectorized, 1024-row batches", s_vec),
        ("3  push closures (data-centric)", s_push),
        ("4  hand-written fused loop", s_fused),
    ] {
        println!(
            "{name:<34}{:>8.1}ms{:>13.1}M/s",
            s * 1e3,
            N as f64 / s / 1e6
        );
    }
    println!(
        "\nspeedup vs boxed volcano: generic {:.1}x, vectorized {:.1}x, push {:.1}x, fused {:.1}x",
        s_boxed / s_gen,
        s_boxed / s_vec,
        s_boxed / s_push,
        s_boxed / s_fused
    );
    println!(
        "(boxed pays ~4 indirect calls/tuple; generic inlines them; vectorized amortises over 1024 rows and is branch-free;\n the fused loop mispredicts its data-dependent `if`, so it can lose to vectorization)"
    );

    // ---- lifecycle + LIMIT early exit: pull model stops upstream work for free
    scanned.set(0);
    let mut plan = Limit {
        child: Filter {
            child: Scan {
                t: &t,
                pos: 0,
                scanned: &scanned,
            },
        },
        left: 10,
    };
    plan.open(); //  open() recurses down the tree: allocate buffers, reset cursors
    let mut got = 0;
    while let Some(_r) = plan.next() {
        got += 1; //  each next() pulls exactly as much upstream work as needed
    }
    plan.close(); //  close() recurses down: release resources (even when stopping early!)
    let pulled = scanned.get();
    println!("\nVolcano LIMIT 10: {got} rows returned, scan touched only {pulled} of {N} rows");
    assert_eq!(got, 10);
    assert!(
        pulled < 200,
        "pull model should stop scanning right after the 10th match"
    );

    // ---- push model: LIMIT needs cancellation plumbing (the `false` return value)
    let mut seen = 0usize;
    let mut pushed = 0usize;
    scan_push(&t, |tp| {
        pushed += 1;
        if tp.price > PRICE_MIN && tp.qty < QTY_MAX {
            seen += 1;
        }
        seen < 10 // false = cancel the producer; without this check the scan would run to the end
    });
    println!(
        "Push LIMIT 10 with cancellation flag: scanned {pushed} rows (same work, but every stage must forward the signal)"
    );
    assert_eq!(seen, 10);
    assert_eq!(
        pushed, pulled,
        "both models scan exactly up to the 10th match"
    );

    let small = gen_table(10_000);
    let c = Cell::new(0);
    assert!(run_volcano_boxed(&small, &c) == run_fused(&small));
    println!("\nall assertions passed");
}
