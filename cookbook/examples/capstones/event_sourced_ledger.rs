//! CAPSTONE 10 (database): an event-sourced bank ledger with CQRS projections, snapshots, upcasters and an outbox
//! LEVEL: capstone
//! Use in: any system that must explain HOW it reached its state (finance, inventory, audit-heavy domains); also the core idea behind database logs.   Run: cargo run --release --example event_sourced_ledger
#![allow(dead_code)]
//!
//! WHAT: state is never stored, only the facts that produced it. The EVENT STORE is append-only, has one totally ordered global
//! log plus a version counter per stream, and `append(stream, expected_version, events)` fails with a Conflict when another
//! writer got there first (optimistic concurrency, no locks). Two stores share one trait: in memory, and file-backed with
//! the framing of wal_append_log (len + crc32 per BATCH, so a torn write loses a whole batch, never half of one).
//! COMMAND SIDE: `decide(state, command) -> Result<Vec<Event>, DomainError>` is pure; `evolve(state, event) -> state` is
//! pure and total (an event is a fact that already happened, so it must never fail). Commands carry an id; the aggregate
//! remembers the ids it has seen, so a retried or duplicated command is a no-op. SNAPSHOTS every N events bound replay cost
//! and are only an optimisation: snapshot + tail must equal full replay (asserted).
//! QUERY SIDE (CQRS): read models (balances, daily totals, top spenders) subscribe to the global log with a CHECKPOINT. They
//! are disposable: rebuilding from position 0 must give exactly the incrementally maintained model (asserted).
//! EVOLUTION: events live forever, so old shapes are kept as stored and UPCAST on read (v1 -> v2) instead of rewriting history.
//! UNDO: a mistake is corrected by a compensating event (`Reversed`), never by deleting; the audit trail stays complete.
//! OUTBOX: events to publish to the outside world are recorded in the same log as the state change (one atomic write), and a
//! relay delivers them AT LEAST ONCE (acks get lost); the consumer dedupes by event id, giving effectively-once.
//! TRANSFERS span two aggregates, which a single-stream append cannot make atomic. So a transfer is two commands: the source
//! emits `TransferredOut`, a reactor (process manager) later sends `ReceiveTransfer` to the target with a deterministic
//! command id (idempotent under redelivery). Money is conserved as: balances + in-flight (out - in) == external net.
//! HOW (verification): seeded xorshift drives several writers whose load / commit steps are interleaved deterministically
//! (so stale reads and conflicts really happen), a flaky reactor and relay, then checks every invariant. Finally the file
//! store is truncated mid-batch, reopened, and reprojected.
//! COMMON USES: bank and payment ledgers, order and inventory systems, audit trails, undo history, debugging by replay.
//! Real-world: EventStoreDB, Axon, Marten (.NET), Kafka log + Kafka Streams as projections, DynamoDB Streams, the
//! transactional-outbox pattern with Debezium; double-entry accounting is the oldest event-sourced system.
//! Read: https://martinfowler.com/eaaDev/EventSourcing.html , https://martinfowler.com/bliki/CQRS.html ,
//! https://microservices.io/patterns/data/transactional-outbox.html and
//! https://martin.kleppmann.com/2012/12/05/schema-evolution-in-avro-protocol-buffers-thrift.html
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use thiserror::Error;

// ---------------------------------------------------------------- tiny utilities

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
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
    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }
}

fn crc32(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = (c >> 1) ^ (0xEDB8_8320 & 0u32.wrapping_sub(c & 1));
        }
    }
    !c
}

fn fnv(bytes: &[u8], mut h: u64) -> u64 {
    for &b in bytes {
        h = (h ^ b as u64).wrapping_mul(0x100_0000_01b3);
    }
    h
}

// ---------------------------------------------------------------- domain: events, commands, aggregate

/// The CURRENT event shape (schema 2). Adjacently tagged so the stored JSON is `{"type": .., "data": {..}}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
enum Event {
    Opened,
    Deposited {
        amount: i64,
    },
    Withdrawn {
        amount: i64,
        reason: String,
    },
    TransferredOut {
        transfer: u64,
        to: String,
        amount: i64,
    },
    TransferredIn {
        transfer: u64,
        from: String,
        amount: i64,
    },
    /// compensating event: `delta` is the balance change that cancels the effect of the event at version `target`
    Reversed {
        target: u64,
        delta: i64,
    },
}

#[derive(Debug, Clone, PartialEq)]
enum Command {
    Open,
    Deposit {
        amount: i64,
    },
    Withdraw {
        amount: i64,
        reason: String,
    },
    TransferOut {
        transfer: u64,
        to: String,
        amount: i64,
    },
    ReceiveTransfer {
        transfer: u64,
        from: String,
        amount: i64,
    },
    Reverse {
        target: u64,
    },
}

#[derive(Debug, Error, PartialEq, Clone)]
enum DomainError {
    #[error("account is not open")]
    NotOpen,
    #[error("account is already open")]
    AlreadyOpen,
    #[error("insufficient funds: have {have}, need {need}")]
    Insufficient { have: i64, need: i64 },
    #[error("amount must be positive")]
    NonPositive,
    #[error("event at version {0} cannot be reversed")]
    NotReversible(u64),
    #[error("event at version {0} was already reversed")]
    AlreadyReversed(u64),
}

/// The aggregate state. `version` is the stream version it reflects; `cmds` powers idempotency; `deltas` remembers which
/// versions changed the balance (and by how much) so a later `Reverse` can be validated without reading history.
#[derive(Debug, Clone, Default, PartialEq)]
struct Account {
    version: u64,
    open: bool,
    balance: i64,
    cmds: BTreeSet<u64>,
    deltas: BTreeMap<u64, i64>,
    reversed: BTreeSet<u64>,
}

fn positive(amount: i64) -> Result<i64, DomainError> {
    if amount > 0 {
        Ok(amount)
    } else {
        Err(DomainError::NonPositive)
    }
}

fn require_funds(a: &Account, need: i64) -> Result<(), DomainError> {
    if a.balance >= need {
        Ok(())
    } else {
        Err(DomainError::Insufficient {
            have: a.balance,
            need,
        })
    }
}

/// PURE: look at the state, decide which facts to record. No IO, no clock, so it is trivially testable.
fn decide(a: &Account, cmd: &Command) -> Result<Vec<Event>, DomainError> {
    if let Command::Open = cmd {
        return if a.open {
            Err(DomainError::AlreadyOpen)
        } else {
            Ok(vec![Event::Opened])
        };
    }
    if !a.open {
        return Err(DomainError::NotOpen);
    }
    let event = match cmd {
        Command::Open => unreachable!("handled above"),
        Command::Deposit { amount } => Event::Deposited {
            amount: positive(*amount)?,
        },
        Command::Withdraw { amount, reason } => {
            require_funds(a, positive(*amount)?)?;
            Event::Withdrawn {
                amount: *amount,
                reason: reason.clone(),
            }
        }
        Command::TransferOut {
            transfer,
            to,
            amount,
        } => {
            require_funds(a, positive(*amount)?)?;
            Event::TransferredOut {
                transfer: *transfer,
                to: to.clone(),
                amount: *amount,
            }
        }
        // the receiving side can never refuse: the money already left the source
        Command::ReceiveTransfer {
            transfer,
            from,
            amount,
        } => Event::TransferredIn {
            transfer: *transfer,
            from: from.clone(),
            amount: positive(*amount)?,
        },
        Command::Reverse { target } => {
            let delta = *a
                .deltas
                .get(target)
                .ok_or(DomainError::NotReversible(*target))?;
            if a.reversed.contains(target) {
                return Err(DomainError::AlreadyReversed(*target));
            }
            require_funds(a, delta)?; // undoing a deposit must not overdraw
            Event::Reversed {
                target: *target,
                delta: -delta,
            }
        }
    };
    Ok(vec![event])
}

/// PURE and TOTAL: applies a fact that already happened. Never validates (validation belongs to `decide`).
fn evolve(mut a: Account, e: &Event) -> Account {
    a.version += 1;
    match e {
        Event::Opened => a.open = true,
        Event::Deposited { amount } => {
            a.balance += amount;
            a.deltas.insert(a.version, *amount);
        }
        Event::Withdrawn { amount, .. } => {
            a.balance -= amount;
            a.deltas.insert(a.version, -amount);
        }
        Event::TransferredOut { amount, .. } => a.balance -= amount,
        Event::TransferredIn { amount, .. } => a.balance += amount,
        Event::Reversed { target, delta } => {
            a.balance += delta;
            a.reversed.insert(*target);
        }
    }
    a
}

// ---------------------------------------------------------------- stored events and schema versioning

const SCHEMA: u32 = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Stored {
    pos: u64, // position in the global log, gapless from 0
    stream: String,
    version: u64, // position in the stream, gapless from 1
    cmd: u64,
    ts: u64, // seconds; supplied by the writer's clock, never read inside decide/evolve
    schema: u32,
    payload: Value,
}

struct NewEvent {
    cmd: u64,
    ts: u64,
    schema: u32,
    payload: Value,
}

impl NewEvent {
    fn current(cmd: u64, ts: u64, e: &Event) -> NewEvent {
        NewEvent {
            cmd,
            ts,
            schema: SCHEMA,
            payload: serde_json::to_value(e).expect("event serialises"),
        }
    }
}

/// One upcasting step: schema n -> n+1 for one event type. v1 used different field names and had no `reason`.
fn upcast_step(kind: &str, data: &mut Value) {
    let Some(obj) = data.as_object_mut() else {
        return;
    };
    match kind {
        "Deposited" => {
            if let Some(c) = obj.remove("cents") {
                obj.insert("amount".into(), c);
            }
        }
        "Withdrawn" => {
            if let Some(c) = obj.remove("amt") {
                obj.insert("amount".into(), c);
            }
            obj.entry("reason").or_insert(json!("legacy"));
        }
        _ => {}
    }
}

/// Old events are upcast IN MEMORY on read; the bytes on disk are never rewritten (history is immutable).
fn upcast(schema: u32, mut payload: Value) -> Value {
    for _ in schema..SCHEMA {
        let kind = payload["type"].as_str().unwrap_or_default().to_owned();
        if let Some(data) = payload.get_mut("data") {
            upcast_step(&kind, data);
        }
    }
    payload
}

fn decode(s: &Stored) -> Event {
    serde_json::from_value(upcast(s.schema, s.payload.clone()))
        .expect("stored event decodes after upcasting")
}

fn fold(acc: Account, s: &Stored) -> Account {
    debug_assert_eq!(acc.version + 1, s.version, "versions must be gapless");
    let mut next = evolve(acc, &decode(s));
    next.cmds.insert(s.cmd); // the envelope, not the event, carries the command id
    next
}

// ---------------------------------------------------------------- event store: trait, memory, file

#[derive(Debug, Error)]
enum StoreError {
    #[error("version conflict on {stream}: expected {expected}, actual {actual}")]
    Conflict {
        stream: String,
        expected: u64,
        actual: u64,
    },
    #[error("io: {0}")]
    Io(#[from] io::Error),
}

trait EventStore {
    /// atomically append a batch iff the stream is at `expected` (0 = empty stream)
    fn append(
        &mut self,
        stream: &str,
        expected: u64,
        events: Vec<NewEvent>,
    ) -> Result<Vec<Stored>, StoreError>;
    /// events of one stream with version > `after`
    fn read_stream(&self, stream: &str, after: u64) -> Vec<Stored>;
    /// the global log from position `from` (the subscription primitive)
    fn read_all(&self, from: u64) -> &[Stored];
    fn version(&self, stream: &str) -> u64;
    fn streams(&self) -> Vec<String>;
}

#[derive(Default)]
struct MemStore {
    log: Vec<Stored>,
    by_stream: HashMap<String, Vec<usize>>,
}

impl MemStore {
    /// validate and assign positions WITHOUT mutating, so a durable store can write before it commits to memory
    fn prepare(
        &self,
        stream: &str,
        expected: u64,
        events: Vec<NewEvent>,
    ) -> Result<Vec<Stored>, StoreError> {
        let actual = self.version(stream);
        if actual != expected {
            return Err(StoreError::Conflict {
                stream: stream.into(),
                expected,
                actual,
            });
        }
        let base = self.log.len() as u64;
        Ok(events
            .into_iter()
            .enumerate()
            .map(|(i, e)| Stored {
                pos: base + i as u64,
                stream: stream.into(),
                version: expected + 1 + i as u64,
                cmd: e.cmd,
                ts: e.ts,
                schema: e.schema,
                payload: e.payload,
            })
            .collect())
    }

    fn commit(&mut self, batch: Vec<Stored>) {
        for s in batch {
            self.by_stream
                .entry(s.stream.clone())
                .or_default()
                .push(self.log.len());
            self.log.push(s);
        }
    }
}

impl EventStore for MemStore {
    fn append(
        &mut self,
        stream: &str,
        expected: u64,
        events: Vec<NewEvent>,
    ) -> Result<Vec<Stored>, StoreError> {
        let batch = self.prepare(stream, expected, events)?;
        self.commit(batch.clone());
        Ok(batch)
    }
    fn read_stream(&self, stream: &str, after: u64) -> Vec<Stored> {
        let idx = self.by_stream.get(stream).map(Vec::as_slice).unwrap_or(&[]);
        idx.iter()
            .skip(after as usize)
            .map(|&i| self.log[i].clone())
            .collect()
    }
    fn read_all(&self, from: u64) -> &[Stored] {
        self.log.get(from as usize..).unwrap_or(&[])
    }
    fn version(&self, stream: &str) -> u64 {
        self.by_stream.get(stream).map_or(0, |v| v.len() as u64)
    }
    fn streams(&self) -> Vec<String> {
        let mut s: Vec<String> = self.by_stream.keys().cloned().collect();
        s.sort();
        s
    }
}

#[derive(Debug, PartialEq)]
enum Tail {
    Clean,
    Truncated { at: u64 },
    BadChecksum { at: u64 },
}

/// scan [len u32][crc u32][payload] frames; stop at the first torn or corrupt one (same format as wal_append_log)
fn scan(bytes: &[u8]) -> (Vec<&[u8]>, usize, Tail) {
    let (mut recs, mut pos) = (vec![], 0usize);
    loop {
        if pos == bytes.len() {
            return (recs, pos, Tail::Clean);
        }
        let Some(h) = bytes.get(pos..pos + 8) else {
            return (recs, pos, Tail::Truncated { at: pos as u64 });
        };
        let len = u32::from_le_bytes(h[..4].try_into().unwrap()) as usize;
        let crc = u32::from_le_bytes(h[4..].try_into().unwrap());
        let Some(p) = bytes.get(pos + 8..pos + 8 + len) else {
            return (recs, pos, Tail::Truncated { at: pos as u64 });
        };
        if crc32(p) != crc {
            return (recs, pos, Tail::BadChecksum { at: pos as u64 });
        }
        recs.push(p);
        pos += 8 + len;
    }
}

/// File-backed store: WRITE-AHEAD (frame hits the file before the in-memory index changes). One frame = one batch, so
/// appending several events to a stream is atomic across crashes.
struct FileStore {
    mem: MemStore,
    file: File,
    sync_each: bool,
}

impl FileStore {
    fn open(path: &Path, sync_each: bool) -> io::Result<(FileStore, Tail)> {
        let bytes = if path.exists() {
            fs::read(path)?
        } else {
            vec![]
        };
        let (recs, valid, tail) = scan(&bytes);
        let mut mem = MemStore::default();
        for r in recs {
            mem.commit(serde_json::from_slice(r).map_err(io::Error::other)?);
        }
        if valid != bytes.len() {
            // drop the torn tail so new frames start on a clean boundary
            OpenOptions::new()
                .write(true)
                .open(path)?
                .set_len(valid as u64)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok((
            FileStore {
                mem,
                file,
                sync_each,
            },
            tail,
        ))
    }

    fn sync(&self) -> io::Result<()> {
        self.file.sync_data()
    }
}

impl EventStore for FileStore {
    fn append(
        &mut self,
        stream: &str,
        expected: u64,
        events: Vec<NewEvent>,
    ) -> Result<Vec<Stored>, StoreError> {
        let batch = self.mem.prepare(stream, expected, events)?;
        let payload = serde_json::to_vec(&batch).map_err(io::Error::other)?;
        let mut frame = Vec::with_capacity(8 + payload.len());
        frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        frame.extend_from_slice(&crc32(&payload).to_le_bytes());
        frame.extend_from_slice(&payload);
        self.file.write_all(&frame)?; // one write call keeps the frame contiguous
        if self.sync_each {
            self.file.sync_data()?; // fsync per batch is the durable (slow) mode; the simulation batches syncs instead
        }
        self.mem.commit(batch.clone());
        Ok(batch)
    }
    fn read_stream(&self, stream: &str, after: u64) -> Vec<Stored> {
        self.mem.read_stream(stream, after)
    }
    fn read_all(&self, from: u64) -> &[Stored] {
        self.mem.read_all(from)
    }
    fn version(&self, stream: &str) -> u64 {
        self.mem.version(stream)
    }
    fn streams(&self) -> Vec<String> {
        self.mem.streams()
    }
}

// ---------------------------------------------------------------- command handling with snapshots

#[derive(Debug, PartialEq)]
enum Outcome {
    Applied(usize),
    Duplicate,
}

#[derive(Debug, Error)]
enum CmdError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error(transparent)]
    Store(#[from] StoreError),
}

fn is_conflict(e: &CmdError) -> bool {
    matches!(e, CmdError::Store(StoreError::Conflict { .. }))
}

struct Ledger<S: EventStore> {
    store: S,
    snaps: HashMap<String, Account>,
    snap_every: u64,
    now: u64,
    snapshot_hits: u64,
}

impl<S: EventStore> Ledger<S> {
    fn new(store: S, snap_every: u64) -> Self {
        Ledger {
            store,
            snaps: HashMap::new(),
            snap_every,
            now: 0,
            snapshot_hits: 0,
        }
    }

    /// latest snapshot (if any) + the tail of events after it
    fn load(&mut self, stream: &str) -> Account {
        let base = self.snaps.get(stream).cloned();
        self.snapshot_hits += base.is_some() as u64;
        let acc = base.unwrap_or_default();
        self.store
            .read_stream(stream, acc.version)
            .iter()
            .fold(acc, fold)
    }

    fn load_full(&self, stream: &str) -> Account {
        self.state_at(stream, u64::MAX)
    }

    /// TIME TRAVEL: the aggregate as it was right after event `version`
    fn state_at(&self, stream: &str, version: u64) -> Account {
        self.store
            .read_stream(stream, 0)
            .iter()
            .take_while(|s| s.version <= version)
            .fold(Account::default(), fold)
    }

    /// One commit attempt against an already-loaded (possibly stale) state. Splitting load from commit lets the simulation
    /// interleave writers between the two, which is exactly where real conflicts come from.
    fn try_commit(
        &mut self,
        stream: &str,
        loaded: &Account,
        id: u64,
        cmd: &Command,
    ) -> Result<Outcome, CmdError> {
        if loaded.cmds.contains(&id) {
            return Ok(Outcome::Duplicate); // idempotent: already handled, answer "fine" without new events
        }
        let events = decide(loaded, cmd)?;
        let new = events
            .iter()
            .map(|e| NewEvent::current(id, self.now, e))
            .collect();
        let stored = self.store.append(stream, loaded.version, new)?;
        let next = stored.iter().fold(loaded.clone(), fold);
        if next.version / self.snap_every > loaded.version / self.snap_every {
            self.snaps.insert(stream.to_owned(), next); // crossed a multiple of N: keep a snapshot
        }
        Ok(Outcome::Applied(stored.len()))
    }

    /// load + commit with retry on conflict (the normal single-threaded path)
    fn execute(&mut self, stream: &str, id: u64, cmd: &Command) -> Result<Outcome, DomainError> {
        loop {
            let acc = self.load(stream);
            match self.try_commit(stream, &acc, id, cmd) {
                Ok(o) => return Ok(o),
                Err(CmdError::Domain(d)) => return Err(d),
                Err(e) if is_conflict(&e) => continue,
                Err(e) => panic!("unexpected store failure: {e}"),
            }
        }
    }
}

// ---------------------------------------------------------------- projections (CQRS read side)

#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct Day {
    deposited: i64,
    withdrawn: i64,
    transferred: i64,
    events: u64,
}

/// Three read models in one struct, sharing one checkpoint (`next_pos`). Everything is derived: delete it and rebuild.
#[derive(Debug, Default, Clone, PartialEq)]
struct ReadModels {
    next_pos: u64,
    balances: BTreeMap<String, i64>,
    daily: BTreeMap<u64, Day>,
    spent: BTreeMap<String, i64>,
}

impl ReadModels {
    fn apply(&mut self, s: &Stored) {
        let day = self.daily.entry(s.ts / 86_400).or_default();
        day.events += 1;
        let bal = self.balances.entry(s.stream.clone()).or_insert(0);
        let spent = self.spent.entry(s.stream.clone()).or_insert(0);
        match decode(s) {
            Event::Opened => {}
            Event::Deposited { amount } => {
                *bal += amount;
                day.deposited += amount;
            }
            Event::Withdrawn { amount, .. } => {
                *bal -= amount;
                *spent += amount;
                day.withdrawn += amount;
            }
            Event::TransferredOut { amount, .. } => {
                *bal -= amount;
                *spent += amount;
                day.transferred += amount;
            }
            Event::TransferredIn { amount, .. } => *bal += amount,
            Event::Reversed { delta, .. } => {
                *bal += delta;
                // a positive delta gave money back (undoing a spend); a negative one removed a deposit
                if delta > 0 {
                    *spent -= delta;
                    day.withdrawn -= delta;
                } else {
                    day.deposited += delta;
                }
            }
        }
    }

    /// subscribe: process everything after the checkpoint, then advance it (at-least-once safe because apply is only
    /// ever followed by the checkpoint bump in the same in-memory step; a durable model would store both atomically)
    fn catch_up<S: EventStore>(&mut self, store: &S) {
        for s in store.read_all(self.next_pos) {
            self.apply(s);
            self.next_pos = s.pos + 1;
        }
    }

    fn rebuild<S: EventStore>(store: &S) -> ReadModels {
        let mut rm = ReadModels::default();
        rm.catch_up(store);
        rm
    }

    /// time travel on the read side: the model as of global position `pos` (exclusive)
    fn as_of<S: EventStore>(store: &S, pos: u64) -> ReadModels {
        let mut rm = ReadModels::default();
        for s in store.read_all(0).iter().take_while(|s| s.pos < pos) {
            rm.apply(s);
            rm.next_pos = s.pos + 1;
        }
        rm
    }

    fn top_spenders(&self, n: usize) -> Vec<(String, i64)> {
        let mut v: Vec<(String, i64)> = self.spent.iter().map(|(k, v)| (k.clone(), *v)).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        v.truncate(n);
        v
    }
}

// ---------------------------------------------------------------- reactor (process manager) and outbox

const RECEIVE_BASE: u64 = 1 << 62;

/// Turns `TransferredOut` into `ReceiveTransfer` on the target. At-least-once: it may crash before saving its checkpoint
/// and re-process, or deliver twice; the deterministic command id makes every repeat a no-op.
#[derive(Default)]
struct Reactor {
    next_pos: u64,
    sent: u64,
    absorbed: u64,
    crash_pct: u64,
}

impl Reactor {
    fn tick<S: EventStore>(&mut self, ledger: &mut Ledger<S>, rng: &mut Rng) {
        let fresh: Vec<Stored> = ledger.store.read_all(self.next_pos).to_vec();
        for s in &fresh {
            if let Event::TransferredOut {
                transfer,
                to,
                amount,
            } = decode(s)
            {
                let cmd = Command::ReceiveTransfer {
                    transfer,
                    from: s.stream.clone(),
                    amount,
                };
                let times = if rng.chance(25) { 2 } else { 1 }; // duplicate delivery
                for _ in 0..times {
                    match ledger
                        .execute(&to, RECEIVE_BASE | transfer, &cmd)
                        .expect("receive never fails")
                    {
                        Outcome::Applied(_) => self.sent += 1,
                        Outcome::Duplicate => self.absorbed += 1,
                    }
                }
            }
        }
        if !rng.chance(self.crash_pct) {
            self.next_pos += fresh.len() as u64; // "crash before checkpoint" = skip this line
        }
    }
}

fn publishable(s: &Stored) -> bool {
    matches!(
        decode(s),
        Event::TransferredOut { .. } | Event::TransferredIn { .. }
    )
}

/// The relay half of the outbox: rows that still need delivery. A row leaves `pending` only after an ack, and acks get lost.
#[derive(Default)]
struct Outbox {
    next_pos: u64,
    pending: VecDeque<u64>,
    marked: u64,
}

#[derive(Default)]
struct Consumer {
    seen: HashSet<u64>,
    received: u64,
    applied: Vec<u64>,
}

impl Consumer {
    fn deliver(&mut self, event_id: u64) {
        self.received += 1;
        if self.seen.insert(event_id) {
            self.applied.push(event_id); // first time only: dedupe by event id
        }
    }
}

impl Outbox {
    fn scan<S: EventStore>(&mut self, store: &S) {
        for s in store.read_all(self.next_pos) {
            if publishable(s) {
                self.pending.push_back(s.pos);
            }
            self.next_pos = s.pos + 1;
        }
    }

    fn pump(&mut self, consumer: &mut Consumer, rng: &mut Rng) {
        for _ in 0..self.pending.len() {
            let Some(id) = self.pending.pop_front() else {
                break;
            };
            consumer.deliver(id);
            if rng.chance(20) {
                consumer.deliver(id); // broker redelivery
            }
            if rng.chance(70) {
                self.marked += 1; // ack received: mark published
            } else {
                self.pending.push_back(id); // ack lost: will be sent again
            }
        }
    }
}

// ---------------------------------------------------------------- the simulation

fn acct(i: u64) -> String {
    format!("acct-{i}")
}

struct Job {
    stream: String,
    id: u64,
    cmd: Command,
    loaded: Option<Account>,
}

#[derive(Default, Debug)]
struct Stats {
    applied: u64,
    rejected: u64,
    conflicts: u64,
    duplicates: u64,
}

struct Sim<S: EventStore> {
    ledger: Ledger<S>,
    rng: Rng,
    accounts: u64,
    writers: Vec<(u64, Option<Job>)>, // (sequence counter, in-flight job)
    rm: ReadModels,
    reactor: Reactor,
    outbox: Outbox,
    consumer: Consumer,
    stats: Stats,
}

impl<S: EventStore> Sim<S> {
    fn new(store: S, seed: u64, writers: usize, accounts: u64) -> Self {
        let mut ledger = Ledger::new(store, 8);
        for i in 0..accounts {
            ledger
                .execute(&acct(i), (1 << 40) | i, &Command::Open)
                .unwrap();
            ledger
                .execute(&acct(i), (1 << 41) | i, &Command::Deposit { amount: 500 })
                .unwrap();
        }
        Sim {
            ledger,
            rng: Rng::new(seed),
            accounts,
            writers: (0..writers).map(|_| (0, None)).collect(),
            rm: ReadModels::default(),
            reactor: Reactor {
                crash_pct: 10,
                ..Reactor::default()
            },
            outbox: Outbox::default(),
            consumer: Consumer::default(),
            stats: Stats::default(),
        }
    }

    fn gen_job(&mut self, w: usize) -> Job {
        self.writers[w].0 += 1;
        let id = ((w as u64 + 1) << 32) | self.writers[w].0;
        let from = self.rng.below(self.accounts);
        let stream = acct(from);
        let amount = 1 + self.rng.below(200) as i64;
        let cmd = match self.rng.below(100) {
            0..35 => Command::Deposit { amount },
            35..55 => Command::Withdraw {
                amount,
                reason: ["rent", "food", "fun"][self.rng.below(3) as usize].into(),
            },
            55..85 => {
                let to = acct((from + 1 + self.rng.below(self.accounts - 1)) % self.accounts);
                Command::TransferOut {
                    transfer: id,
                    to,
                    amount,
                }
            }
            _ => Command::Reverse {
                target: 1 + self.rng.below(self.ledger.store.version(&stream) + 1),
            },
        };
        Job {
            stream,
            id,
            cmd,
            loaded: None,
        }
    }

    /// one scheduler tick: advance ONE writer by ONE step (load, or commit), then maybe run background machinery
    fn step(&mut self, generate: bool) {
        self.ledger.now += self.rng.below(3000);
        let w = self.rng.below(self.writers.len() as u64) as usize;
        let job = match self.writers[w].1.take() {
            Some(j) => Some(j),
            None if generate => Some(self.gen_job(w)),
            None => None,
        };
        if let Some(job) = job {
            self.writers[w].1 = self.advance(job);
        }
        if self.rng.chance(30) {
            self.reactor.tick(&mut self.ledger, &mut self.rng);
        }
        if self.rng.chance(25) {
            self.rm.catch_up(&self.ledger.store);
        }
        if self.rng.chance(30) {
            self.outbox.scan(&self.ledger.store);
            self.outbox.pump(&mut self.consumer, &mut self.rng);
        }
    }

    /// returns the job if it must continue (after loading, or after a conflict), None when finished
    fn advance(&mut self, mut job: Job) -> Option<Job> {
        let Some(acc) = job.loaded.take() else {
            job.loaded = Some(self.ledger.load(&job.stream)); // may be stale by the time we commit
            return Some(job);
        };
        match self.ledger.try_commit(&job.stream, &acc, job.id, &job.cmd) {
            Ok(Outcome::Applied(_)) => self.stats.applied += 1,
            Ok(Outcome::Duplicate) => self.stats.duplicates += 1,
            Err(CmdError::Domain(_)) => self.stats.rejected += 1,
            Err(e) if is_conflict(&e) => {
                self.stats.conflicts += 1;
                return Some(job); // retry from a fresh load, SAME command id
            }
            Err(e) => panic!("{e}"),
        }
        None
    }

    fn run(&mut self, steps: usize) {
        for i in 0..steps {
            self.step(true);
            if i % 700 == 699 {
                check_money(&self.ledger.store, &self.rm.clone_balances(), false);
            }
        }
    }

    /// finish in-flight commands, drain the reactor and outbox, catch the projections up
    fn quiesce(&mut self) {
        while self.writers.iter().any(|w| w.1.is_some()) {
            self.step(false);
        }
        self.reactor.crash_pct = 0;
        self.reactor.tick(&mut self.ledger, &mut self.rng);
        self.rm.catch_up(&self.ledger.store);
        while {
            self.outbox.scan(&self.ledger.store);
            !self.outbox.pending.is_empty()
        } {
            self.outbox.pump(&mut self.consumer, &mut self.rng);
        }
    }
}

impl ReadModels {
    fn clone_balances(&self) -> BTreeMap<String, i64> {
        self.balances.clone()
    }
}

/// external net = money that entered/left the bank; in flight = out - in. Conservation: sum(balances) + in_flight == net
fn totals<S: EventStore>(store: &S) -> (i64, i64, i64) {
    let (mut net, mut outs, mut ins) = (0, 0, 0);
    for s in store.read_all(0) {
        match decode(s) {
            Event::Deposited { amount } => net += amount,
            Event::Withdrawn { amount, .. } => net -= amount,
            Event::Reversed { delta, .. } => net += delta,
            Event::TransferredOut { amount, .. } => outs += amount,
            Event::TransferredIn { amount, .. } => ins += amount,
            Event::Opened => {}
        }
    }
    (net, outs, ins)
}

fn check_money<S: EventStore>(store: &S, _rm_balances: &BTreeMap<String, i64>, drained: bool) {
    let (net, outs, ins) = totals(store);
    let sum: i64 = store
        .streams()
        .iter()
        .map(|s| {
            store
                .read_stream(s, 0)
                .iter()
                .fold(Account::default(), fold)
                .balance
        })
        .sum();
    assert_eq!(
        sum + outs - ins,
        net,
        "money conservation (balances + in flight == external net)"
    );
    assert!(outs >= ins, "cannot receive more than was sent");
    if drained {
        assert_eq!(outs, ins, "after draining the reactor nothing is in flight");
    }
}

/// every structural invariant of the finished run
fn verify<S: EventStore>(sim: &mut Sim<S>) {
    let all = sim.ledger.store.read_all(0).to_vec();
    for (i, s) in all.iter().enumerate() {
        assert_eq!(s.pos, i as u64, "global positions are gapless");
    }
    for stream in sim.ledger.store.streams() {
        let evs = sim.ledger.store.read_stream(&stream, 0);
        let mut acc = Account::default();
        for (i, s) in evs.iter().enumerate() {
            assert_eq!(s.version, i as u64 + 1, "stream versions are gapless");
            acc = fold(acc, s);
            assert!(
                acc.balance >= 0,
                "{stream}: negative balance at version {}",
                s.version
            );
        }
        assert_eq!(
            sim.ledger.load(&stream),
            sim.ledger.load_full(&stream),
            "snapshot + tail == full replay"
        );
        assert_eq!(
            sim.ledger.load_full(&stream),
            acc,
            "replay is deterministic"
        );
    }
    // incremental projections == rebuilt from scratch == the aggregates
    let rebuilt = ReadModels::rebuild(&sim.ledger.store);
    assert_eq!(
        sim.rm, rebuilt,
        "incrementally maintained read models == rebuilt"
    );
    for (stream, bal) in &rebuilt.balances {
        assert_eq!(
            *bal,
            sim.ledger.load_full(stream).balance,
            "balances projection == aggregate"
        );
    }
    check_money(&sim.ledger.store, &rebuilt.balances, true);
    // outbox: every publishable event was applied by the consumer exactly once, despite duplicate deliveries
    let want: BTreeSet<u64> = all
        .iter()
        .filter(|s| publishable(s))
        .map(|s| s.pos)
        .collect();
    let got: BTreeSet<u64> = sim.consumer.seen.iter().copied().collect();
    assert_eq!(want, got, "every outbox event delivered");
    assert_eq!(
        sim.consumer.applied.len(),
        want.len(),
        "consumer applied each event exactly once"
    );
    assert!(sim.consumer.received >= sim.consumer.applied.len() as u64);
}

fn fingerprint(log: &[Stored]) -> u64 {
    log.iter().fold(0xcbf2_9ce4_8422_2325, |h, s| {
        fnv(&serde_json::to_vec(s).unwrap(), h)
    })
}

// ---------------------------------------------------------------- demos

fn demo_basics() {
    let mut l = Ledger::new(MemStore::default(), 4);
    let a = "acct-demo";
    l.execute(a, 1, &Command::Open).unwrap();
    l.execute(a, 2, &Command::Deposit { amount: 100 }).unwrap();
    // idempotency: the very same command id again changes nothing
    assert_eq!(
        l.execute(a, 2, &Command::Deposit { amount: 100 }).unwrap(),
        Outcome::Duplicate
    );
    assert_eq!(l.load(a).balance, 100);
    // domain rules live in decide
    let err = l
        .execute(
            a,
            3,
            &Command::Withdraw {
                amount: 500,
                reason: "x".into(),
            },
        )
        .unwrap_err();
    assert_eq!(
        err,
        DomainError::Insufficient {
            have: 100,
            need: 500
        }
    );
    println!("decide rejected an overdraft: {err}");

    // UNDO via compensation: history grows, nothing is deleted
    l.execute(
        a,
        4,
        &Command::Withdraw {
            amount: 30,
            reason: "mistake".into(),
        },
    )
    .unwrap(); // version 3
    assert_eq!(l.load(a).balance, 70);
    l.execute(a, 5, &Command::Reverse { target: 3 }).unwrap(); // version 4
    assert_eq!(l.load(a).balance, 100);
    assert_eq!(
        l.store.version(a),
        4,
        "the mistake AND its correction are both in the log"
    );
    assert_eq!(
        l.execute(a, 6, &Command::Reverse { target: 3 })
            .unwrap_err(),
        DomainError::AlreadyReversed(3)
    );
    assert_eq!(
        l.execute(a, 7, &Command::Reverse { target: 1 })
            .unwrap_err(),
        DomainError::NotReversible(1)
    );

    // TIME TRAVEL: state as of each version
    let balances: Vec<i64> = (1..=4).map(|v| l.state_at(a, v).balance).collect();
    println!("balance as of v1..v4: {balances:?}");
    assert_eq!(balances, vec![0, 100, 70, 100]);
    assert_eq!(l.state_at(a, 4), l.load_full(a));

    // SNAPSHOTS every 4 events (crossed at version 4): the next load starts from it
    assert_eq!(l.snaps.get(a).map(|s| s.version), Some(4));
    l.execute(a, 8, &Command::Deposit { amount: 1 }).unwrap();
    assert_eq!(l.load(a), l.load_full(a));
    assert!(l.snapshot_hits >= 1);
}

fn demo_upcasting() {
    let mut store = MemStore::default();
    // history written by the OLD code (schema 1): different field names, no `reason`
    let old = |payload: Value| NewEvent {
        cmd: 0,
        ts: 0,
        schema: 1,
        payload,
    };
    store
        .append("acct-old", 0, vec![old(json!({"type": "Opened"}))])
        .unwrap();
    store
        .append(
            "acct-old",
            1,
            vec![old(json!({"type": "Deposited", "data": {"cents": 1000}}))],
        )
        .unwrap();
    store
        .append(
            "acct-old",
            2,
            vec![old(json!({"type": "Withdrawn", "data": {"amt": 250}}))],
        )
        .unwrap();
    // the NEW code appends schema 2 to the same stream
    let new = NewEvent::current(
        9,
        0,
        &Event::Withdrawn {
            amount: 50,
            reason: "atm".into(),
        },
    );
    store.append("acct-old", 3, vec![new]).unwrap();

    let log = store.read_stream("acct-old", 0);
    assert_eq!(
        decode(&log[2]),
        Event::Withdrawn {
            amount: 250,
            reason: "legacy".into()
        }
    );
    assert_eq!(log[2].schema, 1, "stored bytes keep their original schema");
    assert_eq!(
        log[2].payload["data"]["amt"], 250,
        "history is never rewritten"
    );
    let acc = log.iter().fold(Account::default(), fold);
    assert_eq!(acc.balance, 700);
    println!(
        "upcast v1 -> v2 on read: v1 {} became {:?}",
        log[2].payload,
        decode(&log[2])
    );
}

fn demo_optimistic_concurrency() {
    let mut store = MemStore::default();
    let ev = || vec![NewEvent::current(1, 0, &Event::Opened)];
    store.append("s", 0, ev()).unwrap();
    let err = store.append("s", 0, ev()).unwrap_err(); // second writer used a stale expected version
    assert!(matches!(
        err,
        StoreError::Conflict {
            expected: 0,
            actual: 1,
            ..
        }
    ));
    println!("stale writer rejected: {err}");
}

// ---------------------------------------------------------------- crash recovery on the file store

fn demo_crash_recovery(seed: u64) -> io::Result<()> {
    let dir = std::env::temp_dir().join("flowforge_cookbook_c10");
    fs::create_dir_all(&dir)?;
    let path = dir.join("ledger.log");
    let _ = fs::remove_file(&path);

    let (store, tail) = FileStore::open(&path, false)?;
    assert_eq!(tail, Tail::Clean);
    let mut sim = Sim::new(store, seed, 4, 6);
    sim.run(1500);
    sim.quiesce();
    verify(&mut sim);
    sim.ledger.store.sync()?; // group commit: one fsync for the whole run
    let original = sim.ledger.store.read_all(0).to_vec();

    // the same seed on the in-memory store produces the byte-identical log (store is a pure detail)
    let mut twin = Sim::new(MemStore::default(), seed, 4, 6);
    twin.run(1500);
    twin.quiesce();
    assert_eq!(
        fingerprint(&original),
        fingerprint(twin.ledger.store.read_all(0)),
        "file store == memory store"
    );
    drop(sim);

    // clean reopen: everything is back
    let (reopened, tail) = FileStore::open(&path, false)?;
    assert_eq!(
        (tail, reopened.read_all(0).len()),
        (Tail::Clean, original.len())
    );
    drop(reopened);

    // CRASH: chop the last 5 bytes, as if power failed mid-write of the last batch
    let bytes = fs::read(&path)?;
    fs::write(&path, &bytes[..bytes.len() - 5])?;
    let (recovered, tail) = FileStore::open(&path, true)?;
    let kept = recovered.read_all(0).len();
    println!(
        "crash: {} of {} events survive, tail was {tail:?}",
        kept,
        original.len()
    );
    assert!(matches!(tail, Tail::Truncated { .. }));
    assert!(
        kept < original.len() && kept + 2 >= original.len(),
        "only the last batch is lost"
    );
    assert_eq!(
        recovered.read_all(0),
        &original[..kept],
        "the survivors are an exact prefix"
    );

    // reproject from scratch and cross-check with the aggregates; versions are still gapless
    let mut ledger = Ledger::new(recovered, 8);
    let rm = ReadModels::rebuild(&ledger.store);
    for stream in ledger.store.streams() {
        let evs = ledger.store.read_stream(&stream, 0);
        assert!(
            evs.iter()
                .enumerate()
                .all(|(i, s)| s.version == i as u64 + 1)
        );
        assert_eq!(rm.balances[&stream], ledger.load_full(&stream).balance);
    }
    // the system keeps working after recovery, and a second reopen sees the new events too
    ledger
        .execute("acct-0", 7_000_001, &Command::Deposit { amount: 5 })
        .unwrap();
    let total = ledger.store.read_all(0).len();
    drop(ledger);
    let (again, tail) = FileStore::open(&path, true)?;
    assert_eq!((tail, again.read_all(0).len()), (Tail::Clean, total));

    // bit rot: flip one payload byte; the checksum stops the scan at that frame
    let mut bytes = fs::read(&path)?;
    bytes[20] ^= 0x40;
    fs::write(&path, &bytes)?;
    let (_, tail) = FileStore::open(&path, true)?;
    assert!(matches!(tail, Tail::BadChecksum { at: 0 }));
    println!("bit flip in the first frame detected: {tail:?}");
    fs::remove_file(&path)?;
    Ok(())
}

// ---------------------------------------------------------------- main

fn main() -> io::Result<()> {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    demo_basics();
    demo_upcasting();
    demo_optimistic_concurrency();

    // one big run on the memory store, with a report
    let mut sim = Sim::new(MemStore::default(), 42, 4, 6);
    sim.run(6000);
    sim.quiesce();
    verify(&mut sim);
    println!(
        "stats: {:?}, reactor sent {} absorbed {} duplicates",
        sim.stats, sim.reactor.sent, sim.reactor.absorbed
    );
    println!(
        "outbox: {} events, consumer received {} deliveries, applied {} (deduped)",
        sim.outbox.marked,
        sim.consumer.received,
        sim.consumer.applied.len()
    );
    assert!(
        sim.stats.conflicts > 0,
        "interleaved writers must produce real conflicts"
    );
    assert!(sim.stats.rejected > 0 && sim.stats.applied > 100);
    assert!(
        sim.reactor.absorbed > 0,
        "idempotent commands absorbed duplicate deliveries"
    );
    assert!(sim.ledger.snapshot_hits > 0, "snapshots were used");
    println!("top spenders: {:?}", sim.rm.top_spenders(3));
    let day_rows: Vec<_> = sim.rm.daily.iter().take(2).collect();
    println!("daily totals (first days): {day_rows:?}");
    assert_eq!(sim.rm.top_spenders(3).len(), 3);

    // read-side time travel: the model at an earlier global position equals a replay up to there
    let mid = sim.ledger.store.read_all(0).len() as u64 / 2;
    let past = ReadModels::as_of(&sim.ledger.store, mid);
    assert_eq!(past.next_pos, mid);
    assert_ne!(past, sim.rm);
    let mut manual = ReadModels::default();
    for s in &sim.ledger.store.read_all(0)[..mid as usize] {
        manual.apply(s);
    }
    manual.next_pos = mid;
    assert_eq!(past, manual);

    // determinism: same seed, same log; different seed, different log
    let fp = fingerprint(sim.ledger.store.read_all(0));
    let mut again = Sim::new(MemStore::default(), 42, 4, 6);
    again.run(6000);
    again.quiesce();
    assert_eq!(
        fp,
        fingerprint(again.ledger.store.read_all(0)),
        "replay is deterministic"
    );
    let mut other = Sim::new(MemStore::default(), 43, 4, 6);
    other.run(6000);
    other.quiesce();
    assert_ne!(fp, fingerprint(other.ledger.store.read_all(0)));

    // many seeds, varying writer counts, all invariants each time
    for seed in 1..=25u64 {
        let mut s = Sim::new(
            MemStore::default(),
            seed,
            2 + (seed % 5) as usize,
            3 + seed % 6,
        );
        s.run(1500);
        s.quiesce();
        verify(&mut s);
    }
    println!("25 more seeded runs: all invariants hold");

    demo_crash_recovery(7)?;
    println!("OK");
    Ok(())
}
