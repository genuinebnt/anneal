//! A one-shot gate: threads wait at it until it is opened, and once open it stays open.

use std::sync::{Condvar, Mutex};

pub struct Gate {
    open: Mutex<bool>,
    changed: Condvar,
}

impl Gate {
    pub fn new() -> Gate {
        Gate { open: Mutex::new(false), changed: Condvar::new() }
    }

    pub fn is_open(&self) -> bool {
        *self.open.lock().unwrap()
    }

    /// Opens the gate for everyone: waiting threads go on, and later waiters do not wait at all.
    pub fn open(&self) {
        *self.open.lock().unwrap() = true;
        // TODO(1b-c2): this looks right and has a bug; find it and fix it
        self.changed.notify_one();
    }

    /// Waits until the gate is open.
    pub fn wait(&self) {
        let mut open = self.open.lock().unwrap();
        while !*open {
            open = self.changed.wait(open).unwrap();
        }
    }
}

impl Default for Gate {
    fn default() -> Self {
        Gate::new()
    }
}
