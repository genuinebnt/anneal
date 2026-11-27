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
    account.balance += change;
    account
}

/// The account after all of these, from a new one.
pub fn replay(events: &[Event]) -> Account {
    events.iter().fold(Account::default(), evolve)
}
