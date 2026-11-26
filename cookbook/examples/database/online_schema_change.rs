//! RECIPE 852 (database): online schema change without locking: shadow-table copy + change log (gh-ost style), expand/contract with batched backfill
//! LEVEL: advanced
//! Use in: migrating a hot production table (add/alter a column, change a type, repartition) without taking writes offline; rolling application deploys.   Run: cargo run --release --example online_schema_change
#![allow(dead_code)]
//!
//! WHY NOT ALTER TABLE: a blocking `ALTER` rewrites every row while holding a lock that stops writes (and, behind it, reads
//! queue up): minutes to hours on a big table. ONLINE tools never hold the lock for the copy; they hold it only for a swap.
//! PART A: the SHADOW-TABLE method (gh-ost, pt-online-schema-change, GitHub's and Percona's tools, Vitess VReplication):
//!   1. create an empty SHADOW table with the new shape (here rows gain a column `b = f(a)`);
//!   2. start capturing every write to the original from a CHANGE LOG (gh-ost reads the binlog, pt-osc uses triggers; here a
//!      `binlog` of full row images with Put/Del events);
//!   3. COPY existing rows in id-ordered CHUNKS with `INSERT IGNORE ... SELECT`: one atomic statement per chunk, and it must
//!      never overwrite a row the log already delivered. The log applier uses REPLACE / DELETE: idempotent, so replays after a
//!      crash are harmless. The copy cursor and the log offset are persisted, so a crashed migrator RESUMES;
//!   4. THROTTLE: when the unapplied log backlog (a stand-in for replica lag) is above a threshold the migrator stops copying
//!      and only applies the log, so it never makes the system fall behind;
//!   5. VERIFY chunk by chunk: catch the log up, then compare a checksum of `f(original rows)` with the shadow rows of an id
//!      range. A mismatch aborts the migration (nothing was exposed yet);
//!   6. CUT-OVER: lock WRITES only (reads continue), apply the last few log events, check, and swap with ONE atomic rename. The lock lasts a handful of
//!      ticks regardless of table size, versus a naive ALTER's copy time. Clients see the old table or the new table, never a mixture.
//!   Why the chunk copy is a single statement: if the migrator read a chunk, then (after concurrent writes) wrote it, a row deleted in
//!   between would come back as a ZOMBIE that no later log event removes. `Bug::TwoPhaseCopy` below reproduces that and the
//!   checksum catches it; `Bug::DropDeletes` and `Bug::InsertIgnoreLog` are the other classic mistakes.
//! PART B: EXPAND / CONTRACT (a.k.a. parallel change) for a change the APPLICATION takes part in, no copy of the table:
//!   expand (add column, nullable) -> roll out code that writes both shapes while OLD code is still running -> only new writers
//!   -> BACKFILL old-shape rows in resumable batches -> verify no old shape remains -> switch readers to the new column ->
//!   contract (readers go strict, old path deleted). During the whole time a VERSIONED READER tolerates old and new rows
//!   (derives the value when the new column is absent). Contracting early is refused while old-shape rows exist.
//! VERIFICATION: 1500 random workloads for each part (concurrent inserts/updates/deletes during the copy, migrator crashes
//! before/after persisting its cursor): a READER probes the live table every tick and must see exactly the oracle's logical content in
//! ONE consistent shape (so no write is ever lost or half-visible); the final table must equal the original's final state with the
//! transformation applied. Three injected bugs must be caught by the checksums in at least one seed each, and a buggy run that
//! happens to finish must still be correct. Lock duration is printed against a naive ALTER.
//! COMMON USES: gh-ost, pt-online-schema-change, Vitess online DDL, PlanetScale deploy requests, Postgres `pg_repack` and
//! `CREATE INDEX CONCURRENTLY` (same capture-and-catch-up idea), Rails strong_migrations / Django two-step deploys.
//! Read: https://github.com/github/gh-ost , https://github.com/github/gh-ost/blob/master/doc/why-triggerless.md ,
//! https://martinfowler.com/bliki/ParallelChange.html
//!
//! TRY NEXT:
//!   - [S] Raise `RATE` to 1 and then 20 and predict the lock-held ticks before running; which of backlog size or copy speed does the lock depend on?
//!   - [S] Change `THROTTLE` to `usize::MAX` and measure the maximum backlog `max_backlog` reached; what does that mean for a real replica?
//!   - [M] Add a `Bug::SkipVerifyChunk` that never checks the last chunk and find a seed where a wrong table is swapped in, then explain why the full check under the lock is still essential.
//!   - [M] Port `backfill_step` to a SQL loop (`UPDATE ... WHERE id > ? AND b IS NULL ORDER BY id LIMIT 1000`) using rusqlite and verify resume after killing it mid-batch.
//!   - [A] PROJECT: combine with `cdc_logical_replication_log` (use its decoder as the change log instead of the toy `binlog`) and `deterministic_simulation_testing` (drive migrator crashes from one seed).

use std::collections::BTreeMap;

type Id = u64;
const MAXID: Id = 400;
const CHUNK: usize = 8; // rows per copy statement
const RATE: usize = 12; // log events applied per tick
const THROTTLE: usize = 20; // backlog above which we stop copying
const VERIFY_RANGE: Id = 50;

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Rng {
        let mut r = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        for _ in 0..4 {
            r.next();
        }
        r
    }
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
    fn pm(&mut self, p: u64) -> bool {
        self.below(1000) < p
    }
}

#[derive(Clone, Debug, PartialEq)]
struct V1 {
    a: i64,
}
#[derive(Clone, Debug, PartialEq)]
struct V2 {
    a: i64,
    b: i64,
}
/// The schema transformation under test: the new column is derived from the old one.
fn f(r: &V1) -> V2 {
    V2 {
        a: r.a,
        b: r.a * 2 + 1,
    }
}
fn fnv(h: &mut u64, x: i64) {
    for byte in x.to_le_bytes() {
        *h ^= byte as u64;
        *h = h.wrapping_mul(0x100_0000_01b3);
    }
}
fn checksum<'a>(rows: impl Iterator<Item = (&'a Id, V2)>) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for (id, r) in rows {
        fnv(&mut h, *id as i64);
        fnv(&mut h, r.a);
        fnv(&mut h, r.b);
    }
    h
}

#[derive(Clone)]
enum Ev {
    Put(Id, V1), // full row image after the change (insert or update)
    Del(Id),
}
enum Phys {
    V1(BTreeMap<Id, V1>),
    V2(BTreeMap<Id, V2>),
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Bug {
    None,
    DropDeletes,
    InsertIgnoreLog,
    TwoPhaseCopy,
}

struct Db {
    phys: Phys,
    logical: BTreeMap<Id, i64>, // oracle: what the clients believe the table contains
    binlog: Vec<Ev>,
    logging: bool,
    locked: bool,
    shadow: BTreeMap<Id, V2>,
    stalled_writes: u64,
    max_backlog: usize,
}

impl Db {
    fn new(rng: &mut Rng) -> Db {
        let mut rows = BTreeMap::new();
        let mut logical = BTreeMap::new();
        for _ in 0..(100 + rng.below(200)) {
            let id = 1 + rng.below(MAXID);
            let a = rng.below(1000) as i64;
            rows.insert(id, V1 { a });
            logical.insert(id, a);
        }
        Db {
            phys: Phys::V1(rows),
            logical,
            binlog: vec![],
            logging: false,
            locked: false,
            shadow: BTreeMap::new(),
            stalled_writes: 0,
            max_backlog: 0,
        }
    }
    /// One client write (insert / update / delete by id). Writers are blocked while the table is write-locked.
    fn client_write(&mut self, rng: &mut Rng) {
        let id = 1 + rng.below(MAXID);
        let a = rng.below(1000) as i64;
        if self.locked {
            self.stalled_writes += 1; // the client waits; in a real system it is a pile of blocked connections
            return;
        }
        let delete = self.logical.contains_key(&id) && rng.pm(300);
        match &mut self.phys {
            Phys::V1(m) => {
                if delete {
                    m.remove(&id);
                } else {
                    m.insert(id, V1 { a });
                }
            }
            Phys::V2(m) => {
                if delete {
                    m.remove(&id);
                } else {
                    m.insert(id, f(&V1 { a }));
                }
            }
        }
        if delete {
            self.logical.remove(&id);
            if self.logging {
                self.binlog.push(Ev::Del(id));
            }
        } else {
            self.logical.insert(id, a);
            if self.logging {
                self.binlog.push(Ev::Put(id, V1 { a }));
            }
        }
    }
    fn old(&self) -> &BTreeMap<Id, V1> {
        match &self.phys {
            Phys::V1(m) => m,
            Phys::V2(_) => panic!("original already swapped"),
        }
    }
    /// The READER probe: whatever shape is live, it must equal the oracle transformed to that shape, row for row.
    fn probe(&self) {
        match &self.phys {
            Phys::V1(m) => {
                let got: BTreeMap<Id, i64> = m.iter().map(|(k, v)| (*k, v.a)).collect();
                assert_eq!(
                    got, self.logical,
                    "reader saw a v1 table that differs from the clients' content (lost write)"
                );
            }
            Phys::V2(m) => {
                let want: BTreeMap<Id, V2> = self
                    .logical
                    .iter()
                    .map(|(k, a)| (*k, f(&V1 { a: *a })))
                    .collect();
                assert_eq!(
                    *m, want,
                    "reader saw a v2 table that differs from f(content): mixed or lost state"
                );
            }
        }
    }
}

enum Phase {
    Copy,
    Verify(Id),
    Cutover,
    Done,
    Failed(String),
}

struct Mig {
    phase: Phase,
    bug: Bug,
    cursor: Id,                     // persisted: last copied id
    applied: usize,                 // persisted: log offset
    pending: Option<Vec<(Id, V1)>>, // volatile, only used by Bug::TwoPhaseCopy
    crashes: u64,
    throttled: u64,
    lock_start: u64,
    lock_ticks: u64,
}

impl Mig {
    fn new(bug: Bug) -> Mig {
        Mig {
            phase: Phase::Copy,
            bug,
            cursor: 0,
            applied: 0,
            pending: None,
            crashes: 0,
            throttled: 0,
            lock_start: 0,
            lock_ticks: 0,
        }
    }
    fn apply_event(&self, db: &mut Db, ev: &Ev) {
        match ev {
            Ev::Put(id, r) => {
                if self.bug == Bug::InsertIgnoreLog {
                    db.shadow.entry(*id).or_insert_with(|| f(r)); // BUG: updates are lost
                } else {
                    db.shadow.insert(*id, f(r)); // REPLACE INTO: idempotent
                }
            }
            Ev::Del(id) => {
                if self.bug != Bug::DropDeletes {
                    db.shadow.remove(id); // DELETE: idempotent
                }
            }
        }
    }
    fn apply_log(&mut self, db: &mut Db, max: usize) -> usize {
        let end = self.applied.saturating_add(max).min(db.binlog.len());
        let evs: Vec<Ev> = db.binlog[self.applied..end].to_vec();
        for e in &evs {
            self.apply_event(db, e);
        }
        evs.len()
    }
    fn copy_chunk(&mut self, db: &mut Db) -> Option<Id> {
        // INSERT IGNORE INTO shadow SELECT f(*) FROM orig WHERE id > cursor ORDER BY id LIMIT CHUNK, as ONE atomic statement
        if self.bug == Bug::TwoPhaseCopy {
            match self.pending.take() {
                None => {
                    self.pending = Some(
                        db.old()
                            .range(self.cursor + 1..)
                            .take(CHUNK)
                            .map(|(k, v)| (*k, v.clone()))
                            .collect(),
                    );
                    return Some(self.cursor); // read now, write on the next tick: concurrent deletes in between resurrect rows
                }
                Some(rows) => {
                    for (k, v) in &rows {
                        db.shadow.entry(*k).or_insert_with(|| f(v));
                    }
                    return rows.last().map(|(k, _)| *k);
                }
            }
        }
        let rows: Vec<(Id, V1)> = db
            .old()
            .range(self.cursor + 1..)
            .take(CHUNK)
            .map(|(k, v)| (*k, v.clone()))
            .collect();
        for (k, v) in &rows {
            db.shadow.entry(*k).or_insert_with(|| f(v));
        }
        rows.last().map(|(k, _)| *k)
    }
    fn old_range_checksum(db: &Db, lo: Id, hi: Id) -> u64 {
        checksum(db.old().range(lo..hi).map(|(k, v)| (k, f(v))))
    }
    fn tick(&mut self, db: &mut Db, rng: &mut Rng, now: u64) {
        // a crash either loses the volatile state before acting, or acts and dies before persisting the cursor
        let crash_before_persist = rng.pm(30);
        if rng.pm(15) {
            self.crashes += 1;
            self.pending = None;
            return;
        }
        match &self.phase {
            Phase::Copy => {
                let backlog = db.binlog.len() - self.applied;
                db.max_backlog = db.max_backlog.max(backlog);
                if backlog > THROTTLE || rng.pm(400) {
                    if backlog > THROTTLE {
                        self.throttled += 1;
                    }
                    let n = self.apply_log(db, RATE);
                    if !crash_before_persist {
                        self.applied += n;
                    } else {
                        self.crashes += 1; // replayed next tick: harmless because REPLACE/DELETE are idempotent
                    }
                } else {
                    match self.copy_chunk(db) {
                        None => {
                            self.phase = Phase::Verify(1);
                        }
                        Some(last) => {
                            if crash_before_persist {
                                self.crashes += 1; // chunk is redone: INSERT IGNORE makes it idempotent
                            } else {
                                self.cursor = last;
                            }
                        }
                    }
                }
            }
            Phase::Verify(lo) => {
                let lo = *lo;
                // catch the log up to the head, then compare one id range atomically
                let n = self.apply_log(db, usize::MAX);
                self.applied += n;
                let hi = lo + VERIFY_RANGE;
                let want = Self::old_range_checksum(db, lo, hi);
                let got = checksum(db.shadow.range(lo..hi).map(|(k, v)| (k, v.clone())));
                if want != got {
                    self.phase = Phase::Failed(format!("checksum mismatch in ids [{lo},{hi})"));
                } else if hi > MAXID {
                    db.locked = true; // short write lock begins
                    self.lock_start = now;
                    self.phase = Phase::Cutover;
                } else {
                    self.phase = Phase::Verify(hi);
                }
            }
            Phase::Cutover => {
                let n = self.apply_log(db, RATE);
                self.applied += n;
                if self.applied == db.binlog.len() {
                    // final full check under the lock, then ONE atomic rename: orig -> v2
                    let want = checksum(db.old().iter().map(|(k, v)| (k, f(v))));
                    let got = checksum(db.shadow.iter().map(|(k, v)| (k, v.clone())));
                    db.locked = false;
                    if want != got {
                        self.phase = Phase::Failed("final check under the lock failed".into());
                    } else {
                        db.phys = Phys::V2(std::mem::take(&mut db.shadow));
                        db.logging = false;
                        self.lock_ticks = now - self.lock_start + 1;
                        self.phase = Phase::Done;
                    }
                }
            }
            Phase::Done | Phase::Failed(_) => {}
        }
    }
}

struct Outcome {
    done: bool,
    failed: bool,
    lock_ticks: u64,
    stalled: u64,
    crashes: u64,
    throttled: u64,
    max_backlog: usize,
    rows: usize,
}

fn run_shadow(seed: u64, bug: Bug) -> Outcome {
    let mut rng = Rng::new(seed);
    let mut db = Db::new(&mut rng);
    let rows = db.logical.len();
    db.logging = true;
    let mut mig = Mig::new(bug);
    let write_rate = 1 + rng.below(8);
    let mut after_done = 0;
    for now in 1..20_000u64 {
        for _ in 0..rng.below(write_rate + 1) {
            db.client_write(&mut rng);
        }
        mig.tick(&mut db, &mut rng, now);
        db.probe(); // the reader checks the live table between every two steps, including mid-cutover
        match mig.phase {
            Phase::Failed(_) => break,
            Phase::Done => {
                after_done += 1;
                if after_done > 30 {
                    break;
                }
            }
            _ => {}
        }
    }
    let done = matches!(mig.phase, Phase::Done);
    if done {
        db.probe(); // final table == f(final oracle state)
    }
    Outcome {
        done,
        failed: matches!(mig.phase, Phase::Failed(_)),
        lock_ticks: mig.lock_ticks,
        stalled: db.stalled_writes,
        crashes: mig.crashes,
        throttled: mig.throttled,
        max_backlog: db.max_backlog,
        rows,
    }
}

// --------------------------------------------------------------------------------------------- Part B: expand / contract

#[derive(Clone, Debug)]
enum Shape {
    Old(i64),
    New(i64, i64), // a, b
}

struct Svc {
    rows: BTreeMap<Id, Shape>,
    logical: BTreeMap<Id, i64>,
    new_writers_only: bool,
    strict: bool,
    cursor: Id,
    batches: u64,
    resumes: u64,
}
impl Svc {
    fn write(&mut self, rng: &mut Rng) {
        let id = 1 + rng.below(MAXID);
        let a = rng.below(1000) as i64;
        if self.logical.contains_key(&id) && rng.pm(250) {
            self.rows.remove(&id);
            self.logical.remove(&id);
            return;
        }
        // old code (still deployed during the rollout) writes the whole row in the old shape; new code dual-writes
        let old_code = !self.new_writers_only && rng.pm(500);
        let shape = if old_code {
            Shape::Old(a)
        } else {
            Shape::New(a, f(&V1 { a }).b)
        };
        self.rows.insert(id, shape);
        self.logical.insert(id, a);
    }
    /// versioned reader: tolerates both shapes until the contract step makes the old one illegal
    fn read_b(&self, id: Id) -> Option<i64> {
        match self.rows.get(&id)? {
            Shape::New(_, b) => Some(*b),
            Shape::Old(a) => {
                assert!(
                    !self.strict,
                    "strict reader met an old-shape row: contracted too early"
                );
                Some(f(&V1 { a: *a }).b)
            }
        }
    }
    fn old_rows(&self) -> usize {
        self.rows
            .values()
            .filter(|s| matches!(s, Shape::Old(_)))
            .count()
    }
    fn probe(&self) {
        let ids: Vec<Id> = self.rows.keys().copied().collect();
        let want: Vec<Id> = self.logical.keys().copied().collect();
        assert_eq!(ids, want, "row set diverged from the clients' content");
        for (id, a) in &self.logical {
            assert_eq!(
                self.read_b(*id),
                Some(f(&V1 { a: *a }).b),
                "reader got a wrong derived column for id {id}"
            );
        }
    }
    /// Refuses while old-shape rows remain: the guard that makes contract safe.
    fn contract(&mut self) -> Result<(), usize> {
        match self.old_rows() {
            0 => {
                self.strict = true;
                Ok(())
            }
            n => Err(n),
        }
    }
    /// One resumable batch: UPDATE ... SET b=f(a) WHERE id > cursor AND b IS NULL ORDER BY id LIMIT 5.
    /// Returns true when the scan reached the end.
    fn backfill_step(&mut self, rng: &mut Rng) -> bool {
        let ids: Vec<Id> = self
            .rows
            .range(self.cursor + 1..)
            .take(5)
            .map(|(k, _)| *k)
            .collect();
        if ids.is_empty() {
            return true;
        }
        for id in &ids {
            if let Some(Shape::Old(a)) = self.rows.get(id).cloned() {
                self.rows.insert(*id, Shape::New(a, f(&V1 { a }).b)); // guarded: only rows still in the old shape
            }
        }
        if rng.pm(100) {
            self.resumes += 1; // died before persisting the cursor: the batch is simply redone
        } else {
            self.cursor = *ids.last().unwrap();
        }
        self.batches += 1;
        false
    }
}

fn run_expand_contract(seed: u64) -> (u64, u64, usize) {
    let mut rng = Rng::new(seed);
    let mut s = Svc {
        rows: BTreeMap::new(),
        logical: BTreeMap::new(),
        new_writers_only: false,
        strict: false,
        cursor: 0,
        batches: 0,
        resumes: 0,
    };
    for _ in 0..40 {
        s.write(&mut rng); // before the rollout: mix of both shapes
    }
    let rollout_ticks = 20 + rng.below(40);
    let mut old_at_backfill_start = 0;
    for tick in 0..10_000u64 {
        for _ in 0..rng.below(3) {
            s.write(&mut rng);
        }
        if tick == rollout_ticks {
            s.new_writers_only = true;
            old_at_backfill_start = s.old_rows();
            if old_at_backfill_start > 0 {
                assert!(
                    s.contract().is_err(),
                    "contract must be refused while old rows exist"
                );
            }
        }
        if tick > rollout_ticks && !s.strict && rng.pm(600) && s.backfill_step(&mut rng) {
            match s.contract() {
                Ok(()) => {}
                Err(_) => s.cursor = 0, // a straggler appeared: rescan
            }
        }
        s.probe();
        if s.strict && tick > rollout_ticks + 40 {
            break;
        }
    }
    assert!(
        s.strict && s.old_rows() == 0,
        "seed {seed}: expand/contract did not finish"
    );
    (s.batches, s.resumes, old_at_backfill_start)
}

fn demo() {
    let mut rng = Rng::new(42);
    let mut db = Db::new(&mut rng);
    db.logging = true;
    println!(
        "table has {} rows; shadow copy runs while clients keep writing",
        db.logical.len()
    );
    let mut mig = Mig::new(Bug::None);
    let mut now = 0;
    while !matches!(mig.phase, Phase::Done) {
        now += 1;
        db.client_write(&mut rng);
        db.client_write(&mut rng);
        mig.tick(&mut db, &mut rng, now);
        db.probe();
        assert!(now < 5000);
    }
    println!(
        "  done after {now} ticks: write lock held {} tick(s), {} writes stalled, {} migrator restarts, {} throttled ticks",
        mig.lock_ticks, db.stalled_writes, mig.crashes, mig.throttled
    );
}

fn main() {
    demo();

    // PART A: 1500 random workloads, correct implementation
    let (mut lock_sum, mut lock_max, mut naive_sum, mut naive_max) = (0u64, 0u64, 0u64, 0u64);
    let (mut crashes, mut throttled, mut stalled, mut backlog_max) = (0u64, 0u64, 0u64, 0usize);
    for seed in 1..=1500 {
        let o = run_shadow(seed, Bug::None);
        assert!(
            o.done && !o.failed,
            "seed {seed}: correct migration did not finish cleanly"
        );
        let naive = (o.rows as u64).div_ceil(CHUNK as u64) + 1; // lock held for the WHOLE copy, then the swap
        lock_sum += o.lock_ticks;
        lock_max = lock_max.max(o.lock_ticks);
        naive_sum += naive;
        naive_max = naive_max.max(naive);
        crashes += o.crashes;
        throttled += o.throttled;
        stalled += o.stalled;
        backlog_max = backlog_max.max(o.max_backlog);
    }
    println!(
        "\nPART A: 1500 workloads OK (final table == f(final state), reader never saw a lost/mixed state)"
    );
    println!(
        "  write-lock ticks, online: mean {:.1} max {}   vs naive blocking ALTER: mean {:.1} max {}",
        lock_sum as f64 / 1500.0,
        lock_max,
        naive_sum as f64 / 1500.0,
        naive_max
    );
    println!(
        "  migrator restarts {crashes}, throttled ticks {throttled}, max log backlog {backlog_max}, writes stalled by the lock {stalled}"
    );
    assert!(lock_sum < naive_sum && crashes > 0 && throttled > 0);

    // injected bugs: the verification must catch each; a bug that slips through must still produce a correct table
    for bug in [Bug::DropDeletes, Bug::InsertIgnoreLog, Bug::TwoPhaseCopy] {
        let (mut caught, mut finished) = (0, 0);
        for seed in 1..=300 {
            let o = run_shadow(seed, bug);
            caught += o.failed as u32;
            finished += o.done as u32;
        }
        println!(
            "  injected {bug:?}: caught by checksums in {caught}/300 seeds, {finished} finished (and passed the final equality)"
        );
        assert!(caught > 0, "{bug:?} was never detected");
    }

    // PART B
    let (mut batches, mut resumes, mut old) = (0, 0, 0);
    for seed in 1..=1500 {
        let (b, r, o) = run_expand_contract(seed);
        batches += b;
        resumes += r;
        old += o;
    }
    println!(
        "\nPART B: 1500 expand/contract workloads OK: {batches} backfill batches, {resumes} resumed after a crash, avg {:.1} old-shape rows left for the backfill,",
        old as f64 / 1500.0
    );
    println!(
        "  the versioned reader was correct at every tick and the early contract was refused each time old rows existed"
    );
}
