//! RECIPE 732 (database): MVCC implementation choices: version storage (append-only O2N/N2O, time-travel, delta), garbage collection, index pointers.
//! LEVEL: advanced
//! Use in: choosing or building the storage layer of an MVCC engine (PostgreSQL vs Oracle/MySQL vs Hekaton vs HyPer styles).   Run: cargo run --release --example mvcc_version_storage_gc
#![allow(dead_code)]
//!
//! Wu et al. ("An Empirical Evaluation of In-Memory Multi-Version Concurrency Control") split an MVCC engine into four
//! independent design decisions. This recipe implements each decision as a swappable part and runs the SAME random
//! histories through every combination, comparing every read with a trivial oracle.
//! * CONCURRENCY CONTROL (2 variants): MVTO, multi-version timestamp ordering. A transaction's timestamp is its version stamp;
//!   writes create uncommitted versions in place (`begin = Tx(id)`), a write aborts if a younger transaction already read the
//!   newest version (`rts`) or a newer version exists; a reader that meets another transaction's uncommitted version in its
//!   past aborts (no-wait). MVOCC-lite: snapshot reads at the last commit timestamp, writes buffered privately, first-committer-
//!   wins validation at commit, then install with a fresh commit timestamp.
//! * COMMIT-TIMESTAMP LOOKUP: version `begin`/`end` fields hold either a timestamp or `Tx(id)`. A reader resolves `Tx(id)`
//!   through the TRANSACTION TABLE (id -> timestamp + Active/Committed/Aborted), so commit is O(1) and needs no pass over
//!   the written versions (Hekaton does this and post-processes lazily; here garbage collection rewrites the stamps).
//!   VISIBILITY: a version is visible to timestamp t if begin <= t < end after resolution; own uncommitted versions are always
//!   visible to their writer (READ-YOUR-WRITES).
//! * VERSION STORAGE (4 variants, rows have 8 columns, updates change one):
//!     APPEND-ONLY O2N  all versions in one chain, oldest first: new versions are cheap to add, current reads walk the chain;
//!     APPEND-ONLY N2O  newest first (PostgreSQL-like order for current reads), old snapshots walk far;
//!     TIME-TRAVEL      main table keeps the newest full version, older ones are copied to a separate version table;
//!     DELTA            main table keeps the newest full row, older versions are UNDO RECORDS (changed columns only) that a
//!                      reader applies backwards to reconstruct an old row (Oracle undo, MySQL rollback segment). Smallest space,
//!                      costly old reads.
//! * GARBAGE COLLECTION: a version is dead when its `end` committed at or before the WATERMARK = oldest active snapshot (nobody
//!   can ever see it). COOPERATIVE tuple-level: a reader unlinks dead versions it walks past (HANA, Hekaton). BACKGROUND VACUUM:
//!   periodically scan every key (PostgreSQL autovacuum; cost proportional to the whole table). EPOCH / transaction-level: a
//!   committing transaction queues the keys it superseded and they are cleaned once the watermark passes its timestamp, visiting
//!   only touched keys. One long-lived snapshot pins the watermark and every strategy bloats equally: the real fix is shorter
//!   transactions.
//! * INDEXES: PHYSICAL pointers (secondary index entry = address of a version) force every update that moves the newest version
//!   (append-only) to touch ALL secondary indexes; LOGICAL pointers (entry = primary key) leave them alone when no indexed
//!   column changed, at the price of an extra primary-index probe per lookup. The recipe verifies a logical secondary index
//!   (a superset of entries, filtered by visibility) against a scan and counts maintenance writes for both.
//! * Checks: 1000 random histories x 2 CC x 4 storages x 4 GC modes (32000 runs) all return exactly the oracle's reads and all
//!   storages produce identical outcome traces; before and after every GC pass every active transaction's reads are unchanged
//!   (GC never removes a visible version); with no active transaction a full vacuum leaves exactly one version per key.
//!
//! COMMON USES: PostgreSQL (append-only, vacuum), Oracle / MySQL InnoDB (delta undo), MS SQL Server tempdb version store
//! (time-travel style), Hekaton / HyPer / Peloton (in-memory), SAP HANA; any store that keeps old versions for readers.
//! Course: CMU 15-445 Lecture 20 (multi-version concurrency control) and CMU 15-721 (MVCC / Hekaton lectures).
//! Read: "An Empirical Evaluation of In-Memory Multi-Version Concurrency Control" (Wu et al., VLDB 2017) https://www.vldb.org/pvldb/vol10/p781-Wu.pdf ,
//! "High-Performance Concurrency Control Mechanisms for Main-Memory Databases" (Larson et al., Hekaton) https://www.vldb.org/pvldb/vol5/p298_per-akelarson_vldb2012.pdf
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

type Ts = u64;
const COLS: usize = 8;
type Row = [i64; COLS];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Stamp {
    Inf,
    Ts(Ts),
    Tx(usize),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TxState {
    Active,
    Committed,
    Aborted,
}

#[derive(Clone, Copy)]
struct Entry {
    ts: Ts,
    state: TxState,
}
type TxTable = HashMap<usize, Entry>;

fn resolve(tt: &TxTable, s: Stamp) -> Option<(Ts, TxState)> {
    match s {
        Stamp::Inf => None,
        Stamp::Ts(t) => Some((t, TxState::Committed)),
        Stamp::Tx(id) => tt.get(&id).map(|e| (e.ts, e.state)),
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Vis {
    Yes,
    No,
    Conflict,
}

/// begin <= ts < end after resolving transaction ids; uncommitted data of others in our past is a Conflict (we would have to wait).
fn visible(tt: &TxTable, me: usize, ts: Ts, begin: Stamp, end: Stamp) -> Vis {
    if begin != Stamp::Tx(me) {
        match resolve(tt, begin) {
            Some((b, TxState::Committed)) if b <= ts => {}
            Some((b, TxState::Active)) if b <= ts => return Vis::Conflict,
            _ => return Vis::No,
        }
    }
    match end {
        Stamp::Inf => Vis::Yes,
        Stamp::Tx(id) if id == me => Vis::No,
        _ => match resolve(tt, end) {
            Some((e, TxState::Committed)) => {
                if e > ts {
                    Vis::Yes
                } else {
                    Vis::No
                }
            }
            Some((e, TxState::Active)) => {
                if e > ts {
                    Vis::Yes
                } else {
                    Vis::Conflict
                }
            }
            _ => Vis::Yes,
        },
    }
}

#[derive(Clone)]
struct Ver {
    begin: Stamp,
    end: Stamp,
    row: Row,
}

struct Lookup {
    row: Option<Row>,
    end: Stamp,
    conflict: bool,
    steps: usize,
}

type VisFn<'a> = &'a mut dyn FnMut(Stamp, Stamp) -> Vis;
type DeadFn<'a> = &'a dyn Fn(Stamp) -> bool;
type FixFn<'a> = &'a dyn Fn(Stamp) -> Stamp;

/// A version storage scheme. Everything that differs between schemes is behind this trait.
trait Storage {
    fn name(&self) -> &'static str;
    fn load(&mut self, k: usize, row: Row);
    /// Find the version visible under `vis`, counting every version (or delta) examined.
    fn lookup(&mut self, k: usize, vis: VisFn) -> Lookup;
    fn newest(&self, k: usize) -> (Stamp, Row);
    /// Add a new newest version created by transaction `by`; the predecessor's `end` becomes `Tx(by)`.
    fn install(&mut self, k: usize, row: Row, by: usize);
    fn overwrite_newest(&mut self, k: usize, row: Row);
    /// Undo the newest version (abort): the predecessor becomes current again.
    fn rollback(&mut self, k: usize);
    /// Remove dead versions of one key and normalise stamps. Returns (examined, removed).
    fn clean_key(&mut self, k: usize, dead: DeadFn, fix: FixFn) -> (usize, usize);
    fn bytes(&self) -> usize;
    fn versions(&self) -> usize;
}

// Sizes: stamps are 8 bytes (tag in the top bit), pointer 8, a row 64.
const VER_BYTES: usize = 64 + 8 + 8 + 8;
const DELTA_BASE: usize = 8 + 8 + 8;
const DELTA_COL: usize = 16;

fn fix_ver(v: &mut Ver, fix: FixFn) {
    v.begin = fix(v.begin);
    v.end = fix(v.end);
}

fn scan_chain<'a>(it: impl Iterator<Item = &'a Ver>, vis: VisFn) -> Lookup {
    let mut l = Lookup {
        row: None,
        end: Stamp::Inf,
        conflict: false,
        steps: 0,
    };
    for v in it {
        l.steps += 1;
        match vis(v.begin, v.end) {
            Vis::Yes => {
                l.row = Some(v.row);
                l.end = v.end;
                return l;
            }
            Vis::Conflict => {
                l.conflict = true;
                return l;
            }
            Vis::No => {}
        }
    }
    l
}

/// All versions of a key in one deque. `newest_first` decides the direction; lookups always scan front to back.
struct Chain {
    newest_first: bool,
    chains: Vec<VecDeque<Ver>>,
}

impl Chain {
    fn newest_idx(&self, k: usize) -> usize {
        if self.newest_first {
            0
        } else {
            self.chains[k].len() - 1
        }
    }
}

impl Storage for Chain {
    fn name(&self) -> &'static str {
        if self.newest_first {
            "append N2O"
        } else {
            "append O2N"
        }
    }
    fn load(&mut self, k: usize, row: Row) {
        while self.chains.len() <= k {
            self.chains.push(VecDeque::new());
        }
        self.chains[k].push_back(Ver {
            begin: Stamp::Ts(0),
            end: Stamp::Inf,
            row,
        });
    }
    fn lookup(&mut self, k: usize, vis: VisFn) -> Lookup {
        scan_chain(self.chains[k].iter(), vis)
    }
    fn newest(&self, k: usize) -> (Stamp, Row) {
        let v = &self.chains[k][self.newest_idx(k)];
        (v.begin, v.row)
    }
    fn install(&mut self, k: usize, row: Row, by: usize) {
        let i = self.newest_idx(k);
        self.chains[k][i].end = Stamp::Tx(by);
        let v = Ver {
            begin: Stamp::Tx(by),
            end: Stamp::Inf,
            row,
        };
        if self.newest_first {
            self.chains[k].push_front(v)
        } else {
            self.chains[k].push_back(v)
        }
    }
    fn overwrite_newest(&mut self, k: usize, row: Row) {
        let i = self.newest_idx(k);
        self.chains[k][i].row = row;
    }
    fn rollback(&mut self, k: usize) {
        if self.newest_first {
            self.chains[k].pop_front()
        } else {
            self.chains[k].pop_back()
        };
        let i = self.newest_idx(k);
        self.chains[k][i].end = Stamp::Inf;
    }
    fn clean_key(&mut self, k: usize, dead: DeadFn, fix: FixFn) -> (usize, usize) {
        let before = self.chains[k].len();
        self.chains[k].retain(|v| v.end == Stamp::Inf || !dead(v.end));
        self.chains[k].iter_mut().for_each(|v| fix_ver(v, fix));
        (before, before - self.chains[k].len())
    }
    fn bytes(&self) -> usize {
        self.versions() * VER_BYTES
    }
    fn versions(&self) -> usize {
        self.chains.iter().map(|c| c.len()).sum()
    }
}

/// Newest version in the main table, older ones in a separate version table (oldest first, scanned from the back).
struct TimeTravel {
    main: Vec<Ver>,
    hist: Vec<Vec<Ver>>,
}

impl Storage for TimeTravel {
    fn name(&self) -> &'static str {
        "time-travel"
    }
    fn load(&mut self, k: usize, row: Row) {
        while self.main.len() <= k {
            self.main.push(Ver {
                begin: Stamp::Ts(0),
                end: Stamp::Inf,
                row: [0; COLS],
            });
            self.hist.push(Vec::new());
        }
        self.main[k].row = row;
    }
    fn lookup(&mut self, k: usize, vis: VisFn) -> Lookup {
        scan_chain(
            std::iter::once(&self.main[k]).chain(self.hist[k].iter().rev()),
            vis,
        )
    }
    fn newest(&self, k: usize) -> (Stamp, Row) {
        (self.main[k].begin, self.main[k].row)
    }
    fn install(&mut self, k: usize, row: Row, by: usize) {
        let mut old = self.main[k].clone();
        old.end = Stamp::Tx(by);
        self.hist[k].push(old); // copy the old master to the version table
        self.main[k] = Ver {
            begin: Stamp::Tx(by),
            end: Stamp::Inf,
            row,
        };
    }
    fn overwrite_newest(&mut self, k: usize, row: Row) {
        self.main[k].row = row;
    }
    fn rollback(&mut self, k: usize) {
        let mut old = self.hist[k].pop().expect("predecessor");
        old.end = Stamp::Inf;
        self.main[k] = old;
    }
    fn clean_key(&mut self, k: usize, dead: DeadFn, fix: FixFn) -> (usize, usize) {
        let before = self.hist[k].len();
        self.hist[k].retain(|v| !dead(v.end));
        self.hist[k].iter_mut().for_each(|v| fix_ver(v, fix));
        fix_ver(&mut self.main[k], fix);
        (before + 1, before - self.hist[k].len())
    }
    fn bytes(&self) -> usize {
        self.versions() * VER_BYTES
    }
    fn versions(&self) -> usize {
        self.main.len() + self.hist.iter().map(|h| h.len()).sum::<usize>()
    }
}

/// Undo record: the OLD value of every column the update changed, and the stamps of the version it reconstructs.
struct Delta {
    begin: Stamp,
    end: Stamp,
    cols: Vec<(usize, i64)>,
}

struct DeltaStore {
    main: Vec<Ver>,
    deltas: Vec<Vec<Delta>>, // oldest first; reconstruction walks from the back applying undo
}

impl Storage for DeltaStore {
    fn name(&self) -> &'static str {
        "delta"
    }
    fn load(&mut self, k: usize, row: Row) {
        while self.main.len() <= k {
            self.main.push(Ver {
                begin: Stamp::Ts(0),
                end: Stamp::Inf,
                row: [0; COLS],
            });
            self.deltas.push(Vec::new());
        }
        self.main[k].row = row;
    }
    fn lookup(&mut self, k: usize, vis: VisFn) -> Lookup {
        let mut l = Lookup {
            row: None,
            end: Stamp::Inf,
            conflict: false,
            steps: 1,
        };
        let m = &self.main[k];
        match vis(m.begin, m.end) {
            Vis::Yes => {
                l.row = Some(m.row);
                return l;
            }
            Vis::Conflict => {
                l.conflict = true;
                return l;
            }
            Vis::No => {}
        }
        let mut row = m.row;
        for d in self.deltas[k].iter().rev() {
            l.steps += 1;
            d.cols.iter().for_each(|&(c, v)| row[c] = v); // apply the undo record backwards
            match vis(d.begin, d.end) {
                Vis::Yes => {
                    l.row = Some(row);
                    l.end = d.end;
                    return l;
                }
                Vis::Conflict => {
                    l.conflict = true;
                    return l;
                }
                Vis::No => {}
            }
        }
        l
    }
    fn newest(&self, k: usize) -> (Stamp, Row) {
        (self.main[k].begin, self.main[k].row)
    }
    fn install(&mut self, k: usize, row: Row, by: usize) {
        let old = &self.main[k];
        let cols = (0..COLS)
            .filter(|&c| old.row[c] != row[c])
            .map(|c| (c, old.row[c]))
            .collect();
        self.deltas[k].push(Delta {
            begin: old.begin,
            end: Stamp::Tx(by),
            cols,
        });
        self.main[k] = Ver {
            begin: Stamp::Tx(by),
            end: Stamp::Inf,
            row,
        };
    }
    fn overwrite_newest(&mut self, k: usize, row: Row) {
        // keep the undo record complete when the same transaction changes more columns
        let cur = self.main[k].row;
        let d = self.deltas[k]
            .last_mut()
            .expect("own version has an undo record");
        for c in 0..COLS {
            if cur[c] != row[c] && !d.cols.iter().any(|&(cc, _)| cc == c) {
                d.cols.push((c, cur[c]));
            }
        }
        self.main[k].row = row;
    }
    fn rollback(&mut self, k: usize) {
        let d = self.deltas[k].pop().expect("undo record");
        d.cols.iter().for_each(|&(c, v)| self.main[k].row[c] = v);
        self.main[k].begin = d.begin;
        self.main[k].end = Stamp::Inf;
    }
    fn clean_key(&mut self, k: usize, dead: DeadFn, fix: FixFn) -> (usize, usize) {
        let before = self.deltas[k].len();
        // dead undo records form a prefix (ends grow along the chain), and the prefix is all reconstruction can lose
        let n = self.deltas[k]
            .iter()
            .take_while(|d| d.end != Stamp::Inf && dead(d.end))
            .count();
        self.deltas[k].drain(..n);
        for d in &mut self.deltas[k] {
            d.begin = fix(d.begin);
            d.end = fix(d.end);
        }
        fix_ver(&mut self.main[k], fix);
        (before + 1, n)
    }
    fn bytes(&self) -> usize {
        self.main.len() * VER_BYTES
            + self
                .deltas
                .iter()
                .flatten()
                .map(|d| DELTA_BASE + DELTA_COL * d.cols.len())
                .sum::<usize>()
    }
    fn versions(&self) -> usize {
        self.main.len() + self.deltas.iter().map(|d| d.len()).sum::<usize>()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Scheme {
    O2N,
    N2O,
    TimeTravel,
    Delta,
}
const SCHEMES: [Scheme; 4] = [Scheme::O2N, Scheme::N2O, Scheme::TimeTravel, Scheme::Delta];

fn make_store(s: Scheme) -> Box<dyn Storage> {
    match s {
        Scheme::O2N => Box::new(Chain {
            newest_first: false,
            chains: vec![],
        }),
        Scheme::N2O => Box::new(Chain {
            newest_first: true,
            chains: vec![],
        }),
        Scheme::TimeTravel => Box::new(TimeTravel {
            main: vec![],
            hist: vec![],
        }),
        Scheme::Delta => Box::new(DeltaStore {
            main: vec![],
            deltas: vec![],
        }),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Cc {
    Mvto,
    MvoccLite,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Gc {
    Off,
    Cooperative,
    Vacuum,
    Epoch,
}
const GCS: [Gc; 4] = [Gc::Off, Gc::Cooperative, Gc::Vacuum, Gc::Epoch];

#[derive(Clone, Copy, Debug)]
enum Kind {
    Read(usize),
    Write(usize, usize, i64),
    Commit,
    Abort,
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

struct Tx {
    id: usize,
    snap: Ts,
    writes: Vec<usize>,         // MVTO: keys with an installed version
    ws: BTreeMap<usize, Row>,   // MVOCC: private workspace
    pend: BTreeMap<usize, Row>, // what the oracle believes this transaction wrote
}

#[derive(Default)]
struct Stats {
    reads: u64,
    read_steps: u64,
    old_reads: u64,
    old_steps: u64,
    commits: u64,
    aborts: u64,
    installs: u64,
    index_col_changes: u64,
    gc_examined: u64,
    gc_removed: u64,
    peak_bytes: usize,
}

struct Eng {
    cc: Cc,
    gc: Gc,
    store: Box<dyn Storage>,
    tt: TxTable,
    clock: Ts,
    next_id: usize,
    rts: Vec<Ts>,
    slots: Vec<Option<Tx>>,
    oracle: Vec<BTreeMap<Ts, Row>>,
    epoch_q: VecDeque<(Ts, Vec<usize>)>,
    sec: BTreeMap<i64, BTreeSet<usize>>, // logical secondary index on column 0: value -> primary keys (superset)
    paranoid: bool,
    st: Stats,
    trace: Vec<(u8, Row)>,
}

impl Eng {
    fn new(cc: Cc, gc: Gc, scheme: Scheme, keys: usize, slots: usize, paranoid: bool) -> Eng {
        let mut e = Eng {
            cc,
            gc,
            store: make_store(scheme),
            tt: HashMap::new(),
            clock: 0,
            next_id: 1,
            rts: vec![0; keys],
            slots: (0..slots).map(|_| None).collect(),
            oracle: vec![BTreeMap::new(); keys],
            epoch_q: VecDeque::new(),
            sec: BTreeMap::new(),
            paranoid,
            st: Stats::default(),
            trace: Vec::new(),
        };
        for k in 0..keys {
            let mut row = [0i64; COLS];
            row[0] = (k % 6) as i64;
            row[1] = k as i64;
            e.store.load(k, row);
            e.oracle[k].insert(0, row);
            e.sec.entry(row[0]).or_default().insert(k);
        }
        e
    }

    fn watermark(&self) -> Ts {
        self.slots
            .iter()
            .flatten()
            .map(|t| t.snap)
            .min()
            .unwrap_or(self.clock)
    }

    fn begin(&mut self, slot: usize) {
        let id = self.next_id;
        self.next_id += 1;
        let snap = match self.cc {
            Cc::Mvto => {
                self.clock += 1; // MVTO: the start timestamp IS the version stamp
                self.clock
            }
            Cc::MvoccLite => self.clock, // snapshot = last commit timestamp
        };
        self.tt.insert(
            id,
            Entry {
                ts: snap,
                state: TxState::Active,
            },
        );
        self.slots[slot] = Some(Tx {
            id,
            snap,
            writes: vec![],
            ws: BTreeMap::new(),
            pend: BTreeMap::new(),
        });
    }

    fn oracle_read(&self, slot: usize, k: usize) -> Row {
        let tx = self.slots[slot].as_ref().unwrap();
        tx.pend
            .get(&k)
            .copied()
            .unwrap_or_else(|| *self.oracle[k].range(..=tx.snap).next_back().unwrap().1)
    }

    fn read(&mut self, slot: usize, k: usize) -> Result<Row, ()> {
        let tx = self.slots[slot].as_ref().unwrap();
        if let Some(r) = tx.ws.get(&k) {
            return Ok(*r); // read-your-writes from the private workspace
        }
        let (id, snap) = (tx.id, tx.snap);
        let tt = &self.tt;
        let l = self
            .store
            .lookup(k, &mut |b, e| visible(tt, id, snap, b, e));
        self.st.reads += 1;
        self.st.read_steps += l.steps as u64;
        if slot == 0 {
            self.st.old_reads += 1;
            self.st.old_steps += l.steps as u64;
        }
        if l.conflict {
            return Err(());
        }
        let row = l
            .row
            .expect("a visible version must exist (GC must never remove it)");
        if self.cc == Cc::Mvto && l.end == Stamp::Inf {
            self.rts[k] = self.rts[k].max(snap); // the newest version was read at snap: older writers must now abort
        }
        assert_eq!(
            row,
            self.oracle_read(slot, k),
            "read differs from oracle ({:?}, key {k})",
            self.store.name()
        );
        Ok(row)
    }

    fn write(&mut self, slot: usize, k: usize, col: usize, v: i64) -> Result<(), ()> {
        let (id, snap) = {
            let tx = self.slots[slot].as_ref().unwrap();
            (tx.id, tx.snap)
        };
        let mut row;
        match self.cc {
            Cc::Mvto => {
                let (nb, nrow) = self.store.newest(k);
                row = nrow;
                row[col] = v;
                if nb == Stamp::Tx(id) {
                    self.store.overwrite_newest(k, row);
                } else {
                    match resolve(&self.tt, nb) {
                        Some((b, TxState::Committed)) if b <= snap && self.rts[k] <= snap => {}
                        _ => return Err(()), // newer version exists / younger reader saw the old one / uncommitted by someone else
                    }
                    self.st.installs += 1;
                    self.st.index_col_changes += (col == 0 && nrow[0] != v) as u64;
                    self.store.install(k, row, id);
                    self.rts[k] = snap;
                    self.slots[slot].as_mut().unwrap().writes.push(k);
                }
            }
            Cc::MvoccLite => {
                row = self.read(slot, k)?;
                row[col] = v;
                self.slots[slot].as_mut().unwrap().ws.insert(k, row);
            }
        }
        if col == 0 {
            self.sec.entry(v).or_default().insert(k); // logical index: entries are only ever added here, cleaned by GC
        }
        let base = self.oracle_read(slot, k);
        let mut orow = base;
        orow[col] = v;
        debug_assert_eq!(orow, row);
        self.slots[slot].as_mut().unwrap().pend.insert(k, orow);
        Ok(())
    }

    fn commit(&mut self, slot: usize) -> Result<Ts, ()> {
        let tx = self.slots[slot].take().unwrap();
        let stamp_ts;
        match self.cc {
            Cc::Mvto => {
                stamp_ts = tx.snap;
                self.tt.get_mut(&tx.id).unwrap().state = TxState::Committed; // O(1): readers resolve Tx(id) lazily
                if !tx.writes.is_empty() {
                    self.epoch_q.push_back((stamp_ts, tx.writes.clone()));
                }
            }
            Cc::MvoccLite => {
                for &k in tx.ws.keys() {
                    let (nb, _) = self.store.newest(k);
                    match resolve(&self.tt, nb) {
                        Some((b, TxState::Committed)) if b <= tx.snap => {}
                        _ => {
                            self.tt.get_mut(&tx.id).unwrap().state = TxState::Aborted;
                            return Err(()); // first committer wins
                        }
                    }
                }
                self.clock += 1;
                stamp_ts = self.clock;
                *self.tt.get_mut(&tx.id).unwrap() = Entry {
                    ts: stamp_ts,
                    state: TxState::Committed,
                };
                for (&k, &row) in &tx.ws {
                    self.st.installs += 1;
                    self.st.index_col_changes += (self.store.newest(k).1[0] != row[0]) as u64;
                    self.store.install(k, row, tx.id);
                }
                if !tx.ws.is_empty() {
                    self.epoch_q
                        .push_back((stamp_ts, tx.ws.keys().copied().collect()));
                }
            }
        }
        for (&k, &row) in &tx.pend {
            self.oracle[k].insert(stamp_ts, row);
        }
        self.st.commits += 1;
        Ok(stamp_ts)
    }

    fn abort(&mut self, slot: usize) {
        let tx = self.slots[slot].take().unwrap();
        for &k in tx.writes.iter().rev() {
            self.store.rollback(k);
        }
        self.tt.get_mut(&tx.id).unwrap().state = TxState::Aborted;
        self.st.aborts += 1;
    }

    fn step(&mut self, slot: usize, kind: Kind, i: usize) {
        if self.slots[slot].is_none() {
            self.begin(slot);
            self.trace.push((0, [0; COLS]));
            return;
        }
        let out = match kind {
            Kind::Read(k) => match self.read(slot, k) {
                Ok(r) => (1, r),
                Err(()) => (9, [0; COLS]),
            },
            Kind::Write(k, c, v) => match self.write(slot, k, c, v) {
                Ok(()) => (2, [0; COLS]),
                Err(()) => (9, [0; COLS]),
            },
            Kind::Commit => match self.commit(slot) {
                Ok(ts) => (3, [ts as i64; COLS]),
                Err(()) => (9, [0; COLS]),
            },
            Kind::Abort => {
                self.abort(slot);
                (4, [0; COLS])
            }
        };
        if out.0 == 9 && self.slots[slot].is_some() {
            self.abort(slot); // the engine aborted the transaction: roll back and free the slot
        }
        self.trace.push(out);
        match (self.gc, kind) {
            (Gc::Cooperative, Kind::Read(k)) => self.collect(&[k]),
            (Gc::Vacuum, _) if i % 40 == 0 => {
                let all: Vec<usize> = (0..self.rts.len()).collect();
                self.collect(&all);
            }
            (Gc::Epoch, _) if i % 10 == 0 => self.epoch_collect(),
            _ => {}
        }
        if i % 16 == 0 {
            self.st.peak_bytes = self.st.peak_bytes.max(self.store.bytes());
        }
    }

    fn epoch_collect(&mut self) {
        let wm = self.watermark();
        let mut keys = BTreeSet::new();
        while self.epoch_q.front().is_some_and(|(ts, _)| *ts <= wm) {
            keys.extend(self.epoch_q.pop_front().unwrap().1);
        }
        let keys: Vec<usize> = keys.into_iter().collect();
        self.collect(&keys);
    }

    /// Reads of every active transaction on every key, without side effects on rts or statistics.
    fn peek_all(&mut self) -> Vec<(bool, Option<Row>)> {
        let mut out = Vec::new();
        let actives: Vec<(usize, Ts)> = self
            .slots
            .iter()
            .flatten()
            .map(|t| (t.id, t.snap))
            .collect();
        for (id, snap) in actives {
            for k in 0..self.rts.len() {
                let tt = &self.tt;
                let l = self
                    .store
                    .lookup(k, &mut |b, e| visible(tt, id, snap, b, e));
                out.push((l.conflict, l.row));
            }
        }
        out
    }

    fn collect(&mut self, keys: &[usize]) {
        let before = if self.paranoid {
            Some(self.peek_all())
        } else {
            None
        };
        let wm = self.watermark();
        let tt = &self.tt;
        let dead =
            |end: Stamp| matches!(resolve(tt, end), Some((e, TxState::Committed)) if e <= wm);
        let fix = |s: Stamp| match s {
            Stamp::Tx(id) => match tt.get(&id) {
                Some(e) if e.state == TxState::Committed => Stamp::Ts(e.ts),
                _ => s,
            },
            _ => s,
        };
        for &k in keys {
            let (ex, rm) = self.store.clean_key(k, &dead, &fix);
            self.st.gc_examined += ex as u64;
            self.st.gc_removed += rm as u64;
        }
        if let Some(b) = before {
            assert_eq!(
                b,
                self.peek_all(),
                "GC changed what an active transaction sees ({:?})",
                self.store.name()
            );
        }
    }

    /// Abort everything, vacuum with nobody active, and verify the final state against the oracle and the secondary index.
    fn finish(&mut self) {
        for s in 0..self.slots.len() {
            if self.slots[s].is_some() {
                self.abort(s);
            }
        }
        let keys = self.rts.len();
        let all: Vec<usize> = (0..keys).collect();
        self.collect(&all);
        assert_eq!(
            self.store.versions(),
            keys,
            "with no active transaction exactly the newest version of each key remains ({})",
            self.store.name()
        );
        self.begin(0);
        let mut by_value: BTreeMap<i64, BTreeSet<usize>> = BTreeMap::new();
        for k in 0..keys {
            let r = self.read(0, k).expect("no conflicts left");
            by_value.entry(r[0]).or_default().insert(k);
        }
        // logical-pointer secondary index: candidates from the index, then visibility + predicate re-check through the primary key
        for (&v, cands) in &self.sec.clone() {
            let found: BTreeSet<usize> = cands
                .iter()
                .copied()
                .filter(|&k| self.read(0, k).unwrap()[0] == v)
                .collect();
            assert_eq!(
                &found,
                by_value.get(&v).unwrap_or(&BTreeSet::new()),
                "secondary index lookup for value {v}"
            );
        }
        self.slots[0] = None;
    }
}

fn gen_ops(
    rng: &mut Rng,
    n: usize,
    slots: usize,
    keys: usize,
    write_pct: usize,
) -> Vec<(usize, Kind)> {
    (0..n)
        .map(|_| {
            let slot = rng.below(slots);
            let p = rng.below(1000);
            let k = rng.below(keys);
            let kind = if slot == 0 {
                // slot 0 is the long-running transaction: it mostly reads and almost never ends, pinning the watermark
                if p < 700 {
                    Kind::Read(k)
                } else if p < 992 {
                    Kind::Write(k, rng.below(COLS), rng.below(6) as i64)
                } else if p < 998 {
                    Kind::Commit
                } else {
                    Kind::Abort
                }
            } else if p < 1000 - write_pct * 10 - 150 {
                Kind::Read(k)
            } else if p < 1000 - 150 {
                Kind::Write(k, rng.below(COLS), rng.below(6) as i64)
            } else if p < 970 {
                Kind::Commit
            } else {
                Kind::Abort
            };
            (slot, kind)
        })
        .collect()
}

fn run(
    ops: &[(usize, Kind)],
    cc: Cc,
    gc: Gc,
    scheme: Scheme,
    keys: usize,
    slots: usize,
    paranoid: bool,
) -> Eng {
    let mut e = Eng::new(cc, gc, scheme, keys, slots, paranoid);
    for (i, &(slot, kind)) in ops.iter().enumerate() {
        e.step(slot, kind, i + 1);
    }
    e.finish();
    e
}

fn scripted_demo() {
    println!(
        "-- scripted: visibility, read-your-writes, transaction-table lookup, GC pinned by a long reader"
    );
    let mut e = Eng::new(Cc::MvoccLite, Gc::Off, Scheme::Delta, 2, 3, true);
    e.begin(0); // long reader: snapshot 0
    e.begin(1);
    e.write(1, 0, 3, 77).unwrap();
    assert_eq!(
        e.read(1, 0).unwrap()[3],
        77,
        "read-your-writes from the workspace"
    );
    assert_eq!(
        e.read(0, 0).unwrap()[3],
        0,
        "the long reader does not see uncommitted data"
    );
    let cts = e.commit(1).unwrap();
    println!(
        "   T1 committed at ts {cts}; its versions carry Tx(id) until GC rewrites them (lookup via the transaction table)"
    );
    assert_eq!(
        e.read(0, 0).unwrap()[3],
        0,
        "snapshot 0 still sees the old row (reconstructed from the undo record)"
    );
    e.begin(2);
    assert_eq!(e.read(2, 0).unwrap()[3], 77);
    let l = e.store.lookup(0, &mut |b, en| visible(&e.tt, 99, 0, b, en));
    println!(
        "   old snapshot needed {} steps (main row + 1 undo record)",
        l.steps
    );
    assert_eq!(l.steps, 2);
    e.begin(1);
    for v in 0..5 {
        e.write(1, 1, 2, v).unwrap();
        e.commit(1).unwrap();
        e.begin(1);
    }
    e.collect(&[0, 1]);
    println!(
        "   after 5 more commits with the old reader pinned: {} versions kept for 2 keys (watermark {})",
        e.store.versions(),
        e.watermark()
    );
    assert_eq!(
        e.store.versions(),
        2 + 1 + 5,
        "nothing needed by snapshot 0 may be dropped; key 1's chain is intact"
    );
    e.abort(0);
    e.abort(2); // T3 (snapshot 1) also pins key 1's versions until it ends
    e.collect(&[0, 1]);
    assert_eq!(
        e.store.versions(),
        2,
        "once the reader is gone old versions go: {}",
        e.store.versions()
    );
    println!(
        "   reader finished -> {} versions remain",
        e.store.versions()
    );
}

fn random_histories() {
    println!(
        "-- 1000 random histories x 2 CC x 4 storages x 4 GC modes: oracle equality, trace equality, GC safety"
    );
    let mut rng = Rng(0xfeed_beef_0123_4567);
    let (mut runs, mut commits, mut aborts, mut removed) = (0, 0u64, 0u64, 0u64);
    let mut per_cc: HashMap<Cc, (u64, u64)> = HashMap::new();
    for _ in 0..1000 {
        let ops = gen_ops(&mut rng, 250, 4, 12, 40);
        for cc in [Cc::Mvto, Cc::MvoccLite] {
            let mut reference: Option<Vec<(u8, Row)>> = None;
            for scheme in SCHEMES {
                for gc in GCS {
                    let e = run(&ops, cc, gc, scheme, 12, 4, true);
                    match &reference {
                        None => {
                            commits += e.st.commits;
                            aborts += e.st.aborts;
                            let t = per_cc.entry(cc).or_default();
                            *t = (t.0 + e.st.commits, t.1 + e.st.aborts);
                            reference = Some(e.trace.clone());
                        }
                        Some(r) => assert_eq!(
                            r, &e.trace,
                            "{cc:?} {scheme:?} {gc:?}: outcome trace differs from the first storage"
                        ),
                    }
                    removed += e.st.gc_removed;
                    runs += 1;
                }
            }
        }
    }
    println!(
        "   {runs} runs ok; MVTO commits/aborts {:?}, MVOCC-lite commits/aborts {:?}; versions reclaimed {removed}",
        per_cc[&Cc::Mvto],
        per_cc[&Cc::MvoccLite]
    );
    assert!(commits > 5000 && aborts > 500 && removed > 0);
    let _ = (commits, aborts);
}

struct PerfRow {
    name: &'static str,
    gc: Gc,
    new_steps: f64,
    old_steps: f64,
    peak: usize,
    end: usize,
    examined: u64,
    removed: u64,
    installs: u64,
    idx_changes: u64,
}

fn perf(scheme: Scheme, gc: Gc) -> PerfRow {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    // update-heavy: 100 keys, 4 concurrent transactions, ~75% writes; slot 0 is a long reader holding an old snapshot
    let ops = gen_ops(&mut rng, 120_000, 4, 100, 62);
    let e = run(&ops, Cc::MvoccLite, gc, scheme, 100, 4, false);
    let s = &e.st;
    PerfRow {
        name: e.store.name(),
        gc,
        new_steps: (s.read_steps - s.old_steps) as f64 / (s.reads - s.old_reads) as f64,
        old_steps: s.old_steps as f64 / s.old_reads.max(1) as f64,
        peak: s.peak_bytes,
        end: 0,
        examined: s.gc_examined,
        removed: s.gc_removed,
        installs: s.installs,
        idx_changes: s.index_col_changes,
    }
}

fn perf_table() {
    println!(
        "-- update-heavy workload (120k steps, 100 keys, MVOCC-lite, long reader in slot 0); steps = versions or undo records examined per read"
    );
    println!(
        "   {:<12}{:<13}{:>11}{:>11}{:>11}{:>12}{:>10}",
        "storage", "gc", "steps/read", "steps/old", "peak KB", "gc examined", "removed"
    );
    let mut rows = Vec::new();
    for scheme in SCHEMES {
        for gc in GCS {
            let r = perf(scheme, gc);
            println!(
                "   {:<12}{:<13}{:>11.2}{:>11.2}{:>11.1}{:>12}{:>10}",
                r.name,
                format!("{:?}", r.gc),
                r.new_steps,
                r.old_steps,
                r.peak as f64 / 1024.0,
                r.examined,
                r.removed
            );
            rows.push(r);
        }
    }
    let get = |n: &str, g: Gc| rows.iter().find(|r| r.name == n && r.gc == g).unwrap();
    // direction of the chain decides who pays: new snapshots favour N2O, old snapshots favour O2N
    assert!(
        get("append N2O", Gc::Off).new_steps < get("append O2N", Gc::Off).new_steps,
        "N2O: current reads are short"
    );
    assert!(
        get("append N2O", Gc::Off).old_steps > get("append N2O", Gc::Off).new_steps,
        "N2O: old snapshots walk farther than current ones"
    );
    assert!(
        get("append N2O", Gc::Epoch).new_steps < get("append O2N", Gc::Epoch).new_steps,
        "N2O beats O2N for current reads even with GC"
    );
    assert!(
        get("delta", Gc::Off).peak < get("append N2O", Gc::Off).peak,
        "delta storage is smaller than full versions"
    );
    for scheme in ["append N2O", "time-travel", "delta"] {
        assert!(
            get(scheme, Gc::Epoch).peak <= get(scheme, Gc::Off).peak,
            "GC bounds space ({scheme})"
        );
        assert!(
            get(scheme, Gc::Epoch).examined < get(scheme, Gc::Vacuum).examined,
            "epoch GC visits only touched keys ({scheme})"
        );
    }
    let r = get("append N2O", Gc::Off);
    let m = 3u64;
    println!(
        "-- index maintenance for {} installed versions ({} changed the indexed column), {m} secondary indexes",
        r.installs, r.idx_changes
    );
    println!("   {:<44}{:>12}", "scheme", "index writes");
    println!(
        "   {:<44}{:>12}",
        "append-only + physical pointers",
        r.installs * m
    );
    println!(
        "   {:<44}{:>12}",
        "time-travel/delta + physical (main slot fixed)", r.idx_changes
    );
    println!(
        "   {:<44}{:>12}",
        "any scheme + logical pointers (PK)", r.idx_changes
    );
    println!(
        "   (logical pointers pay one extra primary-index probe on every secondary lookup instead)"
    );
    assert!(r.idx_changes < r.installs);
}

fn main() {
    scripted_demo();
    random_histories();
    perf_table();
    println!("all MVCC version storage / GC checks passed");
}
