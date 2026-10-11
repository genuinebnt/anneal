//! A table of balances with one lock per slot.

use std::sync::Mutex;

#[derive(Debug, PartialEq, Eq)]
pub enum TransferError {
    NoSuchSlot(usize),
    Insufficient,
}

pub struct Slots {
    // @begin 1g-c5
    slots: Vec<Mutex<i64>>,
    //~ _slots: (),
    // @end
}

impl Slots {
    /// One slot per balance, each behind its own lock.
    pub fn new(balances: &[i64]) -> Slots {
        // @begin 1g-c5
        Slots { slots: balances.iter().map(|&b| Mutex::new(b)).collect() }
        //~ todo!("1g-c5: one lock per slot")
        // @end
    }

    /// The balance of slot `i`; `None` if there is no such slot.
    pub fn balance(&self, i: usize) -> Option<i64> {
        // @begin 1g-c5
        self.slots.get(i).map(|m| *m.lock().unwrap())
        //~ todo!("1g-c5: the balance of one slot")
        // @end
    }

    /// The sum of every balance at one instant: no transfer is half done in it.
    pub fn total(&self) -> i64 {
        // @begin 1g-c5
        let guards: Vec<_> = self.slots.iter().map(|m| m.lock().unwrap()).collect();
        guards.iter().map(|g| **g).sum()
        //~ todo!("1g-c5: every balance at one instant")
        // @end
    }

    /// Moves `amount` from slot `from` to slot `to` atomically: no thread sees the money in neither slot or in both, and two opposite
    /// transfers cannot deadlock.
    ///
    /// # Errors
    ///
    /// [`TransferError::NoSuchSlot`] if a slot does not exist, [`TransferError::Insufficient`] if `from` holds less than `amount`.
    pub fn transfer(&self, from: usize, to: usize, amount: i64) -> Result<(), TransferError> {
        // @begin 1g-c5
        let n = self.slots.len();
        if from >= n {
            return Err(TransferError::NoSuchSlot(from));
        }
        if to >= n {
            return Err(TransferError::NoSuchSlot(to));
        }
        if from == to {
            return Ok(());
        }
        let (lo, hi) = if from < to { (from, to) } else { (to, from) };
        let mut a = self.slots[lo].lock().unwrap();
        let mut b = self.slots[hi].lock().unwrap();
        let (src, dst) = if from < to { (&mut *a, &mut *b) } else { (&mut *b, &mut *a) };
        if *src < amount {
            return Err(TransferError::Insufficient);
        }
        *src -= amount;
        *dst += amount;
        Ok(())
        //~ todo!("1g-c5: both locks, in an order every thread agrees on; then the move")
        // @end
    }
}
