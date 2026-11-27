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
#[should_panic]
fn a_budget_of_zero_panics() {
    let _ = Importer::new(0);
}

#[test]
fn submit_does_not_run_the_job_itself() {
    run(async {
        let (imp, p) = (Importer::new(1), Probe::new());
        imp.submit(1, p.job(0, 1, 5)).unwrap();
        check!("jobs started before submit's caller yielded", p.starts(), vec![]);
        idle(&imp).await;
    });
}

#[test]
fn a_panic_returns_the_cost() {
    run(async {
        let (imp, p) = (Importer::new(2), Probe::new());
        imp.submit(2, async { panic!("decoder crashed") }).unwrap();
        imp.submit(2, p.job(1, 2, 5)).unwrap();
        idle(&imp).await;
        check!("the job after a panicking one", p.ends(), vec![1]);
    });
}

#[test]
fn idle_waits_for_queued_jobs_too() {
    run(async {
        let (imp, p) = (Importer::new(1), Probe::new());
        for id in 0..4 {
            imp.submit(1, p.job(id, 1, 10)).unwrap();
        }
        let start = Instant::now();
        idle(&imp).await;
        check!("four 10 ms jobs one at a time: (ended, elapsed)", (p.ends(), start.elapsed()), (vec![0, 1, 2, 3], ms(40)));
    });
}

#[test]
fn giving_up_on_idle_stops_nothing() {
    run(async {
        let (imp, p) = (Importer::new(1), Probe::new());
        for id in 0..3 {
            imp.submit(1, p.job(id, 1, 100)).unwrap();
        }
        let waited = tokio::time::timeout(ms(150), imp.idle()).await;
        check!("idle() after 150 of 300 ms has not returned", waited.is_err(), true);
        sleep(ms(1000)).await;
        check!("all jobs still ran", p.ends(), vec![0, 1, 2]);
    });
}

#[test]
fn several_waiters_and_late_submits() {
    run(async {
        let (imp, p) = (Arc::new(Importer::new(1)), Probe::new());
        imp.submit(1, p.job(0, 1, 10)).unwrap();
        let waiters: Vec<_> = (0..3)
            .map(|_| {
                let (imp, p) = (imp.clone(), p.clone());
                tokio::spawn(async move {
                    idle(&imp).await;
                    p.ends().len()
                })
            })
            .collect();
        settle().await;
        imp.submit(1, p.job(1, 1, 10)).unwrap();
        for w in waiters {
            check!("jobs ended when idle() returned", w.await.unwrap(), 2);
        }
    });
}

#[test]
fn finished_jobs_are_not_kept() {
    run(async {
        let (imp, p) = (Importer::new(100), Probe::new());
        for round in 0..20 {
            for id in 0..50 {
                imp.submit(1, p.job(id, 1, 1)).unwrap();
            }
            sleep(ms(10)).await;
            imp.submit(1, p.job(99, 1, 5)).unwrap();
            check!(format!("round {round}: held() right after a submit, one job unfinished"), imp.held(), 1);
            sleep(ms(10)).await;
        }
    });
}

#[test]
fn held_counts_unfinished_jobs() {
    run(async {
        let (imp, p) = (Importer::new(1), Probe::new());
        for id in 0..5 {
            imp.submit(1, p.job(id, 1, 10)).unwrap();
        }
        check!("five unfinished", imp.held(), 5);
        sleep(ms(25)).await;
        imp.submit(1, p.job(5, 1, 10)).unwrap();
        check!("two ended, one more submitted", imp.held(), 4);
    });
}

#[test]
fn dropping_the_importer_cancels_everything() {
    run(async {
        let (imp, p) = (Importer::new(2), Probe::new());
        for id in 0..6 {
            imp.submit(1, p.job(id, 1, 100)).unwrap();
        }
        settle().await;
        drop(imp);
        settle().await;
        check!("futures still alive after the drop", p.alive(), 0);
        sleep(ms(1000)).await;
        check!("(jobs that ever started, jobs that finished)", (p.starts().len(), p.ends().len()), (2, 0));
    });
}

#[test]
fn works_on_a_multi_thread_runtime() {
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_time().build().unwrap();
    runtime.block_on(async {
        let imp = Importer::new(7);
        let used = Arc::new(AtomicUsize::new(0));
        let done = Arc::new(AtomicUsize::new(0));
        for i in 0..2000u32 {
            let cost = 1 + i % 7;
            let (used, done) = (used.clone(), done.clone());
            imp.submit(cost, async move {
                let _held = Count::new(&used, cost as usize);
                assert!(used.load(Ordering::SeqCst) <= 7);
                tokio::task::yield_now().await;
                // Counted only if the budget held.
                if used.load(Ordering::SeqCst) <= 7 {
                    done.fetch_add(1, Ordering::SeqCst);
                }
            })
            .unwrap();
        }
        tokio::time::timeout(Duration::from_secs(10), imp.idle()).await.expect("idle() never returned");
        check!("jobs that ran within the budget", done.load(Ordering::SeqCst), 2000);
    });
}

#[test]
fn random_loads_match_a_simulation() {
    let mut rng = anneal_prelude::Rng::new(21303);
    for _ in 0..200 {
        let budget = 1 + rng.below(6) as u32;
        let n = rng.below(9);
        let jobs: Vec<(u32, u64)> = (0..n).map(|_| (1 + rng.below(budget as usize) as u32, 1 + rng.below(9) as u64)).collect();
        // The head of the queue starts as soon as it fits. `running` holds (end time, cost).
        let mut want = Vec::new();
        let mut running: Vec<(u64, u32)> = Vec::new();
        let mut now = 0u64;
        for (id, &(cost, dur)) in jobs.iter().enumerate() {
            while running.iter().map(|r| r.1).sum::<u32>() + cost > budget {
                now = running.iter().map(|r| r.0).min().unwrap();
                running.retain(|r| r.0 > now);
            }
            want.push((now, id as u32));
            running.push((now + dur, cost));
        }
        let got = run(async {
            let (imp, p) = (Importer::new(budget), Probe::new());
            for (id, &(cost, dur)) in jobs.iter().enumerate() {
                imp.submit(cost, p.job(id as u32, cost, dur)).unwrap();
            }
            idle(&imp).await;
            let mut starts = p.starts();
            starts.sort_by_key(|s| (s.0, s.1));
            starts
        });
        check!(format!("budget {budget}, jobs (cost, ms) {jobs:?}: (start ms, id)"), got, want);
    }
}
