//! An iterator that can go back to a marked position.

use std::collections::VecDeque;

pub struct Rewindable<I: Iterator> {
    _rewind: std::marker::PhantomData<I>,
}

impl<I: Iterator> Rewindable<I>
where
    I::Item: Clone,
{
    /// Wraps `inner`; nothing is marked, so nothing is buffered.
    pub fn new(inner: I) -> Rewindable<I> {
        todo!("3e-c5: wrap the iterator, nothing marked")
    }

    /// Marks the current position: from here on, items read are kept so that [`reset`](Self::reset) can replay them. Items still waiting to be replayed
    /// come after the mark and stay buffered.
    pub fn mark(&mut self) {
        todo!("3e-c5: forget what was read before this point, keep what is yet to be replayed")
    }

    /// Goes back to the mark, so the items read since are produced again. Without a mark it does nothing.
    pub fn reset(&mut self) {
        todo!("3e-c5: replay everything since the mark")
    }

    /// How many items are being held for a possible reset.
    pub fn buffered(&self) -> usize {
        todo!("3e-c5: how many items are kept")
    }
}

impl<I: Iterator> Iterator for Rewindable<I>
where
    I::Item: Clone,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        todo!("3e-c5: replayed items first, then the underlying iterator (keeping what is read since the mark)")
    }
}
