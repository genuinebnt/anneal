use solution::*;

#[allow(unused_imports)]
use std::sync::atomic::{AtomicUsize, Ordering};
#[allow(unused_imports)]
use std::sync::{Arc, Mutex};
#[allow(unused_imports)]
use std::time::Duration;
#[allow(unused_imports)]
use tokio::time::{Instant, sleep};

#[allow(dead_code)]
/// A current-thread runtime with a clock that jumps: no test waits in real time.
pub fn run<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().enable_time().start_paused(true).build().unwrap().block_on(f)
}

#[allow(dead_code)]
pub fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[allow(dead_code)]
pub async fn settle() {
    sleep(ms(1)).await;
}

#[allow(dead_code)]
/// What the jobs did: (virtual start time in ms, job id), and the cost in use.
#[derive(Clone)]
pub struct Probe {
    pub zero: Instant,
    pub starts: Arc<Mutex<Vec<(u64, u32)>>>,
    pub ends: Arc<Mutex<Vec<u32>>>,
    pub used: Arc<AtomicUsize>,
    pub peak: Arc<AtomicUsize>,
    pub alive: Arc<AtomicUsize>,
}

#[allow(dead_code)]
/// Counts something up until dropped.
pub struct Count(Arc<AtomicUsize>, usize);

#[allow(dead_code)]
impl Count {
    pub fn new(n: &Arc<AtomicUsize>, by: usize) -> Self {
        n.fetch_add(by, Ordering::SeqCst);
        Count(n.clone(), by)
    }
}

#[allow(dead_code)]
impl Drop for Count {
    fn drop(&mut self) {
        self.0.fetch_sub(self.1, Ordering::SeqCst);
    }
}

#[allow(dead_code)]
#[allow(clippy::new_without_default)]
impl Probe {
    pub fn new() -> Self {
        Probe { zero: Instant::now(), starts: Default::default(), ends: Default::default(), used: Default::default(), peak: Default::default(), alive: Default::default() }
    }

    /// A job that records itself and takes `millis` of virtual time.
    pub fn job(&self, id: u32, cost: u32, millis: u64) -> impl std::future::Future<Output = ()> + Send + 'static + use<> {
        let p = self.clone();
        let alive = Count::new(&p.alive, 1);
        async move {
            let _alive = alive;
            p.starts.lock().unwrap().push((p.zero.elapsed().as_millis() as u64, id));
            let _used = Count::new(&p.used, cost as usize);
            p.peak.fetch_max(p.used.load(Ordering::SeqCst), Ordering::SeqCst);
            sleep(ms(millis)).await;
            p.ends.lock().unwrap().push(id);
        }
    }

    pub fn starts(&self) -> Vec<(u64, u32)> {
        self.starts.lock().unwrap().clone()
    }

    pub fn ends(&self) -> Vec<u32> {
        self.ends.lock().unwrap().clone()
    }

    pub fn alive(&self) -> usize {
        self.alive.load(Ordering::SeqCst)
    }
}

#[allow(dead_code)]
/// `idle()`, but a test failure instead of a hang.
pub async fn idle(importer: &Importer) {
    tokio::time::timeout(Duration::from_secs(3600), importer.idle()).await.expect("idle() did not return within an hour of virtual time");
}

#[test]
fn jobs_that_fit_run_together() {
    run(async {
        let (imp, p) = (Importer::new(10), Probe::new());
        for id in 0..3 {
            check!("submit cost 3", imp.submit(3, p.job(id, 3, 50)), Ok(()));
        }
        idle(&imp).await;
        check!("three jobs of cost 3 in a budget of 10: (start ms, id)", p.starts(), vec![(0, 0), (0, 1), (0, 2)]);
    });
}

#[test]
fn the_budget_is_not_exceeded() {
    run(async {
        let (imp, p) = (Importer::new(10), Probe::new());
        for id in 0..6 {
            imp.submit(4, p.job(id, 4, 10)).unwrap();
        }
        idle(&imp).await;
        check!("most cost in use at once", p.peak.load(Ordering::SeqCst), 8);
        check!("(start ms, id)", p.starts(), vec![(0, 0), (0, 1), (10, 2), (10, 3), (20, 4), (20, 5)]);
    });
}

#[test]
fn a_large_job_is_not_overtaken() {
    run(async {
        let (imp, p) = (Importer::new(10), Probe::new());
        imp.submit(6, p.job(0, 6, 20)).unwrap();
        imp.submit(10, p.job(1, 10, 20)).unwrap();
        imp.submit(1, p.job(2, 1, 20)).unwrap();
        idle(&imp).await;
        check!("costs 6, 10, 1: (start ms, id)", p.starts(), vec![(0, 0), (20, 1), (40, 2)]);
    });
}

#[test]
fn impossible_costs_are_refused() {
    run(async {
        let (imp, p) = (Importer::new(5), Probe::new());
        check!("cost 6 of 5", imp.submit(6, p.job(0, 6, 1)), Err(BadCost));
        check!("cost 0", imp.submit(0, p.job(1, 0, 1)), Err(BadCost));
        check!("cost 5 of 5", imp.submit(5, p.job(2, 5, 1)), Ok(()));
        idle(&imp).await;
        check!("jobs that started", p.starts(), vec![(0, 2)]);
    });
}

#[test]
fn nothing_to_wait_for() {
    run(async {
        let imp = Importer::new(1);
        idle(&imp).await;
        check!("held()", imp.held(), 0);
    });
}
