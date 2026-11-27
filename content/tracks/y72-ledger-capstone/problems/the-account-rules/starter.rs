use std::collections::{BTreeMap, BTreeSet};

// ---------------------------------------------------------------- the account: events, commands, rules

/// Something that happened to one account. Stored for ever.
#[derive(Debug, Clone, PartialEq, Eq)]
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

/// What follows from a command, or why it cannot be done. Reads the account and changes nothing.
pub fn decide(account: &Account, command: &Command) -> Result<Vec<Event>, DomainError> {
    todo!()
}

/// Applies something that happened. Never refuses and never panics.
pub fn evolve(account: Account, event: &Event) -> Account {
    todo!()
}

/// The account after all of these, from a new one.
pub fn replay(events: &[Event]) -> Account {
    todo!()
}
