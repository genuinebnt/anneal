use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::io;

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
    Insufficient { have: i64, need: i64 },
    NotReversible(u64),
    AlreadyReversed(u64),
    Overflow,
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

// ---------------------------------------------------------------- reports

pub const SECONDS_PER_DAY: u64 = 86_400;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Day {
    pub deposited: i128,
    pub withdrawn: i128,
    pub transferred: i128,
    /// The net balance change of the reversals made on this day, whatever day they correct.
    pub reversed: i128,
    /// How many journal lines are of this day.
    pub events: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reports {
    next_pos: u64,
    balances: BTreeMap<String, i128>,
    days: BTreeMap<u64, Day>,
    spent: BTreeMap<String, i128>,
}

impl Reports {
    /// The position of the first line not yet counted.
    pub fn next_pos(&self) -> u64 {
        self.next_pos
    }

    pub fn balance(&self, stream: &str) -> i128 {
        self.balances.get(stream).copied().unwrap_or(0)
    }

    pub fn day(&self, day: u64) -> Day {
        self.days.get(&day).copied().unwrap_or_default()
    }

    pub fn top_spenders(&self, n: usize) -> Vec<(String, i128)> {
        let mut all: Vec<(String, i128)> =
            self.spent.iter().map(|(k, v)| (k.clone(), *v)).collect();
        all.retain(|p| p.1 != 0);
        all.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        all.truncate(n);
        all
    }

    fn count(&mut self, line: &Stored) {
        let event = decode(line).expect("the store admits only readable lines");
        let day = self.days.entry(line.ts / SECONDS_PER_DAY).or_default();
        day.events += 1;
        if !self.spent.contains_key(&line.stream) {
            self.spent.insert(line.stream.clone(), 0);
            self.balances.insert(line.stream.clone(), 0);
        }
        let balance = self.balances.get_mut(&line.stream).expect("inserted");
        let spent = self.spent.get_mut(&line.stream).expect("inserted");
        let wide = i128::from;
        match event {
            Event::Opened => {}
            Event::Deposited { amount } => {
                *balance += wide(amount);
                day.deposited += wide(amount);
            }
            Event::Withdrawn { amount, .. } => {
                *balance -= wide(amount);
                *spent += wide(amount);
                day.withdrawn += wide(amount);
            }
            Event::TransferredOut { amount, .. } => {
                *balance -= wide(amount);
                *spent += wide(amount);
                day.transferred += wide(amount);
            }
            Event::TransferredIn { amount, .. } => *balance += wide(amount),
            Event::Reversed { delta, .. } => {
                *balance += wide(delta);
                day.reversed += wide(delta);
                // money that comes back undoes a withdrawal, so it was not spent after all
                if delta > 0 {
                    *spent -= wide(delta);
                }
            }
        }
        self.next_pos = line.pos + 1;
    }

    /// Counts every line the store has that these reports have not counted.
    pub fn catch_up<S: EventStore>(&mut self, store: &S) {
        for line in store.read_all(self.next_pos) {
            self.count(line);
        }
    }

    /// The reports for exactly these lines, which are the start of a log.
    pub fn from_log(lines: &[Stored]) -> Reports {
        let mut reports = Reports::default();
        for line in lines {
            reports.count(line);
        }
        reports
    }

    /// The reports as they were when the log had `pos` lines.
    pub fn as_of<S: EventStore>(store: &S, pos: u64) -> Reports {
        let all = store.read_all(0);
        let end = usize::try_from(pos).map_or(all.len(), |p| p.min(all.len()));
        Reports::from_log(&all[..end])
    }
}
