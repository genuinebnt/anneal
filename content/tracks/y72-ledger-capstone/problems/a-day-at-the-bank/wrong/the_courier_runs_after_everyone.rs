use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io;
use std::io::Write;
use std::path::Path;

// ---------------------------------------------------------------- the account: events, commands, rules

/// Something that happened to one account. Stored for ever.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum Event {
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
    /// Cancels the event at version `target`. `delta` is what this does to the balance.
    Reversed {
        target: u64,
        delta: i64,
    },
}

/// Something somebody asks the account to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainError {
    NotOpen,
    AlreadyOpen,
    NonPositive,
    Insufficient {
        have: i64,
        need: i64,
    },
    NotReversible(u64),
    AlreadyReversed(u64),
    Overflow,
    /// The command id was already used for a different command.
    IdReused,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Account {
    /// How many events it has had.
    pub version: u64,
    pub open: bool,
    pub balance: i64,
    /// version of each deposit and withdrawal -> what it did to the balance
    reversible: BTreeMap<u64, i64>,
    reversed: BTreeSet<u64>,
    /// command id -> fingerprint of the command it was used for
    commands: BTreeMap<CmdId, u64>,
}

fn positive(amount: i64) -> Result<i64, DomainError> {
    if amount > 0 {
        Ok(amount)
    } else {
        Err(DomainError::NonPositive)
    }
}

/// Can the balance change by `delta`: not below zero when it goes down, not past the largest number when it goes up.
fn allow(account: &Account, delta: i64) -> Result<(), DomainError> {
    match account.balance.checked_add(delta) {
        Some(after) if delta >= 0 || after >= 0 => Ok(()),
        _ if delta < 0 => Err(DomainError::Insufficient {
            have: account.balance,
            need: delta.saturating_neg(),
        }),
        _ => Err(DomainError::Overflow),
    }
}

/// What follows from a command, or why it cannot be done. Reads the account and changes nothing.
pub fn decide(account: &Account, command: &Command) -> Result<Vec<Event>, DomainError> {
    if let Command::Open = command {
        return if account.open {
            Err(DomainError::AlreadyOpen)
        } else {
            Ok(vec![Event::Opened])
        };
    }
    if !account.open {
        return Err(DomainError::NotOpen);
    }
    let event = match command {
        Command::Open => unreachable!("handled above"),
        Command::Deposit { amount } => {
            allow(account, positive(*amount)?)?;
            Event::Deposited { amount: *amount }
        }
        Command::Withdraw { amount, reason } => {
            allow(account, -positive(*amount)?)?;
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
            allow(account, -positive(*amount)?)?;
            Event::TransferredOut {
                transfer: *transfer,
                to: to.clone(),
                amount: *amount,
            }
        }
        Command::ReceiveTransfer {
            transfer,
            from,
            amount,
        } => {
            allow(account, positive(*amount)?)?;
            Event::TransferredIn {
                transfer: *transfer,
                from: from.clone(),
                amount: *amount,
            }
        }
        Command::Reverse { target } => {
            let effect = *account
                .reversible
                .get(target)
                .ok_or(DomainError::NotReversible(*target))?;
            if account.reversed.contains(target) {
                return Err(DomainError::AlreadyReversed(*target));
            }
            let delta = effect.checked_neg().ok_or(DomainError::Overflow)?;
            allow(account, delta)?;
            Event::Reversed {
                target: *target,
                delta,
            }
        }
    };
    Ok(vec![event])
}

/// Applies something that happened. Never refuses and never panics.
pub fn evolve(mut account: Account, event: &Event) -> Account {
    account.version = account.version.saturating_add(1);
    let change = match event {
        Event::Opened => {
            account.open = true;
            0
        }
        Event::Deposited { amount } => {
            account.reversible.insert(account.version, *amount);
            *amount
        }
        Event::Withdrawn { amount, .. } => {
            let effect = amount.saturating_neg();
            account.reversible.insert(account.version, effect);
            effect
        }
        Event::TransferredOut { amount, .. } => amount.saturating_neg(),
        Event::TransferredIn { amount, .. } => *amount,
        Event::Reversed { target, delta } => {
            account.reversed.insert(*target);
            *delta
        }
    };
    account.balance = account.balance.saturating_add(change);
    account
}

/// The account after all of these, from a new one.
pub fn replay(events: &[Event]) -> Account {
    events.iter().fold(Account::default(), evolve)
}

// ---------------------------------------------------------------- the journal: stored events, old shapes, the store

/// Who made up a command id. Clients use only `Client`; the other two are made from log positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CmdId {
    Client(u64),
    /// The answer to the event at this log position.
    Reaction(u64),
    /// The return of the money sent by the event at this log position.
    Bounce(u64),
}

/// The schema this code writes.
pub const SCHEMA: u32 = 2;

/// One line of the journal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stored {
    /// Place in the whole log, from 0.
    pub pos: u64,
    pub stream: String,
    /// Place in its stream, from 1.
    pub version: u64,
    pub cmd: CmdId,
    /// Fingerprint of the command, or 0 when the history does not know it.
    pub print: u64,
    /// Seconds, from the writer's clock.
    pub ts: u64,
    pub schema: u32,
    /// The event, in the shape of `schema`. Never rewritten.
    pub payload: Value,
}

/// An event that has not been given a place yet.
#[derive(Debug, Clone, PartialEq)]
pub struct NewEvent {
    pub cmd: CmdId,
    pub print: u64,
    pub ts: u64,
    pub schema: u32,
    pub payload: Value,
}

impl NewEvent {
    /// An event in today's shape.
    pub fn current(cmd: CmdId, print: u64, ts: u64, event: &Event) -> NewEvent {
        NewEvent {
            cmd,
            print,
            ts,
            schema: SCHEMA,
            payload: serde_json::to_value(event).expect("an event serialises"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    UnknownSchema(u32),
    BadPayload,
}

#[derive(Debug)]
pub enum StoreError {
    Conflict {
        stream: String,
        expected: u64,
        actual: u64,
    },
    BadEvent(DecodeError),
    Io(io::Error),
    /// An earlier write failed. The store takes no more until it is opened again.
    Poisoned,
}

pub trait EventStore {
    /// Adds the events to the stream, all or none, only if the stream is at version `expected`.
    fn append(
        &mut self,
        stream: &str,
        expected: u64,
        events: Vec<NewEvent>,
    ) -> Result<Vec<Stored>, StoreError>;
    /// The events of one stream with a version above `after`.
    fn read_stream(&self, stream: &str, after: u64) -> Vec<Stored>;
    /// The whole log from position `from`.
    fn read_all(&self, from: u64) -> &[Stored];
    fn version(&self, stream: &str) -> u64;
    /// The streams that have events, sorted.
    fn streams(&self) -> Vec<String>;
}

/// Brings a version 1 payload to version 2.
fn from_1(payload: &mut Value) -> Option<()> {
    let old_name = match payload.get("type")?.as_str()? {
        "Deposited" => "cents",
        "Withdrawn" => "amt",
        _ => return Some(()),
    };
    let withdrawn = old_name == "amt";
    let data = payload.get_mut("data")?.as_object_mut()?;
    let amount = data.remove(old_name)?;
    data.insert("amount".into(), amount);
    if withdrawn {
        data.insert("reason".into(), "legacy".into());
    }
    Some(())
}

/// The event a stored line stands for, whatever version wrote it.
pub fn decode(stored: &Stored) -> Result<Event, DecodeError> {
    if stored.schema == 0 || stored.schema > SCHEMA {
        return Err(DecodeError::UnknownSchema(stored.schema));
    }
    let mut payload = stored.payload.clone();
    if stored.schema == 1 {
        from_1(&mut payload).ok_or(DecodeError::BadPayload)?;
    }
    serde_json::from_value(payload).map_err(|_| DecodeError::BadPayload)
}

/// Why a batch read from somewhere else cannot continue this log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadBatch {
    Empty,
    /// This line's position or stream version is not the next one.
    OutOfSequence {
        pos: u64,
    },
    Unreadable {
        pos: u64,
        why: DecodeError,
    },
}

#[derive(Debug, Default)]
pub struct MemStore {
    log: Vec<Stored>,
    /// stream -> places in `log`
    streams: BTreeMap<String, Vec<usize>>,
}

impl MemStore {
    pub fn new() -> MemStore {
        MemStore::default()
    }

    /// Checks an append and gives its events their places, without changing anything.
    pub fn prepare(
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
        let batch: Vec<Stored> = (0u64..)
            .zip(events)
            .map(|(i, e)| Stored {
                pos: self.log.len() as u64 + i,
                stream: stream.into(),
                version: expected + 1 + i,
                cmd: e.cmd,
                print: e.print,
                ts: e.ts,
                schema: e.schema,
                payload: e.payload,
            })
            .collect();
        for line in &batch {
            decode(line).map_err(StoreError::BadEvent)?;
        }
        Ok(batch)
    }

    /// Adds lines that already have their places, all or none.
    pub fn admit(&mut self, batch: Vec<Stored>) -> Result<(), BadBatch> {
        if batch.is_empty() {
            return Err(BadBatch::Empty);
        }
        let mut next: BTreeMap<&str, u64> = BTreeMap::new();
        for (i, line) in batch.iter().enumerate() {
            let version = next
                .entry(&line.stream)
                .or_insert_with(|| self.version(&line.stream));
            *version += 1;
            if line.pos != (self.log.len() + i) as u64 || line.version != *version {
                return Err(BadBatch::OutOfSequence { pos: line.pos });
            }
            decode(line).map_err(|why| BadBatch::Unreadable { pos: line.pos, why })?;
        }
        for line in batch {
            let places = self.streams.entry(line.stream.clone()).or_default();
            places.push(self.log.len());
            self.log.push(line);
        }
        Ok(())
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
        if !batch.is_empty() {
            self.admit(batch.clone())
                .expect("a prepared batch continues the log");
        }
        Ok(batch)
    }

    fn read_stream(&self, stream: &str, after: u64) -> Vec<Stored> {
        let places = self.streams.get(stream).map_or(&[][..], Vec::as_slice);
        let start = usize::try_from(after).unwrap_or(usize::MAX);
        let tail = places.get(start..).unwrap_or(&[]);
        tail.iter().map(|&i| self.log[i].clone()).collect()
    }

    fn read_all(&self, from: u64) -> &[Stored] {
        let start = usize::try_from(from).unwrap_or(usize::MAX);
        self.log.get(start..).unwrap_or(&[])
    }

    fn version(&self, stream: &str) -> u64 {
        self.streams.get(stream).map_or(0, |p| p.len() as u64)
    }

    fn streams(&self) -> Vec<String> {
        self.streams.keys().cloned().collect()
    }
}

// ---------------------------------------------------------------- handling commands: ids, conflicts, snapshots

/// A fingerprint that tells commands apart. Never 0. (Given.)
pub fn fingerprint(command: &Command) -> u64 {
    fnv(format!("{command:?}").as_bytes(), FNV_START) | 1
}

pub const FNV_START: u64 = 0xcbf2_9ce4_8422_2325;

/// FNV-1a, 64 bit.
pub fn fnv(bytes: &[u8], mut hash: u64) -> u64 {
    for &b in bytes {
        hash = (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// This many events were added.
    Applied(usize),
    /// This command id had done this command already.
    Duplicate,
}

#[derive(Debug)]
pub enum CmdError {
    Domain(DomainError),
    Store(StoreError),
}

impl From<DomainError> for CmdError {
    fn from(e: DomainError) -> CmdError {
        CmdError::Domain(e)
    }
}

impl From<StoreError> for CmdError {
    fn from(e: StoreError) -> CmdError {
        CmdError::Store(e)
    }
}

pub struct Ledger<S: EventStore> {
    pub store: S,
    /// The writer's clock, in seconds. The caller sets it.
    pub now: u64,
    snapshots: BTreeMap<String, Account>,
    snapshot_every: u64,
}

/// The account after one more stored line. The store has checked that the line decodes.
pub fn fold(account: Account, line: &Stored) -> Account {
    let event = decode(line).expect("the store admits only readable lines");
    let mut next = evolve(account, &event);
    next.commands.insert(line.cmd, line.print);
    next
}

impl<S: EventStore> Ledger<S> {
    /// A snapshot is taken each time a stream passes a multiple of `snapshot_every` events; 0 means never.
    pub fn new(store: S, snapshot_every: u64) -> Ledger<S> {
        Ledger {
            store,
            now: 0,
            snapshots: BTreeMap::new(),
            snapshot_every,
        }
    }

    pub fn snapshot_version(&self, stream: &str) -> Option<u64> {
        self.snapshots.get(stream).map(|a| a.version)
    }

    /// The account now: the snapshot, if there is one, and the events after it.
    pub fn load(&self, stream: &str) -> Account {
        let start = self.snapshots.get(stream).cloned().unwrap_or_default();
        let tail = self.store.read_stream(stream, start.version);
        tail.iter().fold(start, fold)
    }

    /// The account as it was right after the event with this version.
    pub fn state_at(&self, stream: &str, version: u64) -> Account {
        let lines = self.store.read_stream(stream, 0);
        lines
            .iter()
            .take_while(|l| l.version <= version)
            .fold(Account::default(), fold)
    }

    /// One attempt to do a command, from an account that was loaded earlier and may be out of date.
    pub fn try_commit(
        &mut self,
        stream: &str,
        loaded: &Account,
        id: CmdId,
        command: &Command,
    ) -> Result<Outcome, CmdError> {
        let print = fingerprint(command);
        if let Some(&used) = loaded.commands.get(&id) {
            return if used == print || used == 0 {
                Ok(Outcome::Duplicate)
            } else {
                Err(DomainError::IdReused.into())
            };
        }
        let events = match decide(loaded, command) {
            Ok(events) => events,
            Err(refusal) => {
                // a "no" is only as good as the state it came from
                let actual = self.store.version(stream);
                if actual != loaded.version {
                    return Err(StoreError::Conflict {
                        stream: stream.into(),
                        expected: loaded.version,
                        actual,
                    }
                    .into());
                }
                return Err(refusal.into());
            }
        };
        let new = events
            .iter()
            .map(|e| NewEvent::current(id, print, self.now, e))
            .collect();
        let lines = self.store.append(stream, loaded.version, new)?;
        let next = lines.iter().fold(loaded.clone(), fold);
        let passed = |version: u64| version.checked_div(self.snapshot_every);
        if passed(next.version) > passed(loaded.version) {
            self.snapshots.insert(stream.into(), next);
        }
        Ok(Outcome::Applied(lines.len()))
    }

    /// Loads and commits, and starts again for as long as somebody else got there first.
    pub fn execute(
        &mut self,
        stream: &str,
        id: CmdId,
        command: &Command,
    ) -> Result<Outcome, CmdError> {
        loop {
            let loaded = self.load(stream);
            match self.try_commit(stream, &loaded, id, command) {
                Err(CmdError::Store(StoreError::Conflict { .. })) => continue,
                other => return other,
            }
        }
    }
}

// ---------------------------------------------------------------- the journal on disk

/// CRC-32, as used by zip and Ethernet. (Given.)
pub fn crc32(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in data {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = (c >> 1) ^ (0xEDB8_8320 & 0u32.wrapping_sub(c & 1));
        }
    }
    !c
}

/// The bytes in front of a batch: its length, the checksum of the length's four bytes, and the checksum of the batch.
pub const HEADER: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tail {
    Clean,
    /// An unfinished last write starts at this byte.
    Torn {
        at: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadError {
    /// Bytes at this offset that no unfinished write explains.
    Damaged { at: usize },
    /// The frame at this offset arrived whole, and what it holds cannot continue the log.
    BadBatch { at: usize },
}

#[derive(Debug)]
pub struct Recovered {
    pub store: MemStore,
    /// How many bytes from the start are whole frames.
    pub keep: usize,
    pub tail: Tail,
}

#[derive(Debug)]
pub enum OpenError {
    Io(io::Error),
    Load(LoadError),
}

impl From<io::Error> for OpenError {
    fn from(e: io::Error) -> OpenError {
        OpenError::Io(e)
    }
}

/// What the store needs from a file, so that a test can make a write or a sync fail.
pub trait Device {
    /// Adds bytes at the end. When it fails, any prefix of them may have been added.
    fn append(&mut self, bytes: &[u8]) -> io::Result<()>;
    /// Makes what was added survive a power cut.
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

pub struct FileStore<D: Device = File> {
    memory: MemStore,
    pub device: D,
    poisoned: bool,
}

/// One batch as it lies in the file.
pub fn frame(batch: &[Stored]) -> Vec<u8> {
    let payload = serde_json::to_vec(batch).expect("a batch serialises");
    let length = (payload.len() as u32).to_le_bytes();
    let mut out = Vec::with_capacity(HEADER + payload.len());
    out.extend_from_slice(&length);
    out.extend_from_slice(&crc32(&length).to_le_bytes());
    out.extend_from_slice(&crc32(&payload).to_le_bytes());
    out.extend_from_slice(&payload);
    out
}

fn word(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes.try_into().expect("four bytes"))
}

/// Rebuilds the journal from the bytes of the file. Changes nothing anywhere.
pub fn recover(bytes: &[u8]) -> Result<Recovered, LoadError> {
    let zeros = |b: &[u8]| b.iter().all(|&x| x == 0);
    let mut store = MemStore::new();
    let mut at = 0;
    let torn = |store, at| {
        Ok(Recovered {
            store,
            keep: at,
            tail: Tail::Torn { at },
        })
    };
    while at < bytes.len() {
        let rest = &bytes[at..];
        if rest.len() < HEADER {
            return torn(store, at);
        }
        if crc32(&rest[..4]) != word(&rest[4..8]) || word(&rest[..4]) == 0 {
            return if zeros(rest) {
                torn(store, at)
            } else {
                Err(LoadError::Damaged { at })
            };
        }
        // the length has its own checksum, so from here it can be believed
        let end = HEADER + word(&rest[..4]) as usize;
        let Some(payload) = rest.get(HEADER..end) else {
            return torn(store, at);
        };
        if crc32(payload) != word(&rest[8..HEADER]) {
            return if zeros(&rest[end..]) {
                torn(store, at)
            } else {
                Err(LoadError::Damaged { at })
            };
        }
        let batch: Vec<Stored> =
            serde_json::from_slice(payload).map_err(|_| LoadError::BadBatch { at })?;
        store.admit(batch).map_err(|_| LoadError::BadBatch { at })?;
        at += end;
    }
    Ok(Recovered {
        store,
        keep: at,
        tail: Tail::Clean,
    })
}

impl<D: Device> FileStore<D> {
    /// A store over a device that holds exactly the frames of `memory`.
    pub fn over(memory: MemStore, device: D) -> FileStore<D> {
        FileStore {
            memory,
            device,
            poisoned: false,
        }
    }
}

impl FileStore<File> {
    /// Opens the journal in this file, creating it if it is missing.
    pub fn open(path: &Path) -> Result<(FileStore<File>, Tail), OpenError> {
        let existed = path.exists();
        let bytes = if existed {
            std::fs::read(path)?
        } else {
            Vec::new()
        };
        // an error here has changed nothing on disk
        let found = recover(&bytes).map_err(OpenError::Load)?;
        if found.keep != bytes.len() {
            let file = OpenOptions::new().write(true).open(path)?;
            file.set_len(found.keep as u64)?;
            file.sync_all()?;
        }
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        if !existed {
            // a new file's name is durable only once its directory is synced
            let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
            File::open(parent.unwrap_or(Path::new(".")))?.sync_all()?;
        }
        Ok((FileStore::over(found.store, file), found.tail))
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
        let batch = self.memory.prepare(stream, expected, events)?;
        if batch.is_empty() {
            return Ok(batch);
        }
        let written = self.device.append(&frame(&batch));
        if let Err(e) = written.and_then(|()| self.device.sync()) {
            // what the file holds is unknown now: nothing may be written behind it
            self.poisoned = true;
            return Err(StoreError::Io(e));
        }
        self.memory
            .admit(batch.clone())
            .expect("a prepared batch continues the log");
        Ok(batch)
    }

    fn read_stream(&self, stream: &str, after: u64) -> Vec<Stored> {
        self.memory.read_stream(stream, after)
    }
    fn read_all(&self, from: u64) -> &[Stored] {
        self.memory.read_all(from)
    }
    fn version(&self, stream: &str) -> u64 {
        self.memory.version(stream)
    }
    fn streams(&self) -> Vec<String> {
        self.memory.streams()
    }
}

// ---------------------------------------------------------------- moving money between accounts

#[derive(Debug, Default)]
pub struct Tick {
    /// Transfers that arrived in this tick.
    pub received: u64,
    /// Transfers that went back to their sender in this tick.
    pub bounced: u64,
    /// Log positions of transfers that could neither arrive nor go back.
    pub stuck: Vec<u64>,
}

pub struct Courier {
    /// The first log position not yet dealt with. A restart may begin from any older value.
    pub next_pos: u64,
}

impl Courier {
    /// Deals with every transfer in the log from `next_pos` on.
    pub fn tick<S: EventStore>(&mut self, ledger: &mut Ledger<S>) -> Result<Tick, StoreError> {
        let fresh: Vec<Stored> = ledger.store.read_all(self.next_pos).to_vec();
        let mut done = Tick::default();
        for line in &fresh {
            let event = decode(line).expect("the store admits only readable lines");
            if let Event::TransferredOut {
                transfer,
                to,
                amount,
            } = event
            {
                deliver(ledger, line, transfer, &to, amount, &mut done)?;
            }
            // only now: a store failure above leaves this line to be seen again
            self.next_pos = line.pos + 1;
        }
        Ok(done)
    }
}

fn deliver<S: EventStore>(
    ledger: &mut Ledger<S>,
    line: &Stored,
    transfer: u64,
    to: &str,
    amount: i64,
    done: &mut Tick,
) -> Result<(), StoreError> {
    let bounce = CmdId::Bounce(line.pos);
    // money that went back must not also arrive, even if the target could take it now
    if ledger.load(&line.stream).commands.contains_key(&bounce) {
        return Ok(());
    }
    let receive = Command::ReceiveTransfer {
        transfer,
        from: line.stream.clone(),
        amount,
    };
    match ledger.execute(to, CmdId::Reaction(line.pos), &receive) {
        Ok(Outcome::Applied(_)) => done.received += 1,
        Ok(Outcome::Duplicate) => {}
        Err(CmdError::Store(e)) => return Err(e),
        Err(CmdError::Domain(_)) => {
            let back = Command::ReceiveTransfer {
                transfer,
                from: to.to_string(),
                amount,
            };
            match ledger.execute(&line.stream, bounce, &back) {
                Ok(_) => done.bounced += 1,
                Err(CmdError::Store(e)) => return Err(e),
                Err(CmdError::Domain(_)) => done.stuck.push(line.pos),
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- a whole day, one small step at a time

/// What one writer wants done, in order: (stream, command).
pub type Script = Vec<(String, Command)>;

/// The id of command number `index` of writer number `writer`, both from 0.
pub fn command_id(writer: usize, index: usize) -> CmdId {
    CmdId::Client(((writer as u64 + 1) << 32) | index as u64)
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct DayReport {
    pub applied: u64,
    pub duplicates: u64,
    pub conflicts: u64,
    /// (writer, index, why), in the order the refusals happened.
    pub refused: Vec<(usize, usize, DomainError)>,
    pub received: u64,
    pub bounced: u64,
}

struct Writer<'a> {
    script: &'a Script,
    next: usize,
    loaded: Option<Account>,
}

impl Writer<'_> {
    fn finished(&self) -> bool {
        self.next == self.script.len()
    }

    /// One step: load for the current command, or try to commit it.
    fn step<S: EventStore>(
        &mut self,
        me: usize,
        ledger: &mut Ledger<S>,
        report: &mut DayReport,
    ) -> Result<(), StoreError> {
        let (stream, command) = &self.script[self.next];
        let Some(loaded) = self.loaded.take() else {
            self.loaded = Some(ledger.load(stream));
            return Ok(());
        };
        match ledger.try_commit(stream, &loaded, command_id(me, self.next), command) {
            Ok(Outcome::Applied(_)) => report.applied += 1,
            Ok(Outcome::Duplicate) => report.duplicates += 1,
            Err(CmdError::Domain(why)) => report.refused.push((me, self.next, why)),
            Err(CmdError::Store(StoreError::Conflict { .. })) => {
                report.conflicts += 1;
                return Ok(()); // the same command, loaded again next time
            }
            Err(CmdError::Store(e)) => return Err(e),
        }
        self.next += 1;
        Ok(())
    }
}

/// Runs the day to its end. `choose(n)` picks who moves next among `n`.
pub fn run_day<S: EventStore>(
    ledger: &mut Ledger<S>,
    scripts: &[Script],
    mut choose: impl FnMut(usize) -> usize,
) -> Result<DayReport, StoreError> {
    let mut writers: Vec<Writer> = scripts
        .iter()
        .map(|script| Writer {
            script,
            next: 0,
            loaded: None,
        })
        .collect();
    let mut courier = Courier { next_pos: 0 };
    let mut report = DayReport::default();
    loop {
        let mut ready: Vec<usize> = (0..writers.len())
            .filter(|&w| !writers[w].finished())
            .collect();
        // the courier is one more worker, listed last, while it has lines it has not seen
        if ready.is_empty() && !ledger.store.read_all(courier.next_pos).is_empty() {
            ready.push(writers.len());
        }
        if ready.is_empty() {
            return Ok(report);
        }
        let who = ready[choose(ready.len())];
        match writers.get_mut(who) {
            Some(writer) => writer.step(who, ledger, &mut report)?,
            None => {
                let tick = courier.tick(ledger)?;
                report.received += tick.received;
                report.bounced += tick.bounced;
            }
        }
    }
}
