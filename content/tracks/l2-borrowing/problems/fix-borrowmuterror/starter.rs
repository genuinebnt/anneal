use std::cell::RefCell;
use std::collections::HashMap;

pub struct Scheduler {
    queue: RefCell<Vec<u32>>,
    seen: RefCell<HashMap<u32, u32>>,
    done: RefCell<Vec<u32>>,
}

impl Scheduler {
    pub fn new() -> Self {
        Scheduler {
            queue: RefCell::new(Vec::new()),
            seen: RefCell::new(HashMap::new()),
            done: RefCell::new(Vec::new()),
        }
    }

    /// Counts a submission of `job`, and queues it if the counts had no entry for it yet. Returns whether it
    /// queued.
    pub fn submit(&self, job: u32) -> bool {
        let seen = self.seen.borrow();
        let first = !seen.contains_key(&job);
        *self.seen.borrow_mut().entry(job).or_insert(0) += 1;
        if first {
            self.queue.borrow_mut().push(job);
        }
        first
    }

    /// How many times `job` was submitted. A job never submitted gets an entry with 0.
    pub fn count(&self, job: u32) -> u32 {
        if let Some(n) = self.seen.borrow().get(&job) {
            *n
        } else {
            self.seen.borrow_mut().insert(job, 0);
            0
        }
    }

    /// Runs queued jobs, newest first, until the queue is empty. Running an even job above 0 submits job / 2.
    /// Returns how many jobs ran.
    pub fn run(&self) -> usize {
        let mut ran = 0;
        while let Some(job) = self.queue.borrow_mut().pop() {
            self.done.borrow_mut().push(job);
            if job > 0 && job % 2 == 0 {
                self.submit(job / 2);
            }
            ran += 1;
        }
        ran
    }

    pub fn done(&self) -> Vec<u32> {
        self.done.borrow().to_vec()
    }

    /// How many jobs have an entry in the submission counts.
    pub fn known(&self) -> usize {
        self.seen.borrow().len()
    }
}
