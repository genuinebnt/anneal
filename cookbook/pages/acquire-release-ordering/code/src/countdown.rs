//! "Last one out": the ordering pattern behind `Arc`'s drop, without the allocation tricks.
//! Two owners each leave a result. Whoever finishes second reads both.

use crate::sync::{Arc, AtomicUsize, Ordering, UnsafeCell, fence};

struct Shared {
    results: [UnsafeCell<u64>; 2],
    remaining: AtomicUsize,
}

// SAFETY: slot `i` is written only by the `Part` with index `i`, and there is one
// such `Part`. Both slots are read only by the thread that takes `remaining` to
// zero, after its Acquire fence.
unsafe impl Sync for Shared {}

pub struct Part {
    shared: Arc<Shared>,
    index: usize,
}

pub fn pair() -> [Part; 2] {
    let shared = Arc::new(Shared {
        results: [UnsafeCell::new(0), UnsafeCell::new(0)],
        remaining: AtomicUsize::new(2),
    });
    [
        Part {
            shared: shared.clone(),
            index: 0,
        },
        Part { shared, index: 1 },
    ]
}

impl Part {
    /// Returns the sum of both results to whichever call comes second.
    pub fn finish(self, result: u64) -> Option<u64> {
        let mine = &self.shared.results[self.index];
        // SAFETY: only this `Part` writes this slot, and no one reads it before
        // the decrement below.
        mine.with_mut(|p| unsafe { *p = result });

        // Release: my write above stays before my decrement.
        if self.shared.remaining.fetch_sub(1, Ordering::Release) != 1 {
            return None;
        }
        // Acquire: pairs with the other thread's Release decrement, which this
        // thread's own decrement read from.
        fence(Ordering::Acquire);

        let results = &self.shared.results;
        // SAFETY: both decrements happened, and the fence orders the other
        // thread's write before these reads.
        Some(results.iter().map(|c| c.with(|p| unsafe { *p })).sum())
    }
}
