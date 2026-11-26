//! RECIPE 851 (database): change data capture and logical replication: WAL decoding in commit order, replication slots, snapshot+stream bootstrap, idempotent sink
//! LEVEL: advanced
//! Use in: streaming a database's changes to search indexes, caches, warehouses, other services; zero-downtime migrations; read replicas of a different shape.   Run: cargo run --release --example cdc_logical_replication_log
#![allow(dead_code)]
//!
//! PHYSICAL vs LOGICAL: physical replication ships page-level WAL bytes (replica must be the same engine and version).
//! LOGICAL replication DECODES the WAL back into row-level changes (insert / update / delete with BEFORE and AFTER images),
//! which any consumer can apply. That is change data capture (CDC).
//! THE PRIMARY (below): a keyed table; each statement appends `Change{xid, key, op, before, after}` to the WAL at write time
//! (so records of concurrent transactions INTERLEAVE), and a transaction ends with a `Commit` or `Abort` record. Row locks keep
//! two open transactions off the same key. Committed state is only updated at commit. A `Ddl` record carries the new column
//! list (ADD COLUMN with default 0), allowed only with no transaction open, as if under an exclusive lock.
//! LOGICAL DECODING (`Primary::decode`): scan the WAL from the slot's RESTART_LSN, buffer changes per xid in a REORDER BUFFER,
//! emit a whole transaction only when its Commit record is seen (so output is in COMMIT order, not change order), drop it on
//! Abort. Messages at or below the consumer's confirmed position are not re-sent.
//! REPLICATION SLOT: the primary's durable bookmark for one consumer. `confirmed_flush` = how far the consumer has durably
//! applied; `restart_lsn` = oldest WAL needed to re-decode everything after it: the first record of any transaction that was
//! still in flight at that position (its changes began earlier than its commit). The primary may only delete WAL older than
//! the minimum restart_lsn over all slots (and open transactions). A stalled slot therefore makes WAL grow without bound:
//! the classic production incident, shown below (Postgres caps it with `max_slot_wal_keep_size`).
//! THE EVENT ENVELOPE: Debezium-style JSON `{before, after, source{db,table,lsn,txId,snapshot}, op, ts_ms}` with op `c`reate,
//! `u`pdate, `d`elete, `r`ead (snapshot row); a `schema` message announces columns. The sink parses these strings; nothing
//! else crosses the boundary, like a real topic.
//! IDEMPOTENT SINK: delivery is AT-LEAST-ONCE (consumer crashes between apply and confirm, or mid-transaction). Every change is
//! applied only if its LSN is greater than the per-key FENCE (the last applied LSN, also kept for deleted keys as a tombstone;
//! otherwise replaying `insert` then `delete` after a crash would resurrect the row). Updates and deletes also check the BEFORE
//! image against the sink's row: a cheap divergence detector.
//! BOOTSTRAP (new replica while writes continue): (1) create the slot FIRST: from now on the primary retains every change
//! (these are the "buffered changes"); (2) take a consistent snapshot of committed state at position SNAP_LSN, each row stamped
//! with the commit LSN that last wrote it; (3) load it as `r` events; (4) stream from the slot, SKIPPING messages with
//! position <= SNAP_LSN (already inside the snapshot; this is the dedupe by LSN), applying the rest. Gap-free because the slot
//! was created before the snapshot, duplicate-free because of the LSN cut. Doing it in the other order (snapshot, then create
//! slot) loses the commits in between.
//! VERIFICATION: 2000 random workloads (concurrent interleaved transactions, aborts, DDL, a replica from the start and a second
//! replica bootstrapped at a random moment mid-workload), consumer crashes at random points (before applying, in the middle of
//! a transaction, after applying but before confirming), restarts from the slot's confirmed position. At the end both sinks
//! equal the primary, per-key applied LSNs strictly increase, replayed changes were fenced, and WAL below the slots' restart_lsn is reclaimed.
//! COMMON USES: PostgreSQL logical decoding / pgoutput, Debezium (outbox pattern, cache invalidation, search indexing), MySQL
//! binlog readers (Maxwell, Canal), MongoDB change streams, DynamoDB streams, AWS DMS, zero-downtime database migrations.
//! Read: https://www.postgresql.org/docs/current/logicaldecoding-explanation.html ,
//! https://debezium.io/documentation/reference/stable/connectors/postgresql.html ("DBLog: A Watermark Based Change-Data-Capture
//! Framework" (Netflix) is the incremental-snapshot alternative; cited by title only, not checked)
//!
//! TRY NEXT:
//!   - [S] Swap the bootstrap order in `scenario` (snapshot first, then `create_slot`) and run: which assertion fails first and for how many seeds?
//!   - [S] Remove the tombstone behaviour (delete the key from `fence` in `apply_event` on `d`): predict which crash point makes a deleted row come back, then find a seed.
//!   - [M] Make `restart_for` return `c + 1` always (ignoring in-flight transactions) and see which transactions are silently lost; why is it only wrong after a crash?
//!   - [M] Add `ALTER COLUMN` (rename) as a second `Ddl` kind and decide what the fence and the envelope must carry.
//!   - [A] PROJECT: feed `decode` output into `transactional_outbox_inbox_sqlite`'s consumer (inbox keyed by commit LSN) so the CDC stream is the relay; compare with `kafka_log_model` partitions keyed by row key.

use serde_json::{Value, json};
use std::collections::{BTreeMap, VecDeque};

type Lsn = u64;

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

#[derive(Clone, Copy, Debug, PartialEq)]
enum Op {
    Insert,
    Update,
    Delete,
}

#[derive(Clone, Debug)]
enum Rec {
    Change {
        xid: u32,
        key: i64,
        op: Op,
        before: Option<Vec<i64>>,
        after: Option<Vec<i64>>,
    },
    Commit {
        xid: u32,
        ts: u64,
    },
    Abort {
        xid: u32,
    },
    Ddl {
        columns: Vec<String>,
    },
}

#[derive(Default)]
struct TxnState {
    first_lsn: Option<Lsn>,
    staged: BTreeMap<i64, Option<Vec<i64>>>,
}

struct Slot {
    confirmed: Lsn,
    restart: Lsn,
}

/// One output unit of logical decoding: a whole committed transaction (or a schema announcement).
#[derive(Clone, Debug)]
struct Msg {
    pos: Lsn,
    xid: Option<u32>,
    events: Vec<String>,
}

struct Primary {
    wal: BTreeMap<Lsn, Rec>,
    next_lsn: Lsn,
    committed: BTreeMap<i64, (Vec<i64>, Lsn)>, // key -> (values, commit LSN of the last writer)
    columns: Vec<String>,
    txns: BTreeMap<u32, TxnState>,
    locks: BTreeMap<i64, u32>,
    next_xid: u32,
    slots: Vec<Slot>,
    last_pos: Lsn, // position of the newest commit or DDL
    clock: u64,
}

impl Primary {
    fn new() -> Primary {
        Primary {
            wal: BTreeMap::new(),
            next_lsn: 1,
            committed: BTreeMap::new(),
            columns: vec!["a".to_string()],
            txns: BTreeMap::new(),
            locks: BTreeMap::new(),
            next_xid: 1,
            slots: vec![],
            last_pos: 0,
            clock: 1_700_000_000_000,
        }
    }
    fn append(&mut self, rec: Rec) -> Lsn {
        let lsn = self.next_lsn;
        self.next_lsn += 1;
        self.clock += 3;
        self.wal.insert(lsn, rec);
        lsn
    }
    fn begin(&mut self) -> u32 {
        let x = self.next_xid;
        self.next_xid += 1;
        self.txns.insert(x, TxnState::default());
        x
    }
    fn pad(&self, mut v: Vec<i64>) -> Vec<i64> {
        v.resize(self.columns.len(), 0); // ADD COLUMN ... DEFAULT 0 is applied lazily on read, no table rewrite
        v
    }
    fn visible(&self, xid: u32, key: i64) -> Option<Vec<i64>> {
        let row = match self.txns[&xid].staged.get(&key) {
            Some(staged) => staged.clone(),
            None => self.committed.get(&key).map(|(r, _)| r.clone()),
        };
        row.map(|r| self.pad(r))
    }
    /// One statement: insert if absent, else update or delete. Returns false on a row-lock conflict.
    fn write(&mut self, xid: u32, key: i64, rng: &mut Rng) -> bool {
        if matches!(self.locks.get(&key), Some(&o) if o != xid) {
            return false;
        }
        let before = self.visible(xid, key);
        let fresh: Vec<i64> = (0..self.columns.len())
            .map(|_| rng.below(1000) as i64)
            .collect();
        let (op, after) = match &before {
            None => (Op::Insert, Some(fresh)),
            Some(_) if rng.pm(300) => (Op::Delete, None),
            Some(_) => (Op::Update, Some(fresh)),
        };
        self.locks.insert(key, xid);
        let lsn = self.append(Rec::Change {
            xid,
            key,
            op,
            before,
            after: after.clone(),
        });
        let t = self.txns.get_mut(&xid).unwrap();
        t.first_lsn.get_or_insert(lsn);
        t.staged.insert(key, after);
        true
    }
    fn commit(&mut self, xid: u32) {
        let t = self.txns.remove(&xid).unwrap();
        if t.first_lsn.is_none() {
            return; // read-only: no WAL
        }
        let ts = self.clock;
        let lsn = self.append(Rec::Commit { xid, ts });
        for (key, row) in t.staged {
            match row {
                Some(r) => self.committed.insert(key, (r, lsn)),
                None => self.committed.remove(&key),
            };
            self.locks.remove(&key);
        }
        self.last_pos = lsn;
    }
    fn abort(&mut self, xid: u32) {
        let t = self.txns.remove(&xid).unwrap();
        if t.first_lsn.is_some() {
            self.append(Rec::Abort { xid });
        }
        for key in t.staged.keys() {
            self.locks.remove(key);
        }
    }
    fn add_column(&mut self, name: &str) {
        assert!(
            self.txns.is_empty(),
            "DDL needs an exclusive lock in this model"
        );
        self.columns.push(name.to_string());
        let lsn = self.append(Rec::Ddl {
            columns: self.columns.clone(),
        });
        self.last_pos = lsn;
    }

    // ---- slots and retention

    /// Oldest WAL position needed to re-decode every transaction that commits after `c`: the first record of any
    /// transaction that had started by `c` and had not finished by `c`.
    fn restart_for(&self, c: Lsn) -> Lsn {
        let (mut first, mut end) = (BTreeMap::new(), BTreeMap::new());
        for (lsn, rec) in &self.wal {
            match rec {
                Rec::Change { xid, .. } => {
                    first.entry(*xid).or_insert(*lsn);
                }
                Rec::Commit { xid, .. } | Rec::Abort { xid } => {
                    end.insert(*xid, *lsn);
                }
                Rec::Ddl { .. } => {}
            }
        }
        let mut r = c + 1;
        for (x, f) in first {
            let e = end.get(&x).copied().unwrap_or(u64::MAX);
            if f <= c && e > c {
                r = r.min(f);
            }
        }
        r
    }
    fn create_slot(&mut self) -> usize {
        let c = self.next_lsn - 1;
        let restart = self.restart_for(c);
        self.slots.push(Slot {
            confirmed: c,
            restart,
        });
        self.slots.len() - 1
    }
    fn confirm(&mut self, slot: usize, pos: Lsn) {
        if pos > self.slots[slot].confirmed {
            let r = self.restart_for(pos);
            self.slots[slot] = Slot {
                confirmed: pos,
                restart: r,
            };
            self.truncate_wal();
        }
    }
    fn truncate_wal(&mut self) {
        let mut floor = self.next_lsn;
        for s in &self.slots {
            floor = floor.min(s.restart);
        }
        for t in self.txns.values() {
            if let Some(f) = t.first_lsn {
                floor = floor.min(f);
            }
        }
        self.wal = self.wal.split_off(&floor);
    }

    // ---- logical decoding

    fn col_names(&self, width: usize) -> &[String] {
        &self.columns[..width] // columns are append-only, so any older image is a prefix
    }
    fn row_json(&self, key: i64, vals: &[i64]) -> Value {
        let mut m = serde_json::Map::new();
        m.insert("id".into(), json!(key));
        for (n, v) in self.col_names(vals.len()).iter().zip(vals) {
            m.insert(n.clone(), json!(v));
        }
        Value::Object(m)
    }
    fn envelope(
        &self,
        lsn: Lsn,
        xid: u32,
        ts: u64,
        key: i64,
        op: Op,
        before: &Option<Vec<i64>>,
        after: &Option<Vec<i64>>,
        snap: bool,
    ) -> String {
        let code = match (op, snap) {
            (_, true) => "r",
            (Op::Insert, _) => "c",
            (Op::Update, _) => "u",
            (Op::Delete, _) => "d",
        };
        json!({
            "before": before.as_ref().map(|r| self.row_json(key, r)),
            "after": after.as_ref().map(|r| self.row_json(key, r)),
            "source": {"db": "primary", "table": "items", "lsn": lsn, "txId": xid, "snapshot": snap},
            "op": code,
            "ts_ms": ts,
        })
        .to_string()
    }
    fn schema_event(&self, pos: Lsn, columns: &[String]) -> String {
        json!({"op": "schema", "source": {"lsn": pos}, "columns": columns}).to_string()
    }

    /// Decode everything after the slot's confirmed position, in commit order.
    fn decode(&self, slot: usize) -> Vec<Msg> {
        let s = &self.slots[slot];
        let mut buf: BTreeMap<u32, Vec<(Lsn, &Rec)>> = BTreeMap::new(); // the REORDER BUFFER
        let mut out = vec![];
        for (&lsn, rec) in self.wal.range(s.restart..) {
            match rec {
                Rec::Change { xid, .. } => buf.entry(*xid).or_default().push((lsn, rec)),
                Rec::Abort { xid } => {
                    buf.remove(xid); // aborted transactions never leave the primary
                }
                Rec::Commit { xid, ts } => {
                    let changes = buf.remove(xid).unwrap_or_default();
                    if lsn > s.confirmed {
                        let events = changes
                            .iter()
                            .map(|(l, r)| match r {
                                Rec::Change {
                                    key,
                                    op,
                                    before,
                                    after,
                                    ..
                                } => self.envelope(*l, *xid, *ts, *key, *op, before, after, false),
                                _ => unreachable!(),
                            })
                            .collect();
                        out.push(Msg {
                            pos: lsn,
                            xid: Some(*xid),
                            events,
                        });
                    }
                }
                Rec::Ddl { columns } => {
                    if lsn > s.confirmed {
                        out.push(Msg {
                            pos: lsn,
                            xid: None,
                            events: vec![self.schema_event(lsn, columns)],
                        });
                    }
                }
            }
        }
        out
    }

    /// Consistent snapshot of committed state, rows stamped with their last-writer commit LSN.
    fn snapshot(&self) -> Msg {
        let mut events = vec![self.schema_event(self.last_pos, &self.columns)];
        for (&k, (r, lsn)) in &self.committed {
            let r = Some(self.pad(r.clone()));
            events.push(self.envelope(*lsn, 0, self.clock, k, Op::Insert, &None, &r, true));
        }
        Msg {
            pos: self.last_pos,
            xid: None,
            events,
        }
    }
}

// ------------------------------------------------------------------------------------------------ the sink

#[derive(Default)]
struct Sink {
    rows: BTreeMap<i64, BTreeMap<String, i64>>,
    fence: BTreeMap<i64, Lsn>,
    columns: Vec<String>,
    applied: BTreeMap<i64, Vec<Lsn>>,
    replays_fenced: u64,
    msgs_skipped_by_snapshot: u64,
    snap_pos: Lsn,
}

impl Sink {
    fn ensure_columns(&mut self, cols: impl Iterator<Item = String>) {
        for c in cols {
            if !self.columns.contains(&c) {
                self.columns.push(c.clone());
                for r in self.rows.values_mut() {
                    r.insert(c.clone(), 0);
                }
            }
        }
    }
    fn obj_to_row(&mut self, o: &Value) -> (i64, BTreeMap<String, i64>) {
        let m = o.as_object().unwrap();
        let key = m["id"].as_i64().unwrap();
        self.ensure_columns(m.keys().filter(|k| *k != "id").cloned());
        let row = self
            .columns
            .iter()
            .map(|c| (c.clone(), m.get(c).map_or(0, |v| v.as_i64().unwrap())))
            .collect();
        (key, row)
    }
    fn apply_event(&mut self, raw: &str) {
        let v: Value = serde_json::from_str(raw).unwrap();
        let op = v["op"].as_str().unwrap();
        if op == "schema" {
            let cols: Vec<String> = v["columns"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c.as_str().unwrap().to_string())
                .collect();
            self.ensure_columns(cols.into_iter());
            return;
        }
        let lsn = v["source"]["lsn"].as_u64().unwrap();
        let probe = if v["after"].is_null() {
            &v["before"]
        } else {
            &v["after"]
        };
        let key = probe["id"].as_i64().unwrap();
        if self.fence.get(&key).is_some_and(|&f| lsn <= f) {
            self.replays_fenced += 1; // already applied (or older than what is applied): idempotent no-op
            return;
        }
        if op == "u" || op == "d" {
            let (_, before) = self.obj_to_row(&v["before"]);
            assert_eq!(
                self.rows.get(&key),
                Some(&before),
                "before-image of key {key} at lsn {lsn} diverges from the sink"
            );
        }
        if op == "c" {
            assert!(
                !self.rows.contains_key(&key),
                "insert of an existing key {key}"
            );
        }
        if v["after"].is_null() {
            self.rows.remove(&key);
        } else {
            let (k, row) = self.obj_to_row(&v["after"]);
            self.rows.insert(k, row);
        }
        self.fence.insert(key, lsn); // kept after delete: a TOMBSTONE fence
        self.applied.entry(key).or_default().push(lsn);
    }
    /// Returns false when the consumer "crashes" part-way; `crash_at == events.len()` means after the last event.
    fn apply_msg(&mut self, m: &Msg, crash_at: Option<usize>) -> bool {
        if m.pos <= self.snap_pos {
            self.msgs_skipped_by_snapshot += 1; // already contained in the snapshot
            return true;
        }
        for (i, e) in m.events.iter().enumerate() {
            if crash_at == Some(i) {
                return false;
            }
            self.apply_event(e);
        }
        crash_at != Some(m.events.len())
    }
    fn load_snapshot(&mut self, snap: &Msg) {
        for e in &snap.events {
            self.apply_event(e);
        }
        self.snap_pos = snap.pos;
    }
}

struct Consumer {
    slot: usize,
    sink: Sink,
    pending: VecDeque<Msg>, // RAM only: lost on crash
    bootstrapped: bool,
    crashes: u32,
}

fn consumer_step(p: &mut Primary, c: &mut Consumer, rng: &mut Rng, chaos: bool) {
    if !c.bootstrapped {
        return; // the slot is accumulating changes for us
    }
    if c.pending.is_empty() {
        c.pending = p.decode(c.slot).into();
    }
    for _ in 0..1 + rng.below(3) {
        let Some(m) = c.pending.pop_front() else {
            return;
        };
        let crash_at = if chaos && rng.pm(70) {
            Some(rng.below(m.events.len() as u64 + 1) as usize)
        } else {
            None
        };
        if !c.sink.apply_msg(&m, crash_at) {
            c.crash();
            return;
        }
        if !chaos || rng.pm(700) {
            p.confirm(c.slot, m.pos); // durable ack back to the slot
        } else if rng.pm(150) {
            c.crash(); // applied but not confirmed: it will be redelivered and must be fenced
            return;
        }
    }
}
impl Consumer {
    fn crash(&mut self) {
        self.pending.clear(); // restart: next poll re-decodes from the slot's confirmed position
        self.crashes += 1;
    }
}

// ------------------------------------------------------------------------------------------------ scenarios

fn check_equal(p: &Primary, c: &Consumer, seed: u64, who: &str) {
    let want: BTreeMap<i64, BTreeMap<String, i64>> = p
        .committed
        .iter()
        .map(|(&k, (r, _))| (k, p.columns.iter().cloned().zip(p.pad(r.clone())).collect()))
        .collect();
    assert_eq!(
        c.sink.rows, want,
        "seed {seed}: replica {who} differs from the primary"
    );
    let mut cols = c.sink.columns.clone();
    cols.sort();
    let mut pc = p.columns.clone();
    pc.sort();
    assert_eq!(cols, pc, "seed {seed}: replica {who} schema differs");
    for (k, l) in &c.sink.applied {
        assert!(
            l.windows(2).all(|w| w[0] < w[1]),
            "seed {seed}: key {k} applied out of order: {l:?}"
        );
    }
}

#[derive(Default)]
struct Totals {
    msgs_skipped: u64,
    replays_fenced: u64,
    crashes: u64,
    txns_aborted: u64,
    txns_committed: u64,
    ddls: u64,
}

fn scenario(seed: u64, t: &mut Totals) {
    let mut rng = Rng::new(seed);
    let mut p = Primary::new();
    let a_slot = p.create_slot(); // replica A follows from the very beginning
    let mut a = Consumer {
        slot: a_slot,
        sink: Sink::default(),
        pending: VecDeque::new(),
        bootstrapped: true,
        crashes: 0,
    };
    let steps = 150 + rng.below(150);
    let b_create = 10 + rng.below(80);
    let b_snap = b_create + rng.below(40);
    let mut b: Option<Consumer> = None;
    let mut open: Vec<u32> = vec![];
    for step in 0..steps {
        if step == b_create {
            // (1) slot first ...
            let slot = p.create_slot();
            b = Some(Consumer {
                slot,
                sink: Sink::default(),
                pending: VecDeque::new(),
                bootstrapped: false,
                crashes: 0,
            });
        }
        if step == b_snap {
            // (2) ... snapshot later, writes continued in between; (3) load; (4) stream resumes with the LSN cut
            let bb = b.as_mut().unwrap();
            let snap = p.snapshot();
            bb.sink.load_snapshot(&snap);
            bb.bootstrapped = true;
        }
        match rng.below(100) {
            0..=44 => {
                if open.len() < 3 && (open.is_empty() || rng.pm(300)) {
                    open.push(p.begin());
                }
                let x = open[rng.below(open.len() as u64) as usize];
                let key = 1 + rng.below(10) as i64;
                p.write(x, key, &mut rng);
            }
            45..=60 if !open.is_empty() => {
                let i = rng.below(open.len() as u64) as usize;
                p.commit(open.swap_remove(i));
                t.txns_committed += 1;
            }
            61..=65 if !open.is_empty() => {
                let i = rng.below(open.len() as u64) as usize;
                p.abort(open.swap_remove(i));
                t.txns_aborted += 1;
            }
            66..=68 if p.columns.len() < 5 => {
                // quiesce, then DDL
                for x in open.drain(..).collect::<Vec<_>>() {
                    p.commit(x);
                    t.txns_committed += 1;
                }
                let name = format!("c{}", p.columns.len());
                p.add_column(&name);
                t.ddls += 1;
            }
            69..=84 => consumer_step(&mut p, &mut a, &mut rng, true),
            _ => {
                if let Some(bb) = b.as_mut() {
                    consumer_step(&mut p, bb, &mut rng, true);
                }
            }
        }
    }
    // finish the workload (commit survivors), bootstrap B if the random schedule did not, then drain without chaos
    for x in open.drain(..).collect::<Vec<_>>() {
        p.commit(x);
        t.txns_committed += 1;
    }
    if let Some(bb) = b.as_mut() {
        if !bb.bootstrapped {
            let snap = p.snapshot();
            bb.sink.load_snapshot(&snap);
            bb.bootstrapped = true;
        }
    }
    let mut b = b.unwrap();
    for _ in 0..200 {
        consumer_step(&mut p, &mut a, &mut rng, false);
        consumer_step(&mut p, &mut b, &mut rng, false);
    }
    check_equal(&p, &a, seed, "A (from start)");
    check_equal(&p, &b, seed, "B (bootstrapped mid-stream)");
    // both slots confirmed the final commit; WAL below the oldest restart_lsn is gone (a transaction that was in flight at the
    // confirmed position and aborted later can legitimately pin a few records: restart_lsn only moves on the next confirm)
    assert!(
        p.slots.iter().all(|s| s.confirmed == p.last_pos),
        "seed {seed}: slot did not reach the last position"
    );
    let min_restart = p.slots.iter().map(|s| s.restart).min().unwrap();
    assert!(
        p.wal.keys().next().is_none_or(|&f| f >= min_restart),
        "seed {seed}: WAL below restart_lsn was kept"
    );
    assert!(
        p.wal.len() < 40,
        "seed {seed}: WAL not reclaimed ({} records)",
        p.wal.len()
    );
    t.msgs_skipped += b.sink.msgs_skipped_by_snapshot;
    t.replays_fenced += a.sink.replays_fenced + b.sink.replays_fenced;
    t.crashes += (a.crashes + b.crashes) as u64;
}

/// Hand-built scenario: interleaved transactions, an abort, and the envelope format.
fn demo_decoding() {
    let mut p = Primary::new();
    let slot = p.create_slot();
    let mut rng = Rng::new(1);
    let (t1, t2, t3) = (p.begin(), p.begin(), p.begin());
    p.write(t1, 1, &mut rng); // insert k1 (T1)
    p.write(t2, 2, &mut rng); // insert k2 (T2)
    p.write(t3, 3, &mut rng); // insert k3 (T3, will abort)
    assert!(!p.write(t2, 1, &mut rng), "row lock: k1 is held by T1");
    p.write(t1, 4, &mut rng);
    p.commit(t2); // T2 commits first although T1 wrote first
    p.abort(t3);
    p.commit(t1);
    let msgs = p.decode(slot);
    let order: Vec<u32> = msgs.iter().map(|m| m.xid.unwrap()).collect();
    assert_eq!(order, vec![t2, t1], "commit order, aborted T3 dropped");
    assert_eq!(msgs[1].events.len(), 2);
    println!(
        "decoded in commit order: T{} then T{} (T{} aborted, never emitted)",
        t2, t1, t3
    );
    println!("sample Debezium-style envelope: {}", msgs[0].events[0]);
    for m in &msgs {
        for e in &m.events {
            let v: Value = serde_json::from_str(e).unwrap();
            match v["op"].as_str().unwrap() {
                "c" => assert!(v["before"].is_null() && !v["after"].is_null()),
                "d" => assert!(!v["before"].is_null() && v["after"].is_null()),
                "u" => assert!(!v["before"].is_null() && !v["after"].is_null()),
                o => panic!("unexpected op {o}"),
            }
        }
    }
    // the restart point reaches back to the start of a transaction still open at the confirmed position
    let t4 = p.begin();
    p.write(t4, 7, &mut rng);
    let first = *p.wal.keys().last().unwrap();
    let t5 = p.begin();
    p.write(t5, 8, &mut rng);
    p.commit(t5);
    let c5 = p.last_pos;
    p.confirm(slot, c5);
    assert_eq!(
        p.slots[slot].restart, first,
        "T4 was in flight at the confirmed position: its first change must be retained"
    );
    p.commit(t4);
    let after = p.decode(slot);
    assert_eq!(
        after.len(),
        1,
        "only T4 is new, and it is delivered whole although its change precedes the confirmed LSN"
    );
    println!(
        "restart_lsn={} (< confirmed={}) keeps the in-flight transaction decodable",
        first, c5
    );
}

/// A slot nobody confirms pins the WAL; a confirmed one lets it go.
fn demo_retention() {
    let run = |confirm: bool| -> usize {
        let mut p = Primary::new();
        let slot = p.create_slot();
        let mut rng = Rng::new(5);
        for i in 0..400 {
            let x = p.begin();
            p.write(x, 1 + (i % 7), &mut rng);
            p.commit(x);
            if confirm && i % 10 == 9 {
                p.confirm(slot, p.last_pos);
            }
        }
        p.wal.len()
    };
    let (healthy, stalled) = (run(true), run(false));
    println!("WAL records retained after 400 txns: healthy slot {healthy}, stalled slot {stalled}");
    assert!(healthy < 40 && stalled >= 800);
}

fn main() {
    demo_decoding();
    demo_retention();
    let mut t = Totals::default();
    for seed in 1..=2000 {
        scenario(seed, &mut t);
    }
    println!(
        "2000 workloads OK: {} committed txns, {} aborted, {} DDLs; {} consumer crashes; {} replayed changes absorbed by the LSN fence; {} stream messages skipped as already in the bootstrap snapshot",
        t.txns_committed, t.txns_aborted, t.ddls, t.crashes, t.replays_fenced, t.msgs_skipped
    );
    assert!(t.replays_fenced > 0 && t.msgs_skipped > 0 && t.crashes > 0 && t.txns_aborted > 0);
}
