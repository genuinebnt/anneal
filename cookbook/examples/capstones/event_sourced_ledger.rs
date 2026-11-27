//! CAPSTONE 10 (database): an event-sourced bank ledger with CQRS projections, snapshots, upcasters and an outbox
//! LEVEL: capstone
//! Use in: any system that must explain HOW it reached its state (finance, inventory, audit-heavy domains); also the core idea behind database logs.   Run: cargo run --release --example event_sourced_ledger
#![allow(dead_code)]
//!
//! WHAT: state is never stored, only the facts that produced it. The EVENT STORE is append-only, has one totally ordered global
//! log plus a version counter per stream, and `append(stream, expected_version, events)` fails with a Conflict when another
//! writer got there first (optimistic concurrency, no locks). Two stores share one trait: in memory, and file-backed with one
//! checksummed frame per BATCH, so a torn write loses a whole batch, never half of one. A frame is
//! [len u32][crc32(len) u32][crc32(payload) u32][payload]: the second field says whether the length itself can be believed.
//! A torn END is cut off; damage with more data behind it makes `open` fail and leaves the file alone. After a failed write or
//! sync the store takes no more writes until it is reopened.
//! COMMAND SIDE: `decide(state, command) -> Result<Vec<Event>, DomainError>` is pure; `evolve(state, event) -> state` is
//! pure and total (an event is a fact that already happened, so it must never fail). Commands carry an id; the aggregate
//! remembers the ids it has seen with a fingerprint of the command, so a retry is a no-op and a DIFFERENT command under a
//! used id is refused. A rejection decided on stale state is reported as a conflict, never as the answer. SNAPSHOTS every N events bound replay cost
//! and are only an optimisation: snapshot + tail must equal full replay (asserted).
//! QUERY SIDE (CQRS): read models (balances, daily totals, top spenders) subscribe to the global log with a CHECKPOINT. They
//! are disposable: rebuilding from position 0 must give exactly the incrementally maintained model (asserted).
//! EVOLUTION: events live forever, so old shapes are kept as stored and UPCAST on read (v1 -> v2) instead of rewriting history.
//! UNDO: a mistake is corrected by a compensating event (`Reversed`), never by deleting; the audit trail stays complete.
//! OUTBOX: events to publish to the outside world are recorded in the same log as the state change (one atomic write), and a
//! relay delivers them AT LEAST ONCE (acks get lost); the consumer dedupes by event id, giving effectively-once.
//! TRANSFERS span two aggregates, which a single-stream append cannot make atomic. So a transfer is two commands: the source
//! emits `TransferredOut`, a reactor (process manager) later sends `ReceiveTransfer` to the target with a command id made
//! from the log position of that event (unique per transfer, and the same on every redelivery). A target that cannot take
//! the money (never opened, or its balance would overflow) makes the reactor send it back to the source. Money is conserved
//! as: balances + in-flight (out - in) == external net.
//! LIMITS: snapshots, projection checkpoints, outbox acks and the consumer's seen-set live in memory only. An account keeps
//! every command id and reversible amount for ever, and each load clones them. One process owns the file: nothing locks it.
//! Damage to the data of the very LAST frame cannot be told apart from a torn write, so it is cut off. A cut or zeroed
//! tail can silently remove several acknowledged batches. Zeros before the length and its check are complete are
//! refused as damage. After a failed sync the batch may
//! still be there on reopen, so a caller must retry with the same command id. Rejected commands are not remembered.
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

#[derive(Debug, Clone, PartialEq, Serialize)]
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
    #[error("the balance would not fit")]
    Overflow,
    #[error("command id was already used for a different command")]
    IdReused,
}

/// Who made up a command id. The two kinds can never collide: a client cannot produce a `Reaction` id by choosing a number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
enum CmdId {
    Client(u64),
    /// the answer to the event at this global log position
    Reaction(u64),
    /// the return of the money sent by the event at this position
    Bounce(u64),
}

impl From<u64> for CmdId {
    fn from(n: u64) -> CmdId {
        CmdId::Client(n)
    }
}

fn command_print(cmd: &Command) -> u64 {
    fnv(
        &serde_json::to_vec(cmd).expect("command serialises"),
        0xcbf2_9ce4_8422_2325,
    )
}

/// The aggregate state. `version` is the stream version it reflects; `cmds` powers idempotency; `deltas` remembers which
/// versions changed the balance (and by how much) so a later `Reverse` can be validated without reading history.
#[derive(Debug, Clone, Default, PartialEq)]
struct Account {
    version: u64,
    open: bool,
    balance: i64,
    /// command id -> fingerprint of the command it was used for
    cmds: BTreeMap<CmdId, u64>,
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

fn fits(a: &Account, delta: i64) -> Result<i64, DomainError> {
    a.balance.checked_add(delta).ok_or(DomainError::Overflow)
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
        Command::Deposit { amount } => {
            fits(a, positive(*amount)?)?;
            Event::Deposited { amount: *amount }
        }
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
        // no business rule can refuse money that already left the source. Only a missing account or an overflow can,
        // and then the reactor sends it back
        Command::ReceiveTransfer {
            transfer,
            from,
            amount,
        } => {
            fits(a, positive(*amount)?)?;
            Event::TransferredIn {
                transfer: *transfer,
                from: from.clone(),
                amount: *amount,
            }
        }
        Command::Reverse { target } => {
            let delta = *a
                .deltas
                .get(target)
                .ok_or(DomainError::NotReversible(*target))?;
            if a.reversed.contains(target) {
                return Err(DomainError::AlreadyReversed(*target));
            }
            require_funds(a, delta)?; // undoing a deposit must not overdraw
            // a stored event may hold any number, and the smallest one has no negative
            let back = delta.checked_neg().ok_or(DomainError::Overflow)?;
            fits(a, back)?;
            Event::Reversed {
                target: *target,
                delta: back,
            }
        }
    };
    Ok(vec![event])
}

/// PURE and TOTAL: applies a fact that already happened. Never validates (validation belongs to `decide`), and never
/// panics: facts from `decide` cannot overflow, and facts from anywhere else saturate.
fn evolve(mut a: Account, e: &Event) -> Account {
    a.version += 1;
    match e {
        Event::Opened => a.open = true,
        Event::Deposited { amount } => {
            a.balance = a.balance.saturating_add(*amount);
            a.deltas.insert(a.version, *amount);
        }
        Event::Withdrawn { amount, .. } => {
            a.balance = a.balance.saturating_sub(*amount);
            a.deltas.insert(a.version, amount.saturating_neg());
        }
        Event::TransferredOut { amount, .. } => a.balance = a.balance.saturating_sub(*amount),
        Event::TransferredIn { amount, .. } => a.balance = a.balance.saturating_add(*amount),
        Event::Reversed { target, delta } => {
            a.balance = a.balance.saturating_add(*delta);
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
    cmd: CmdId,
    print: u64, // fingerprint of the command, 0 when unknown (history written before fingerprints)
    ts: u64,    // seconds; supplied by the writer's clock, never read inside decide/evolve
    schema: u32,
    payload: Value,
}

struct NewEvent {
    cmd: CmdId,
    print: u64,
    ts: u64,
    schema: u32,
    payload: Value,
}

impl NewEvent {
    fn current(cmd: impl Into<CmdId>, ts: u64, e: &Event) -> NewEvent {
        NewEvent {
            cmd: cmd.into(),
            print: 0,
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

/// An error for a schema this code does not know, or a payload that is not an event. A store checks every record with
/// this when it opens, so nothing it hands out can fail to decode later.
fn try_decode(s: &Stored) -> Result<Event, String> {
    if s.schema == 0 || s.schema > SCHEMA {
        return Err(format!("schema {} is not known here", s.schema));
    }
    serde_json::from_value(upcast(s.schema, s.payload.clone())).map_err(|e| e.to_string())
}

fn decode(s: &Stored) -> Event {
    try_decode(s).expect("every stored event was checked when it entered the store")
}

fn fold(acc: Account, s: &Stored) -> Account {
    debug_assert_eq!(acc.version + 1, s.version, "versions must be gapless");
    let mut next = evolve(acc, &decode(s));
    next.cmds.insert(s.cmd, s.print); // the envelope, not the event, carries the command id
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
    #[error("an earlier write failed: reopen the store")]
    Poisoned,
    #[error("event refused: {0}")]
    BadEvent(String),
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
        let batch: Vec<Stored> = events
            .into_iter()
            .enumerate()
            .map(|(i, e)| Stored {
                pos: base + i as u64,
                stream: stream.into(),
                version: expected + 1 + i as u64,
                cmd: e.cmd,
                print: e.print,
                ts: e.ts,
                schema: e.schema,
                payload: e.payload,
            })
            .collect();
        for s in &batch {
            try_decode(s).map_err(StoreError::BadEvent)?;
        }
        Ok(batch)
    }

    /// a batch read back from disk must continue the log exactly where it stands
    fn accepts(&self, batch: &[Stored]) -> Result<(), String> {
        let mut next: HashMap<&str, u64> = HashMap::new();
        for (i, s) in batch.iter().enumerate() {
            let version = next
                .entry(&s.stream)
                .or_insert_with(|| self.version(&s.stream));
            *version += 1;
            if s.pos != (self.log.len() + i) as u64 || s.version != *version {
                return Err(format!("position {} is out of sequence", s.pos));
            }
            try_decode(s)?;
        }
        Ok(())
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
    /// an unfinished last write, safe to cut off
    Torn {
        at: u64,
    },
    /// bytes that no unfinished write explains: cutting here could lose acknowledged batches
    Damaged {
        at: u64,
    },
}

const HEADER: usize = 12;

fn frame(payload: &[u8]) -> Vec<u8> {
    let len = (payload.len() as u32).to_le_bytes();
    let mut out = Vec::with_capacity(HEADER + payload.len());
    out.extend_from_slice(&len);
    out.extend_from_slice(&crc32(&len).to_le_bytes());
    out.extend_from_slice(&crc32(payload).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

/// Scan frames from the start. Returns the whole good payloads, the bytes they take, and what follows them.
fn scan(bytes: &[u8]) -> (Vec<&[u8]>, usize, Tail) {
    let (mut recs, mut pos) = (vec![], 0usize);
    let zeros = |b: &[u8]| b.iter().all(|&x| x == 0);
    let word = |b: &[u8]| u32::from_le_bytes(b.try_into().expect("four bytes"));
    loop {
        let rest = &bytes[pos..];
        if rest.is_empty() {
            return (recs, pos, Tail::Clean);
        }
        let at = pos as u64;
        let end = if rest.len() < HEADER {
            return (recs, pos, Tail::Torn { at });
        } else if crc32(&rest[..4]) != word(&rest[4..8]) || word(&rest[..4]) == 0 {
            // a file that grew before its data arrived reads as zeros. Anything else here is damage
            let tail = if zeros(rest) {
                Tail::Torn { at }
            } else {
                Tail::Damaged { at }
            };
            return (recs, pos, tail);
        } else {
            HEADER + word(&rest[..4]) as usize // the length is trustworthy from here on
        };
        let Some(payload) = rest.get(HEADER..end) else {
            return (recs, pos, Tail::Torn { at });
        };
        if crc32(payload) != word(&rest[8..HEADER]) {
            // with nothing but zeros behind it, this is the last write: possibly only partly on disk
            let tail = if zeros(&rest[end..]) {
                Tail::Torn { at }
            } else {
                Tail::Damaged { at }
            };
            return (recs, pos, tail);
        }
        recs.push(payload);
        pos += end;
    }
}

/// What the store needs from a file, so that a test can make a write or a sync fail.
trait Device {
    fn append(&mut self, bytes: &[u8]) -> io::Result<()>;
    fn sync(&mut self) -> io::Result<()>;
}

impl Device for File {
    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.write_all(bytes)
    }
    fn sync(&mut self) -> io::Result<()> {
        self.sync_data()
    }
}

/// File-backed store: WRITE-AHEAD (frame hits the file before the in-memory index changes). One frame = one batch, so
/// appending several events to a stream is atomic across crashes.
struct FileStore<D: Device = File> {
    mem: MemStore,
    file: D,
    sync_each: bool,
    /// a write or sync failed: what the file holds is unknown until it is read again
    poisoned: bool,
}

/// Rebuild the in-memory index from the bytes of a log. Returns it with the number of bytes to keep.
fn recover(bytes: &[u8]) -> io::Result<(MemStore, usize, Tail)> {
    let bad = |m: String| io::Error::new(io::ErrorKind::InvalidData, m);
    let (recs, valid, tail) = scan(bytes);
    if let Tail::Damaged { at } = tail {
        return Err(bad(format!(
            "the log is damaged at byte {at}; it was left as it is"
        )));
    }
    let mut mem = MemStore::default();
    for r in recs {
        let batch: Vec<Stored> = serde_json::from_slice(r).map_err(|e| bad(e.to_string()))?;
        mem.accepts(&batch).map_err(bad)?;
        mem.commit(batch);
    }
    Ok((mem, valid, tail))
}

impl FileStore<File> {
    fn open(path: &Path, sync_each: bool) -> io::Result<(Self, Tail)> {
        let existed = path.exists();
        let bytes = if existed { fs::read(path)? } else { vec![] };
        let (mem, valid, tail) = recover(&bytes)?; // an error here has changed nothing on disk
        if valid != bytes.len() {
            // drop the torn tail so new frames start on a clean boundary, and make the cut durable
            let file = OpenOptions::new().write(true).open(path)?;
            file.set_len(valid as u64)?;
            file.sync_all()?;
        }
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        if !existed {
            // a new file's NAME is durable only once its directory is synced
            let dir = path.parent().filter(|d| !d.as_os_str().is_empty());
            File::open(dir.unwrap_or(Path::new(".")))?.sync_all()?;
        }
        Ok((FileStore::over(mem, file, sync_each), tail))
    }
}

impl<D: Device> FileStore<D> {
    fn over(mem: MemStore, file: D, sync_each: bool) -> Self {
        FileStore {
            mem,
            file,
            sync_each,
            poisoned: false,
        }
    }

    /// group commit. A failed sync poisons the store like a failed write: trying again proves nothing
    fn sync(&mut self) -> Result<(), StoreError> {
        if self.poisoned {
            return Err(StoreError::Poisoned);
        }
        self.file.sync().map_err(|e| {
            self.poisoned = true;
            StoreError::Io(e)
        })
    }
}

impl<D: Device> EventStore for FileStore<D> {
    fn append(
        &mut self,
        stream: &str,
        expected: u64,
        events: Vec<NewEvent>,
    ) -> Result<Vec<Stored>, StoreError> {
        if self.poisoned {
            return Err(StoreError::Poisoned);
        }
        let batch = self.mem.prepare(stream, expected, events)?;
        if batch.is_empty() {
            return Ok(batch);
        }
        let payload = serde_json::to_vec(&batch).map_err(io::Error::other)?;
        if let Err(e) = self.file.append(&frame(&payload)) {
            self.poisoned = true; // part of the frame may be there: nothing may be written behind it
            return Err(e.into());
        }
        if self.sync_each {
            self.sync()?; // fsync per batch is the durable (slow) mode; the simulation batches syncs instead
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
        id: impl Into<CmdId>,
        cmd: &Command,
    ) -> Result<Outcome, CmdError> {
        let (id, print) = (id.into(), command_print(cmd));
        if let Some(&used) = loaded.cmds.get(&id) {
            return if used == print || used == 0 {
                Ok(Outcome::Duplicate) // idempotent: already handled, answer "fine" without new events
            } else {
                Err(DomainError::IdReused.into())
            };
        }
        let events = match decide(loaded, cmd) {
            Ok(events) => events,
            Err(rule) => {
                // a "no" is only as good as the state it was based on
                let actual = self.store.version(stream);
                if actual != loaded.version {
                    return Err(StoreError::Conflict {
                        stream: stream.into(),
                        expected: loaded.version,
                        actual,
                    }
                    .into());
                }
                return Err(rule.into());
            }
        };
        let new = events
            .iter()
            .map(|e| NewEvent {
                print,
                ..NewEvent::current(id, self.now, e)
            })
            .collect();
        let stored = self.store.append(stream, loaded.version, new)?;
        let next = stored.iter().fold(loaded.clone(), fold);
        // snap_every == 0 means no snapshots
        let crossed = |v: u64| v.checked_div(self.snap_every);
        if crossed(next.version) > crossed(loaded.version) {
            self.snaps.insert(stream.to_owned(), next); // crossed a multiple of N: keep a snapshot
        }
        Ok(Outcome::Applied(stored.len()))
    }

    /// load + commit with retry on conflict (the normal single-threaded path)
    fn execute(
        &mut self,
        stream: &str,
        id: impl Into<CmdId>,
        cmd: &Command,
    ) -> Result<Outcome, CmdError> {
        let id = id.into();
        loop {
            let acc = self.load(stream);
            match self.try_commit(stream, &acc, id, cmd) {
                Err(e) if is_conflict(&e) => continue,
                other => return other,
            }
        }
    }
}

// ---------------------------------------------------------------- projections (CQRS read side)

#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct Day {
    deposited: i128,
    withdrawn: i128,
    transferred: i128,
    /// net balance change of the reversals made on this day, whatever day they correct
    reversed: i128,
    events: u64,
}

/// Three read models in one struct, sharing one checkpoint (`next_pos`). Everything is derived: delete it and rebuild.
#[derive(Debug, Default, Clone, PartialEq)]
struct ReadModels {
    next_pos: u64,
    balances: BTreeMap<String, i64>,
    daily: BTreeMap<u64, Day>,
    spent: BTreeMap<String, i128>,
}

impl ReadModels {
    fn apply(&mut self, s: &Stored) {
        let day = self.daily.entry(s.ts / 86_400).or_default();
        day.events += 1;
        let bal = self.balances.entry(s.stream.clone()).or_insert(0);
        let spent = self.spent.entry(s.stream.clone()).or_insert(0);
        let wide = i128::from;
        match decode(s) {
            Event::Opened => {}
            Event::Deposited { amount } => {
                *bal = bal.saturating_add(amount);
                day.deposited += wide(amount);
            }
            Event::Withdrawn { amount, .. } => {
                *bal = bal.saturating_sub(amount);
                *spent += wide(amount);
                day.withdrawn += wide(amount);
            }
            Event::TransferredOut { amount, .. } => {
                *bal = bal.saturating_sub(amount);
                *spent += wide(amount);
                day.transferred += wide(amount);
            }
            Event::TransferredIn { amount, .. } => *bal = bal.saturating_add(amount),
            Event::Reversed { delta, .. } => {
                *bal = bal.saturating_add(delta);
                // an earlier day's totals are history: the correction is counted on its own day, in its own column
                day.reversed += wide(delta);
                if delta > 0 {
                    *spent -= wide(delta); // a withdrawal that was undone was not spent
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

    fn top_spenders(&self, n: usize) -> Vec<(String, i128)> {
        let mut v: Vec<(String, i128)> = self.spent.iter().map(|(k, v)| (k.clone(), *v)).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        v.truncate(n);
        v
    }
}

// ---------------------------------------------------------------- reactor (process manager) and outbox

/// Turns `TransferredOut` into `ReceiveTransfer` on the target. At-least-once: it may crash before saving its checkpoint
/// and re-process, or deliver twice. The command id is made from the log position of the event being answered, so every
/// repeat is a no-op and two transfers can never share an id, whatever numbers their senders chose.
#[derive(Default)]
struct Reactor {
    next_pos: u64,
    sent: u64,
    absorbed: u64,
    bounced: u64,
    /// transfers that could neither arrive nor go back: still in flight, for a person to look at
    stuck: BTreeSet<u64>,
    crash_pct: u64,
}

impl Reactor {
    /// A store failure stops the tick before the checkpoint moves, so the same events are seen again.
    fn tick<S: EventStore>(
        &mut self,
        ledger: &mut Ledger<S>,
        rng: &mut Rng,
    ) -> Result<(), StoreError> {
        let fresh: Vec<Stored> = ledger.store.read_all(self.next_pos).to_vec();
        for s in &fresh {
            if let Event::TransferredOut {
                transfer,
                to,
                amount,
            } = decode(s)
            {
                let times = if rng.chance(25) { 2 } else { 1 }; // duplicate delivery
                for _ in 0..times {
                    self.deliver(ledger, s, transfer, &to, amount)?;
                }
            }
        }
        if !rng.chance(self.crash_pct) {
            self.next_pos += fresh.len() as u64; // "crash before checkpoint" = skip this line
        }
        Ok(())
    }

    fn deliver<S: EventStore>(
        &mut self,
        ledger: &mut Ledger<S>,
        s: &Stored,
        transfer: u64,
        to: &str,
        amount: i64,
    ) -> Result<(), StoreError> {
        // once the money has been sent back it must not arrive as well, even if the target can take it now
        let bounce = CmdId::Bounce(s.pos);
        if ledger.load(&s.stream).cmds.contains_key(&bounce) {
            self.absorbed += 1;
            return Ok(());
        }
        let receive = Command::ReceiveTransfer {
            transfer,
            from: s.stream.clone(),
            amount,
        };
        match ledger.execute(to, CmdId::Reaction(s.pos), &receive) {
            Ok(Outcome::Applied(_)) => self.sent += 1,
            Ok(Outcome::Duplicate) => self.absorbed += 1,
            Err(CmdError::Store(e)) => return Err(e),
            Err(CmdError::Domain(_)) => {
                let back = Command::ReceiveTransfer {
                    transfer,
                    from: to.to_owned(),
                    amount,
                };
                match ledger.execute(&s.stream, bounce, &back) {
                    Ok(_) => self.bounced += 1,
                    Err(CmdError::Store(e)) => return Err(e),
                    Err(CmdError::Domain(_)) => {
                        self.stuck.insert(s.pos);
                    }
                }
            }
        }
        Ok(())
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
            self.reactor
                .tick(&mut self.ledger, &mut self.rng)
                .expect("the store works");
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
                check_money(&self.ledger.store, false);
                // a lagging read model is still exactly the log up to its checkpoint
                let then = ReadModels::as_of(&self.ledger.store, self.rm.next_pos);
                assert_eq!(self.rm, then);
            }
        }
    }

    /// finish in-flight commands, drain the reactor and outbox, catch the projections up
    fn quiesce(&mut self) {
        while self.writers.iter().any(|w| w.1.is_some()) {
            self.step(false);
        }
        self.reactor.crash_pct = 0;
        self.reactor
            .tick(&mut self.ledger, &mut self.rng)
            .expect("the store works");
        self.rm.catch_up(&self.ledger.store);
        while {
            self.outbox.scan(&self.ledger.store);
            !self.outbox.pending.is_empty()
        } {
            self.outbox.pump(&mut self.consumer, &mut self.rng);
        }
    }
}

/// external net = money that entered/left the bank; in flight = out - in. Conservation: sum(balances) + in_flight == net
fn totals<S: EventStore>(store: &S) -> (i128, i128, i128) {
    let (mut net, mut outs, mut ins) = (0i128, 0i128, 0i128);
    let wide = i128::from;
    for s in store.read_all(0) {
        match decode(s) {
            Event::Deposited { amount } => net += wide(amount),
            Event::Withdrawn { amount, .. } => net -= wide(amount),
            Event::Reversed { delta, .. } => net += wide(delta),
            Event::TransferredOut { amount, .. } => outs += wide(amount),
            Event::TransferredIn { amount, .. } => ins += wide(amount),
            Event::Opened => {}
        }
    }
    (net, outs, ins)
}

fn check_money<S: EventStore>(store: &S, drained: bool) {
    let (net, outs, ins) = totals(store);
    let sum: i128 = store
        .streams()
        .iter()
        .map(|s| {
            store
                .read_stream(s, 0)
                .iter()
                .fold(Account::default(), fold)
                .balance as i128
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
    check_money(&sim.ledger.store, true);
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
    assert!(matches!(
        err,
        CmdError::Domain(DomainError::Insufficient {
            have: 100,
            need: 500
        })
    ));
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
    assert!(matches!(
        l.execute(a, 6, &Command::Reverse { target: 3 }),
        Err(CmdError::Domain(DomainError::AlreadyReversed(3)))
    ));
    assert!(matches!(
        l.execute(a, 7, &Command::Reverse { target: 1 }),
        Err(CmdError::Domain(DomainError::NotReversible(1)))
    ));

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
        cmd: CmdId::Client(0),
        print: 0,
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
    // a directory of this run's own: two runs at once must not share a file
    let dir = std::env::temp_dir().join(format!("cookbook_ledger_{}", std::process::id()));
    fs::create_dir_all(&dir)?;
    let path = dir.join("ledger.log");
    let _ = fs::remove_file(&path);

    let (store, tail) = FileStore::open(&path, false)?;
    assert_eq!(tail, Tail::Clean);
    let mut sim = Sim::new(store, seed, 4, 6);
    sim.run(1500);
    sim.quiesce();
    verify(&mut sim);
    sim.ledger.store.sync().expect("sync"); // group commit: one fsync for the whole run
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
    assert!(matches!(tail, Tail::Torn { .. }));
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

    // bit rot in the middle: open refuses, and every byte stays where it was for someone to repair
    let mut bytes = fs::read(&path)?;
    bytes[20] ^= 0x40;
    fs::write(&path, &bytes)?;
    let refused = FileStore::open(&path, true).err().expect("damage");
    assert_eq!(refused.kind(), io::ErrorKind::InvalidData);
    assert_eq!(fs::read(&path)?, bytes, "nothing was cut off");
    println!("bit flip in the first frame: {refused}");
    fs::remove_dir_all(&dir)?;
    Ok(())
}

// ---------------------------------------------------------------- checks for the edges

fn opened(l: &mut Ledger<MemStore>, name: &str, amount: i64) {
    l.execute(name, 1, &Command::Open).unwrap();
    if amount > 0 {
        l.execute(name, 2, &Command::Deposit { amount }).unwrap();
    }
}

fn check_commands() {
    let mut l = Ledger::new(MemStore::default(), 0); // 0: snapshots off, not a division by zero
    opened(&mut l, "a", 0);
    let stale = l.load("a");
    l.execute("a", 2, &Command::Deposit { amount: 100 })
        .unwrap();
    // the stale state says "no funds", the current one does not: that is a conflict, not a refusal
    let spend = Command::Withdraw {
        amount: 50,
        reason: "r".into(),
    };
    let err = l.try_commit("a", &stale, 3, &spend).unwrap_err();
    assert!(is_conflict(&err), "{err}");
    // the same id for something else is not "already done"
    assert!(matches!(
        l.execute("a", 2, &spend),
        Err(CmdError::Domain(DomainError::IdReused))
    ));
    // a balance that would not fit is refused, not a panic or a negative number
    let huge = Command::Deposit { amount: i64::MAX };
    assert!(matches!(
        l.execute("a", 4, &huge),
        Err(CmdError::Domain(DomainError::Overflow))
    ));
    assert_eq!(l.load("a").balance, 100);
    // facts that `decide` never made still apply without a panic
    let flood = Event::Deposited { amount: i64::MAX };
    assert_eq!(evolve(l.load("a"), &flood).balance, i64::MAX);
    // and reversing such a fact is refused, not a panic
    let mut odd = evolve(Account::default(), &Event::Opened);
    odd = evolve(odd, &Event::Deposited { amount: i64::MIN });
    assert_eq!(
        decide(&odd, &Command::Reverse { target: 2 }),
        Err(DomainError::Overflow)
    );
    // and a store takes nothing that it could not decode later
    let junk = NewEvent {
        payload: json!({"type": "Nonsense"}),
        ..NewEvent::current(9, 0, &Event::Opened)
    };
    let version = l.store.version("a");
    assert!(matches!(
        l.store.append("a", version, vec![junk]),
        Err(StoreError::BadEvent(_))
    ));
}

fn check_reactor() {
    let mut l = Ledger::new(MemStore::default(), 8);
    for name in ["a", "b", "c"] {
        opened(&mut l, name, 100);
    }
    // two senders happen to pick the same transfer number
    for (from, amount) in [("a", 10), ("b", 20)] {
        let cmd = Command::TransferOut {
            transfer: 7,
            to: "c".into(),
            amount,
        };
        l.execute(from, 3, &cmd).unwrap();
    }
    // and one sends money to an account that was never opened
    let lost = Command::TransferOut {
        transfer: 8,
        to: "nobody".into(),
        amount: 5,
    };
    l.execute("a", 4, &lost).unwrap();
    let mut reactor = Reactor::default();
    reactor.tick(&mut l, &mut Rng::new(3)).unwrap();
    assert_eq!(l.load("c").balance, 130, "both transfers arrived");
    assert_eq!(l.load("a").balance, 90, "the undeliverable money came back");
    check_money(&l.store, true);
    // the target is opened later, then the reactor loses its checkpoint: the bounced money must not also arrive
    opened(&mut l, "nobody", 0);
    reactor.next_pos = 0;
    reactor.tick(&mut l, &mut Rng::new(4)).unwrap();
    assert_eq!(l.load("nobody").balance, 0);
    check_money(&l.store, true);
}

fn check_projection() {
    let mut l = Ledger::new(MemStore::default(), 8);
    opened(&mut l, "a", 100);
    l.now = 90_000;
    l.execute("a", 3, &Command::Reverse { target: 2 }).unwrap();
    let rm = ReadModels::rebuild(&l.store);
    let (first, second) = (rm.daily[&0], rm.daily[&1]);
    assert_eq!((first.deposited, first.reversed), (100, 0));
    assert_eq!((second.deposited, second.reversed), (0, -100));
}

/// A file in memory whose next write can stop half way, or whose next sync can fail.
#[derive(Default)]
struct Flaky {
    bytes: Vec<u8>,
    short_write: bool,
    bad_sync: bool,
}

impl Device for Flaky {
    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        if std::mem::take(&mut self.short_write) {
            self.bytes.extend_from_slice(&bytes[..bytes.len() / 2]);
            return Err(io::Error::other("disk full"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
    fn sync(&mut self) -> io::Result<()> {
        if std::mem::take(&mut self.bad_sync) {
            return Err(io::Error::other("sync failed"));
        }
        Ok(())
    }
}

fn check_files() {
    let ev = |id: u64| vec![NewEvent::current(id, 0, &Event::Opened)];
    for fail_the_sync in [false, true] {
        let mut store = FileStore::over(MemStore::default(), Flaky::default(), true);
        store.append("a", 0, ev(1)).unwrap();
        store.file.short_write = !fail_the_sync;
        store.file.bad_sync = fail_the_sync;
        assert!(matches!(
            store.append("b", 0, ev(2)),
            Err(StoreError::Io(_))
        ));
        let kept = store.file.bytes.len();
        // the device works again, but writing now would bury the unknown bytes or repeat a position
        assert!(matches!(
            store.append("c", 0, ev(3)),
            Err(StoreError::Poisoned)
        ));
        assert!(matches!(store.sync(), Err(StoreError::Poisoned)));
        assert_eq!(store.file.bytes.len(), kept);
        let (mem, _, _) = recover(&store.file.bytes).expect("a readable log");
        assert_eq!(mem.log.len(), if fail_the_sync { 2 } else { 1 });
    }

    let mut store = FileStore::over(MemStore::default(), Flaky::default(), false);
    store.append("a", 0, ev(1)).unwrap();
    let first = store.file.bytes.len();
    store.append("b", 0, ev(2)).unwrap();
    let good = store.file.bytes.clone();
    // a crash can leave zeros after the data: that is a torn end, not a record
    for zeros in [1, 11, 12, 4096] {
        let mut b = good.clone();
        b.resize(b.len() + zeros, 0);
        let (mem, valid, tail) = recover(&b).expect("zeros are a torn end");
        assert_eq!((mem.log.len(), valid), (2, good.len()));
        assert!(matches!(tail, Tail::Torn { .. }));
    }
    // every cut of the file keeps whole batches only
    for cut in 0..good.len() {
        let (mem, valid, _) = recover(&good[..cut]).expect("a torn end");
        assert_eq!(
            (mem.log.len(), valid),
            if cut < first { (0, 0) } else { (1, first) }
        );
    }
    // any flipped bit in the first frame, its length included, is damage: nothing may be cut off
    for bit in 0..first * 8 {
        let mut b = good.clone();
        b[bit / 8] ^= 1 << (bit % 8);
        assert!(recover(&b).is_err(), "bit {bit}");
    }
    // a zeroed header in front of a good frame is not a torn end: cutting there would lose an acknowledged batch
    let mut b = good.clone();
    b[..HEADER].fill(0);
    assert!(recover(&b).is_err(), "a zeroed header before good data");
    // nor is a bad frame with good data far behind it, past any number of zeros
    let mut b = good[..first].to_vec();
    b[first - 1] ^= 1;
    b.resize(first + 4096, 0);
    b.extend_from_slice(&good[first..]);
    assert!(recover(&b).is_err(), "data far behind a bad frame");
    // a frame whose checksums are right but whose content is wrong is refused when opening, not met later
    let mut batch: Vec<Stored> = serde_json::from_slice(&scan(&good).0[0]).unwrap();
    assert!(
        recover(&[good.clone(), frame(&serde_json::to_vec(&batch).unwrap())].concat()).is_err(),
        "a repeated position"
    );
    batch[0].schema = SCHEMA + 1;
    assert!(
        recover(&frame(&serde_json::to_vec(&batch).unwrap())).is_err(),
        "a newer schema"
    );
}

// ---------------------------------------------------------------- main

fn main() -> io::Result<()> {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    check_commands();
    check_reactor();
    check_projection();
    check_files();
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
