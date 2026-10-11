//! Locks on half-open key ranges.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Shared,
    Exclusive,
}

#[derive(Debug, PartialEq, Eq)]
pub struct EmptyRange;

#[derive(Default)]
pub struct RangeLocks {
    _rl: (),
}

impl RangeLocks {
    /// A set of range locks with none held.
    pub fn new() -> RangeLocks {
        todo!("4d-c2: no locks held")
    }

    /// Locks the half-open range `lo..hi` for `txn` in `mode`; returns whether it was granted.
    ///
    /// A lock is refused when it overlaps an incompatible lock of another transaction; two shared locks are compatible.
    ///
    /// # Errors
    ///
    /// [`EmptyRange`] if `lo >= hi`.
    pub fn try_lock(&mut self, txn: u32, lo: i64, hi: i64, mode: Mode) -> Result<bool, EmptyRange> {
        todo!("4d-c2: refuse on an overlapping incompatible lock of another transaction; otherwise record it")
    }

    /// Releases every lock `txn` holds and returns how many there were.
    pub fn unlock_all(&mut self, txn: u32) -> usize {
        todo!("4d-c2: drop the transaction's locks")
    }

    /// How many locks `txn` holds.
    pub fn held_by(&self, txn: u32) -> usize {
        todo!("4d-c2: how many locks the transaction holds")
    }
}
