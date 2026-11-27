use std::time::{Duration, Instant};

/// Names one alarm, for cancelling it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AlarmId(u64);

/// Sets and cancels alarms. Cheap to clone and usable from any thread.
#[derive(Clone)]
pub struct Alarms {
    // TODO: the fields
}

/// Owns the thread that runs the alarms.
pub struct AlarmClock {
    // TODO: the fields
}

impl AlarmClock {
    pub fn start() -> AlarmClock {
        todo!()
    }

    pub fn alarms(&self) -> Alarms {
        todo!()
    }
}

impl Alarms {
    /// Runs `f` on the clock's thread, no earlier than `when`.
    pub fn at(&self, when: Instant, f: impl FnOnce() + Send + 'static) -> AlarmId {
        todo!()
    }

    pub fn after(&self, delay: Duration, f: impl FnOnce() + Send + 'static) -> AlarmId {
        todo!()
    }

    /// `true` if the alarm had not started and now never will.
    pub fn cancel(&self, id: AlarmId) -> bool {
        todo!()
    }

    /// Alarms not yet started and not cancelled.
    pub fn pending(&self) -> usize {
        todo!()
    }
}
