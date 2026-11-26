//! RECIPE 412 (database): ARIES crash recovery: WAL with LSNs and CLRs, steal/no-force buffer pool, fuzzy checkpoints, analysis/redo/undo.
//! LEVEL: advanced
//! Use in: database storage engines, durable key-value stores, filesystems with journals; anything that must survive a crash mid-write.   Run: cargo run --release --example aries_recovery
#![allow(dead_code)]
//!
//! A simulated page store: pages live on "disk" (they survive a crash), the buffer pool and the log tail are RAM (lost).
//! * WRITE-AHEAD LOG: every change appends a record with a monotonically increasing LSN BEFORE the page is modified. A record
//!   carries the before AND after image of the slot (physiological logging), and the transaction's previous LSN (`prev`), so a
//!   transaction's records form a backward chain. Each page stores the LSN of the last record applied to it: its PAGE LSN.
//! * STEAL / NO-FORCE: the pool may write a dirty page to disk at ANY time, even holding uncommitted data (STEAL, so the pool
//!   can be small), and a commit only forces the LOG up to the commit record, never the data pages (NO-FORCE, so commit is one
//!   sequential write). Consequences: after a crash disk may hold uncommitted updates (needs UNDO) and miss committed ones
//!   (needs REDO). THE WAL RULE makes this safe: a page may be written only after the log is durable up to its page LSN;
//!   `write_page_checked` refuses otherwise, and every simulated crash image is asserted to obey it.
//! * FUZZY CHECKPOINT: BeginCheckpoint, then an EndCheckpoint record carrying the transaction table (ATT) and dirty page table
//!   (DPT, with each page's recLSN = LSN of the first update since it was last clean). Nothing is flushed and the system keeps
//!   running during it. The master record (durable pointer) names the BeginCheckpoint, set only once EndCheckpoint is durable.
//! * RECOVERY, three passes. (1) ANALYSIS scans forward from the checkpoint rebuilding the ATT and DPT: who was active, who
//!   committed, which pages may be dirty. (2) REDO repeats history from min(recLSN): re-applies every update AND every
//!   compensation record unless the page's LSN says it is already there (loser transactions are redone too: simple and
//!   exact). (3) UNDO rolls the losers back newest-first, writing a COMPENSATION LOG RECORD (CLR) per undone update; a CLR is
//!   redo-only and carries `undo_next`, the next record to undo, so if the system crashes DURING undo (or recovery runs twice)
//!   the CLRs are redone and the undo resumes where it stopped instead of undoing an undo: recovery is idempotent.
//! * The test crashes at EVERY event of 300 seeded random workloads (each log append, log flush, page write, checkpoint step),
//!   in two flavours (only flushed log survives / all appended log survives), and recovers. It also crashes during redo and
//!   undo (twice in a row), recovers again, and compares with an ORACLE: apply exactly the committed transactions' updates.
//!
//! COMMON USES: PostgreSQL / MySQL InnoDB / SQL Server / DB2 recovery, SQLite WAL mode, journaling filesystems (ext4, NTFS),
//! LSM and B-tree key-value engines, any system with a write-ahead log and a page cache.
//! Read: "ARIES: A Transaction Recovery Method Supporting Fine-Granularity Locking" (Mohan et al., 1992), "Database Management
//! Systems" (Ramakrishnan, Gehrke) recovery chapter. https://web.stanford.edu/class/cs345d-01/rl/aries.pdf ,
//! https://www.postgresql.org/docs/current/wal-internals.html
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

type Lsn = u64; // 1-based position in the log; 0 = none
type TxId = u32;
const PAGES: usize = 6;
const SLOTS: usize = 4;

#[derive(Clone, Debug, PartialEq)]
struct Page {
    lsn: Lsn,
    data: [i64; SLOTS],
}

#[derive(Clone, Debug)]
enum Kind {
    Update {
        page: usize,
        slot: usize,
        before: i64,
        after: i64,
    },
    /// compensation log record: redo-only; `after` is the restored value, `undo_next` the next record of this txn to undo
    Clr {
        page: usize,
        slot: usize,
        after: i64,
        undo_next: Lsn,
    },
    Abort,
    Commit,
    End,
    BeginCkpt,
    EndCkpt {
        att: Vec<(TxId, Lsn, bool)>,
        dpt: Vec<(usize, Lsn)>,
    },
}

#[derive(Clone, Debug)]
struct Rec {
    lsn: Lsn,
    txn: TxId,
    prev: Lsn,
    kind: Kind,
}

/// what survives a crash at one instant (plus the full log so the test can choose how much of the tail made it)
#[derive(Clone)]
struct Image {
    disk: Vec<Page>,
    master: Lsn,
    log: Vec<Rec>,
    flushed: usize,
}

#[derive(Clone, Copy)]
struct AttEntry {
    last_lsn: Lsn,
    committed: bool,
}

struct Crashed;
struct WalViolation;

/// crash injector for recovery: fails when `budget` events have happened
struct Tick {
    budget: Option<usize>,
    used: usize,
}
impl Tick {
    fn none() -> Tick {
        Tick {
            budget: None,
            used: 0,
        }
    }
    fn after(n: usize) -> Tick {
        Tick {
            budget: Some(n),
            used: 0,
        }
    }
    fn tick(&mut self) -> Result<(), Crashed> {
        if self.budget == Some(self.used) {
            return Err(Crashed);
        }
        self.used += 1;
        Ok(())
    }
}

#[derive(Clone)]
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

#[derive(Default, Debug, Clone)]
struct RecStats {
    losers: usize,
    redo_from: Lsn,
    redone: usize,
    skipped: usize,
    clrs: usize,
    events: usize,
}

#[derive(Clone)]
struct Engine {
    disk: Vec<Page>,
    master: Lsn,
    log: Vec<Rec>,
    flushed: usize, // number of durable log records
    pool: BTreeMap<usize, Page>,
    att: BTreeMap<TxId, AttEntry>,
    dpt: BTreeMap<usize, Lsn>, // page -> recLSN; a pool page is dirty iff it is here
    rng: Rng,
    images: Option<Vec<Image>>,
}

impl Engine {
    fn new(seed: u64) -> Engine {
        let disk = vec![
            Page {
                lsn: 0,
                data: [0; SLOTS]
            };
            PAGES
        ];
        Engine {
            disk,
            master: 0,
            log: vec![],
            flushed: 0,
            pool: BTreeMap::new(),
            att: BTreeMap::new(),
            dpt: BTreeMap::new(),
            rng: Rng(seed | 1),
            images: None,
        }
    }

    /// the system as restarted after a crash: disk and the durable log prefix only
    fn from_image(img: &Image, durable: usize, seed: u64) -> Engine {
        let mut e = Engine::new(seed);
        e.disk = img.disk.clone();
        e.master = img.master;
        e.log = img.log[..durable].to_vec();
        e.flushed = durable;
        e
    }

    fn snap(&mut self) {
        if self.images.is_some() {
            let img = Image {
                disk: self.disk.clone(),
                master: self.master,
                log: self.log.clone(),
                flushed: self.flushed,
            };
            self.images.as_mut().unwrap().push(img);
        }
    }

    /// lose everything volatile: buffer pool, tables and the unflushed log tail
    fn crash(&mut self) {
        self.log.truncate(self.flushed);
        self.pool.clear();
        self.att.clear();
        self.dpt.clear();
    }

    // ---- log ----
    fn append(&mut self, txn: TxId, prev: Lsn, kind: Kind) -> Lsn {
        let lsn = self.log.len() as Lsn + 1;
        self.log.push(Rec {
            lsn,
            txn,
            prev,
            kind,
        });
        self.snap();
        lsn
    }

    fn flush_log(&mut self, upto: Lsn) {
        self.flushed = self.flushed.max((upto as usize).min(self.log.len()));
        self.snap();
    }

    // ---- buffer pool ----
    fn fetch(&mut self, p: usize) {
        let disk = &self.disk;
        self.pool.entry(p).or_insert_with(|| disk[p].clone());
    }

    /// change a slot in the pool; the caller has ALREADY logged it (write-ahead)
    fn apply(&mut self, p: usize, slot: usize, val: i64, lsn: Lsn) {
        self.fetch(p);
        let pg = self.pool.get_mut(&p).unwrap();
        pg.data[slot] = val;
        pg.lsn = lsn;
        self.dpt.entry(p).or_insert(lsn); // recLSN: first change since the page was last clean
    }

    fn write_page_checked(&mut self, p: usize) -> Result<(), WalViolation> {
        let pg = self.pool[&p].clone();
        if self.flushed < pg.lsn as usize {
            return Err(WalViolation); // the log describing this page state is not durable yet
        }
        self.disk[p] = pg;
        self.dpt.remove(&p);
        self.snap();
        Ok(())
    }

    /// STEAL: write a dirty page now, whoever's data it holds. The WAL rule is satisfied by forcing the log first.
    fn flush_page(&mut self, p: usize) {
        if self.dpt.contains_key(&p) {
            let lsn = self.pool[&p].lsn;
            self.flush_log(lsn);
            assert!(self.write_page_checked(p).is_ok());
        }
    }

    fn evict(&mut self, p: usize) {
        self.flush_page(p);
        self.pool.remove(&p);
    }

    fn maybe_steal(&mut self) {
        if self.rng.below(3) == 0 {
            let dirty: Vec<usize> = self.dpt.keys().copied().collect();
            if !dirty.is_empty() {
                let p = dirty[self.rng.below(dirty.len())];
                self.flush_page(p);
            }
        }
    }

    // ---- normal operation ----
    fn update(&mut self, t: TxId, p: usize, slot: usize, val: i64) {
        self.fetch(p);
        let before = self.pool[&p].data[slot];
        let prev = self.att.get(&t).map_or(0, |a| a.last_lsn);
        let lsn = self.append(
            t,
            prev,
            Kind::Update {
                page: p,
                slot,
                before,
                after: val,
            },
        );
        self.apply(p, slot, val, lsn);
        self.att.insert(
            t,
            AttEntry {
                last_lsn: lsn,
                committed: false,
            },
        );
    }

    /// NO-FORCE commit: force only the log. The data pages stay in the pool.
    fn commit(&mut self, t: TxId) {
        let prev = self.att[&t].last_lsn;
        let c = self.append(t, prev, Kind::Commit);
        self.flush_log(c);
        self.append(t, c, Kind::End);
        self.att.remove(&t);
    }

    fn rollback(&mut self, t: TxId) {
        let prev = self.att[&t].last_lsn;
        let a = self.append(t, prev, Kind::Abort);
        self.att.get_mut(&t).unwrap().last_lsn = a;
        self.undo_pass(BTreeSet::from([a]), &mut Tick::none()).ok();
    }

    fn begin_checkpoint(&mut self) -> Lsn {
        self.append(0, 0, Kind::BeginCkpt)
    }

    /// the tables are copied NOW, while updates may have happened since BeginCheckpoint: that is what makes it fuzzy
    fn end_checkpoint(&mut self, begin: Lsn) {
        let att = self
            .att
            .iter()
            .map(|(&t, a)| (t, a.last_lsn, a.committed))
            .collect();
        let dpt = self.dpt.iter().map(|(&p, &l)| (p, l)).collect();
        let e = self.append(0, 0, Kind::EndCkpt { att, dpt });
        self.flush_log(e);
        self.master = begin; // only after EndCheckpoint is durable
        self.snap();
    }

    // ---- recovery ----
    /// roll back the chains starting at `to_undo`, newest record first across all losers, writing CLRs
    fn undo_pass(&mut self, mut to_undo: BTreeSet<Lsn>, tick: &mut Tick) -> Result<usize, Crashed> {
        let mut clrs = 0;
        while let Some(&lsn) = to_undo.iter().next_back() {
            to_undo.remove(&lsn);
            let rec = self.log[lsn as usize - 1].clone();
            let next = match rec.kind {
                Kind::Update {
                    page, slot, before, ..
                } => {
                    tick.tick()?;
                    let prev = self.att[&rec.txn].last_lsn;
                    let clr = self.append(
                        rec.txn,
                        prev,
                        Kind::Clr {
                            page,
                            slot,
                            after: before,
                            undo_next: rec.prev,
                        },
                    );
                    self.apply(page, slot, before, clr);
                    self.att.get_mut(&rec.txn).unwrap().last_lsn = clr;
                    clrs += 1;
                    self.maybe_steal();
                    rec.prev
                }
                Kind::Clr { undo_next, .. } => undo_next, // never undo an undo: jump over what was already compensated
                _ => rec.prev,
            };
            if next == 0 {
                let last = self.att[&rec.txn].last_lsn;
                self.append(rec.txn, last, Kind::End);
                self.att.remove(&rec.txn);
            } else {
                to_undo.insert(next);
            }
        }
        Ok(clrs)
    }

    fn analysis(&mut self) -> (BTreeMap<usize, Lsn>, Lsn) {
        let start = if self.master == 0 { 1 } else { self.master };
        let mut att: BTreeMap<TxId, AttEntry> = BTreeMap::new();
        let mut dpt: BTreeMap<usize, Lsn> = BTreeMap::new();
        for rec in self.log[start as usize - 1..].iter() {
            match &rec.kind {
                Kind::Update { page, .. } | Kind::Clr { page, .. } => {
                    att.insert(
                        rec.txn,
                        AttEntry {
                            last_lsn: rec.lsn,
                            committed: false,
                        },
                    );
                    dpt.entry(*page).or_insert(rec.lsn);
                }
                Kind::Abort => {
                    att.insert(
                        rec.txn,
                        AttEntry {
                            last_lsn: rec.lsn,
                            committed: false,
                        },
                    );
                }
                Kind::Commit => {
                    att.insert(
                        rec.txn,
                        AttEntry {
                            last_lsn: rec.lsn,
                            committed: true,
                        },
                    );
                }
                Kind::End => {
                    att.remove(&rec.txn);
                }
                Kind::BeginCkpt => {}
                Kind::EndCkpt { att: a, dpt: d } => {
                    for &(t, last, committed) in a {
                        let e = att.entry(t).or_insert(AttEntry {
                            last_lsn: last,
                            committed,
                        });
                        e.last_lsn = e.last_lsn.max(last);
                        e.committed |= committed;
                    }
                    for &(p, rl) in d {
                        let e = dpt.entry(p).or_insert(rl);
                        *e = (*e).min(rl); // the checkpoint knows the page was dirty earlier than the scan saw
                    }
                }
            }
        }
        // committed but no End record yet: just finish them, they are winners
        let done: Vec<TxId> = att
            .iter()
            .filter(|(_, a)| a.committed)
            .map(|(&t, _)| t)
            .collect();
        self.att = att;
        for t in done {
            let last = self.att[&t].last_lsn;
            self.append(t, last, Kind::End);
            self.att.remove(&t);
        }
        self.dpt.clear(); // the live DPT is rebuilt by redo; the analysis DPT is only a hint
        (dpt, start)
    }

    fn recover(&mut self, tick: &mut Tick) -> Result<RecStats, Crashed> {
        let mut st = RecStats::default();
        let (redo_dpt, _) = self.analysis();
        st.losers = self.att.len();
        let end = self.log.len();
        // REDO: repeat history, skip what the page already has
        if let Some(&from) = redo_dpt.values().min() {
            st.redo_from = from;
            for i in from as usize - 1..end {
                let rec = self.log[i].clone();
                let (page, slot, after) = match rec.kind {
                    Kind::Update {
                        page, slot, after, ..
                    }
                    | Kind::Clr {
                        page, slot, after, ..
                    } => (page, slot, after),
                    _ => continue,
                };
                if redo_dpt.get(&page).is_none_or(|&rl| rec.lsn < rl) {
                    st.skipped += 1; // page was clean before this record: it is certainly on disk
                    continue;
                }
                self.fetch(page);
                if self.pool[&page].lsn >= rec.lsn {
                    st.skipped += 1; // pageLSN says the effect is already there
                    continue;
                }
                tick.tick()?;
                self.apply(page, slot, after, rec.lsn);
                st.redone += 1;
                self.maybe_steal();
            }
        }
        // UNDO losers
        let to_undo: BTreeSet<Lsn> = self.att.values().map(|a| a.last_lsn).collect();
        st.clrs = self.undo_pass(to_undo, tick)?;
        self.flush_log(self.log.len() as Lsn);
        st.events = tick.used;
        Ok(st)
    }

    fn state(&self) -> Vec<i64> {
        (0..PAGES)
            .flat_map(|p| self.pool.get(&p).unwrap_or(&self.disk[p]).data)
            .collect()
    }
}

/// the truth: apply exactly the committed transactions' updates, in log order
fn oracle(log: &[Rec]) -> Vec<i64> {
    let winners: HashSet<TxId> = log
        .iter()
        .filter(|r| matches!(r.kind, Kind::Commit))
        .map(|r| r.txn)
        .collect();
    let mut st = vec![0; PAGES * SLOTS];
    for r in log {
        if let Kind::Update {
            page, slot, after, ..
        } = r.kind
        {
            if winners.contains(&r.txn) {
                st[page * SLOTS + slot] = after;
            }
        }
    }
    st
}

// ---- random workloads ---------------------------------------------------------------------------------------------------
fn run_workload(seed: u64) -> Engine {
    let mut e = Engine::new(seed);
    e.images = Some(vec![]);
    let mut r = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut active: Vec<TxId> = vec![];
    let mut owner: HashMap<(usize, usize), TxId> = HashMap::new(); // slot locks: strict 2PL stand-in
    let (mut next_txn, mut ckpt) = (1, None);
    let release = |owner: &mut HashMap<(usize, usize), TxId>, t: TxId| owner.retain(|_, o| *o != t);
    for _ in 0..45 {
        match r.below(100) {
            0..=49 => {
                if active.is_empty() || (active.len() < 4 && r.below(4) == 0) {
                    active.push(next_txn);
                    next_txn += 1;
                }
                let t = active[r.below(active.len())];
                let (p, s) = (r.below(PAGES), r.below(SLOTS));
                if owner.get(&(p, s)).is_none_or(|&o| o == t) {
                    owner.insert((p, s), t);
                    e.update(t, p, s, 1 + r.below(99) as i64);
                }
            }
            50..=62 => {
                let started: Vec<TxId> = active
                    .iter()
                    .copied()
                    .filter(|t| e.att.contains_key(t))
                    .collect();
                if !started.is_empty() {
                    let t = started[r.below(started.len())];
                    e.commit(t);
                    active.retain(|&a| a != t);
                    release(&mut owner, t);
                }
            }
            63..=69 => {
                let started: Vec<TxId> = active
                    .iter()
                    .copied()
                    .filter(|t| e.att.contains_key(t))
                    .collect();
                if !started.is_empty() {
                    let t = started[r.below(started.len())];
                    e.rollback(t);
                    active.retain(|&a| a != t);
                    release(&mut owner, t);
                }
            }
            70..=79 => e.flush_page(r.below(PAGES)),
            80..=84 => e.evict(r.below(PAGES)),
            85..=89 => {
                let upto = r.below(e.log.len() + 1) as Lsn;
                e.flush_log(upto);
            }
            90..=94 => {
                if ckpt.is_none() {
                    ckpt = Some(e.begin_checkpoint());
                }
            }
            _ => {
                if let Some(b) = ckpt.take() {
                    e.end_checkpoint(b);
                }
            }
        }
    }
    e
}

#[derive(Default)]
struct Totals {
    workloads: usize,
    images: usize,
    full_recoveries: usize,
    crashed_recoveries: usize,
    double_crashes: usize,
    redone: usize,
    skipped: usize,
    clrs: usize,
    losers: usize,
    max_events: usize,
}

fn check_image(img: &Image, seed: u64, t: &mut Totals) {
    // the WAL rule, observed from outside: no page on disk is newer than the durable log
    assert!(
        img.disk.iter().all(|p| p.lsn as usize <= img.flushed),
        "WAL rule: a page on disk is ahead of the durable log"
    );
    for durable in [img.flushed, img.log.len()] {
        let want = oracle(&img.log[..durable]);
        t.images += 1;
        // 1. plain recovery, then again after another crash: same answer, and the second one has no undo work
        let mut e = Engine::from_image(img, durable, seed);
        let st = e
            .recover(&mut Tick::none())
            .ok()
            .expect("no crash injected");
        assert_eq!(
            e.state(),
            want,
            "seed {seed}: recovered state differs from the committed-only oracle"
        );
        let (state1, log_len) = (e.state(), e.log.len());
        e.crash();
        let st2 = e.recover(&mut Tick::none()).ok().unwrap();
        assert_eq!(e.state(), state1, "recovery must be idempotent");
        assert_eq!(
            (st2.clrs, st2.losers, e.log.len()),
            (0, 0, log_len),
            "second recovery has nothing to undo or log"
        );
        t.full_recoveries += 2;
        (t.redone, t.skipped, t.clrs, t.losers, t.max_events) = (
            t.redone + st.redone,
            t.skipped + st.skipped,
            t.clrs + st.clrs,
            t.losers + st.losers,
            t.max_events.max(st.events),
        );
        // 2. crash at every event of recovery (redo applications and CLR writes); then crash AGAIN during the retry
        for b in 0..st.events {
            let mut e = Engine::from_image(img, durable, seed);
            assert!(e.recover(&mut Tick::after(b)).is_err());
            e.crash();
            t.crashed_recoveries += 1;
            if b % 2 == 1 {
                if e.recover(&mut Tick::after(b / 2)).is_err() {
                    e.crash();
                    t.double_crashes += 1;
                }
            }
            e.recover(&mut Tick::none()).ok().unwrap();
            assert_eq!(
                e.state(),
                want,
                "seed {seed}: crash during recovery (event {b}) broke the final state"
            );
        }
    }
}

// ---- demo --------------------------------------------------------------------------------------------------------------------
fn fmt_rec(r: &Rec) -> String {
    let body = match &r.kind {
        Kind::Update {
            page,
            slot,
            before,
            after,
        } => format!("UPDATE p{page}.{slot}: {before} -> {after}"),
        Kind::Clr {
            page,
            slot,
            after,
            undo_next,
        } => format!("CLR    p{page}.{slot} := {after}   undoNext={undo_next}"),
        Kind::Abort => "ABORT".into(),
        Kind::Commit => "COMMIT".into(),
        Kind::End => "END".into(),
        Kind::BeginCkpt => "BEGIN_CHECKPOINT".into(),
        Kind::EndCkpt { att, dpt } => format!(
            "END_CHECKPOINT att={:?} dpt={:?}",
            att.iter().map(|a| (a.0, a.1)).collect::<Vec<_>>(),
            dpt
        ),
    };
    let who = if r.txn == 0 {
        "  ".to_string()
    } else {
        format!("T{}", r.txn)
    };
    format!("{:>3} {who} prev={:<3} {body}", r.lsn, r.prev)
}

fn demo() {
    println!(
        "-- a hand-built crash: steal of uncommitted data, a fuzzy checkpoint, a committed txn whose pages never reached disk"
    );
    let mut e = Engine::new(1);
    e.update(1, 0, 0, 10); //  T1 will be a loser
    e.update(2, 1, 0, 20); //  T2 will commit
    e.update(1, 0, 1, 11);
    e.flush_page(0); // STEAL: page 0 holds uncommitted T1 data on disk (the log was forced first)
    let b = e.begin_checkpoint();
    e.update(2, 1, 1, 21); // activity DURING the checkpoint: fuzzy
    e.end_checkpoint(b);
    e.commit(2); // NO-FORCE: page 1 never written, only the log
    e.update(3, 2, 0, 30); // T3 loser
    e.update(1, 3, 0, 12);
    e.flush_log(e.log.len() as Lsn);
    e.crash();
    for r in &e.log {
        println!("   {}", fmt_rec(r));
    }
    println!(
        "   on disk: page0 lsn={} {:?}; page1 lsn={} {:?}  (page1 is stale: committed but never written)",
        e.disk[0].lsn, e.disk[0].data, e.disk[1].lsn, e.disk[1].data
    );
    assert_eq!(
        e.disk[0].data[0], 10,
        "uncommitted data reached disk (steal)"
    );
    assert_eq!(
        e.disk[1].data[0], 0,
        "committed data did not reach disk (no-force)"
    );
    let want = oracle(&e.log);
    let st = e.recover(&mut Tick::none()).ok().unwrap();
    println!(
        "   analysis: losers={} (T1, T3); redo from LSN {}; redone {} skipped {} (pageLSN already covers it); undo wrote {} CLRs",
        st.losers, st.redo_from, st.redone, st.skipped, st.clrs
    );
    for r in &e.log[e.log.len() - st.clrs - 3..] {
        println!("   {}", fmt_rec(r));
    }
    assert_eq!(e.state(), want);
    assert_eq!((st.losers, st.clrs), (2, 4));
    assert!(st.skipped > 0 && st.redone > 0);
    let s = e.state();
    println!(
        "   recovered slots (page.slot): p1.0={} p1.1={} (T2 winner), p0.0={} p0.1={} p2.0={} p3.0={} (losers undone)",
        s[SLOTS],
        s[SLOTS + 1],
        s[0],
        s[1],
        s[2 * SLOTS],
        s[3 * SLOTS]
    );
    assert_eq!(
        (
            s[SLOTS],
            s[SLOTS + 1],
            s[0],
            s[1],
            s[2 * SLOTS],
            s[3 * SLOTS]
        ),
        (20, 21, 0, 0, 0, 0)
    );
}

fn demo_wal_rule() {
    println!("-- the WAL rule: a page must not reach disk before its log record");
    let mut e = Engine::new(2);
    e.update(1, 0, 0, 5);
    assert!(
        e.write_page_checked(0).is_err(),
        "log not durable yet: the write is refused"
    );
    e.flush_page(0); // forces the log first
    assert!(e.flushed >= 1 && e.disk[0].data[0] == 5);
    println!(
        "   page write before its log was durable was refused; after forcing the log the page write succeeded (disk page lsn {})",
        e.disk[0].lsn
    );
}

fn main() {
    demo();
    demo_wal_rule();
    println!("-- crash at every event of 300 random workloads, plus crashes inside recovery");
    let mut t = Totals::default();
    let start = std::time::Instant::now();
    for seed in 1..=300u64 {
        let e = run_workload(seed);
        for img in e.images.as_ref().unwrap() {
            check_image(img, seed, &mut t);
        }
        t.workloads += 1;
    }
    println!(
        "   workloads {}, crash images {}, recoveries to completion {}, recoveries crashed midway {} ({} crashed twice)",
        t.workloads, t.images, t.full_recoveries, t.crashed_recoveries, t.double_crashes
    );
    println!(
        "   redo applied {} / skipped by pageLSN or DPT {}, CLRs written {}, losers rolled back {}, longest recovery {} events ({:?})",
        t.redone,
        t.skipped,
        t.clrs,
        t.losers,
        t.max_events,
        start.elapsed()
    );
    assert!(t.skipped > 0 && t.redone > 0 && t.clrs > 0 && t.double_crashes > 0);
    println!("all ARIES checks passed");
}
