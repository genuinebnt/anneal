//! Claims on keys for first-updater-wins conflict detection.

use std::collections::HashMap;

#[derive(Debug, PartialEq, Eq)]
pub struct Conflict {
    pub owner: u64,
}

#[derive(Default)]
pub struct WriteClaims {
    _claims: (),
}

impl WriteClaims {
    /// No key is claimed.
    pub fn new() -> WriteClaims {
        todo!("4b-c1: nothing is claimed")
    }

    /// Takes `key` for `txn` if it is free or already `txn`'s.
    ///
    /// # Errors
    ///
    /// [`Conflict`] naming the owner if another transaction holds it.
    pub fn claim(&mut self, txn: u64, key: i64) -> Result<(), Conflict> {
        todo!("4b-c1: take the key if it is free or already yours")
    }

    /// Frees every key `txn` holds and returns how many there were.
    pub fn release_all(&mut self, txn: u64) -> usize {
        todo!("4b-c1: free every key the transaction holds")
    }

    /// The transaction that holds `key`, if any.
    pub fn owner_of(&self, key: i64) -> Option<u64> {
        todo!("4b-c1: who holds the key")
    }
}
