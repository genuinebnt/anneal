use std::collections::VecDeque;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::thread::JoinHandle;

type Job = Box<dyn FnOnce() + Send + 'static>;

struct Queue {
    jobs: VecDeque<Job>,
    closed: bool,
}

struct Shared {
    queue: Mutex<Queue>,
    wake: Condvar,
    panicked: AtomicUsize,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TaskPanicked;

/// The result of one `submit`.
pub struct TaskHandle<R> {
    rx: mpsc::Receiver<R>,
}

impl<R> TaskHandle<R> {
    /// Waits for the task. `Err` if it panicked.
    pub fn join(self) -> Result<R, TaskPanicked> {
        self.rx.recv().map_err(|_| TaskPanicked)
    }
}

pub struct ThreadPool {
    shared: Arc<Shared>,
    workers: Vec<JoinHandle<()>>,
}

fn worker(shared: Arc<Shared>) {
    loop {
        let job = {
            let mut q = shared.queue.lock().unwrap();
            loop {
                if let Some(job) = q.jobs.pop_front() {
                    break Some(job);
                }
                if q.closed {
                    break None; // closed AND drained: every queued job has been taken
                }
                q = shared.wake.wait(q).unwrap();
            }
        };
        match job {
            Some(job) => {
                // run outside the lock; a panicking job must not take the worker down with it
                if catch_unwind(AssertUnwindSafe(job)).is_err() {
                    shared.panicked.fetch_add(1, Ordering::SeqCst);
                }
            }
            None => return,
        }
    }
}

impl ThreadPool {
    /// Panics if `workers` is 0.
    pub fn new(workers: usize) -> Self {
        assert!(workers > 0, "a pool needs at least one worker");
        let shared = Arc::new(Shared { queue: Mutex::new(Queue { jobs: VecDeque::new(), closed: false }), wake: Condvar::new(), panicked: AtomicUsize::new(0) });
        let workers = (0..workers)
            .map(|i| {
                let shared = shared.clone();
                std::thread::Builder::new().name(format!("pool-worker-{i}")).spawn(move || worker(shared)).unwrap()
            })
            .collect();
        ThreadPool { shared, workers }
    }

    pub fn execute<F: FnOnce() + Send + 'static>(&self, f: F) {
        self.shared.queue.lock().unwrap().jobs.push_back(Box::new(f));
        self.shared.wake.notify_one();
    }

    pub fn submit<F, R>(&self, f: F) -> TaskHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let (tx, rx) = mpsc::channel();
        self.execute(move || {
            let _ = tx.send(f()); // if `f` panics, `tx` is dropped unsent and `join` reports it
        });
        TaskHandle { rx }
    }

    /// Stops accepting work, lets the workers finish EVERY job already queued, joins them, and returns how many jobs panicked.
    pub fn shutdown(mut self) -> usize {
        self.finish()
    }

    fn finish(&mut self) -> usize {
        self.shared.queue.lock().unwrap().closed = true;
        self.shared.wake.notify_all();
        for w in self.workers.drain(..) {
            let _ = w.join();
        }
        self.shared.panicked.load(Ordering::SeqCst)
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        self.finish();
    }
}
