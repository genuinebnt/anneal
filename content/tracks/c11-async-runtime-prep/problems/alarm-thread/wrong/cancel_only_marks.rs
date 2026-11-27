use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Names one alarm, for cancelling it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AlarmId(u64);

type Ring = Box<dyn FnOnce() + Send>;

#[derive(Default)]
struct State {
    /// Smallest `(time, id)` first. Ids grow, so equal times keep the order they were set in.
    order: BinaryHeap<Reverse<(Instant, u64)>>,
    /// The closures of pending alarms. A cancelled alarm leaves here at once; its heap entry stays until later.
    rings: HashMap<u64, Ring>,
    /// Ids that must not run. Their closures leave `rings` when their time comes.
    cancelled: HashSet<u64>,
    next_id: u64,
    stopped: bool,
}

#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    changed: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn run(&self) {
        let mut state = self.lock();
        while !state.stopped {
            let Some(&Reverse((when, id))) = state.order.peek() else {
                state = self
                    .changed
                    .wait(state)
                    .unwrap_or_else(PoisonError::into_inner);
                continue;
            };
            if !state.rings.contains_key(&id) {
                state.order.pop();
                continue;
            }
            let now = Instant::now();
            if when > now {
                // woken early by a new alarm, by stop, or by nothing: the loop looks again
                state = self
                    .changed
                    .wait_timeout(state, when - now)
                    .unwrap_or_else(PoisonError::into_inner)
                    .0;
                continue;
            }
            state.order.pop();
            let mut ring = state.rings.remove(&id);
            if state.cancelled.remove(&id) {
                ring = None;
            }
            drop(state);
            if let Some(ring) = ring {
                // the closure is consumed; nothing it half changed is looked at here
                let _ = catch_unwind(AssertUnwindSafe(ring));
            }
            state = self.lock();
        }
    }
}

/// Sets and cancels alarms. Cheap to clone and usable from any thread.
#[derive(Clone)]
pub struct Alarms {
    shared: Arc<Shared>,
}

/// Owns the thread that runs the alarms.
pub struct AlarmClock {
    alarms: Alarms,
    thread: Option<JoinHandle<()>>,
}

impl AlarmClock {
    pub fn start() -> AlarmClock {
        let shared = Arc::new(Shared::default());
        let on_thread = shared.clone();
        AlarmClock {
            alarms: Alarms { shared },
            thread: Some(std::thread::spawn(move || on_thread.run())),
        }
    }

    pub fn alarms(&self) -> Alarms {
        self.alarms.clone()
    }
}

impl Drop for AlarmClock {
    fn drop(&mut self) {
        self.alarms.shared.lock().stopped = true;
        self.alarms.shared.changed.notify_all();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let left = {
            let mut state = self.alarms.shared.lock();
            state.order.clear();
            std::mem::take(&mut state.rings)
        };
        // outside the lock: a captured value's destructor may call back in
        drop(left);
    }
}

impl Alarms {
    /// Runs `f` on the clock's thread, no earlier than `when`.
    pub fn at(&self, when: Instant, f: impl FnOnce() + Send + 'static) -> AlarmId {
        let mut state = self.shared.lock();
        let id = state.next_id;
        state.next_id += 1;
        if state.stopped {
            drop(state);
            drop(f);
            return AlarmId(id);
        }
        state.order.push(Reverse((when, id)));
        state.rings.insert(id, Box::new(f));
        drop(state);
        // the thread may be asleep until a later time
        self.shared.changed.notify_all();
        AlarmId(id)
    }

    pub fn after(&self, delay: Duration, f: impl FnOnce() + Send + 'static) -> AlarmId {
        self.at(Instant::now() + delay, f)
    }

    /// `true` if the alarm had not started and now never will.
    pub fn cancel(&self, id: AlarmId) -> bool {
        let mut state = self.shared.lock();
        state.rings.contains_key(&id.0) && state.cancelled.insert(id.0)
    }

    /// Alarms not yet started and not cancelled.
    pub fn pending(&self) -> usize {
        let state = self.shared.lock();
        state.rings.len() - state.cancelled.len()
    }
}
