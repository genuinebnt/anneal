use std::sync::atomic::{AtomicU8, Ordering};

const IDLE: u8 = 0;
const QUEUED: u8 = 1;
const RUNNING: u8 = 2;
/// Running, and asked for again since the run began.
const RUNNING_AGAIN: u8 = 3;
const DONE: u8 = 4;

/// Where one job is: waiting for work, in the queue, running, or done for ever.
pub struct QueueState {
    state: AtomicU8,
}

impl QueueState {
    pub fn new() -> QueueState {
        QueueState {
            state: AtomicU8::new(IDLE),
        }
    }

    /// New work for this job. `true`: the caller must put it in the queue.
    pub fn request(&self) -> bool {
        // Always a read-modify-write, even when the value does not change. Its release half publishes what this thread
        // wrote, and the next `begin` (an acquire read-modify-write on the same atomic) is sure to come after it.
        let before = self.step(|s| {
            Some(match s {
                IDLE => QUEUED,
                RUNNING => RUNNING_AGAIN,
                other => other,
            })
        });
        before == Ok(IDLE) || before == Ok(RUNNING)
    }

    /// A worker starts running the queued job.
    pub fn begin(&self) {
        let before = self.step(|s| (s == QUEUED).then_some(RUNNING));
        assert!(before.is_ok(), "begin: the job is not queued");
    }

    /// The run is over. `true`: put the job back in the queue.
    pub fn end(&self) -> bool {
        let before = self.step(|s| match s {
            RUNNING => Some(IDLE),
            RUNNING_AGAIN => Some(QUEUED),
            _ => None,
        });
        match before {
            Ok(state) => state == RUNNING_AGAIN,
            Err(_) => panic!("end: the job is not running"),
        }
    }

    /// The run is over and the job will never run again.
    pub fn finish(&self) {
        let before = self.step(|s| matches!(s, RUNNING | RUNNING_AGAIN).then_some(DONE));
        assert!(before.is_ok(), "finish: the job is not running");
    }

    /// One atomic transition: the compare-and-swap loop behind every method.
    fn step(&self, next: impl FnMut(u8) -> Option<u8>) -> Result<u8, u8> {
        self.state
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, next)
    }
}

impl Default for QueueState {
    fn default() -> Self {
        QueueState::new()
    }
}
