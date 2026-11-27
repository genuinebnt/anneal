use std::marker::PhantomData;

/// What each worker did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub per_worker: Vec<usize>,
}

/// Lets a handler add jobs.
pub struct Spawner<'a, J> {
    // TODO: the fields
    _todo: PhantomData<&'a J>,
}

impl<J> Spawner<'_, J> {
    pub fn spawn(&self, job: J) {
        todo!()
    }

    /// The worker running this handler, counted from 0.
    pub fn worker(&self) -> usize {
        todo!()
    }
}

/// Handles every job, and every job they spawn, on `workers` threads.
pub fn run<J, H>(workers: usize, jobs: Vec<J>, handle: H) -> Report
where
    J: Send,
    H: Fn(J, &Spawner<'_, J>) + Sync,
{
    todo!()
}
