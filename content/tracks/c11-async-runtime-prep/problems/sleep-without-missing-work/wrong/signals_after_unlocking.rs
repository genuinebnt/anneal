use monitor::{Guard, Lock, Signal};

// ---------------------------------------------------------------------------------------------------------------------
// Given: a lock and a signal that the tests can watch and steer. Use them as you would `Mutex` and `Condvar`, and keep
// this module as it is.
// ---------------------------------------------------------------------------------------------------------------------

pub mod monitor {
    use std::cell::{Cell, RefCell};
    use std::ops::{Deref, DerefMut};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
    use std::thread::{LocalKey, Thread};

    type Hook = RefCell<Option<Box<dyn FnOnce()>>>;

    thread_local! {
        static AFTER_UNLOCK: Hook = const { RefCell::new(None) };
        static ASLEEP: Hook = const { RefCell::new(None) };
        static SPURIOUS: Cell<bool> = const { Cell::new(false) };
    }

    fn run(hook: &'static LocalKey<Hook>) {
        let f = hook.with(|h| h.borrow_mut().take());
        if let Some(f) = f {
            f();
        }
    }

    /// For tests: runs `f` once, right after this thread next releases a `Lock` by dropping its guard.
    pub fn after_next_unlock(f: impl FnOnce() + 'static) {
        AFTER_UNLOCK.with(|h| *h.borrow_mut() = Some(Box::new(f)));
    }

    /// For tests: runs `f` once, when this thread next goes to sleep on a `Signal`. By then it can be notified.
    pub fn when_next_asleep(f: impl FnOnce() + 'static) {
        ASLEEP.with(|h| *h.borrow_mut() = Some(Box::new(f)));
    }

    /// For tests: this thread's next `Signal::wait` wakes up at once, though nobody notified.
    pub fn wake_once_for_no_reason() {
        SPURIOUS.with(|s| s.set(true));
    }

    /// A mutex. `lock` returns the guard directly: there is no poisoning to handle.
    #[derive(Default)]
    pub struct Lock<T>(Mutex<T>);

    impl<T> Lock<T> {
        pub fn new(value: T) -> Lock<T> {
            Lock(Mutex::new(value))
        }

        pub fn lock(&self) -> Guard<'_, T> {
            let held = self.0.lock().unwrap_or_else(PoisonError::into_inner);
            Guard {
                lock: self,
                held: Some(held),
            }
        }
    }

    pub struct Guard<'a, T> {
        lock: &'a Lock<T>,
        held: Option<MutexGuard<'a, T>>,
    }

    impl<T> Deref for Guard<'_, T> {
        type Target = T;
        fn deref(&self) -> &T {
            self.held.as_ref().expect("the guard holds its lock")
        }
    }

    impl<T> DerefMut for Guard<'_, T> {
        fn deref_mut(&mut self) -> &mut T {
            self.held.as_mut().expect("the guard holds its lock")
        }
    }

    impl<T> Drop for Guard<'_, T> {
        fn drop(&mut self) {
            if let Some(held) = self.held.take() {
                drop(held);
                if !std::thread::panicking() {
                    run(&AFTER_UNLOCK);
                }
            }
        }
    }

    struct Sleeper {
        woken: AtomicBool,
        thread: Thread,
    }

    /// A condition variable that may be signalled with or without the lock. A real one may wake any of its sleepers. This
    /// one always picks the **newest**. A real one may also wake a thread for no reason; this one does so only when a test
    /// asks for it.
    #[derive(Default)]
    pub struct Signal {
        sleepers: Mutex<Vec<Arc<Sleeper>>>,
    }

    impl Signal {
        pub fn new() -> Signal {
            Signal::default()
        }

        fn sleepers(&self) -> MutexGuard<'_, Vec<Arc<Sleeper>>> {
            self.sleepers.lock().unwrap_or_else(PoisonError::into_inner)
        }

        /// Releases the lock and sleeps until notified, as one step. Returns with the lock held again.
        pub fn wait<'a, T>(&self, mut guard: Guard<'a, T>) -> Guard<'a, T> {
            if SPURIOUS.with(|s| s.replace(false)) {
                let lock = guard.lock;
                drop(guard.held.take());
                return lock.lock();
            }
            let me = Arc::new(Sleeper {
                woken: AtomicBool::new(false),
                thread: std::thread::current(),
            });
            // listed while the lock is still held, so no later notification can miss this thread
            self.sleepers().push(me.clone());
            let lock = guard.lock;
            drop(guard.held.take());
            run(&ASLEEP);
            while !me.woken.load(Ordering::Acquire) {
                std::thread::park();
            }
            lock.lock()
        }

        /// Waits for as long as `condition` is true.
        pub fn wait_while<'a, T>(
            &self,
            mut guard: Guard<'a, T>,
            mut condition: impl FnMut(&mut T) -> bool,
        ) -> Guard<'a, T> {
            while condition(&mut guard) {
                guard = self.wait(guard);
            }
            guard
        }

        pub fn notify_one(&self) {
            let newest = self.sleepers().pop();
            if let Some(sleeper) = newest {
                Signal::wake(&sleeper);
            }
        }

        pub fn notify_all(&self) {
            let all = std::mem::take(&mut *self.sleepers());
            all.iter().for_each(|sleeper| Signal::wake(sleeper));
        }

        fn wake(sleeper: &Sleeper) {
            sleeper.woken.store(true, Ordering::Release);
            sleeper.thread.unpark();
        }
    }
}

/// Proof that its holder announced it is about to sleep. It is used up by `cancel` or `sleep`.
pub struct Ticket {
    /// The number of notifications there had been when it was made.
    seen: u64,
}

#[derive(Default)]
struct State {
    notifications: u64,
    waiting: usize,
}

/// Where idle workers sleep.
pub struct Gate {
    state: Lock<State>,
    changed: Signal,
}

impl Gate {
    pub fn new() -> Gate {
        Gate {
            state: Lock::new(State::default()),
            changed: Signal::new(),
        }
    }

    fn lock(&self) -> Guard<'_, State> {
        self.state.lock()
    }

    /// "I am about to sleep." Look for work once more after this.
    pub fn prepare(&self) -> Ticket {
        let mut state = self.lock();
        state.waiting += 1;
        Ticket {
            seen: state.notifications,
        }
    }

    /// There was work after all.
    pub fn cancel(&self, ticket: Ticket) {
        let _ = ticket;
        self.lock().waiting -= 1;
    }

    /// Blocks until a notification made after `prepare` releases the ticket.
    pub fn sleep(&self, ticket: Ticket) {
        let mut state = self
            .changed
            .wait_while(self.lock(), |state| state.notifications == ticket.seen);
        state.waiting -= 1;
    }

    pub fn notify_one(&self) {
        // every ticket older than this is now releasable; one sleeper is woken to use it
        self.lock().notifications += 1;
        self.changed.notify_one();
    }

    pub fn notify_all(&self) {
        self.lock().notifications += 1;
        self.changed.notify_all();
    }

    /// Tickets prepared and not yet cancelled or returned from `sleep`.
    pub fn waiting(&self) -> usize {
        self.lock().waiting
    }
}

impl Default for Gate {
    fn default() -> Self {
        Gate::new()
    }
}
