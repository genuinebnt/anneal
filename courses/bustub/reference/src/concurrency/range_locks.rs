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
    // @begin 4d-c2
    held: Vec<(u32, i64, i64, Mode)>,
    //~ _rl: (),
    // @end
}

impl RangeLocks {
    /// A set of range locks with none held.
    pub fn new() -> RangeLocks {
        // @begin 4d-c2
        RangeLocks { held: Vec::new() }
        //~ todo!("4d-c2: no locks held")
        // @end
    }

    /// Locks the half-open range `lo..hi` for `txn` in `mode`; returns whether it was granted.
    ///
    /// A lock is refused when it overlaps an incompatible lock of another transaction; two shared locks are compatible.
    ///
    /// # Errors
    ///
    /// [`EmptyRange`] if `lo >= hi`.
    pub fn try_lock(&mut self, txn: u32, lo: i64, hi: i64, mode: Mode) -> Result<bool, EmptyRange> {
        // @begin 4d-c2
        if lo >= hi {
            return Err(EmptyRange);
        }
        let conflict = self.held.iter().any(|&(t, l, h, m)| t != txn && lo < h && l < hi && !(m == Mode::Shared && mode == Mode::Shared));
        if conflict {
            return Ok(false);
        }
        self.held.push((txn, lo, hi, mode));
        Ok(true)
        //~ todo!("4d-c2: refuse on an overlapping incompatible lock of another transaction; otherwise record it")
        // @end
    }

    /// Releases every lock `txn` holds and returns how many there were.
    pub fn unlock_all(&mut self, txn: u32) -> usize {
        // @begin 4d-c2
        let before = self.held.len();
        self.held.retain(|h| h.0 != txn);
        before - self.held.len()
        //~ todo!("4d-c2: drop the transaction's locks")
        // @end
    }

    /// How many locks `txn` holds.
    pub fn held_by(&self, txn: u32) -> usize {
        // @begin 4d-c2
        self.held.iter().filter(|h| h.0 == txn).count()
        //~ todo!("4d-c2: how many locks the transaction holds")
        // @end
    }
}
