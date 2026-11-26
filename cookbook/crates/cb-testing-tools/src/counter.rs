//! Lock-free counters. Both are written against `crate::sync`, so loom can model them.
use crate::sync::{AtomicUsize, Ordering};

/// Hands out unique ids. `Relaxed` is enough: only the atomicity of the RMW matters, nothing
/// else is published through the counter.
#[derive(Debug, Default)]
pub struct IdGen(AtomicUsize);

impl IdGen {
    pub fn new() -> Self {
        IdGen(AtomicUsize::new(0))
    }
    pub fn next_id(&self) -> usize {
        self.0.fetch_add(1, Ordering::Relaxed)
    }
}

/// A counter that never exceeds `limit` (a permit/slot dispenser): CAS loop, not fetch_add.
#[derive(Debug)]
pub struct Bounded {
    used: AtomicUsize,
    limit: usize,
}

impl Bounded {
    pub fn new(limit: usize) -> Self {
        Bounded {
            used: AtomicUsize::new(0),
            limit,
        }
    }
    /// Takes one slot; `false` once `limit` slots are gone.
    pub fn try_take(&self) -> bool {
        let mut cur = self.used.load(Ordering::Relaxed);
        loop {
            if cur >= self.limit {
                return false;
            }
            match self
                .used
                .compare_exchange_weak(cur, cur + 1, Ordering::AcqRel, Ordering::Relaxed)
            {
                Ok(_) => return true,
                Err(seen) => cur = seen,
            }
        }
    }
    pub fn used(&self) -> usize {
        self.used.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_stops_at_limit() {
        let b = Bounded::new(2);
        assert_eq!(
            (b.try_take(), b.try_take(), b.try_take()),
            (true, true, false)
        );
        assert_eq!(b.used(), 2);
        assert_eq!(IdGen::new().next_id(), 0);
    }
}
