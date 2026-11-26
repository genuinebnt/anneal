//! RECIPE 733 (database): Write-ahead logging schemes (physical, logical, physiological), group commit, async commit, log ring buffer, checkpoints.
//! LEVEL: advanced
//! Use in: any durable store: what goes into a log record, how commits share an fsync, how checkpoints bound recovery time.   Run: cargo run --release --example logging_schemes_group_commit
#![allow(dead_code)]
//!
//! PART 1, WHAT TO LOG. A toy store of 16 pages x 512 slots (4 KB pages) is updated by serial transactions (Add / Set /
//! SetSeq on a key). The same workload is logged three ways:
//! * PHYSICAL: before and after IMAGE of the whole page per update (~8 KB). Redo is a memcpy, trivially idempotent, but
//!   the log is huge and the format is tied to the page layout.
//! * PHYSIOLOGICAL (ARIES): page id + slot + a LOGICAL operation inside the page (`Add(delta)` or `Set(before, after)`),
//!   ~30 bytes. Page layout inside the page may be reorganised freely, every record touches one page, and the page LSN makes
//!   redo idempotent: apply only if `page.lsn < record.lsn`.
//! * LOGICAL (command logging, VoltDB-style): one record per transaction at COMMIT carrying the commands (~13 bytes each).
//!   Tiny log, but recovery must RE-EXECUTE the commands on a transaction-consistent snapshot, in order, exactly once, with no
//!   steal (the disk never sees uncommitted data, because the log cannot undo anything). Hazards demonstrated and asserted:
//!   replaying twice double-applies, and NONDETERMINISTIC commands (here `SetSeq`, a sequence value that aborted
//!   transactions also consumed; the same issue arises with NOW(), RANDOM(), triggers) re-execute differently, so a
//!   safe logical log records the RESOLVED values (`Set(key, 2)`), which is what the recovery tests use.
//! * RECOVERY (physical / physiological): analysis finds the transactions with a commit or end record, REDO repeats history from the
//!   redo start point (page LSN test), UNDO removes losers newest-first using before values; an abort writes compensation
//!   records (CLRs, redo-only) and an end record, and undo skips what was already compensated. Disk pages are STOLEN at
//!   random (uncommitted data reaches disk) and the log is forced at commit (no-force for pages), under the WAL rule.
//!   Serial transactions keep the undo simple (only the last transaction can be a loser); `aries_recovery` has the
//!   concurrent version and crashes during recovery itself.
//! * CRASH TEST: for each scheme x checkpoint policy, 150 random workloads are crashed at 12 random events each (every log
//!   append / flush / page write / checkpoint step is an event); the recovered state must equal "exactly the transactions
//!   whose commit record reached the durable log", no more, no less.
//! * CHECKPOINTS bound recovery. BLOCKING: stop, flush every dirty page, then redo starts at the checkpoint. SHARP: the same
//!   but only at a transaction boundary (transaction-consistent; the only kind logical logging can use, as a snapshot). FUZZY: no
//!   flush; record the dirty page table and the active transactions; REDO START = min(recLSN of dirty pages) and the log may
//!   be truncated below min(redo start, first LSN of any active transaction) (undo still needs those records).
//! PART 2, GROUP COMMIT with real threads. Each commit must wait for its record to be fsynced. One fsync per commit caps
//! throughput at 1 / fsync-time; with a LEADER/FOLLOWER protocol the first committer fsyncs everything appended so far while
//! the others wait on a condition variable, so a batch costs one fsync. Fsync is simulated (1 ms sleep inside a single-queue
//! device); throughput is printed but never asserted (batching is asserted via fsync counts). A device crash is injected at the n-th fsync (batch lost, or durable but unacknowledged); every ACKNOWLEDGED commit must be in
//! the durable set. ASYNCHRONOUS COMMIT acknowledges before the flush: higher throughput but a window of acknowledged commits that a crash
//! loses (asserted on a scripted run), like PostgreSQL's synchronous_commit = off.
//! PART 3, LOG BUFFER RING: writers claim space with an atomic `fetch_add` on the LSN (LSN = word offset), copy their record into a
//! ring of atomics, then publish it by storing the header last; a flusher consumes the contiguous completed prefix and writers
//! back-pressure when the ring is full (Aether / InnoDB / PostgreSQL WAL insertion all do variants of this). 8 threads fill a small ring many times
//! over and the flushed stream is verified record by record.
//!
//! COMMON USES: PostgreSQL WAL (physiological + full page images after checkpoints, group commit via commit_delay, async commit),
//! MySQL InnoDB redo log, SQLite WAL, VoltDB command logging, SQL Server log buffer, Kafka-style log append with batched fsync.
//! Course: CMU 15-445 Lecture 21 (database logging) and the CMU 15-721 logging / recovery lectures.
//! Read: "ARIES: A Transaction Recovery Method Supporting Fine-Granularity Locking" (Mohan et al., 1992) https://web.stanford.edu/class/cs345d-01/rl/aries.pdf ,
//! PostgreSQL WAL asynchronous commit docs https://www.postgresql.org/docs/current/wal-async-commit.html , PostgreSQL WAL configuration (checkpoints, commit_delay)
//! https://www.postgresql.org/docs/current/wal-configuration.html
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

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

// ------------------------------------------------------------------------------------------------------------------
// Part 1: logging schemes over a toy page store
// ------------------------------------------------------------------------------------------------------------------

const SLOTS: usize = 512;
const NPAGES: usize = 16;

#[derive(Clone, Debug, PartialEq)]
struct Page {
    lsn: u64,
    v: Vec<i64>,
}

fn blank_pages() -> Vec<Page> {
    vec![
        Page {
            lsn: 0,
            v: vec![0; SLOTS]
        };
        NPAGES
    ]
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Op {
    Add(usize, i64),
    Set(usize, i64),
    /// value = next sequence number: NONDETERMINISTIC (aborted transactions consume numbers too)
    SetSeq(usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum SlotOp {
    Add(i64),
    Set { before: i64, after: i64 },
}

impl SlotOp {
    fn inverse(self) -> SlotOp {
        match self {
            SlotOp::Add(d) => SlotOp::Add(-d),
            SlotOp::Set { before, after } => SlotOp::Set {
                before: after,
                after: before,
            },
        }
    }
    fn apply(self, v: i64) -> i64 {
        match self {
            SlotOp::Add(d) => v + d,
            SlotOp::Set { after, .. } => after,
        }
    }
    fn undo(self, v: i64) -> i64 {
        self.inverse().apply(v)
    }
}

#[derive(Clone, Debug)]
enum Rec {
    Begin {
        lsn: u64,
        tx: u32,
    },
    Commit {
        lsn: u64,
        tx: u32,
    },
    End {
        lsn: u64,
        tx: u32,
    },
    Phys {
        lsn: u64,
        tx: u32,
        page: usize,
        before: Vec<i64>,
        after: Vec<i64>,
        clr: bool,
    },
    Physio {
        lsn: u64,
        tx: u32,
        page: usize,
        slot: usize,
        op: SlotOp,
        clr: bool,
    },
    Logic {
        lsn: u64,
        tx: u32,
        ops: Vec<Op>,
    },
}

impl Rec {
    fn lsn(&self) -> u64 {
        match self {
            Rec::Begin { lsn, .. }
            | Rec::Commit { lsn, .. }
            | Rec::End { lsn, .. }
            | Rec::Phys { lsn, .. }
            | Rec::Physio { lsn, .. }
            | Rec::Logic { lsn, .. } => *lsn,
        }
    }
    fn tx(&self) -> u32 {
        match self {
            Rec::Begin { tx, .. }
            | Rec::Commit { tx, .. }
            | Rec::End { tx, .. }
            | Rec::Phys { tx, .. }
            | Rec::Physio { tx, .. }
            | Rec::Logic { tx, .. } => *tx,
        }
    }
    /// Encoded size: header = lsn 8 + type 1 + tx 4.
    fn bytes(&self) -> usize {
        13 + match self {
            Rec::Begin { .. } | Rec::Commit { .. } | Rec::End { .. } => 0,
            Rec::Phys { before, after, .. } => 4 + 1 + (before.len() + after.len()) * 8,
            Rec::Physio { op, .. } => {
                4 + 2 + 1 + 1 + if matches!(op, SlotOp::Add(_)) { 8 } else { 16 }
            }
            Rec::Logic { ops, .. } => 2 + ops.len() * 13,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Scheme {
    Physical,
    Physiological,
    Logical,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Ck {
    None,
    Blocking,
    Sharp,
    Fuzzy,
}

#[derive(Clone, Copy, Debug)]
struct Master {
    ckpt_lsn: u64,
    redo_start: u64,
    scan_start: u64,
    seq: i64,
}

#[derive(Clone)]
struct Disk {
    pages: Vec<Page>,
    master: Option<Master>,
}

struct TxSpec {
    ops: Vec<Op>,
    abort: bool,
}

struct Crash;

struct TxCtx {
    tx: u32,
    first_lsn: u64,
    undo: Vec<(usize, usize, SlotOp)>, // (page, slot, applied op): rolled back in reverse on abort
    effects: Vec<(usize, i64)>,
    cmds: Vec<Op>,
}

struct Db {
    scheme: Scheme,
    ck: Ck,
    resolve: bool, // logical only: log resolved values instead of raw nondeterministic commands
    mem: Vec<Page>,
    disk: Disk,
    log: VecDeque<Rec>,
    next_lsn: u64,
    flushed: u64,
    dpt: BTreeMap<usize, u64>,
    seq: i64,
    ev: usize,
    crash_at: Option<usize>,
    rng: Rng,
    cur: Option<TxCtx>,
    bytes_appended: u64,
    updates: u64,
    ckpt_pages_flushed: u64,
    commits: Vec<(Vec<(usize, i64)>, u64)>, // (effects, commit lsn) of every committed transaction
    ops_since_ckpt: usize,
    ckpt_pending: bool,
    chaos: bool, // random steals / log flushes
}

/// Redo start point and log truncation point from the dirty page table and the first LSNs of active transactions.
fn redo_points(dpt: &BTreeMap<usize, u64>, att_first: &[u64], next_lsn: u64) -> (u64, u64) {
    let redo = dpt.values().copied().min().unwrap_or(next_lsn);
    let scan = att_first.iter().copied().chain([redo]).min().unwrap();
    (redo, scan)
}

impl Db {
    fn new(scheme: Scheme, ck: Ck, seed: u64, crash_at: Option<usize>, chaos: bool) -> Db {
        Db {
            scheme,
            ck,
            resolve: true,
            mem: blank_pages(),
            disk: Disk {
                pages: blank_pages(),
                master: None,
            },
            log: VecDeque::new(),
            next_lsn: 1,
            flushed: 0,
            dpt: BTreeMap::new(),
            seq: 0,
            ev: 0,
            crash_at,
            rng: Rng(seed | 1),
            cur: None,
            bytes_appended: 0,
            updates: 0,
            ckpt_pages_flushed: 0,
            commits: Vec::new(),
            ops_since_ckpt: 0,
            ckpt_pending: false,
            chaos,
        }
    }

    fn tick(&mut self) -> Result<(), Crash> {
        self.ev += 1;
        if Some(self.ev) == self.crash_at {
            Err(Crash)
        } else {
            Ok(())
        }
    }

    fn append(&mut self, f: impl FnOnce(u64) -> Rec) -> Result<u64, Crash> {
        self.tick()?;
        let lsn = self.next_lsn;
        self.next_lsn += 1;
        let r = f(lsn);
        self.bytes_appended += r.bytes() as u64;
        self.log.push_back(r);
        Ok(lsn)
    }

    fn flush_log(&mut self) -> Result<(), Crash> {
        self.tick()?;
        self.flushed = self.next_lsn - 1;
        Ok(())
    }

    /// Steal: write one page to disk, even with uncommitted data, but only after the log is durable up to its page LSN (WAL rule).
    fn flush_page(&mut self, p: usize) -> Result<(), Crash> {
        if self.mem[p].lsn > self.flushed {
            self.flush_log()?;
        }
        self.tick()?;
        self.disk.pages[p] = self.mem[p].clone();
        self.dpt.remove(&p);
        Ok(())
    }

    fn key(k: usize) -> (usize, usize) {
        (k / SLOTS, k % SLOTS)
    }

    /// Log and apply one slot update. `clr` marks compensation records written by abort.
    fn update(&mut self, page: usize, slot: usize, op: SlotOp, clr: bool) -> Result<(), Crash> {
        let tx = self.cur.as_ref().unwrap().tx;
        let lsn = match self.scheme {
            Scheme::Physiological => self.append(|lsn| Rec::Physio {
                lsn,
                tx,
                page,
                slot,
                op,
                clr,
            })?,
            Scheme::Physical => {
                let before = self.mem[page].v.clone();
                let mut after = before.clone();
                after[slot] = op.apply(before[slot]);
                self.append(|lsn| Rec::Phys {
                    lsn,
                    tx,
                    page,
                    before,
                    after,
                    clr,
                })?
            }
            Scheme::Logical => 0, // logical logging writes nothing until commit
        };
        let v = &mut self.mem[page].v[slot];
        *v = op.apply(*v);
        if self.scheme != Scheme::Logical {
            self.mem[page].lsn = lsn;
            self.dpt.entry(page).or_insert(lsn);
        }
        Ok(())
    }

    fn exec_op(&mut self, op: Op) -> Result<(), Crash> {
        let key = match op {
            Op::Add(k, _) | Op::Set(k, _) | Op::SetSeq(k) => k,
        };
        let (page, slot) = Self::key(key);
        let old = self.mem[page].v[slot];
        let slot_op = match op {
            Op::Add(_, d) => SlotOp::Add(d),
            Op::Set(_, v) => SlotOp::Set {
                before: old,
                after: v,
            },
            Op::SetSeq(_) => {
                self.seq += 1;
                SlotOp::Set {
                    before: old,
                    after: self.seq,
                }
            }
        };
        let logged_cmd = match (op, slot_op) {
            (Op::SetSeq(k), SlotOp::Set { after, .. }) if self.resolve => Op::Set(k, after),
            (o, _) => o,
        };
        self.update(page, slot, slot_op, false)?;
        let new = self.mem[page].v[slot];
        let c = self.cur.as_mut().unwrap();
        c.undo.push((page, slot, slot_op));
        c.effects.push((key, new));
        c.cmds.push(logged_cmd);
        self.updates += 1;
        self.ops_since_ckpt += 1;
        if self.chaos && self.scheme != Scheme::Logical {
            if self.rng.below(100) < 20 && !self.dpt.is_empty() {
                let pages: Vec<usize> = self.dpt.keys().copied().collect();
                let p = pages[self.rng.below(pages.len())];
                self.flush_page(p)?;
            }
            if self.rng.below(100) < 10 {
                self.flush_log()?;
            }
        }
        if matches!(self.ck, Ck::Blocking | Ck::Fuzzy) && self.ops_since_ckpt >= 9 {
            self.checkpoint()?;
        }
        Ok(())
    }

    fn begin(&mut self, tx: u32) -> Result<(), Crash> {
        let first_lsn = if self.scheme == Scheme::Logical {
            self.next_lsn
        } else {
            self.append(|lsn| Rec::Begin { lsn, tx })?
        };
        self.cur = Some(TxCtx {
            tx,
            first_lsn,
            undo: vec![],
            effects: vec![],
            cmds: vec![],
        });
        Ok(())
    }

    fn commit(&mut self) -> Result<(), Crash> {
        let (tx, cmds) = {
            let c = self.cur.as_ref().unwrap();
            (c.tx, c.cmds.clone())
        };
        let lsn = match self.scheme {
            Scheme::Logical => self.append(|lsn| Rec::Logic { lsn, tx, ops: cmds })?,
            _ => self.append(|lsn| Rec::Commit { lsn, tx })?,
        };
        self.flush_log()?; // force the log at commit; pages are NOT forced (no-force)
        let c = self.cur.take().unwrap();
        self.commits.push((c.effects, lsn));
        Ok(())
    }

    fn abort(&mut self) -> Result<(), Crash> {
        let undo = std::mem::take(&mut self.cur.as_mut().unwrap().undo);
        for (page, slot, op) in undo.into_iter().rev() {
            if self.scheme == Scheme::Logical {
                let v = &mut self.mem[page].v[slot];
                *v = op.undo(*v); // nothing was logged, so rolling back memory is enough
            } else {
                self.update(page, slot, op.inverse(), true)?; // compensation record
            }
        }
        if self.scheme != Scheme::Logical {
            let tx = self.cur.as_ref().unwrap().tx;
            self.append(|lsn| Rec::End { lsn, tx })?;
        }
        self.cur = None;
        Ok(())
    }

    fn checkpoint(&mut self) -> Result<(), Crash> {
        self.ops_since_ckpt = 0;
        self.ckpt_pending = false;
        if self.scheme == Scheme::Logical {
            // transaction-consistent snapshot installed atomically together with its master record
            self.flush_log()?;
            self.tick()?;
            self.disk.pages = self.mem.clone();
            let ckpt = self.next_lsn - 1;
            self.disk.master = Some(Master {
                ckpt_lsn: ckpt,
                redo_start: self.next_lsn,
                scan_start: self.next_lsn,
                seq: self.seq,
            });
            self.log.retain(|r| r.lsn() > ckpt);
            return Ok(());
        }
        if self.ck == Ck::Sharp && self.cur.is_some() {
            self.ckpt_pending = true; // must wait for a transaction boundary
            return Ok(());
        }
        if matches!(self.ck, Ck::Blocking | Ck::Sharp) {
            let dirty: Vec<usize> = self.dpt.keys().copied().collect();
            self.ckpt_pages_flushed += dirty.len() as u64;
            for p in dirty {
                self.flush_page(p)?;
            }
        }
        self.flush_log()?;
        let att: Vec<u64> = self.cur.iter().map(|c| c.first_lsn).collect();
        let (redo_start, scan_start) = redo_points(&self.dpt, &att, self.next_lsn);
        self.tick()?;
        self.disk.master = Some(Master {
            ckpt_lsn: self.next_lsn - 1,
            redo_start,
            scan_start,
            seq: self.seq,
        });
        self.log.retain(|r| r.lsn() >= scan_start); // truncate: older records are needed neither for redo nor for undo
        Ok(())
    }

    fn run(&mut self, wl: &[TxSpec]) -> Result<(), Crash> {
        for (i, t) in wl.iter().enumerate() {
            self.begin(i as u32 + 1)?;
            for &op in &t.ops {
                self.exec_op(op)?;
            }
            if t.abort {
                self.abort()?
            } else {
                self.commit()?
            }
            let want_sharp = (self.ck == Ck::Sharp && (i % 3 == 2)) || self.ckpt_pending;
            if want_sharp || (self.scheme == Scheme::Logical && self.ck != Ck::None && i % 3 == 2) {
                self.checkpoint()?;
            }
        }
        Ok(())
    }

    /// What survives a crash: disk pages + master record + the flushed prefix of the log.
    fn image(&self) -> Image {
        Image {
            disk: self.disk.clone(),
            log: self
                .log
                .iter()
                .filter(|r| r.lsn() <= self.flushed)
                .cloned()
                .collect(),
        }
    }
}

struct Image {
    disk: Disk,
    log: Vec<Rec>,
}

#[derive(Default, Clone, Copy)]
struct RecStats {
    scanned: u64,
    redone: u64,
    undone: u64,
}

fn recover(img: &Image, scheme: Scheme, resolve_seq: bool) -> (Vec<Page>, RecStats) {
    let mut pages = img.disk.pages.clone();
    let mut st = RecStats::default();
    if scheme == Scheme::Logical {
        // restore the consistent snapshot, then re-execute the commands of every logged (= committed) transaction in order
        let (ckpt, mut seq) = img.disk.master.map_or((0, 0), |m| (m.ckpt_lsn, m.seq));
        for r in &img.log {
            if let Rec::Logic { lsn, ops, .. } = r {
                if *lsn <= ckpt {
                    continue;
                }
                st.scanned += 1;
                for &op in ops {
                    st.redone += 1;
                    replay_cmd(&mut pages, &mut seq, op);
                }
            }
        }
        return (pages, st);
    }
    let (redo_start, scan_start) = img
        .disk
        .master
        .map_or((0, 0), |m| (m.redo_start, m.scan_start));
    // ANALYSIS: which transactions completed (commit or end record)?
    let mut done: HashSet<u32> = HashSet::new();
    for r in img.log.iter().filter(|r| r.lsn() >= scan_start) {
        if let Rec::Commit { tx, .. } | Rec::End { tx, .. } = r {
            done.insert(*tx);
        }
    }
    // REDO: repeat history, guarded by the page LSN (idempotent)
    for r in img.log.iter().filter(|r| r.lsn() >= redo_start) {
        st.scanned += 1;
        match r {
            Rec::Physio {
                lsn,
                page,
                slot,
                op,
                ..
            } if pages[*page].lsn < *lsn => {
                pages[*page].v[*slot] = op.apply(pages[*page].v[*slot]);
                pages[*page].lsn = *lsn;
                st.redone += 1;
            }
            Rec::Phys {
                lsn, page, after, ..
            } if pages[*page].lsn < *lsn => {
                pages[*page].v.clone_from(after);
                pages[*page].lsn = *lsn;
                st.redone += 1;
            }
            _ => {}
        }
    }
    // UNDO: losers, newest update first, skipping updates that CLRs already compensated
    let mut loser_ups: BTreeMap<u32, (Vec<&Rec>, usize)> = BTreeMap::new();
    for r in img
        .log
        .iter()
        .filter(|r| r.lsn() >= scan_start && !done.contains(&r.tx()))
    {
        let is_clr = matches!(
            r,
            Rec::Physio { clr: true, .. } | Rec::Phys { clr: true, .. }
        );
        if matches!(r, Rec::Physio { .. } | Rec::Phys { .. }) {
            let e = loser_ups.entry(r.tx()).or_default();
            if is_clr { e.1 += 1 } else { e.0.push(r) }
        }
    }
    for (_, (ups, clrs)) in loser_ups {
        for r in ups.iter().rev().skip(clrs) {
            match r {
                Rec::Physio { page, slot, op, .. } => {
                    pages[*page].v[*slot] = op.undo(pages[*page].v[*slot])
                }
                Rec::Phys { page, before, .. } => pages[*page].v.clone_from(before),
                _ => unreachable!(),
            }
            st.undone += 1;
        }
    }
    let _ = resolve_seq;
    (pages, st)
}

fn replay_cmd(pages: &mut [Page], seq: &mut i64, op: Op) {
    match op {
        Op::Add(k, d) => pages[k / SLOTS].v[k % SLOTS] += d,
        Op::Set(k, v) => pages[k / SLOTS].v[k % SLOTS] = v,
        Op::SetSeq(k) => {
            *seq += 1;
            pages[k / SLOTS].v[k % SLOTS] = *seq;
        }
    }
}

fn gen_workload(rng: &mut Rng, n_tx: usize) -> Vec<TxSpec> {
    (0..n_tx)
        .map(|_| {
            let ops = (0..1 + rng.below(4))
                .map(|_| {
                    let key = rng.below(NPAGES) * SLOTS + rng.below(8); // few hot slots per page: updates overlap
                    match rng.below(10) {
                        0..=4 => Op::Add(key, rng.below(100) as i64 - 50),
                        5..=7 => Op::Set(key, rng.below(1000) as i64),
                        _ => Op::SetSeq(key),
                    }
                })
                .collect();
            TxSpec {
                ops,
                abort: rng.below(100) < 15,
            }
        })
        .collect()
}

/// Expected state: the effects of exactly the transactions whose commit record is durable.
fn expected(db: &Db, flushed: u64) -> (Vec<i64>, usize) {
    let mut state = vec![0i64; NPAGES * SLOTS];
    let mut n = 0;
    for (effects, lsn) in &db.commits {
        if *lsn <= flushed {
            n += 1;
            effects.iter().for_each(|&(k, v)| state[k] = v);
        }
    }
    (state, n)
}

fn flatten(pages: &[Page]) -> Vec<i64> {
    pages.iter().flat_map(|p| p.v.iter().copied()).collect()
}

fn crash_tests() {
    println!(
        "-- crash tests: 150 workloads x 12 random crash points per (scheme, checkpoint policy); recovered state == durable commits"
    );
    let configs: Vec<(Scheme, Ck)> = vec![
        (Scheme::Physical, Ck::None),
        (Scheme::Physical, Ck::Blocking),
        (Scheme::Physical, Ck::Fuzzy),
        (Scheme::Physiological, Ck::None),
        (Scheme::Physiological, Ck::Blocking),
        (Scheme::Physiological, Ck::Sharp),
        (Scheme::Physiological, Ck::Fuzzy),
        (Scheme::Logical, Ck::None),
        (Scheme::Logical, Ck::Sharp),
    ];
    println!(
        "   {:<14}{:<10}{:>6}{:>12}{:>12}{:>12}{:>14}{:>14}",
        "scheme", "ckpt", "runs", "redo scan", "redone", "undone", "retained B", "ckpt flushes"
    );
    let mut retained: HashMap2 = HashMap2::new();
    for (scheme, ck) in configs {
        let mut rng = Rng(0x5eed_0000 + scheme as u64 * 77 + ck as u64 * 13);
        let (mut runs, mut scan, mut redone, mut undone, mut kept, mut flushes) =
            (0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
        for w in 0..150u64 {
            let wl = gen_workload(&mut rng, 30);
            let seed = rng.next();
            let mut full = Db::new(scheme, ck, seed, None, true);
            assert!(full.run(&wl).is_ok());
            let total = full.ev;
            for c in 0..12 {
                let crash_at =
                    1 + rng.below(total + 1).min(total.saturating_sub(1)) + (c == 0) as usize * 0;
                let mut db = Db::new(scheme, ck, seed, Some(crash_at), true);
                let _ = db.run(&wl); // Err(Crash) at the chosen event (or completes if the crash point is the very end)
                let img = db.image();
                let (pages, st) = recover(&img, scheme, true);
                let (exp, _) = expected(&db, db.flushed);
                assert_eq!(
                    flatten(&pages),
                    exp,
                    "{scheme:?}/{ck:?} workload {w} crash at event {crash_at}/{total}"
                );
                runs += 1;
                scan += st.scanned;
                redone += st.redone;
                undone += st.undone;
                kept += img.log.iter().map(|r| r.bytes() as u64).sum::<u64>();
                flushes += db.ckpt_pages_flushed;
            }
        }
        retained.insert((scheme, ck), kept / runs);
        println!(
            "   {:<14}{:<10}{runs:>6}{:>12.1}{:>12.1}{:>12.2}{:>14}{:>14.1}",
            format!("{scheme:?}"),
            format!("{ck:?}"),
            scan as f64 / runs as f64,
            redone as f64 / runs as f64,
            undone as f64 / runs as f64,
            kept / runs,
            flushes as f64 / runs as f64
        );
    }
    // truncation really shrinks the retained log
    for scheme in [Scheme::Physical, Scheme::Physiological] {
        assert!(
            retained[&(scheme, Ck::Fuzzy)] < retained[&(scheme, Ck::None)],
            "{scheme:?}: fuzzy checkpoints truncate the log"
        );
        assert!(retained[&(scheme, Ck::Blocking)] < retained[&(scheme, Ck::None)]);
    }
    assert!(retained[&(Scheme::Logical, Ck::Sharp)] < retained[&(Scheme::Logical, Ck::None)]);
    println!(
        "   (fuzzy flushes 0 pages at the checkpoint and keeps a longer log than blocking: its redo start is min recLSN, not the checkpoint)"
    );
}

type HashMap2 = std::collections::HashMap<(Scheme, Ck), u64>;

fn size_and_time() {
    println!(
        "-- same workload (600 txns, 4 small updates each) under each scheme: log volume and full-log recovery"
    );
    let mut rng = Rng(0xabcdef);
    let wl: Vec<TxSpec> = (0..600)
        .map(|_| TxSpec {
            ops: (0..4)
                .map(|_| Op::Add(rng.below(NPAGES * SLOTS), 1 + rng.below(9) as i64))
                .collect(),
            abort: false,
        })
        .collect();
    println!(
        "   {:<14}{:>12}{:>14}{:>12}{:>14}{:>12}",
        "scheme", "log MB", "bytes/update", "redo ops", "recover us", "bytes/txn"
    );
    let mut per_update = BTreeMap::new();
    for scheme in [Scheme::Logical, Scheme::Physiological, Scheme::Physical] {
        let mut db = Db::new(scheme, Ck::None, 1, None, false);
        db.run(&wl).ok().unwrap();
        let img = Image {
            disk: Disk {
                pages: blank_pages(),
                master: None,
            },
            log: db.log.iter().cloned().collect(),
        };
        let t = Instant::now();
        let (pages, st) = recover(&img, scheme, true);
        let el = t.elapsed();
        assert_eq!(
            flatten(&pages),
            flatten(&db.mem),
            "{scheme:?}: full replay reproduces the live state"
        );
        let bpu = db.bytes_appended as f64 / db.updates as f64;
        per_update.insert(scheme as u8, bpu);
        println!(
            "   {:<14}{:>12.2}{bpu:>14.1}{:>12}{:>14}{:>12.0}",
            format!("{scheme:?}"),
            db.bytes_appended as f64 / 1e6,
            st.redone,
            el.as_micros(),
            db.bytes_appended as f64 / 600.0
        );
    }
    let (l, ph, py) = (
        per_update[&(Scheme::Logical as u8)],
        per_update[&(Scheme::Physiological as u8)],
        per_update[&(Scheme::Physical as u8)],
    );
    assert!(
        l < ph && ph < py,
        "logical < physiological < physical bytes per small update: {l} {ph} {py}"
    );
    assert!(py > 100.0 * ph, "a page image dwarfs a slot-level record");
}

fn logical_hazards() {
    println!("-- logical logging hazards");
    let wl = vec![
        TxSpec {
            ops: vec![Op::SetSeq(0)],
            abort: true,
        }, // consumes sequence number 1 and rolls back
        TxSpec {
            ops: vec![Op::SetSeq(1), Op::Add(1, 10)],
            abort: false,
        },
    ];
    for resolve in [false, true] {
        let mut db = Db::new(Scheme::Logical, Ck::None, 1, None, false);
        db.resolve = resolve;
        db.run(&wl).ok().unwrap();
        let img = db.image();
        let (pages, _) = recover(&img, Scheme::Logical, resolve);
        let ok = flatten(&pages) == flatten(&db.mem);
        println!(
            "   log raw SetSeq: {:<5} -> live value {}, replayed value {}  ({})",
            !resolve,
            db.mem[0].v[1],
            pages[0].v[1],
            if ok { "same" } else { "DIVERGED" }
        );
        assert_eq!(
            ok, resolve,
            "raw nondeterministic commands diverge on replay; resolved values do not"
        );
    }
    // replaying a logical log twice double-applies (the log has no page LSN); physiological redo is idempotent
    let mut db = Db::new(Scheme::Logical, Ck::None, 1, None, false);
    db.run(&[TxSpec {
        ops: vec![Op::Add(5, 7)],
        abort: false,
    }])
    .ok()
    .unwrap();
    let img = db.image();
    let mut once = img.disk.pages.clone();
    let mut seq = 0;
    for r in &img.log {
        if let Rec::Logic { ops, .. } = r {
            ops.iter().for_each(|&o| replay_cmd(&mut once, &mut seq, o));
        }
    }
    let mut twice = once.clone();
    for r in &img.log {
        if let Rec::Logic { ops, .. } = r {
            ops.iter()
                .for_each(|&o| replay_cmd(&mut twice, &mut seq, o));
        }
    }
    assert_eq!(
        (once[0].v[5], twice[0].v[5]),
        (7, 14),
        "logical replay is not idempotent: needs a consistent snapshot"
    );
    let mut dbp = Db::new(Scheme::Physiological, Ck::None, 1, None, false);
    dbp.run(&[TxSpec {
        ops: vec![Op::Add(5, 7)],
        abort: false,
    }])
    .ok()
    .unwrap();
    let img = dbp.image();
    let (pages, _) = recover(&img, Scheme::Physiological, true);
    let again = Image {
        disk: Disk {
            pages: pages.clone(),
            master: None,
        },
        log: img.log.clone(),
    };
    let (pages2, st2) = recover(&again, Scheme::Physiological, true);
    assert_eq!((pages[0].v[5], pages2[0].v[5], st2.redone), (7, 7, 0));
    println!(
        "   logical replay twice: 7 -> 14 (double apply); physiological redo twice: 7 -> 7 (page LSN skips everything)"
    );
}

fn checkpoint_points_demo() {
    println!("-- redo start point of a fuzzy checkpoint");
    let dpt: BTreeMap<usize, u64> = [(3, 120), (1, 95), (7, 140)].into_iter().collect();
    let (redo, scan) = redo_points(&dpt, &[80, 130], 150);
    println!(
        "   dirty pages recLSN {{p3:120, p1:95, p7:140}}, active txns first LSN {{80, 130}}: redo starts at {redo}, log kept from {scan}"
    );
    assert_eq!((redo, scan), (95, 80));
    assert_eq!(
        redo_points(&BTreeMap::new(), &[], 150),
        (150, 150),
        "nothing dirty and nobody active: start at the checkpoint"
    );
}

// ------------------------------------------------------------------------------------------------------------------
// Part 2: group commit
// ------------------------------------------------------------------------------------------------------------------

struct DevInner {
    durable: Vec<u64>,
    fsyncs: u64,
    crash_at: Option<u64>,
    crashed: bool,
    keep_batch_on_crash: bool,
}

/// A single-queue "disk": fsync takes `cost` and persists a whole batch atomically (or crashes at the configured one).
struct Device {
    inner: Mutex<DevInner>,
    cost: Duration,
}

impl Device {
    fn new(cost: Duration, crash_at: Option<u64>, keep_batch_on_crash: bool) -> Device {
        Device {
            inner: Mutex::new(DevInner {
                durable: vec![],
                fsyncs: 0,
                crash_at,
                crashed: false,
                keep_batch_on_crash,
            }),
            cost,
        }
    }
    fn fsync(&self, batch: &[u64]) -> Result<(), ()> {
        let mut g = self.inner.lock().unwrap();
        if g.crashed {
            return Err(());
        }
        std::thread::sleep(self.cost); // the device is busy: one fsync at a time
        g.fsyncs += 1;
        if Some(g.fsyncs) == g.crash_at {
            g.crashed = true;
            if g.keep_batch_on_crash {
                g.durable.extend_from_slice(batch); // data reached the platter but the ack never made it back
            }
            return Err(());
        }
        g.durable.extend_from_slice(batch);
        Ok(())
    }
    fn snapshot(&self) -> (Vec<u64>, u64) {
        let g = self.inner.lock().unwrap();
        (g.durable.clone(), g.fsyncs)
    }
}

struct GlState {
    pending: Vec<u64>,
    appended: u64,
    durable_upto: u64,
    flushing: bool,
    failed: bool,
}

struct GroupLog<'a> {
    st: Mutex<GlState>,
    cv: Condvar,
    dev: &'a Device,
    delay: Duration,
}

impl<'a> GroupLog<'a> {
    fn new(dev: &'a Device, delay: Duration) -> Self {
        GroupLog {
            st: Mutex::new(GlState {
                pending: vec![],
                appended: 0,
                durable_upto: 0,
                flushing: false,
                failed: false,
            }),
            cv: Condvar::new(),
            dev,
            delay,
        }
    }

    /// Append the commit record and return only when it is durable. The first waiter becomes the LEADER and fsyncs every
    /// record appended so far; followers sleep on the condition variable and are released by the leader's fsync.
    fn commit(&self, tx: u64) -> Result<(), ()> {
        let mut g = self.st.lock().unwrap();
        g.appended += 1;
        let my = g.appended;
        g.pending.push(tx);
        loop {
            if g.failed {
                return Err(());
            }
            if g.durable_upto >= my {
                return Ok(());
            }
            if !g.flushing {
                g.flushing = true;
                if !self.delay.is_zero() {
                    drop(g);
                    std::thread::sleep(self.delay); // commit_delay: give other committers a chance to join the batch
                    g = self.st.lock().unwrap();
                }
                let batch = std::mem::take(&mut g.pending);
                let upto = g.appended;
                drop(g);
                let r = self.dev.fsync(&batch);
                g = self.st.lock().unwrap();
                g.flushing = false;
                match r {
                    Ok(()) => g.durable_upto = upto,
                    Err(()) => g.failed = true,
                }
                self.cv.notify_all();
            } else {
                g = self.cv.wait(g).unwrap();
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    PerCommit,
    Group(Duration),
}

struct RunResult {
    acked: Vec<u64>,
    durable: Vec<u64>,
    fsyncs: u64,
    elapsed: Duration,
    total_latency: Duration,
}

fn run_commits(
    mode: Mode,
    threads: u64,
    per_thread: u64,
    cost: Duration,
    crash_at: Option<u64>,
    keep: bool,
) -> RunResult {
    let dev = Device::new(cost, crash_at, keep);
    let gl = GroupLog::new(
        &dev,
        if let Mode::Group(d) = mode {
            d
        } else {
            Duration::ZERO
        },
    );
    let acked = Mutex::new(Vec::new());
    let lat = Mutex::new(Duration::ZERO);
    let start = Instant::now();
    std::thread::scope(|s| {
        for t in 0..threads {
            let (gl, dev, acked, lat) = (&gl, &dev, &acked, &lat);
            s.spawn(move || {
                for i in 0..per_thread {
                    let tx = t * 1_000_000 + i;
                    let t0 = Instant::now();
                    let r = match mode {
                        Mode::PerCommit => dev.fsync(&[tx]),
                        Mode::Group(_) => gl.commit(tx),
                    };
                    if r.is_ok() {
                        *lat.lock().unwrap() += t0.elapsed();
                        acked.lock().unwrap().push(tx); // the client is told "committed" only now
                    } else {
                        break; // device failed: this and later commits are not acknowledged
                    }
                }
            });
        }
    });
    let elapsed = start.elapsed();
    let (durable, fsyncs) = dev.snapshot();
    RunResult {
        acked: acked.into_inner().unwrap(),
        durable,
        fsyncs,
        elapsed,
        total_latency: lat.into_inner().unwrap(),
    }
}

/// Proof of batching that no schedule can break: keep the device busy (hold its lock) until all `n` committers have
/// appended. At most one leader got in before that, with whatever had arrived; everyone else must share the next fsync.
fn check_batching_deterministically(n: u64) {
    let dev = Device::new(Duration::ZERO, None, false);
    let gl = GroupLog::new(&dev, Duration::ZERO);
    std::thread::scope(|s| {
        let busy = dev.inner.lock().unwrap();
        for tx in 0..n {
            let gl = &gl;
            s.spawn(move || gl.commit(tx).unwrap());
        }
        while gl.st.lock().unwrap().appended < n {
            std::thread::yield_now();
        }
        drop(busy);
    });
    let (mut durable, fsyncs) = dev.snapshot();
    durable.sort();
    assert_eq!(
        durable,
        (0..n).collect::<Vec<_>>(),
        "every commit is durable"
    );
    assert!(
        (1..=2).contains(&fsyncs),
        "{n} commits that queued behind a busy device need at most 2 fsyncs, got {fsyncs}"
    );
    println!(
        "   device held busy while {n} transactions commit: {fsyncs} fsync(s) made all of them durable"
    );
}

fn group_commit_demo() {
    println!("-- group commit: simulated fsync = 1 ms, 40 commits per thread");
    println!(
        "   {:<22}{:>8}{:>9}{:>8}{:>14}{:>14}",
        "mode", "threads", "commits", "fsyncs", "commits/s", "avg latency ms"
    );
    let cost = Duration::from_millis(1);
    let (mut base8, mut group8) = (0.0, 0.0);
    for threads in [1u64, 2, 4, 8, 16] {
        for (name, mode) in [
            ("fsync per commit", Mode::PerCommit),
            ("group commit", Mode::Group(Duration::ZERO)),
            (
                "group + 300us delay",
                Mode::Group(Duration::from_micros(300)),
            ),
        ] {
            let r = run_commits(mode, threads, 40, cost, None, false);
            let n = r.acked.len() as f64;
            let tput = n / r.elapsed.as_secs_f64();
            println!(
                "   {name:<22}{threads:>8}{:>9}{:>8}{tput:>14.0}{:>14.2}",
                r.acked.len(),
                r.fsyncs,
                r.total_latency.as_secs_f64() * 1000.0 / n
            );
            assert_eq!(r.acked.len() as u64, threads * 40);
            assert_eq!(
                r.acked.iter().collect::<HashSet<_>>(),
                r.durable.iter().collect::<HashSet<_>>(),
                "without crashes everything acked is durable"
            );
            if threads == 8 && mode == Mode::PerCommit {
                base8 = tput;
                assert_eq!(r.fsyncs, 320);
            }
            if threads == 8 && mode == Mode::Group(Duration::ZERO) {
                group8 = tput;
            }
            if let Mode::Group(_) = mode {
                // How much sharing a free run achieves depends on the scheduler: only the always-true bound is asserted.
                assert!(
                    r.fsyncs <= threads * 40,
                    "a batch never costs more than one fsync per commit"
                );
            }
        }
    }
    // Wall-clock throughput is machine dependent (a loaded box can invert it), so it is printed, never asserted.
    check_batching_deterministically(8);
    println!(
        "   at 8 threads: group commit {:.1}x the throughput of one fsync per commit",
        group8 / base8
    );
}

fn crash_injection() {
    println!("-- injected device crashes: every ACKNOWLEDGED commit must be durable");
    let (mut runs, mut crashed_runs, mut lost_unacked, mut total_acked) = (0, 0, 0, 0);
    for keep in [false, true] {
        for crash_at in 1..=8u64 {
            for mode in [Mode::PerCommit, Mode::Group(Duration::ZERO)] {
                let r = run_commits(mode, 4, 8, Duration::from_micros(150), Some(crash_at), keep);
                let durable: HashSet<u64> = r.durable.iter().copied().collect();
                assert!(
                    r.acked.iter().all(|a| durable.contains(a)),
                    "acked commit missing from the durable log (keep={keep}, crash at fsync {crash_at})"
                );
                assert_eq!(durable.len(), r.durable.len(), "no record written twice");
                runs += 1;
                crashed_runs += (r.acked.len() < 32) as u32;
                lost_unacked += r.durable.len() - r.acked.len();
                total_acked += r.acked.len();
            }
        }
    }
    println!(
        "   {runs} runs ({crashed_runs} lost commits to the crash), {total_acked} acknowledged commits all durable, {lost_unacked} durable-but-unacknowledged (allowed: the client just retries or checks)"
    );
    assert!(crashed_runs > 0 && lost_unacked > 0);
}

/// Deterministic model of asynchronous commit: acknowledge immediately, a background flusher persists every few commits.
fn async_commit_demo() {
    println!("-- asynchronous commit: acknowledged before durable");
    let mut durable: Vec<u32> = vec![];
    let mut volatile: Vec<u32> = vec![];
    let mut acked: Vec<u32> = vec![];
    for tx in 1..=10 {
        volatile.push(tx);
        acked.push(tx); // ack right away
        if tx == 6 {
            durable.append(&mut volatile); // the periodic flusher ran once, after commit 6
        }
    }
    // crash now: volatile is lost
    let lost: Vec<u32> = acked
        .iter()
        .copied()
        .filter(|t| !durable.contains(t))
        .collect();
    println!(
        "   acknowledged {}, durable {}, lost after a crash = {:?} (durability window; the database stays consistent, a PREFIX survives)",
        acked.len(),
        durable.len(),
        lost
    );
    assert_eq!(lost, vec![7, 8, 9, 10]);
    assert!(
        durable.iter().zip(1..).all(|(&d, i)| d == i),
        "what survives is a prefix of the commit order"
    );
    println!(
        "   synchronous / group commit: window = 0 by construction (see the crash injection above)"
    );
}

// ------------------------------------------------------------------------------------------------------------------
// Part 3: log buffer ring with atomic LSN allocation
// ------------------------------------------------------------------------------------------------------------------

struct Ring {
    words: Vec<AtomicU64>,
    next_lsn: AtomicU64, // next free word offset = next LSN; claimed with fetch_add
    flushed: AtomicU64,  // everything below this LSN has been consumed by the flusher
}

const HDR_SHIFT: u32 = 16;

impl Ring {
    fn new(cap: usize) -> Ring {
        Ring {
            words: (0..cap).map(|_| AtomicU64::new(0)).collect(),
            next_lsn: AtomicU64::new(0),
            flushed: AtomicU64::new(0),
        }
    }

    /// Append a record: [header, payload...]. Returns its LSN. The header is written LAST with Release, so a flusher that
    /// sees the header (Acquire) also sees the payload.
    fn append(&self, payload: &[u64]) -> u64 {
        let len = payload.len() as u64 + 1;
        let lsn = self.next_lsn.fetch_add(len, Ordering::Relaxed); // atomic LSN allocation: no lock on the hot path
        let cap = self.words.len() as u64;
        while lsn + len > self.flushed.load(Ordering::Acquire) + cap {
            std::thread::yield_now(); // ring full: wait for the flusher (back-pressure)
        }
        for (i, &w) in payload.iter().enumerate() {
            self.words[((lsn + 1 + i as u64) % cap) as usize].store(w, Ordering::Relaxed);
        }
        self.words[(lsn % cap) as usize].store(len | ((lsn + 1) << HDR_SHIFT), Ordering::Release);
        lsn
    }

    /// Consume every contiguous completed record from `flushed`; returns the (lsn, payload) pairs.
    fn drain(&self, out: &mut Vec<(u64, Vec<u64>)>) -> usize {
        let cap = self.words.len() as u64;
        let mut at = self.flushed.load(Ordering::Relaxed);
        let start = at;
        loop {
            let h = self.words[(at % cap) as usize].load(Ordering::Acquire);
            if h >> HDR_SHIFT != at + 1 {
                break; // a hole: the writer that owns this LSN has not published yet
            }
            let len = h & ((1 << HDR_SHIFT) - 1);
            let payload = (1..len)
                .map(|i| self.words[((at + i) % cap) as usize].load(Ordering::Relaxed))
                .collect();
            out.push((at, payload));
            at += len;
        }
        self.flushed.store(at, Ordering::Release);
        (at - start) as usize
    }
}

fn ring_demo() {
    const THREADS: u64 = 8;
    const PER: u64 = 5000;
    let ring = Ring::new(1024); // far smaller than the traffic: wraps ~400 times
    let done = AtomicBool::new(false);
    let mut out: Vec<(u64, Vec<u64>)> = Vec::new();
    let start = Instant::now();
    std::thread::scope(|s| {
        let (ring, done) = (&ring, &done);
        let flusher = s.spawn(move || {
            let mut got = Vec::new();
            loop {
                let n = ring.drain(&mut got);
                if n == 0 {
                    if done.load(Ordering::Acquire)
                        && ring.flushed.load(Ordering::Relaxed)
                            == ring.next_lsn.load(Ordering::Relaxed)
                    {
                        break;
                    }
                    std::thread::yield_now();
                }
            }
            got
        });
        let writers: Vec<_> = (0..THREADS)
            .map(|t| {
                s.spawn(move || {
                    for i in 0..PER {
                        let n = 1 + (i % 5) as usize; // variable-size records
                        let payload: Vec<u64> =
                            (0..n as u64).map(|j| (t << 48) | (i << 8) | j).collect();
                        ring.append(&payload);
                    }
                })
            })
            .collect();
        writers.into_iter().for_each(|w| w.join().unwrap());
        done.store(true, Ordering::Release);
        out = flusher.join().unwrap();
    });
    // verify: contiguous LSNs, intact payloads, per-thread order preserved
    let mut expect_lsn = 0;
    let mut last_seq = vec![None::<u64>; THREADS as usize];
    for (lsn, p) in &out {
        assert_eq!(
            *lsn, expect_lsn,
            "records tile the LSN space without gaps or overlap"
        );
        expect_lsn += p.len() as u64 + 1;
        let (t, i) = (p[0] >> 48, (p[0] >> 8) & 0xff_ffff_ffff);
        assert_eq!(p.len() as u64, 1 + i % 5);
        assert!(
            p.iter()
                .enumerate()
                .all(|(j, &w)| w == (t << 48) | (i << 8) | j as u64),
            "payload intact"
        );
        assert!(
            last_seq[t as usize].is_none_or(|l| l + 1 == i),
            "one thread's records stay in order"
        );
        last_seq[t as usize] = Some(i);
    }
    assert_eq!(out.len() as u64, THREADS * PER);
    assert!(last_seq.iter().all(|l| *l == Some(PER - 1)));
    println!(
        "-- log ring: {THREADS} threads x {PER} variable records through a 1024-word ring: {} records, {} words, all intact ({:?})",
        out.len(),
        expect_lsn,
        start.elapsed()
    );
    assert!(expect_lsn > 1024 * 50, "the ring wrapped many times");
}

fn main() {
    logical_hazards();
    checkpoint_points_demo();
    size_and_time();
    crash_tests();
    async_commit_demo();
    group_commit_demo();
    crash_injection();
    ring_demo();
    println!("all logging / group commit checks passed");
}
