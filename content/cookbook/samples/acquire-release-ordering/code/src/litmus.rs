//! Litmus tests: tiny programs whose outcomes show what an ordering allows.
//! Every shared location is atomic, so choosing a weak ordering is not undefined behaviour.

use crate::sync::{AtomicBool, AtomicU32, AtomicU64, Ordering, fence};

/// Message passing: `data` is the message, `ready` is the flag.
/// The two live on separate cache lines on common machines.
#[derive(Default)]
#[cfg_attr(not(loom), repr(align(256)))]
pub struct Mailbox {
    data: Line<AtomicU64>,
    ready: Line<AtomicBool>,
}

#[derive(Default)]
#[cfg_attr(not(loom), repr(align(128)))]
struct Line<T>(T);

impl Mailbox {
    pub fn new() -> Self {
        Self {
            data: Line(AtomicU64::new(0)),
            ready: Line(AtomicBool::new(false)),
        }
    }

    /// `value` must not be 0, which means "nothing written yet".
    pub fn send(&self, value: u64, flag_store: Ordering) {
        self.data.0.store(value, Ordering::Relaxed);
        self.ready.0.store(true, flag_store);
    }

    /// `None` if the flag is not set. `Some(0)` is a stale read: the flag was
    /// visible and the data was not.
    pub fn try_receive(&self, flag_load: Ordering) -> Option<u64> {
        if !self.ready.0.load(flag_load) {
            return None;
        }
        Some(self.data.0.load(Ordering::Relaxed))
    }
}

/// How each side of the store-buffering test orders "store mine, then load yours".
#[derive(Clone, Copy, Debug)]
pub enum Dekker {
    ReleaseAcquire,
    SeqCstAccesses,
    SeqCstFence,
}

/// Store buffering: two flags, and each thread sets one and reads the other.
#[derive(Default)]
pub struct TwoFlags {
    left: AtomicU32,
    right: AtomicU32,
}

impl TwoFlags {
    pub fn new() -> Self {
        Self {
            left: AtomicU32::new(0),
            right: AtomicU32::new(0),
        }
    }

    pub fn enter_left(&self, how: Dekker) -> u32 {
        store_then_load(&self.left, &self.right, how)
    }

    pub fn enter_right(&self, how: Dekker) -> u32 {
        store_then_load(&self.right, &self.left, how)
    }
}

fn store_then_load(mine: &AtomicU32, theirs: &AtomicU32, how: Dekker) -> u32 {
    match how {
        Dekker::ReleaseAcquire => {
            mine.store(1, Ordering::Release);
            theirs.load(Ordering::Acquire)
        }
        Dekker::SeqCstAccesses => {
            mine.store(1, Ordering::SeqCst);
            theirs.load(Ordering::SeqCst)
        }
        Dekker::SeqCstFence => {
            mine.store(1, Ordering::Relaxed);
            fence(Ordering::SeqCst);
            theirs.load(Ordering::Relaxed)
        }
    }
}

/// Runs message passing over many fresh mailboxes on two real threads and counts
/// stale reads. A count of zero shows only that none happened in this run.
#[cfg(not(loom))]
pub fn count_stale_reads(rounds: usize, flag_store: Ordering, flag_load: Ordering) -> usize {
    let boxes: Vec<Mailbox> = (0..rounds).map(|_| Mailbox::new()).collect();
    std::thread::scope(|s| {
        s.spawn(|| {
            for mailbox in &boxes {
                mailbox.send(1, flag_store);
            }
        });
        let reader = s.spawn(|| {
            let mut stale = 0;
            for mailbox in &boxes {
                let value = loop {
                    match mailbox.try_receive(flag_load) {
                        Some(value) => break value,
                        // Lets the writer run when threads outnumber cores.
                        None => std::thread::yield_now(),
                    }
                };
                stale += usize::from(value == 0);
            }
            stale
        });
        reader.join().expect("reader panicked")
    })
}
