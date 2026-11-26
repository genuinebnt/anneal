use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Totals {
    pub accepted: u64,
    pub refused: u64,
    pub handled: u64,
    pub panicked: u64,
}

#[derive(Default)]
struct Counts {
    waiting: AtomicUsize,
    accepted: AtomicU64,
    refused: AtomicU64,
    handled: AtomicU64,
    panicked: AtomicU64,
}

pub struct Service<T> {
    /// `None` once the service is closing: dropping it is what tells the workers to finish.
    intake: Option<SyncSender<T>>,
    workers: Vec<JoinHandle<()>>,
    counts: Arc<Counts>,
}

fn work<T>(intake: &Mutex<Receiver<T>>, counts: &Counts, handle: &(dyn Fn(T) + Send + Sync)) {
    loop {
        // the lock guard lives to the end of this statement only: others can take work while this one runs
        let next = intake.lock().unwrap_or_else(|e| e.into_inner()).recv();
        let Ok(item) = next else {
            return; // no sender left and nothing queued
        };
        counts.waiting.fetch_sub(1, Ordering::SeqCst);
        let counter = match catch_unwind(AssertUnwindSafe(|| handle(item))) {
            Ok(()) => &counts.handled,
            Err(_) => &counts.panicked,
        };
        counter.fetch_add(1, Ordering::SeqCst);
    }
}

impl<T: Send + 'static> Service<T> {
    pub fn start(workers: usize, room: usize, handle: impl Fn(T) + Send + Sync + 'static) -> Service<T> {
        assert!(workers > 0 && room > 0, "need at least one worker and one place to wait");
        let (sender, receiver) = sync_channel(room);
        let receiver = Arc::new(Mutex::new(receiver));
        let counts = Arc::new(Counts::default());
        let handle: Arc<dyn Fn(T) + Send + Sync> = Arc::new(handle);
        let workers = (0..workers)
            .map(|_| {
                let (receiver, counts, handle) = (receiver.clone(), counts.clone(), handle.clone());
                std::thread::spawn(move || work(&receiver, &counts, &*handle))
            })
            .collect();
        Service { intake: Some(sender), workers, counts }
    }

    pub fn offer(&self, item: T) -> Result<(), T> {
        let Some(intake) = &self.intake else {
            return Err(item);
        };
        match intake.try_send(item) {
            Ok(()) => {
                std::thread::yield_now();
                self.counts.waiting.fetch_add(1, Ordering::SeqCst);
                self.counts.accepted.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
            Err(TrySendError::Full(item) | TrySendError::Disconnected(item)) => {
                self.counts.refused.fetch_add(1, Ordering::SeqCst);
                Err(item)
            }
        }
    }

    pub fn waiting(&self) -> usize {
        self.counts.waiting.load(Ordering::SeqCst)
    }

    pub fn close(mut self) -> Totals {
        self.finish();
        let c = &self.counts;
        Totals {
            accepted: c.accepted.load(Ordering::SeqCst),
            refused: c.refused.load(Ordering::SeqCst),
            handled: c.handled.load(Ordering::SeqCst),
            panicked: c.panicked.load(Ordering::SeqCst),
        }
    }
}

impl<T> Service<T> {
    fn finish(&mut self) {
        self.intake = None;
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

impl<T> Drop for Service<T> {
    fn drop(&mut self) {
        self.finish();
    }
}
