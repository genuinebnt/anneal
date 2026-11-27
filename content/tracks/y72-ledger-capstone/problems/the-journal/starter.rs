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

/// The event a stored line stands for, whatever version wrote it.
pub fn decode(stored: &Stored) -> Result<Event, DecodeError> {
    todo!()
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
    // TODO: your fields
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
        todo!()
    }

    /// Adds lines that already have their places, all or none.
    pub fn admit(&mut self, batch: Vec<Stored>) -> Result<(), BadBatch> {
        todo!()
    }
}

impl EventStore for MemStore {
    fn append(
        &mut self,
        stream: &str,
        expected: u64,
        events: Vec<NewEvent>,
    ) -> Result<Vec<Stored>, StoreError> {
        todo!()
    }

    fn read_stream(&self, stream: &str, after: u64) -> Vec<Stored> {
        todo!()
    }

    fn read_all(&self, from: u64) -> &[Stored] {
        todo!()
    }

    fn version(&self, stream: &str) -> u64 {
        todo!()
    }

    fn streams(&self) -> Vec<String> {
        todo!()
    }
}
