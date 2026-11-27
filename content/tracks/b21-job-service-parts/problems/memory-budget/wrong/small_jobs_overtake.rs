use std::future::Future;
use std::sync::{Arc, Mutex};
use tokio::sync::{watch, Semaphore};
use tokio::task::JoinSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadCost;

pub struct Importer {
    budget: u32,
    permits: Arc<Semaphore>,
    tasks: Mutex<JoinSet<()>>,
    unfinished: Arc<watch::Sender<usize>>,
}

/// Counts one job as unfinished until dropped: after a return, a panic or a cancellation.
struct Unfinished(Arc<watch::Sender<usize>>);

impl Drop for Unfinished {
    fn drop(&mut self) {
        self.0.send_modify(|n| *n -= 1);
    }
}

impl Importer {
    pub fn new(budget: u32) -> Self {
        assert!(budget > 0, "the budget must be at least 1");
        Self {
            budget,
            permits: Arc::new(Semaphore::new(budget as usize)),
            tasks: Mutex::default(),
            unfinished: Arc::new(watch::Sender::new(0)),
        }
    }

    pub fn submit<F>(&self, cost: u32, work: F) -> Result<(), BadCost>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        if cost == 0 || cost > self.budget {
            return Err(BadCost);
        }
        self.unfinished.send_modify(|n| *n += 1);
        let counted = Unfinished(self.unfinished.clone());
        let permits = self.permits.clone();
        let mut tasks = self.tasks.lock().unwrap();
        // A JoinSet keeps every finished task until somebody takes it out.
        while tasks.try_join_next().is_some() {}
        tasks.spawn(async move {
            let _counted = counted;
            // The semaphore serves waiters in turn, so a large request is not overtaken.
            let _permits = loop {
                if let Ok(p) = permits.clone().try_acquire_many_owned(cost) {
                    break p;
                }
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            };
            work.await;
        });
        Ok(())
    }

    pub fn held(&self) -> usize {
        self.tasks.lock().unwrap().len()
    }

    pub async fn idle(&self) {
        // Only watches: dropping this future leaves every job alone.
        let mut unfinished = self.unfinished.subscribe();
        let _ = unfinished.wait_for(|n| *n == 0).await;
    }
}
