use crossbeam::deque::{Injector, Steal, Stealer, Worker};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// What each worker did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub per_worker: Vec<usize>,
}

struct Shared<J> {
    /// The given jobs, taken one at a time so that a worker's own spawns come before the next one.
    given: Injector<J>,
    stealers: Vec<Stealer<J>>,
    /// Jobs given or spawned and not yet finished. Zero means nothing can appear any more.
    unfinished: AtomicUsize,
    failed: AtomicBool,
}

/// Lets a handler add jobs.
pub struct Spawner<'a, J> {
    local: &'a Worker<J>,
    unfinished: &'a AtomicUsize,
    index: usize,
}

impl<J> Spawner<'_, J> {
    pub fn spawn(&self, job: J) {
        // count it before anyone can see it, so the count never reaches zero early
        self.unfinished.fetch_add(1, Ordering::SeqCst);
        self.local.push(job);
    }

    /// The worker running this handler, counted from 0.
    pub fn worker(&self) -> usize {
        self.index
    }
}

/// Counts the job as finished when dropped, even if its handler panicked.
struct Finishing<'a, J>(&'a Shared<J>);

impl<J> Drop for Finishing<'_, J> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.0.failed.store(true, Ordering::SeqCst);
        }
        self.0.unfinished.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Retries while a queue says "try again".
fn steal<J>(mut attempt: impl FnMut() -> Steal<J>) -> Option<J> {
    loop {
        match attempt() {
            Steal::Success(job) => return Some(job),
            Steal::Empty => return None,
            Steal::Retry => {}
        }
    }
}

fn find<J>(shared: &Shared<J>, local: &Worker<J>, index: usize) -> Option<J> {
    local
        .pop()
        .or_else(|| steal(|| shared.given.steal()))
        .or_else(|| {
            return None;
            #[allow(unreachable_code)]
            let others = shared.stealers.iter().enumerate();
            others
                .filter(|(i, _)| *i != index)
                .find_map(|(_, other)| steal(|| other.steal_batch_and_pop(local)))
        })
}

fn work<J, H>(shared: &Shared<J>, local: &Worker<J>, index: usize, handle: &H) -> usize
where
    H: Fn(J, &Spawner<'_, J>),
{
    let spawner = Spawner {
        local,
        unfinished: &shared.unfinished,
        index,
    };
    let mut handled = 0;
    while !shared.failed.load(Ordering::SeqCst) {
        match find(shared, local, index) {
            Some(job) => {
                let _finishing = Finishing(shared);
                handle(job, &spawner);
                handled += 1;
            }
            None if shared.unfinished.load(Ordering::SeqCst) == 0 => break,
            None => std::thread::yield_now(),
        }
    }
    handled
}

/// Handles every job, and every job they spawn, on `workers` threads.
pub fn run<J, H>(workers: usize, jobs: Vec<J>, handle: H) -> Report
where
    J: Send,
    H: Fn(J, &Spawner<'_, J>) + Sync,
{
    assert!(workers > 0, "at least one worker");
    let locals: Vec<Worker<J>> = (0..workers).map(|_| Worker::new_fifo()).collect();
    let shared = Shared {
        given: Injector::new(),
        stealers: locals.iter().map(Worker::stealer).collect(),
        unfinished: AtomicUsize::new(jobs.len()),
        failed: AtomicBool::new(false),
    };
    for job in jobs {
        shared.given.push(job);
    }
    let per_worker = std::thread::scope(|scope| {
        let (shared, handle) = (&shared, &handle);
        let threads: Vec<_> = locals
            .into_iter()
            .enumerate()
            .map(|(index, local)| scope.spawn(move || work(shared, &local, index, handle)))
            .collect();
        // join every thread before reporting a panic
        let joined: Vec<_> = threads.into_iter().map(|t| t.join()).collect();
        joined
            .into_iter()
            .map(|r| r.unwrap_or_else(|payload| std::panic::resume_unwind(payload)))
            .collect()
    });
    Report { per_worker }
}
