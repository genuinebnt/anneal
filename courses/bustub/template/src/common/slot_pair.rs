//! A table of balances with one lock per slot.

use std::sync::Mutex;

#[derive(Debug, PartialEq, Eq)]
pub enum TransferError {
    NoSuchSlot(usize),
    Insufficient,
}

pub struct Slots {
    _slots: (),
}

impl Slots {
    /// One slot per balance, each behind its own lock.
    pub fn new(balances: &[i64]) -> Slots {
        todo!("1g-c5: one lock per slot")
    }

    /// The balance of slot `i`; `None` if there is no such slot.
    pub fn balance(&self, i: usize) -> Option<i64> {
        todo!("1g-c5: the balance of one slot")
    }

    /// The sum of every balance at one instant: no transfer is half done in it.
    pub fn total(&self) -> i64 {
        todo!("1g-c5: every balance at one instant")
    }

    /// Moves `amount` from slot `from` to slot `to` atomically: no thread sees the money in neither slot or in both, and two opposite
    /// transfers cannot deadlock.
    ///
    /// # Errors
    ///
    /// [`TransferError::NoSuchSlot`] if a slot does not exist, [`TransferError::Insufficient`] if `from` holds less than `amount`.
    pub fn transfer(&self, from: usize, to: usize, amount: i64) -> Result<(), TransferError> {
        todo!("1g-c5: both locks, in an order every thread agrees on; then the move")
    }
}
