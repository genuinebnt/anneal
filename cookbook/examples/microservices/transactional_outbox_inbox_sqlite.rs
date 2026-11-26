//! RECIPE 850 (microservices): transactional OUTBOX + INBOX on SQLite: atomic state+event, leased relays, idempotent consumers, crash injection
//! LEVEL: intermediate
//! Use in: any service that must change its own database AND tell other services about it (orders, payments, inventory); event-driven microservices.   Run: cargo run --release --example transactional_outbox_inbox_sqlite
#![allow(dead_code)]
//!
//! THE DUAL-WRITE PROBLEM: "UPDATE my table, then publish to the broker" is two writes to two systems. Crash between them and
//! either the state changed but nobody heard (lost event), or (publish first) others heard about something that never happened.
//! There is no transaction spanning a database and a broker, and 2PC across them is rarely available.
//! THE OUTBOX: write the business change AND a row describing the event into an OUTBOX table in ONE local ACID transaction.
//! Either both exist or neither does. A separate RELAY reads the outbox and publishes to the broker, so publishing becomes a
//! retryable background job instead of part of the request. Delivery is therefore AT-LEAST-ONCE: a crash after publishing but
//! before marking the row sent publishes it again.
//! THE INBOX: the consumer turns at-least-once into effectively-once. It records every processed message id in an INBOX table
//! (primary key = message identity) in the SAME transaction as its business effect. A duplicate hits the key and is skipped.
//! WHAT IS SIMULATED (one thread, one seeded scheduler, two real in-memory SQLite databases: producer DB and consumer DB):
//! * the relay CLAIMS a row with one `UPDATE ... WHERE id=(SELECT ...) RETURNING ...`: the SQLite stand-in for
//!   `SELECT ... FOR UPDATE SKIP LOCKED`. The claim writes `claimed_by` and `lease_until`. A relay that dies leaves a lease that
//!   simply EXPIRES, then another relay takes the row. The final `sent=1` update is FENCED by `claimed_by=me`.
//! * PER-KEY ORDER: each event carries a per-aggregate sequence number assigned inside the producer transaction. The claim only
//!   offers the lowest unsent seq of a key, so a key is never published out of order by relays; the consumer additionally defers
//!   `seq != expected` (the broker reorders), and the inbox drops `seq < expected` (duplicates).
//! * RETRY WITH BACKOFF: a failed publish bumps `attempts` and sets `next_at = now + min(2^attempts, 16)`. After MAX_ATTEMPTS the
//!   row is PARKED (a copy goes to `dead_letter`) and, importantly, still BLOCKS its key, because skipping it would let seq+1
//!   overtake. An operator fixes the cause and requeues; a demo shows this.
//! * THE BROKER: duplicates on publish, lost acks (message enqueued but the publisher sees an error), outage windows, random
//!   delivery order, redelivery until acked.
//! * CRASH POINTS (every step boundary, each hit counted and asserted to have fired): producer before/after commit; relay claim
//!   before/after commit; after publish before mark; mark before/after commit; consumer before commit; after commit before ack.
//! * ORACLE after 3000 random schedules with 3 relays and 2 consumers: every COMMITTED command is in the consumer's state exactly
//!   once, per-key apply order is 1,2,3..., consumer state == producer state, dead letters empty, and the outbox drains to empty.
//! POLLING vs TAIL: a second relay mode keeps a persisted CURSOR and reads `id > cursor` (what a CDC reader does with a log
//! position): no per-row claim/mark writes, one cursor write per batch, but a single reader and an ordering trap noted below.
//! OTHER VARIANTS (described, not built): (1) "LISTEN TO YOURSELF": the service publishes the event first and applies its own state
//! change only when it consumes its own message; no outbox table, but read-your-writes is lost and the broker becomes the commit
//! point. (2) CDC (Debezium outbox event router): the relay is the database's replication log, so the app only INSERTs the
//! outbox row and a connector tails the WAL: no polling load, commit order is exactly log order; costs an extra system to run.
//! TAIL TRAP: with concurrent writers on Postgres, a sequence-assigned `id` order is NOT commit order (a slow txn with a small id
//! commits later), so an `id > cursor` reader can skip it forever. SQLite has one writer so this simulation cannot show it.
//! INBOX RETENTION: the per-key high-water mark (`next_seq`) already rejects old duplicates, so inbox rows are only needed while
//! a duplicate could be in flight; prune by age > the broker's maximum redelivery window.
//! COMMON USES: Debezium outbox event router, Axon/MassTransit/NServiceBus/Wolverine outboxes, Spring Modulith event publication
//! registry, Kafka consumers that dedupe by id, payments (Stripe-style idempotency keys are the same inbox idea).
//! Read: https://microservices.io/patterns/data/transactional-outbox.html , https://debezium.io/documentation/reference/stable/connectors/postgresql.html ,
//! https://www.sqlite.org/lang_returning.html
//!
//! TRY NEXT:
//!   - [S] Remove the `claimed_by=?` fence from the `mark` UPDATE in `relay_step` and re-run: which invariant in `verify` fails first, and does it need a lease expiry to happen?
//!   - [S] Set LEASE to 1 and then to 60: predict, then measure the duplicate count (`stats.deduped`) and the ticks needed to drain.
//!   - [M] Make the consumer dedupe ONLY by `seq < next_seq` (drop the inbox table) and prove with the random schedules whether anything breaks; then reorder events across keys and think about what the inbox gives you that a high-water mark does not.
//!   - [M] Port the claim query to Postgres syntax (`FOR UPDATE SKIP LOCKED`) on paper and decide where per-key ordering is lost with several relays; write the failing interleaving.
//!   - [A] PROJECT: combine with `retry_backoff_jitter` (add jitter to `next_at`) and `saga_orchestrator`: make each saga step command an outbox event and the step reply an inbox message, then crash at the same boundaries.

use rusqlite::{Connection, OptionalExtension, params};
use std::collections::BTreeMap;

const KEYS: [&str; 3] = ["order-A", "order-B", "order-C"];
const LEASE: u64 = 6;
const MAX_ATTEMPTS: i64 = 40;
const CRASH_PM: u64 = 50; // per-mille chance per crash point

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

#[derive(Default, Clone)]
struct Stats {
    publishes: u64,
    broker_dups: u64,
    lost_acks: u64,
    outages: u64,
    deduped: u64,
    deferred: u64,
    retries: u64,
    relay_writes: u64,
    takeovers: u64,
}
impl Stats {
    fn add(&mut self, o: &Stats) {
        self.publishes += o.publishes;
        self.broker_dups += o.broker_dups;
        self.lost_acks += o.lost_acks;
        self.outages += o.outages;
        self.deduped += o.deduped;
        self.deferred += o.deferred;
        self.retries += o.retries;
        self.relay_writes += o.relay_writes;
        self.takeovers += o.takeovers;
    }
}

/// Randomness + crash switch + counters, kept apart from `World` so a live `Transaction` (borrowing a db field)
/// can coexist with `sim.crash(..)` (borrowing another field).
struct Sim {
    rng: Rng,
    crashes_on: bool,
    hits: BTreeMap<&'static str, u64>,
    stats: Stats,
}
impl Sim {
    fn crash(&mut self, label: &'static str) -> bool {
        if self.crashes_on && self.rng.pm(CRASH_PM) {
            *self.hits.entry(label).or_default() += 1;
            true
        } else {
            false
        }
    }
}

#[derive(Clone, Debug)]
struct Msg {
    k: String,
    seq: i64,
    amount: i64,
}

struct Broker {
    q: Vec<(u64, Msg)>,
    next_uid: u64,
    down_until: u64,
    faults: bool,
    poison: Option<String>,
}
impl Broker {
    fn enqueue(&mut self, m: &Msg) {
        self.q.push((self.next_uid, m.clone()));
        self.next_uid += 1;
    }
    /// At-least-once publish with every nasty failure mode of a real broker client.
    fn publish(&mut self, sim: &mut Sim, now: u64, m: &Msg) -> Result<(), ()> {
        sim.stats.publishes += 1;
        if self.poison.as_deref() == Some(m.k.as_str()) {
            return Err(());
        }
        if !self.faults {
            self.enqueue(m);
            return Ok(());
        }
        if now < self.down_until {
            return Err(());
        }
        if sim.rng.pm(30) {
            self.down_until = now + 3 + sim.rng.below(8);
            sim.stats.outages += 1;
            return Err(());
        }
        self.enqueue(m);
        if sim.rng.pm(80) {
            self.enqueue(m);
            sim.stats.broker_dups += 1;
        }
        if sim.rng.pm(80) {
            sim.stats.lost_acks += 1;
            return Err(()); // it IS in the queue; the relay just does not know
        }
        Ok(())
    }
    /// Random pick = arbitrary reordering. Not removed until `ack` = redelivery after a consumer crash.
    fn receive(&self, rng: &mut Rng) -> Option<(u64, Msg)> {
        if self.q.is_empty() {
            return None;
        }
        Some(self.q[rng.below(self.q.len() as u64) as usize].clone())
    }
    fn ack(&mut self, uid: u64) {
        self.q.retain(|(u, _)| *u != uid);
    }
}

const SCHEMA_PRODUCER: &str = "
CREATE TABLE balance(k TEXT PRIMARY KEY, v INTEGER NOT NULL, next_seq INTEGER NOT NULL);
CREATE TABLE outbox(
  id INTEGER PRIMARY KEY AUTOINCREMENT, k TEXT NOT NULL, seq INTEGER NOT NULL, amount INTEGER NOT NULL,
  attempts INTEGER NOT NULL DEFAULT 0, next_at INTEGER NOT NULL DEFAULT 0,
  claimed_by INTEGER, lease_until INTEGER NOT NULL DEFAULT 0,
  sent INTEGER NOT NULL DEFAULT 0, parked INTEGER NOT NULL DEFAULT 0, UNIQUE(k, seq));
CREATE TABLE dead_letter(id INTEGER PRIMARY KEY, k TEXT, seq INTEGER, amount INTEGER, attempts INTEGER, reason TEXT);
CREATE TABLE cursor(id INTEGER NOT NULL);
INSERT INTO cursor VALUES (0);";
const SCHEMA_CONSUMER: &str = "
CREATE TABLE replica(k TEXT PRIMARY KEY, v INTEGER NOT NULL, next_seq INTEGER NOT NULL);
CREATE TABLE inbox(k TEXT NOT NULL, seq INTEGER NOT NULL, PRIMARY KEY(k, seq));";

struct World {
    pdb: Connection,
    cdb: Connection,
    broker: Broker,
    sim: Sim,
    now: u64,
    committed: BTreeMap<String, Vec<i64>>,
    applied: BTreeMap<String, Vec<i64>>,
    incarnations: i64,
}
impl World {
    fn new(seed: u64, chaos: bool) -> World {
        let pdb = Connection::open_in_memory().unwrap();
        pdb.execute_batch(SCHEMA_PRODUCER).unwrap();
        let cdb = Connection::open_in_memory().unwrap();
        cdb.execute_batch(SCHEMA_CONSUMER).unwrap();
        World {
            pdb,
            cdb,
            broker: Broker {
                q: vec![],
                next_uid: 1,
                down_until: 0,
                faults: chaos,
                poison: None,
            },
            sim: Sim {
                rng: Rng::new(seed),
                crashes_on: chaos,
                hits: BTreeMap::new(),
                stats: Stats::default(),
            },
            now: 1,
            committed: BTreeMap::new(),
            applied: BTreeMap::new(),
            incarnations: 0,
        }
    }
    fn new_relay(&mut self) -> Relay {
        self.incarnations += 1;
        Relay {
            me: self.incarnations,
            phase: Phase::Idle,
        }
    }
}

// ---------------------------------------------------------------- producer: state + outbox row in ONE transaction

fn producer_step(w: &mut World) {
    let k = KEYS[w.sim.rng.below(KEYS.len() as u64) as usize];
    let amount = 1 + w.sim.rng.below(100) as i64;
    let tx = w.pdb.unchecked_transaction().unwrap();
    tx.execute("INSERT OR IGNORE INTO balance VALUES (?1, 0, 1)", [k])
        .unwrap();
    let seq: i64 = tx
        .query_row("SELECT next_seq FROM balance WHERE k=?1", [k], |r| r.get(0))
        .unwrap();
    tx.execute(
        "UPDATE balance SET v=v+?2, next_seq=next_seq+1 WHERE k=?1",
        params![k, amount],
    )
    .unwrap();
    tx.execute(
        "INSERT INTO outbox(k, seq, amount) VALUES (?1, ?2, ?3)",
        params![k, seq, amount],
    )
    .unwrap();
    if w.sim.crash("producer:before_commit") {
        return; // dropping `tx` rolls back: neither the balance change nor the event exists
    }
    tx.commit().unwrap();
    w.committed.entry(k.to_string()).or_default().push(amount);
    let _ = w.sim.crash("producer:after_commit"); // nothing in RAM matters here; the point is the invariant still holds
}

// ---------------------------------------------------------------- polling relay: claim -> publish -> mark

#[derive(Clone)]
struct Row {
    id: i64,
    k: String,
    seq: i64,
    amount: i64,
    attempts: i64,
}
enum Phase {
    Idle,
    Claimed(Row),
    Published(Row, Result<(), ()>),
}
struct Relay {
    me: i64,
    phase: Phase,
}

const CLAIM_SQL: &str = "
UPDATE outbox SET claimed_by=?1, lease_until=?2 WHERE id=(
  SELECT o.id FROM outbox o
  WHERE o.sent=0 AND o.parked=0 AND o.next_at<=?3 AND (o.claimed_by IS NULL OR o.lease_until<=?3)
    AND o.seq=(SELECT MIN(x.seq) FROM outbox x WHERE x.k=o.k AND x.sent=0)
  ORDER BY o.id LIMIT 1)
RETURNING id, k, seq, amount, attempts";

fn relay_step(w: &mut World, r: &mut Relay) {
    match std::mem::replace(&mut r.phase, Phase::Idle) {
        Phase::Idle => {
            let tx = w.pdb.unchecked_transaction().unwrap();
            let row = tx
                .query_row(CLAIM_SQL, params![r.me, w.now + LEASE, w.now], |x| {
                    Ok(Row {
                        id: x.get(0)?,
                        k: x.get(1)?,
                        seq: x.get(2)?,
                        amount: x.get(3)?,
                        attempts: x.get(4)?,
                    })
                })
                .optional()
                .unwrap();
            let Some(row) = row else { return };
            if row.attempts > 0 {
                w.sim.stats.retries += 1;
            }
            if w.sim.crash("claim:before_commit") {
                return;
            }
            tx.commit().unwrap();
            w.sim.stats.relay_writes += 1;
            if !w.sim.crash("claim:after_commit") {
                r.phase = Phase::Claimed(row); // else: lease stays, expires, someone else takes over
            }
        }
        Phase::Claimed(row) => {
            let msg = Msg {
                k: row.k.clone(),
                seq: row.seq,
                amount: row.amount,
            };
            let res = w.broker.publish(&mut w.sim, w.now, &msg);
            if !w.sim.crash("publish:after_publish_before_mark") {
                r.phase = Phase::Published(row, res);
            }
        }
        Phase::Published(row, res) => {
            let tx = w.pdb.unchecked_transaction().unwrap();
            match res {
                Ok(()) => {
                    // fenced: if our lease expired and another relay owns the row, this updates 0 rows and that is fine
                    tx.execute(
                        "UPDATE outbox SET sent=1 WHERE id=?1 AND claimed_by=?2",
                        params![row.id, r.me],
                    )
                    .unwrap();
                }
                Err(()) => mark_failed(&tx, &row, r.me, w.now),
            }
            if w.sim.crash("mark:before_commit") {
                return;
            }
            tx.commit().unwrap();
            w.sim.stats.relay_writes += 1;
            let _ = w.sim.crash("mark:after_commit");
        }
    }
}

/// Backoff, and after MAX_ATTEMPTS park the row (it keeps blocking its key) and copy it to the dead-letter table.
fn mark_failed(tx: &rusqlite::Transaction, row: &Row, me: i64, now: u64) {
    let attempts = row.attempts + 1;
    if attempts >= MAX_ATTEMPTS {
        let n = tx
            .execute("UPDATE outbox SET parked=1, attempts=?3, claimed_by=NULL WHERE id=?1 AND claimed_by=?2", params![row.id, me, attempts])
            .unwrap();
        if n == 1 {
            tx.execute(
                "INSERT OR REPLACE INTO dead_letter VALUES (?1, ?2, ?3, ?4, ?5, 'max attempts')",
                params![row.id, row.k, row.seq, row.amount, attempts],
            )
            .unwrap();
        }
    } else {
        let next_at = now + (1u64 << attempts.min(4)).min(16);
        tx.execute(
            "UPDATE outbox SET attempts=?3, next_at=?4, claimed_by=NULL, lease_until=0 WHERE id=?1 AND claimed_by=?2",
            params![row.id, me, attempts, next_at as i64],
        )
        .unwrap();
    }
}

/// Cleanup: sent rows are only history; delete them (a real system keeps them N days for audit, or partitions by day).
fn cleanup(w: &World) {
    w.pdb
        .execute("DELETE FROM outbox WHERE sent=1", [])
        .unwrap();
}

// ---------------------------------------------------------------- tail relay: persisted cursor, no per-row writes

fn tail_step(w: &mut World, _r: &mut Relay) {
    let cursor: i64 = w
        .pdb
        .query_row("SELECT id FROM cursor", [], |x| x.get(0))
        .unwrap();
    let rows: Vec<Row> = {
        let mut st = w
            .pdb
            .prepare("SELECT id,k,seq,amount FROM outbox WHERE id>?1 ORDER BY id LIMIT 4")
            .unwrap();
        st.query_map([cursor], |x| {
            Ok(Row {
                id: x.get(0)?,
                k: x.get(1)?,
                seq: x.get(2)?,
                amount: x.get(3)?,
                attempts: 0,
            })
        })
        .unwrap()
        .map(|x| x.unwrap())
        .collect()
    };
    let mut last_ok = None;
    for row in rows {
        let msg = Msg {
            k: row.k.clone(),
            seq: row.seq,
            amount: row.amount,
        };
        if w.broker.publish(&mut w.sim, w.now, &msg).is_err() {
            break; // strictly in order: stop at the first failure and retry from there
        }
        last_ok = Some(row.id);
        if w.sim.crash("tail:mid_batch") {
            return; // cursor not advanced: the whole batch is re-published later (duplicates; inbox absorbs them)
        }
    }
    if let Some(id) = last_ok {
        let tx = w.pdb.unchecked_transaction().unwrap();
        tx.execute("UPDATE cursor SET id=?1", [id]).unwrap();
        if w.sim.crash("tail:cursor_before_commit") {
            return;
        }
        tx.commit().unwrap();
        w.sim.stats.relay_writes += 1;
    }
}

// ---------------------------------------------------------------- consumer: inbox + effect in one transaction

fn consumer_step(w: &mut World) {
    let Some((uid, m)) = w.broker.receive(&mut w.sim.rng) else {
        return;
    };
    let tx = w.cdb.unchecked_transaction().unwrap();
    let seen: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM inbox WHERE k=?1 AND seq=?2)",
            params![m.k, m.seq],
            |r| r.get(0),
        )
        .unwrap();
    if seen {
        w.sim.stats.deduped += 1;
    } else {
        tx.execute("INSERT OR IGNORE INTO replica VALUES (?1, 0, 1)", [&m.k])
            .unwrap();
        let next: i64 = tx
            .query_row("SELECT next_seq FROM replica WHERE k=?1", [&m.k], |r| {
                r.get(0)
            })
            .unwrap();
        if m.seq != next {
            w.sim.stats.deferred += 1; // from the future (reordered): leave it in the broker, it will come back
            return; // tx dropped
        }
        tx.execute("INSERT INTO inbox VALUES (?1, ?2)", params![m.k, m.seq])
            .unwrap();
        tx.execute(
            "UPDATE replica SET v=v+?2, next_seq=next_seq+1 WHERE k=?1",
            params![m.k, m.amount],
        )
        .unwrap();
        if w.sim.crash("consume:before_commit") {
            return; // effect and inbox row vanish together
        }
        tx.commit().unwrap();
        w.applied.entry(m.k.clone()).or_default().push(m.seq);
        if w.sim.crash("consume:after_commit_before_ack") {
            return; // broker redelivers; the inbox row makes it a no-op
        }
        w.broker.ack(uid);
        return;
    }
    tx.commit().unwrap();
    if !w.sim.crash("consume:dup_before_ack") {
        w.broker.ack(uid);
    }
}

// ---------------------------------------------------------------- schedules and oracle

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Poll,
    Tail,
}

fn unsent(w: &World, mode: Mode) -> i64 {
    match mode {
        Mode::Poll => w
            .pdb
            .query_row("SELECT COUNT(*) FROM outbox WHERE sent=0", [], |r| r.get(0))
            .unwrap(),
        Mode::Tail => w
            .pdb
            .query_row(
                "SELECT COUNT(*) FROM outbox WHERE id > (SELECT id FROM cursor)",
                [],
                |r| r.get(0),
            )
            .unwrap(),
    }
}

fn run_schedule(seed: u64, mode: Mode) -> World {
    let mut w = World::new(seed, true);
    let n_relays = if mode == Mode::Poll { 3 } else { 1 };
    let mut relays: Vec<Relay> = (0..n_relays).map(|_| w.new_relay()).collect();
    let mut left = 15 + w.sim.rng.below(25);
    for _ in 0..500 {
        w.now += 1;
        match w.sim.rng.below(20) {
            0..=5 if left > 0 => {
                left -= 1;
                producer_step(&mut w);
            }
            6..=12 => {
                let i = w.sim.rng.below(relays.len() as u64) as usize;
                match mode {
                    Mode::Poll => relay_step(&mut w, &mut relays[i]),
                    Mode::Tail => tail_step(&mut w, &mut relays[i]),
                }
            }
            13..=18 => consumer_step(&mut w),
            19 if mode == Mode::Poll => cleanup(&w),
            _ => {}
        }
    }
    // leftover commands are produced without chaos so the oracle's committed set is complete
    w.sim.crashes_on = false;
    while left > 0 {
        left -= 1;
        producer_step(&mut w);
    }
    // DRAIN: the world heals (no faults, no crashes); leases are long expired.
    w.broker.faults = false;
    w.now += 1000;
    for _ in 0..20_000 {
        w.now += 1;
        for r in relays.iter_mut() {
            match mode {
                Mode::Poll => relay_step(&mut w, r),
                Mode::Tail => tail_step(&mut w, r),
            }
        }
        consumer_step(&mut w);
        consumer_step(&mut w);
        if unsent(&w, mode) == 0 && w.broker.q.is_empty() {
            break;
        }
    }
    match mode {
        Mode::Poll => cleanup(&w),
        Mode::Tail => {
            w.pdb
                .execute("DELETE FROM outbox WHERE id <= (SELECT id FROM cursor)", [])
                .unwrap();
        }
    }
    w
}

fn verify(w: &World, seed: u64) {
    let total: usize = w.committed.values().map(|v| v.len()).sum();
    assert!(w.broker.q.is_empty(), "seed {seed}: broker did not drain");
    let left: i64 = w
        .pdb
        .query_row("SELECT COUNT(*) FROM outbox", [], |r| r.get(0))
        .unwrap();
    assert_eq!(left, 0, "seed {seed}: outbox did not drain to empty");
    let dead: i64 = w
        .pdb
        .query_row("SELECT COUNT(*) FROM dead_letter", [], |r| r.get(0))
        .unwrap();
    assert_eq!(dead, 0, "seed {seed}: unexpected dead letters");
    let inbox: i64 = w
        .cdb
        .query_row("SELECT COUNT(*) FROM inbox", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        inbox as usize, total,
        "seed {seed}: inbox rows != committed commands (lost or doubled effect)"
    );
    for (k, amounts) in &w.committed {
        let want: i64 = amounts.iter().sum();
        let (pv, pn): (i64, i64) = w
            .pdb
            .query_row("SELECT v,next_seq FROM balance WHERE k=?1", [k], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        let (cv, cn): (i64, i64) = w
            .cdb
            .query_row("SELECT v,next_seq FROM replica WHERE k=?1", [k], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(
            pv, want,
            "seed {seed}: producer state != sum of committed commands for {k}"
        );
        assert_eq!(
            cv, want,
            "seed {seed}: consumer state != producer state for {k} (effect lost or applied twice)"
        );
        assert_eq!(
            (pn, cn),
            (amounts.len() as i64 + 1, amounts.len() as i64 + 1),
            "seed {seed}: seq counters for {k}"
        );
        let order: Vec<i64> = w.applied.get(k).cloned().unwrap_or_default();
        let expect: Vec<i64> = (1..=amounts.len() as i64).collect();
        assert_eq!(
            order, expect,
            "seed {seed}: per-key apply order violated for {k}"
        );
    }
}

// ---------------------------------------------------------------- dead-letter demo

fn demo_dead_letter() {
    let mut w = World::new(7, false);
    w.broker.poison = Some("order-C".into());
    let mut relay = w.new_relay();
    // fixed commands: C gets 3 events, A gets 2
    for k in ["order-C", "order-A", "order-C", "order-A", "order-C"] {
        let tx = w.pdb.unchecked_transaction().unwrap();
        tx.execute("INSERT OR IGNORE INTO balance VALUES (?1, 0, 1)", [k])
            .unwrap();
        let seq: i64 = tx
            .query_row("SELECT next_seq FROM balance WHERE k=?1", [k], |r| r.get(0))
            .unwrap();
        tx.execute(
            "UPDATE balance SET v=v+10, next_seq=next_seq+1 WHERE k=?1",
            [k],
        )
        .unwrap();
        tx.execute(
            "INSERT INTO outbox(k, seq, amount) VALUES (?1, ?2, 10)",
            params![k, seq],
        )
        .unwrap();
        tx.commit().unwrap();
        w.committed.entry(k.to_string()).or_default().push(10);
    }
    for _ in 0..3000 {
        w.now += 1;
        relay_step(&mut w, &mut relay);
        consumer_step(&mut w);
    }
    let dead: i64 = w
        .pdb
        .query_row("SELECT COUNT(*) FROM dead_letter", [], |r| r.get(0))
        .unwrap();
    let blocked: i64 = w
        .pdb
        .query_row(
            "SELECT COUNT(*) FROM outbox WHERE k='order-C' AND sent=0",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let a_done: i64 = w
        .cdb
        .query_row("SELECT next_seq FROM replica WHERE k='order-A'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(dead, 1, "exactly the poison head is dead-lettered");
    assert_eq!(
        blocked, 3,
        "its successors stay queued: skipping the head would reorder the key"
    );
    assert_eq!(
        a_done, 3,
        "other keys are not blocked (head-of-line blocking is per key)"
    );
    println!(
        "dead-letter demo: poison head parked ({dead} dead letter), {blocked} C events held back, key A fully delivered"
    );
    // operator fixes the cause and requeues
    w.broker.poison = None;
    w.pdb.execute("UPDATE outbox SET parked=0, attempts=0, next_at=0 WHERE id IN (SELECT id FROM dead_letter)", []).unwrap();
    w.pdb.execute("DELETE FROM dead_letter", []).unwrap();
    for _ in 0..200 {
        w.now += 1;
        relay_step(&mut w, &mut relay);
        consumer_step(&mut w);
    }
    cleanup(&w);
    verify(&w, 7);
    println!("  after requeue: all 5 events applied in per-key order, outbox empty");
}

fn main() {
    demo_dead_letter();

    for (mode, name, seeds) in [
        (Mode::Poll, "polling relay x3", 3000u64),
        (Mode::Tail, "tail (cursor) relay", 600),
    ] {
        let mut hits: BTreeMap<&'static str, u64> = BTreeMap::new();
        let mut stats = Stats::default();
        let mut events = 0usize;
        for seed in 1..=seeds {
            let w = run_schedule(seed, mode);
            verify(&w, seed);
            events += w.committed.values().map(|v| v.len()).sum::<usize>();
            stats.add(&w.sim.stats);
            for (k, v) in &w.sim.hits {
                *hits.entry(k).or_default() += v;
            }
        }
        println!(
            "\n{name}: {seeds} schedules OK, {events} committed events, each applied exactly once, in order, outbox drained"
        );
        println!(
            "  publishes={} broker_dups={} lost_acks={} outages={} retries={} consumer: deduped={} deferred(reordered)={}",
            stats.publishes,
            stats.broker_dups,
            stats.lost_acks,
            stats.outages,
            stats.retries,
            stats.deduped,
            stats.deferred
        );
        println!(
            "  relay write-transactions per event: {:.2}",
            stats.relay_writes as f64 / events as f64
        );
        println!("  crash points fired: {hits:?}");
        assert!(
            stats.deduped > 0 && stats.deferred > 0 && stats.lost_acks > 0 && stats.outages > 0,
            "faults must really occur"
        );
        let expected: &[&str] = match mode {
            Mode::Poll => &[
                "producer:before_commit",
                "producer:after_commit",
                "claim:before_commit",
                "claim:after_commit",
                "publish:after_publish_before_mark",
                "mark:before_commit",
                "mark:after_commit",
                "consume:before_commit",
                "consume:after_commit_before_ack",
            ],
            Mode::Tail => &[
                "producer:before_commit",
                "tail:mid_batch",
                "tail:cursor_before_commit",
                "consume:before_commit",
            ],
        };
        for label in expected {
            assert!(
                hits.get(label).copied().unwrap_or(0) > 0,
                "crash point {label} never fired"
            );
        }
    }
    println!(
        "\nsummary: outbox gives atomic state+event; at-least-once relay + inbox gives effectively-once; ordering needs seq per key."
    );
}
