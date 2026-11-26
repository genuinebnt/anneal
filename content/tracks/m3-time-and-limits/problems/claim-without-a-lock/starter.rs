use std::ops::Range;

/// `len` numbered units, handed out front to back to any number of threads.
pub struct Region {
    len: usize,
}

impl Region {
    /// Panics if `len > usize::MAX / 2`.
    pub fn new(len: usize) -> Self {
        todo!()
    }

    /// The next `n` units, or `None` if they are not there or the region is closed.
    pub fn claim(&self, n: usize) -> Option<Range<usize>> {
        self.claim_with(|_| n)
    }

    /// Like `claim`, but `choose` is told how many units are free and says how many to take.
    pub fn claim_with(&self, choose: impl FnMut(usize) -> usize) -> Option<Range<usize>> {
        todo!()
    }

    /// Units that could still be claimed: 0 once closed.
    pub fn free(&self) -> usize {
        todo!()
    }

    /// Stops all further claims and returns how many units were handed out.
    pub fn close(&self) -> usize {
        todo!()
    }
}
