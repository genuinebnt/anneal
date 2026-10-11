//! Claims on keys for first-updater-wins conflict detection.

use std::collections::HashMap;

#[derive(Debug, PartialEq, Eq)]
pub struct Conflict {
    pub owner: u64,
}

#[derive(Default)]
pub struct WriteClaims {
    // @begin 4b-c1
    owners: HashMap<i64, u64>,
    //~ _claims: (),
    // @end
}

impl WriteClaims {
    /// No key is claimed.
    pub fn new() -> WriteClaims {
        // @begin 4b-c1
        WriteClaims { owners: HashMap::new() }
        //~ todo!("4b-c1: nothing is claimed")
        // @end
    }

    /// Takes `key` for `txn` if it is free or already `txn`'s.
    ///
    /// # Errors
    ///
    /// [`Conflict`] naming the owner if another transaction holds it.
    pub fn claim(&mut self, txn: u64, key: i64) -> Result<(), Conflict> {
        // @begin 4b-c1
        match self.owners.get(&key) {
            Some(&o) if o != txn => Err(Conflict { owner: o }),
            _ => {
                self.owners.insert(key, txn);
                Ok(())
            }
        }
        //~ todo!("4b-c1: take the key if it is free or already yours")
        // @end
    }

    /// Frees every key `txn` holds and returns how many there were.
    pub fn release_all(&mut self, txn: u64) -> usize {
        // @begin 4b-c1
        let before = self.owners.len();
        self.owners.retain(|_, o| *o != txn);
        before - self.owners.len()
        //~ todo!("4b-c1: free every key the transaction holds")
        // @end
    }

    /// The transaction that holds `key`, if any.
    pub fn owner_of(&self, key: i64) -> Option<u64> {
        // @begin 4b-c1
        self.owners.get(&key).copied()
        //~ todo!("4b-c1: who holds the key")
        // @end
    }
}
